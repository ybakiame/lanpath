use super::*;

impl RewriteEngine {
    /// Whether this URL selects a text body operation at this phase.
    pub fn needs_body(&self, phase: Phase, url: &str) -> Result<bool, PatternError> {
        for rule in &self.rules {
            if matches!(&rule.kind,
                RewriteKind::BodyRewrite { phase: p, .. } |
                RewriteKind::BodyRewritePairs { phase: p, .. } if *p == phase)
                && rule.pattern.is_match(url)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Dry-run ordered request rewrites with the same fallible evaluator as execution.
    pub fn request_body_plan(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
    ) -> Result<(bool, String), PatternError> {
        let mut method = method.to_owned();
        let mut url = url.to_owned();
        let mut headers = headers.to_vec();
        let mut selected = false;
        for rule in &self.rules {
            if matches!(
                &rule.kind,
                RewriteKind::BodyRewrite {
                    phase: Phase::Request,
                    ..
                } | RewriteKind::BodyRewritePairs {
                    phase: Phase::Request,
                    ..
                }
            ) {
                selected |= rule.pattern.is_match(&url)?;
            }
            if self
                .apply_request_rule(rule, &mut method, &mut url, &mut headers, &mut None)?
                .is_some()
            {
                break;
            }
        }
        Ok((selected, url))
    }

    pub fn apply_request(
        &self,
        url: &mut String,
        headers: &mut Vec<(String, String)>,
        body: &mut Option<String>,
    ) -> Result<RewriteAction, PatternError> {
        self.apply_request_with_method(&mut "GET".to_owned(), url, headers, body)
    }

    /// On error the caller must discard this exchange, never forward partial changes.
    pub fn apply_request_with_method(
        &self,
        method: &mut String,
        url: &mut String,
        headers: &mut Vec<(String, String)>,
        body: &mut Option<String>,
    ) -> Result<RewriteAction, PatternError> {
        for rule in &self.rules {
            if let Some(action) = self.apply_request_rule(rule, method, url, headers, body)? {
                return Ok(action);
            }
        }
        Ok(RewriteAction::Continue)
    }

    fn apply_request_rule(
        &self,
        rule: &RewriteRule,
        method: &mut String,
        url: &mut String,
        headers: &mut Vec<(String, String)>,
        body: &mut Option<String>,
    ) -> Result<Option<RewriteAction>, PatternError> {
        if let RewriteKind::HeaderBlockRewrite {
            phase: Phase::Request,
            block_pattern,
            replacement,
        } = &rule.kind
        {
            if rule.pattern.is_match(url)?
                && let Ok(uri) = url.parse::<http::Uri>()
            {
                let path = uri.path_and_query().map(|p| p.as_str()).unwrap_or("/");
                let block = format!("{method} {path} HTTP/1.1\r\n{}", headers_to_block(headers));
                let changed = block_pattern.replace_all(&block, replacement)?;
                if let Some((line, parsed)) = parse_header_block(&changed) {
                    let pieces: Vec<_> = line.split_whitespace().collect();
                    if pieces.len() == 3 && http::Method::from_bytes(pieces[0].as_bytes()).is_ok() {
                        let mut parts = uri.into_parts();
                        if let Ok(path) = pieces[1].parse::<http::uri::PathAndQuery>() {
                            parts.path_and_query = Some(path);
                            if let Ok(uri) = http::Uri::from_parts(parts) {
                                *method = pieces[0].to_owned();
                                *url = uri.to_string();
                                *headers = parsed;
                            }
                        }
                    }
                }
            }
            return Ok(None);
        }
        self.apply_one(rule, Phase::Request, url, headers, body)
    }

    /// On error the caller must discard the response, never forward partial changes.
    pub fn apply_response(
        &self,
        url: &str,
        status: &mut u16,
        headers: &mut Vec<(String, String)>,
        body: &mut Option<String>,
    ) -> Result<RewriteAction, PatternError> {
        if self.rules.is_empty() {
            return Ok(RewriteAction::Continue);
        }
        let mut url = url.to_owned();
        for rule in &self.rules {
            if let RewriteKind::HeaderBlockRewrite {
                phase: Phase::Response,
                block_pattern,
                replacement,
            } = &rule.kind
            {
                if rule.pattern.is_match(&url)? {
                    let reason = reason(*status);
                    let block = format!(
                        "HTTP/1.1 {status} {reason}\r\n{}",
                        headers_to_block(headers)
                    );
                    let changed = block_pattern.replace_all(&block, replacement)?;
                    if let Some((line, parsed)) = parse_header_block(&changed)
                        && let Some(code) = status_code(line)
                    {
                        *status = code;
                        *headers = parsed;
                    }
                }
                continue;
            }
            if let RewriteKind::StatusLineRewrite {
                pattern,
                replacement,
            } = &rule.kind
            {
                if rule.pattern.is_match(&url)? {
                    let line = format!("HTTP/1.1 {status} {}", reason(*status));
                    let changed = pattern.replace(&line, replacement)?;
                    if let Some(code) = status_code(&changed) {
                        *status = code;
                    }
                }
                continue;
            }
            if let Some(action) = self.apply_one(rule, Phase::Response, &mut url, headers, body)? {
                return Ok(action);
            }
        }
        Ok(RewriteAction::Continue)
    }

    fn apply_one(
        &self,
        rule: &RewriteRule,
        phase: Phase,
        url: &mut String,
        headers: &mut Vec<(String, String)>,
        body: &mut Option<String>,
    ) -> Result<Option<RewriteAction>, PatternError> {
        // A different phase must not execute (or fail) this rule's regex.
        match &rule.kind {
            RewriteKind::UrlRewrite { .. } | RewriteKind::Redirect { .. }
                if phase != Phase::Request =>
            {
                return Ok(None);
            }
            RewriteKind::HeaderAppend { phase: p, .. }
            | RewriteKind::HeaderReplace { phase: p, .. }
            | RewriteKind::HeaderRemove { phase: p, .. }
            | RewriteKind::BodyRewritePairs { phase: p, .. }
            | RewriteKind::HeaderRewrite { phase: p, .. }
            | RewriteKind::HeaderValueRewrite { phase: p, .. }
            | RewriteKind::BodyRewrite { phase: p, .. }
                if *p != phase =>
            {
                return Ok(None);
            }
            RewriteKind::HeaderBlockRewrite { .. } | RewriteKind::StatusLineRewrite { .. } => {
                return Ok(None);
            }
            _ => {}
        }
        // URL replacement performs its own search; avoid searching twice.
        if let RewriteKind::UrlRewrite { target } = &rule.kind {
            let changed = rule.pattern.replace(url, target)?;
            if let std::borrow::Cow::Owned(changed) = changed {
                if let Ok(uri) = changed.parse::<http::Uri>()
                    && let Some(authority) = uri.authority()
                {
                    apply_header(headers, "Host", &Some(authority.to_string()));
                }
                *url = changed;
            }
            return Ok(None);
        }
        if !rule.pattern.is_match(url)? {
            return Ok(None);
        }
        match &rule.kind {
            RewriteKind::HeaderAppend { name, value, .. } => {
                headers.push((name.clone(), value.clone()))
            }
            RewriteKind::HeaderReplace { name, value, .. } => {
                for (field, existing) in headers.iter_mut() {
                    if field.eq_ignore_ascii_case(name) {
                        *existing = value.clone();
                    }
                }
            }
            RewriteKind::HeaderRemove { name, .. } => apply_header(headers, name, &None),
            RewriteKind::BodyRewritePairs { pairs, .. } => {
                if let Some(body) = body {
                    for (pattern, replacement) in pairs {
                        if let std::borrow::Cow::Owned(changed) =
                            pattern.replace_all(body, replacement)?
                        {
                            *body = changed;
                        }
                    }
                }
            }
            RewriteKind::Redirect { status, target } => {
                return Ok(Some(RewriteAction::Redirect {
                    status: *status,
                    location: rule.pattern.replace(url, target)?.into_owned(),
                }));
            }
            RewriteKind::HeaderRewrite { name, value, .. } => apply_header(headers, name, value),
            RewriteKind::HeaderValueRewrite {
                name,
                value_pattern,
                replacement,
                ..
            } => {
                for (field, value) in headers.iter_mut() {
                    if field.eq_ignore_ascii_case(name)
                        && let std::borrow::Cow::Owned(changed) =
                            value_pattern.replace(value, replacement)?
                    {
                        *value = changed;
                    }
                }
            }
            RewriteKind::BodyRewrite {
                body_pattern,
                replacement,
                ..
            } => {
                if let Some(body) = body
                    && !body.is_empty()
                {
                    let pattern = body_pattern.as_ref().unwrap_or(&rule.pattern);
                    if let std::borrow::Cow::Owned(changed) =
                        pattern.replace_all(body, replacement)?
                    {
                        *body = changed;
                    }
                }
            }
            RewriteKind::Reject {
                status,
                body,
                content_type,
            } => {
                return Ok(Some(RewriteAction::Mock {
                    status: *status,
                    body: body.clone(),
                    headers: content_type
                        .as_ref()
                        .map(|ct| vec![("Content-Type".to_owned(), ct.clone())])
                        .unwrap_or_default(),
                }));
            }
            RewriteKind::Mock {
                status,
                body,
                headers,
            } => {
                return Ok(Some(RewriteAction::Mock {
                    status: *status,
                    body: body.clone(),
                    headers: headers.clone(),
                }));
            }
            _ => {}
        }
        Ok(None)
    }
}

fn reason(status: u16) -> &'static str {
    http::StatusCode::from_u16(status)
        .ok()
        .and_then(|s| s.canonical_reason())
        .unwrap_or("")
}

fn status_code(line: &str) -> Option<u16> {
    line.split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .filter(|s| http::StatusCode::from_u16(*s).is_ok())
}
