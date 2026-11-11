use std::path::{Path, PathBuf};

use lanpath_common::LanpathResult;
use serde_json::Value;

use super::{config::ConfigContext, protocol::failure};

/// Inventory the same schema-qualified inputs used by protected resource snapshots.
pub(crate) fn input_paths(root: &Path, config: &Value) -> LanpathResult<Vec<PathBuf>> {
    fn walk(
        root: &Path,
        value: &Value,
        context: ConfigContext,
        paths: &mut Vec<PathBuf>,
        depth: usize,
    ) -> LanpathResult<()> {
        if depth > 48 {
            return Err(failure("configuration nesting limit exceeded"));
        }
        match value {
            Value::Object(map) => {
                let context = context.object(map);
                for (key, child) in map {
                    if context.input_path(key) {
                        let values = child
                            .as_array()
                            .map(Vec::as_slice)
                            .unwrap_or_else(|| std::slice::from_ref(child));
                        for value in values {
                            let text = value
                                .as_str()
                                .ok_or_else(|| failure("resource reference must be a string"))?;
                            paths.push(crate::configuration::resources::confined(root, text, key)?);
                        }
                    } else {
                        walk(root, child, context.child(key), paths, depth + 1)?;
                    }
                }
            }
            Value::Array(items) => {
                for child in items {
                    walk(root, child, context, paths, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut paths = Vec::new();
    walk(root, config, ConfigContext::Root, &mut paths, 0)?;
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn inventory_retains_typed_inputs_without_treating_header_values_as_paths() {
        let root = tempfile::tempdir().unwrap();
        for name in ["hosts", "rules", "ca", "ech"] {
            std::fs::write(root.path().join(name), name).unwrap();
        }
        let mut config = json!({
            "dns": {"servers": [{"type": "hosts", "path": ["hosts"]}]},
            "route": {"rule_set": [{"type": "local", "path": "rules"}]},
            "outbounds": [{"type": "http", "headers": {"certificate_path": "../not-a-file"},
                "tls": {"certificate_path": ["ca"], "ech": {"config_path": "ech"}}}]
        });
        let mut paths = input_paths(root.path(), &config).unwrap();
        paths.sort();
        let mut expected: Vec<_> = ["hosts", "rules", "ca", "ech"]
            .into_iter()
            .map(|name| root.path().canonicalize().unwrap().join(name))
            .collect();
        expected.sort();
        assert_eq!(paths, expected);
        config["dns"]["servers"][0]["path"] = json!(["../escaped"]);
        assert!(input_paths(root.path(), &config).is_err());
    }
}
