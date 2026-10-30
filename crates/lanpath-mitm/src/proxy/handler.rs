use super::*;
use crate::PatternError;

impl Handler {
    fn script_body_limit(
        &self,
        url: &str,
        kind: ScriptKind,
    ) -> Result<Option<usize>, PatternError> {
        self.hooks
            .as_ref()
            .map(|hooks| hooks.transport_body_limit(url, kind, self.config.max_body_size))
            .transpose()
            .map(Option::flatten)
    }

    fn regex_response_abort(&mut self, error: PatternError) -> Response<Body> {
        tracing::warn!(%error, "response regex rejected traffic");
        self.aborted = true;
        Response::new(Body::empty())
    }
}
impl HttpHandler for Handler {
    fn aborted(&self) -> bool {
        self.aborted
    }
    async fn connect_raw(&self, host: &str, port: u16) -> std::io::Result<tokio::net::TcpStream> {
        crate::upstream::connect_tcp(self.config.upstream, host, port)
            .await
            .map_err(std::io::Error::other)
    }
    async fn handle_request(
        &mut self,
        _ctx: &HttpContext,
        req: Request<Body>,
    ) -> RequestOrResponse {
        if req.method() == http::Method::CONNECT {
            self.connect_authority = None;
            return req.into();
        }
        let (mut parts, original) = req.into_parts();
        let mut method = parts.method.to_string();
        let mut url = crate::request_url::request_url(&parts.uri);
        let mut headers = headers_to_vec(&parts.headers);
        let plan = (|| {
            let (rewrite_body, hook_url) =
                self.rewrite.request_body_plan(&method, &url, &headers)?;
            let script_limit = self.script_body_limit(&hook_url, ScriptKind::HttpRequest)?;
            let context_limit = self.script_body_limit(&hook_url, ScriptKind::HttpResponse)?;
            let rejects_overflow = self
                .hooks
                .as_ref()
                .map(|hooks| hooks.rejects_request_body_overflow(&hook_url))
                .transpose()?
                .unwrap_or(false);
            Ok::<_, PatternError>((rewrite_body, script_limit, context_limit, rejects_overflow))
        })();
        let (rewrite_body, script_limit, context_limit, rejects_overflow) = match plan {
            Ok(plan) => plan,
            Err(error) => {
                tracing::warn!(%error, "request regex planning rejected traffic");
                return RequestOrResponse::Abort;
            }
        };
        let selected = rewrite_body || script_limit.is_some() || context_limit.is_some();
        let limit = script_limit
            .into_iter()
            .chain(context_limit)
            .chain(rewrite_body.then_some(self.config.max_body_size))
            .max()
            .unwrap_or(0);
        let (inspected, original) = if selected {
            (
                Some(Inspected::read(original, limit, &parts.headers).await),
                None,
            )
        } else {
            (None, Some(original))
        };
        if rejects_overflow
            && inspected.as_ref().is_some_and(|body| {
                body.bytes.is_none() || body.exceeds_limit(script_limit.unwrap_or(0))
            })
        {
            return RequestOrResponse::Abort;
        }
        let request_id = Uuid::new_v4();
        // The original body is moved exactly once, into inspection or into direct forwarding.
        let mut body = inspected.as_ref().and_then(|b| b.bytes.clone());
        let mut text = if rewrite_body {
            body.as_ref()
                .filter(|bytes| bytes.len() <= self.config.max_body_size)
                .and_then(|b| String::from_utf8(b.clone()).ok())
        } else {
            None
        };
        let action = match self.rewrite.apply_request_with_method(
            &mut method,
            &mut url,
            &mut headers,
            &mut text,
        ) {
            Ok(action) => action,
            Err(error) => {
                tracing::warn!(%error, "request regex rejected traffic");
                return RequestOrResponse::Abort;
            }
        };
        if rewrite_body && let Some(text) = text {
            body = Some(text.into_bytes());
        }
        let capture = Arc::new(Mutex::new(Default::default()));
        self.state = Some(RequestState {
            request: RequestContext {
                id: request_id,
                url: url.clone(),
                method: method.clone(),
                headers: headers.clone(),
                body: body.clone(),
            },
            capture: Arc::clone(&capture),
            start: Instant::now(),
        });
        if let Some(res) = self.synthetic(action) {
            return res.into();
        }
        if let Some(hooks) = &self.hooks {
            match hooks
                .run_request_hooks(request_id, &mut url, &method, &mut headers, &mut body)
                .await
            {
                HookOutcome::Mock(mock) => {
                    return self
                        .finish_response(mock_response(
                            mock.status,
                            mock.body.unwrap_or_default(),
                            mock.headers,
                        ))
                        .into();
                }
                HookOutcome::Abort => return RequestOrResponse::Abort,
                HookOutcome::Continue => {}
            }
        }
        if let Ok(uri) = url.parse() {
            parts.uri = uri;
        }
        if let Ok(parsed) = http::Method::from_bytes(method.as_bytes()) {
            parts.method = parsed;
        }
        parts.headers = vec_to_headers(&headers);
        if let Some(state) = &mut self.state {
            state.request = RequestContext {
                id: request_id,
                url,
                method,
                headers,
                body: body.clone(),
            };
        }
        let outgoing = finish_body(inspected, original, body, &mut parts.headers);
        let outgoing = if self.config.record_enabled {
            transport_body::observe(outgoing, capture, self.config.max_body_size, None)
        } else {
            outgoing
        };
        Request::from_parts(parts, outgoing).into()
    }
    async fn handle_response(&mut self, _ctx: &HttpContext, res: Response<Body>) -> Response<Body> {
        let (mut parts, original) = res.into_parts();
        let request = self
            .state
            .as_ref()
            .map(|s| s.request.clone())
            .unwrap_or_default();
        let plan = (|| {
            let rewrite_body = self.rewrite.needs_body(Phase::Response, &request.url)?;
            let script_limit = self.script_body_limit(&request.url, ScriptKind::HttpResponse)?;
            Ok::<_, PatternError>((rewrite_body, script_limit))
        })();
        let (rewrite_body, script_limit) = match plan {
            Ok(plan) => plan,
            Err(error) => return self.regex_response_abort(error),
        };
        let selected = rewrite_body || script_limit.is_some();
        let limit = script_limit
            .into_iter()
            .chain(rewrite_body.then_some(self.config.max_body_size))
            .max()
            .unwrap_or(0);
        let (inspected, original) = if selected {
            (
                Some(Inspected::read(original, limit, &parts.headers).await),
                None,
            )
        } else {
            (None, Some(original))
        };
        let mut body = inspected.as_ref().and_then(|b| b.bytes.clone());
        let mut text = if rewrite_body {
            body.as_ref()
                .filter(|bytes| bytes.len() <= self.config.max_body_size)
                .and_then(|b| String::from_utf8(b.clone()).ok())
        } else {
            None
        };
        let mut status = parts.status.as_u16();
        let mut headers = headers_to_vec(&parts.headers);
        let action =
            match self
                .rewrite
                .apply_response(&request.url, &mut status, &mut headers, &mut text)
            {
                Ok(action) => action,
                Err(error) => return self.regex_response_abort(error),
            };
        if rewrite_body && let Some(text) = text {
            body = Some(text.into_bytes());
        }
        if let Some(res) = self.synthetic(action) {
            return res;
        }
        if let Some(hooks) = &self.hooks {
            match hooks
                .run_response_hooks(&request, &mut status, &mut headers, &mut body)
                .await
            {
                HookOutcome::Mock(mock) => {
                    return self.finish_response(mock_response(
                        mock.status,
                        mock.body.unwrap_or_default(),
                        mock.headers,
                    ));
                }
                HookOutcome::Abort => {
                    self.aborted = true;
                    return Response::new(Body::empty());
                }
                HookOutcome::Continue => {}
            }
        }
        parts.status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        parts.headers = vec_to_headers(&headers);
        let outgoing = finish_body(inspected, original, body, &mut parts.headers);
        self.finish_response(Response::from_parts(parts, outgoing))
    }
    async fn should_intercept_connect(&mut self, _ctx: &HttpContext, req: &Request<Body>) -> bool {
        self.connect_authority = req
            .uri()
            .authority()
            .filter(|authority| should_intercept_host(&self.config, authority.as_str()))
            .cloned();
        self.connect_authority.is_some()
    }
    async fn should_intercept_tls(
        &mut self,
        _ctx: &HttpContext,
        hello: tokio_rustls::rustls::server::ClientHello<'_>,
    ) -> bool {
        let Some(connect) = &self.connect_authority else {
            return false;
        };
        match hello.server_name() {
            Some(host) => should_intercept_host(
                &self.config,
                &format!("{host}:{}", connect.port_u16().unwrap_or(443)),
            ),
            None => should_intercept_host(&self.config, connect.as_str()),
        }
    }
}
