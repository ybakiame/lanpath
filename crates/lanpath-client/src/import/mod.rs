//! Third-party config snippet import (QX / Surge / Loon → lanpath-mitm / lanpath-script rules).
//!
//! This is the common foundation for "remote subscription (2.3b)" and "config import (2.4)":
//! parses Quantumult X / Surge / Loon rewrite / script / task / mitm config snippets into
//! lanpath-mitm [`RewriteRule`] / script hook rules and lanpath-script [`TaskScript`].
//!
//! Design trade-offs:
//! - Only covers commonly used subsets; unknown lines are skipped and recorded in
//!   [`ImportedConfig::warnings`] (comments / blank lines are silently skipped);
//! - Remote scripts are fetched separately; unsupported local assets are never activated.
//! - Unsupported actions produce diagnostics, never approximate network effects.

#[cfg(feature = "mitm")]
use lanpath_common::LanpathResult;
#[cfg(feature = "mitm")]
use lanpath_mitm::{RewriteRule, ScriptRule as HookScriptRule};
#[cfg(feature = "mitm")]
use lanpath_script::ScriptDialect;
#[cfg(feature = "mitm")]
use lanpath_script::TaskScript;

#[cfg(all(test, feature = "mitm"))]
use lanpath_mitm::Phase;
#[cfg(all(test, feature = "mitm"))]
use lanpath_script::ScriptKind;

#[cfg(feature = "mitm")]
mod loon;
#[cfg(feature = "mitm")]
mod loon_current;
mod meta;
#[cfg(feature = "mitm")]
mod mitm_templates;
#[cfg(feature = "mitm")]
mod qx;
#[cfg(feature = "mitm")]
mod stash;
#[cfg(feature = "mitm")]
mod surge;
#[cfg(feature = "mitm")]
mod surge_map_local;
mod utils;

#[cfg(feature = "mitm")]
use loon::*;
pub use meta::*;
#[cfg(feature = "mitm")]
pub(crate) use mitm_templates::resolve_mitm_templates;
#[cfg(feature = "mitm")]
use qx::*;
#[cfg(feature = "mitm")]
use surge::*;
#[cfg(feature = "mitm")]
use utils::*;

/// Result of a single import parse.
#[cfg(feature = "mitm")]
#[derive(Default)]
pub struct ImportedConfig {
    /// Only the explicit local-bundle parser admits local JS identities. It never reads paths.
    #[doc(hidden)]
    pub allow_local_sources: bool,
    /// URL / Header / Body rewrite and Reject / Mock rules.
    pub rewrites: Vec<RewriteRule>,
    /// Script hook rules; `source` is empty, corresponding script URLs are in
    /// [`ImportedConfig::script_urls`].
    pub scripts: Vec<HookScriptRule>,
    /// Script hook remote addresses `(script_name, URL)`, corresponding to `scripts` in order.
    pub script_urls: Vec<(String, String)>,
    /// Scheduled tasks (`TaskScript.source` is empty) and their script URLs.
    pub task_scripts: Vec<(TaskScript, String)>,
    /// Static MITM hostname whitelist; parameterized declarations stay separate.
    pub hostnames: Vec<String>,
    /// Original parameterized MITM hostname declarations, resolved at activation.
    pub mitm_templates: Vec<String>,
    /// Unrecognized lines / mapping deviations that cannot be expressed.
    pub warnings: Vec<String>,
    /// Supported declarations with an explicit runtime limit. Also retained in
    /// `warnings` for existing consumers, but do not indicate a rejected directive.
    pub advisory_warnings: Vec<String>,
    /// File header `#!key=value` metadata (see [`ConfigMeta`]).
    pub meta: ConfigMeta,
}

#[cfg(feature = "mitm")]
impl ImportedConfig {
    pub fn blocking_warnings(&self) -> impl Iterator<Item = &String> {
        self.warnings
            .iter()
            .filter(|warning| !self.advisory_warnings.contains(warning))
    }

    /// Only static diagnostics may bypass dynamic-detail redaction. Keep the
    /// warning visible while distinguishing accepted declarations from rejection.
    fn advisory(&mut self, section: &'static str, message: &'static str) {
        let text = format!("[{section}] {message}");
        tracing::warn!(section, "{text}");
        self.warnings.push(text.clone());
        self.advisory_warnings.push(text);
    }

    /// Record a warning: `tracing::warn` while also writing to the `warnings` list.
    fn warn(&mut self, section: &str, _line: &str, msg: &str) {
        // A diagnostic must not echo configuration text, URLs, arguments or regexes.
        // Dynamic parser details are quoted; redact them before crossing the UI/log boundary.
        let mut detail = String::new();
        let mut quoted = false;
        for ch in msg.chars() {
            if ch == '\'' {
                quoted = !quoted;
                if quoted {
                    detail.push_str("[detail]");
                }
            } else if !quoted {
                detail.push(ch);
            }
        }
        let text = format!("[{section}] {detail}");
        tracing::warn!(section, "{text}");
        self.warnings.push(text);
    }
}

/// Parse QX / Surge / Loon config snippets.
///
/// `dialect` is specified by the caller to indicate the source software, which determines
/// [`TaskScript::dialect`] and other dialect markers; syntactically isomorphic grammars
/// (e.g., Surge / Loon `[Script]`) share the same parsing path.
#[cfg(feature = "mitm")]
pub fn parse_import(content: &str, dialect: ScriptDialect) -> LanpathResult<ImportedConfig> {
    parse(content, dialect, false)
}

/// Parse a GPUI local bundle. Script filenames are identities joined to supplied JS,
/// never filesystem paths or permission to download/read a source.
#[cfg(feature = "mitm")]
pub fn parse_local_resource(
    content: &str,
    dialect: ScriptDialect,
) -> LanpathResult<ImportedConfig> {
    parse(content, dialect, true)
}

#[cfg(feature = "mitm")]
fn parse(
    content: &str,
    dialect: ScriptDialect,
    allow_local_sources: bool,
) -> LanpathResult<ImportedConfig> {
    if dialect == ScriptDialect::Stash {
        return Ok(stash::parse_stash(content));
    }
    let mut cfg = ImportedConfig {
        meta: parse_config_meta(content),
        allow_local_sources,
        ..ImportedConfig::default()
    };
    let mut section = String::new();
    let mut hook_index = 0usize;

    for raw in content.lines() {
        let line = raw.trim();
        if is_comment_or_blank(line) {
            continue;
        }
        if let Some(name) = section_name(line) {
            section = name;
            continue;
        }
        // 任意位置的裸 `hostname = ...` 行（QX snippet 常写在顶层、不在 [mitm] 内）
        // 都按 MITM 白名单解析；各 section 内的同名键也由 parse_mitm_hostnames 键名门槛拦截。
        if line
            .find('=')
            .is_some_and(|eq| line[..eq].trim().eq_ignore_ascii_case("hostname"))
        {
            parse_mitm_hostnames(&mut cfg, line);
            continue;
        }
        match section.as_str() {
            "rewrite_local" | "rewrite_remote" if dialect == ScriptDialect::QuantumultX => {
                parse_qx_rewrite(&mut cfg, &mut hook_index, dialect, line);
            }
            "task_local" if dialect == ScriptDialect::QuantumultX => {
                parse_qx_task(&mut cfg, dialect, line)
            }
            "mitm" => parse_mitm_hostnames(&mut cfg, line),
            "script" => match dialect {
                ScriptDialect::Loon => parse_loon_script(&mut cfg, line),
                ScriptDialect::Surge => parse_surge_script(&mut cfg, dialect, line),
                _ => cfg.warn(
                    "script",
                    line,
                    "unsupported script section for selected dialect",
                ),
            },
            "argument" if dialect == ScriptDialect::Loon => parse_loon_argument(&mut cfg, line),
            "url rewrite" if dialect == ScriptDialect::Surge => {
                parse_surge_url_rewrite(&mut cfg, line)
            }
            "header rewrite" if dialect == ScriptDialect::Surge => {
                parse_surge_header_rewrite(&mut cfg, line)
            }
            "body rewrite" if dialect == ScriptDialect::Surge => {
                parse_surge_body_rewrite(&mut cfg, line)
            }
            "map local" if dialect == ScriptDialect::Surge => {
                surge_map_local::parse_surge_map_local(&mut cfg, line)
            }
            "rewrite" if dialect == ScriptDialect::Loon => parse_loon_rewrite(&mut cfg, line),
            "" if dialect == ScriptDialect::QuantumultX => {
                parse_qx_rewrite(&mut cfg, &mut hook_index, dialect, line);
            }
            _ => cfg.warn(
                "import",
                line,
                "unsupported directive or section; not activated",
            ),
        }
    }
    let defaults = cfg
        .meta
        .arguments
        .iter()
        .map(|arg| (arg.key.clone(), arg.default_value.clone()))
        .collect();
    let (_, template_warnings) =
        resolve_mitm_templates(&cfg.mitm_templates, &Default::default(), &defaults);
    cfg.warnings.extend(template_warnings);
    if cfg.rewrites.is_empty()
        && cfg.scripts.is_empty()
        && cfg.task_scripts.is_empty()
        && cfg.hostnames.is_empty()
        && cfg.mitm_templates.is_empty()
        && cfg.warnings.is_empty()
    {
        cfg.warn(
            "import",
            "",
            "no supported executable rules or hostnames found",
        );
    }

    Ok(cfg)
}

/// `[mitm]` / `[MITM]` hostname line parsing: `hostname = a, b, -exclude`.
///
/// `-` / `!` prefixes are exclusions (normalized to `-`), kept in `cfg.hostnames`
/// alongside whitelist entries; downstream (`build_mitm_proxy` / core routing rules)
/// filters by prefix. Surge's `%APPEND%` prefix is stripped. 键名非 `hostname` 的
/// `[mitm]` 行（如 `skip_validating_cert` / `p12` / `passphrase`）直接忽略。
#[cfg(feature = "mitm")]
fn parse_mitm_hostnames(cfg: &mut ImportedConfig, line: &str) {
    if line.contains('{') || line.contains('}') {
        cfg.mitm_templates.push(line.to_string());
        return;
    }
    let Some(eq) = line.find('=') else {
        cfg.warn(
            "mitm",
            line,
            "unrecognized line (expected 'hostname = ...')",
        );
        return;
    };
    if !line[..eq].trim().eq_ignore_ascii_case("hostname") {
        cfg.warn("mitm", line, "unsupported MITM option; not activated");
        return;
    }
    for entry in line[eq + 1..].split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let entry = entry
            .strip_prefix("%APPEND%")
            .map(str::trim)
            .unwrap_or(entry);
        if let Some(rest) = entry.strip_prefix('!') {
            cfg.hostnames.push(format!("-{rest}"));
        } else {
            cfg.hostnames.push(entry.to_string());
        }
    }
}
#[cfg(all(test, feature = "mitm"))]
mod tests;
