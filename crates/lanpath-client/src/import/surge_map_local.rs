//! Inline Surge Map Local responses; file-backed payloads remain unsupported.
//! Public contract: https://manual.nssurge.com/http/map-local.html.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use lanpath_mitm::{RewriteKind, RewriteRule};

use super::utils::*;

pub(super) fn parse_surge_map_local(cfg: &mut super::ImportedConfig, line: &str) {
    let tokens = split_tokens_keep_quoted(line);
    let Some(src) = tokens.first() else { return };
    let Some(pattern) = compile_pattern(src, cfg, "map local", line) else {
        return;
    };
    let (mut data, mut data_type, mut status) = (None, "file", 200);
    let mut headers = Vec::new();
    for token in &tokens[1..] {
        let Some((key, raw)) = token.split_once('=') else {
            cfg.warn("map local", line, "unsupported parameter; not activated");
            return;
        };
        let value = strip_quotes(raw);
        match key {
            "data" => data = Some(value),
            "data-type" => data_type = value,
            "status-code" => match value.parse::<u16>() {
                Ok(code @ 200..=999) => status = code,
                _ => {
                    cfg.warn("map local", line, "invalid response status; not activated");
                    return;
                }
            },
            "header" => {
                let decoded;
                let value = if value.contains(':') {
                    value
                } else {
                    let Ok(bytes) = STANDARD.decode(value) else {
                        cfg.warn("map local", line, "invalid encoded headers; not activated");
                        return;
                    };
                    let Ok(text) = String::from_utf8(bytes) else {
                        cfg.warn(
                            "map local",
                            line,
                            "non UTF-8 headers unsupported; not activated",
                        );
                        return;
                    };
                    decoded = text;
                    decoded.as_str()
                };
                for header in value.split(['|', '\n']) {
                    let Some((name, value)) = header.trim_end_matches('\r').split_once(':') else {
                        cfg.warn(
                            "map local",
                            line,
                            "invalid header declaration; not activated",
                        );
                        return;
                    };
                    if name.trim().is_empty()
                        || name.contains(['\r', '\n'])
                        || value.contains(['\r', '\n'])
                    {
                        cfg.warn(
                            "map local",
                            line,
                            "invalid header declaration; not activated",
                        );
                        return;
                    }
                    headers.push((name.trim().to_string(), value.trim().to_string()));
                }
            }
            _ => {
                cfg.warn("map local", line, "unsupported parameter; not activated");
                return;
            }
        }
    }
    let (body, content_type) = match data_type {
        "text" => match data {
            Some(data) => (data.as_bytes().to_vec(), "text/plain"),
            None => {
                cfg.warn("map local", line, "missing response data; not activated");
                return;
            }
        },
        "base64" => match data.and_then(|data| STANDARD.decode(data).ok()) {
            Some(data) => (data, "application/octet-stream"),
            None => {
                cfg.warn(
                    "map local",
                    line,
                    "missing or invalid base64 data; not activated",
                );
                return;
            }
        },
        "tiny-gif" => (
            vec![
                71, 73, 70, 56, 57, 97, 1, 0, 1, 0, 128, 0, 0, 0, 0, 0, 255, 255, 255, 33, 249, 4,
                1, 0, 0, 0, 0, 44, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 68, 1, 0, 59,
            ],
            "image/gif",
        ),
        _ => {
            cfg.warn(
                "map local",
                line,
                "file-backed or unknown data type unsupported; not activated",
            );
            return;
        }
    };
    if !headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("content-type"))
    {
        headers.push(("Content-Type".into(), content_type.into()));
    }
    cfg.rewrites.push(RewriteRule {
        pattern,
        kind: RewriteKind::Mock {
            status,
            body,
            headers,
        },
    });
}
