//! Relocate declared filesystem references, never credentials, scripts, or network paths.
use serde_json::Value;

pub fn document_has_paths(name: &str) -> bool {
    matches!(
        name,
        "client.json" | "local_override.json" | "config_slices.json"
    ) || name
        .strip_prefix("subscription_cache/")
        .or_else(|| name.strip_prefix("cache/subscriptions/"))
        .is_some_and(|leaf| {
            !leaf.contains('/') && leaf.ends_with(".json") && !leaf.ends_with(".source.json")
        })
}

/// Returns whether a known path changed. All other JSON values remain opaque.
pub fn relocate_document_paths(name: &str, value: &mut Value, from: &str, to: &str) -> bool {
    if from.is_empty() || from == to {
        return false;
    }
    let mut changed = false;
    match name {
        "client.json" => {
            for pointer in ["/data_dir", "/core_binary", "/mitm/ca_dir"] {
                changed |= relocate_at(value, pointer, from, to);
            }
        }
        "local_override.json" => {
            if let Some(rules) = value
                .pointer_mut("/singbox/rule_sets")
                .and_then(Value::as_array_mut)
            {
                for rule in rules {
                    changed |= relocate_at(rule, "/source/local/path", from, to);
                    changed |= relocate_at(rule, "/source/bundled/name", from, to);
                }
            }
        }
        "config_slices.json" => {
            changed |= relocate_at(value, "/experimental/cache_file/path", from, to);
        }
        _ if document_has_paths(name) => {
            if let Some(nodes) = value.get_mut("singbox_nodes").and_then(Value::as_array_mut) {
                for node in nodes {
                    if node.get("type").and_then(Value::as_str) == Some("ssh") {
                        changed |= relocate_at(node, "/private_key_path", from, to);
                    }
                    for pointer in [
                        "/tls/certificate_path",
                        "/tls/client_certificate_path",
                        "/tls/client_key_path",
                        "/tls/ech/config_path",
                    ] {
                        changed |= relocate_at(node, pointer, from, to);
                    }
                }
            }
        }
        _ => {}
    }
    changed
}

fn relocate_at(value: &mut Value, pointer: &str, from: &str, to: &str) -> bool {
    value
        .pointer_mut(pointer)
        .is_some_and(|value| relocate_path(value, from, to))
}

fn relocate_path(value: &mut Value, from: &str, to: &str) -> bool {
    match value {
        Value::Array(items) => {
            let mut changed = false;
            for item in items {
                changed |= relocate_path(item, from, to);
            }
            changed
        }
        Value::String(path) => {
            let Some(suffix) = strip_root(path, from) else {
                return false;
            };
            if !suffix.is_empty() && !suffix.starts_with(['/', '\\']) {
                return false;
            }
            // Archive placeholders are platform-neutral; real destinations use native
            // separators, including verbatim Windows paths where '/' is not a separator.
            let separator = if to == "$LANPATH_DATA" {
                '/'
            } else {
                std::path::MAIN_SEPARATOR
            };
            let mut relocated = String::with_capacity(to.len() + suffix.len());
            relocated.push_str(to);
            relocated.extend(suffix.chars().map(|c| {
                if matches!(c, '/' | '\\') {
                    separator
                } else {
                    c
                }
            }));
            *path = relocated;
            true
        }
        _ => false,
    }
}

fn windows_path(path: &str) -> (&str, bool) {
    if let Some(path) = path.strip_prefix("\\\\?\\UNC\\") {
        (path, true)
    } else if let Some(path) = path.strip_prefix("\\\\?\\") {
        (path, false)
    } else if let Some(path) = path.strip_prefix("\\\\") {
        (path, true)
    } else {
        (path, false)
    }
}

fn strip_root<'a>(path: &'a str, root: &str) -> Option<&'a str> {
    let windows = root.starts_with("\\\\")
        || (root.as_bytes().get(1) == Some(&b':') && root.as_bytes()[0].is_ascii_alphabetic());
    if !windows {
        return path.strip_prefix(root);
    }
    let (path, path_unc) = windows_path(path);
    let (root, root_unc) = windows_path(root);
    if path_unc != root_unc {
        return None;
    }
    let prefix = path.get(..root.len())?;
    let matches = prefix
        .chars()
        .zip(root.chars())
        .enumerate()
        .all(|(index, (a, b))| {
            a == b || (matches!(a, '/' | '\\') && matches!(b, '/' | '\\'))
            // Drive letters identify the same volume irrespective of case. Do not
            // case-fold directory names: NTFS can enable case-sensitive directories.
            || (!root_unc && index == 0 && a.eq_ignore_ascii_case(&b))
        });
    matches.then(|| &path[root.len()..])
}
