//! Byte-preserving MITM pipeline with opt-in, bounded body inspection.
use crate::{
    ca::CaMaterial,
    config::MitmConfig,
    intercept::should_intercept_host,
    recorder::{TrafficRecord, TrafficRecorder},
    rewrite::{Phase, RewriteAction, RewriteEngine},
    script_hook::{HookOutcome, RequestContext, ScriptHookEngine},
    transport_body::{self, Inspected, SharedCapture, headers_to_vec, vec_to_headers},
    upstream::UpstreamConnector,
};
use chrono::Utc;
use http::{
    HeaderMap, Request, Response, StatusCode,
    header::{CONTENT_LENGTH, TRANSFER_ENCODING},
};
use hudsucker::{Body, HttpContext, HttpHandler, Proxy, RequestOrResponse};
use hudsucker::{
    certificate_authority::RcgenAuthority,
    rcgen::{Issuer, KeyPair},
};
use lanpath_common::error::{LanpathError, LanpathResult};
use lanpath_script::ScriptKind;
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::{net::TcpListener, sync::oneshot};
use uuid::Uuid;

pub struct MitmProxy {
    config: MitmConfig,
    rewrite: Arc<RewriteEngine>,
    hooks: Option<Arc<ScriptHookEngine>>,
    recorder: Arc<dyn TrafficRecorder>,
    ca: CaMaterial,
}
impl MitmProxy {
    pub fn new(
        config: MitmConfig,
        rewrite: RewriteEngine,
        hooks: Option<ScriptHookEngine>,
        recorder: Arc<dyn TrafficRecorder>,
        ca: CaMaterial,
    ) -> Self {
        Self {
            config,
            rewrite: Arc::new(rewrite),
            hooks: hooks.map(Arc::new),
            recorder,
            ca,
        }
    }
    pub async fn start(self) -> LanpathResult<RunningProxy> {
        let listener = TcpListener::bind(self.config.listen_addr)
            .await
            .map_err(|e| LanpathError::Mitm(format!("bind proxy listener: {e}")))?;
        let addr = listener
            .local_addr()
            .map_err(|e| LanpathError::Mitm(format!("proxy address: {e}")))?;
        let provider = hudsucker::rustls::crypto::aws_lc_rs::default_provider();
        let key = KeyPair::from_pem(&self.ca.key_pem)
            .map_err(|e| LanpathError::Mitm(format!("parse CA key: {e}")))?;
        let issuer = Issuer::from_ca_cert_pem(&self.ca.cert_pem, key)
            .map_err(|e| LanpathError::Mitm(format!("parse CA certificate: {e}")))?;
        let ca = RcgenAuthority::new(issuer, 1024, provider.clone());
        let connector = UpstreamConnector::new(self.config.upstream, provider)
            .map_err(|e| LanpathError::Mitm(format!("upstream connector: {e}")))?;
        let websocket_connector =
            hudsucker::tokio_tungstenite::Connector::Rustls(connector.websocket_tls_config());
        let (shutdown, stopped) = oneshot::channel();
        let handler = Handler {
            config: Arc::new(self.config),
            rewrite: self.rewrite,
            hooks: self.hooks,
            recorder: self.recorder,
            state: None,
            connect_authority: None,
            aborted: false,
        };
        let builder = Proxy::builder()
            .with_listener(listener)
            .with_ca(ca)
            .with_http_connector(connector)
            .with_http_handler(handler)
            .with_websocket_connector(websocket_connector)
            .with_graceful_shutdown(async move {
                let _ = stopped.await;
            });
        let proxy = builder
            .build()
            .map_err(|e| LanpathError::Mitm(format!("build proxy: {e}")))?;
        let task = tokio::spawn(async move {
            proxy
                .start()
                .await
                .map_err(|error| LanpathError::Mitm(format!("MITM server terminated: {error}")))
        });
        Ok(RunningProxy {
            addr,
            shutdown: Some(shutdown),
            task: Some(task),
            failure: None,
        })
    }
}
pub struct RunningProxy {
    pub addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<LanpathResult<()>>>,
    failure: Option<String>,
}
impl RunningProxy {
    fn request_shutdown(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }

    /// Returns only after the listener, HTTP connections and tracked upgrades stop.
    /// Cancellation retains the join handle; a failed join cannot become later success.
    pub async fn shutdown(&mut self) -> LanpathResult<()> {
        self.request_shutdown();
        if let Some(failure) = &self.failure {
            return Err(LanpathError::Mitm(failure.clone()));
        }
        let Some(task) = self.task.as_mut() else {
            return Ok(());
        };
        let result = match task.await {
            Ok(result) => result,
            Err(error) => Err(LanpathError::Mitm(format!(
                "joining MITM server failed: {error}"
            ))),
        };
        self.task = None;
        if let Err(error) = &result {
            self.failure = Some(error.to_string());
        }
        result
    }
}
impl Drop for RunningProxy {
    fn drop(&mut self) {
        self.request_shutdown();
    }
}

#[derive(Clone)]
struct RequestState {
    request: RequestContext,
    capture: SharedCapture,
    start: Instant,
}
#[derive(Clone)]
struct Handler {
    config: Arc<MitmConfig>,
    rewrite: Arc<RewriteEngine>,
    hooks: Option<Arc<ScriptHookEngine>>,
    recorder: Arc<dyn TrafficRecorder>,
    state: Option<RequestState>,
    connect_authority: Option<http::uri::Authority>,
    aborted: bool,
}
impl Handler {
    fn finish_response(&mut self, mut res: Response<Body>) -> Response<Body> {
        if !self.config.record_enabled {
            return res;
        }
        let Some(state) = self.state.take() else {
            return res;
        };
        let capture = Arc::new(Mutex::new(Default::default()));
        let recorder = Arc::clone(&self.recorder);
        let response_capture = Arc::clone(&capture);
        let response_headers = headers_to_vec(res.headers());
        let status = res.status().as_u16();
        let limit = self.config.max_body_size;
        let complete = Box::new(move || {
            let request_body = transport_body::recorded(
                &state.capture,
                &vec_to_headers(&state.request.headers),
                limit,
            );
            let response_body = transport_body::recorded(
                &response_capture,
                &vec_to_headers(&response_headers),
                limit,
            );
            recorder.record(TrafficRecord {
                id: state.request.id,
                method: state.request.method,
                url: state.request.url,
                request_headers: state.request.headers,
                request_body,
                response_status: status,
                response_headers,
                response_body,
                timestamp: Utc::now(),
                duration_ms: state.start.elapsed().as_millis().min(u64::MAX as u128) as u64,
            });
        });
        let body = std::mem::replace(res.body_mut(), Body::empty());
        *res.body_mut() =
            transport_body::observe(body, capture, self.config.max_body_size, Some(complete));
        res
    }
    fn synthetic(&mut self, action: RewriteAction) -> Option<Response<Body>> {
        match action {
            RewriteAction::Continue => None,
            RewriteAction::Mock {
                status,
                body,
                headers,
            } => Some(self.finish_response(mock_response(status, body, headers))),
            RewriteAction::Redirect { status, location } => Some(self.finish_response(
                mock_response(status, Vec::new(), vec![("Location".into(), location)]),
            )),
        }
    }
}
mod handler;
fn finish_body(
    inspected: Option<Inspected>,
    original: Option<Body>,
    bytes: Option<Vec<u8>>,
    headers: &mut HeaderMap,
) -> Body {
    if let Some(inspected) = inspected {
        return inspected.finish(bytes, headers);
    }
    if let Some(bytes) = bytes {
        headers.remove(http::header::CONTENT_ENCODING);
        headers.remove(CONTENT_LENGTH);
        headers.remove(TRANSFER_ENCODING);
        Body::from(bytes)
    } else {
        original.unwrap_or_else(Body::empty)
    }
}
fn mock_response(status: u16, body: Vec<u8>, headers: Vec<(String, String)>) -> Response<Body> {
    let mut res = Response::new(Body::from(body));
    *res.status_mut() = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    *res.headers_mut() = vec_to_headers(&headers);
    res.headers_mut().remove(CONTENT_LENGTH);
    res.headers_mut().remove(TRANSFER_ENCODING);
    res
}
