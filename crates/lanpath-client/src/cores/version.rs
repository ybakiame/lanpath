//! Version probing and release naming for supported runtime cores.

use std::path::Path;
use std::process::Command;

use lanpath_common::{CoreType, LanpathError, LanpathResult};

/// Platform binary filename without tying extraction to a particular core type.
pub(super) fn binary_file_name(base: &str) -> String {
    if std::env::consts::OS == "windows" {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

/// On-disk sing-box binary file name (kept for the sing-box bundled seed).
pub(crate) fn binary_name_on_disk() -> String {
    binary_name_on_disk_for(CoreType::SingBox)
}

/// On-disk binary file name for a selected core type.
pub(crate) fn binary_name_on_disk_for(core_type: CoreType) -> String {
    binary_file_name(core_type.binary_name())
}

/// Infer a core type from its executable filename.
pub fn infer_core_type(path: &Path) -> Option<CoreType> {
    let name = path.file_name()?.to_string_lossy().to_lowercase();
    if name.contains("sing-box") || name.contains("singbox") {
        Some(CoreType::SingBox)
    } else if name.contains("mihomo") || name.contains("clash") {
        Some(CoreType::Mihomo)
    } else {
        None
    }
}

pub(crate) fn target_spec_for(core_type: CoreType) -> LanpathResult<(&'static str, bool)> {
    match (core_type, std::env::consts::OS, std::env::consts::ARCH) {
        (_, "linux", "x86_64") => Ok(("linux-amd64", false)),
        (_, "linux", "aarch64") => Ok(("linux-arm64", false)),
        (_, "macos", "x86_64") => Ok(("darwin-amd64", false)),
        (_, "macos", "aarch64") => Ok(("darwin-arm64", false)),
        (_, "windows", "x86_64") => Ok(("windows-amd64", true)),
        (_, "windows", "aarch64") => Ok(("windows-arm64", true)),
        (_, os, arch) => Err(LanpathError::Core(format!(
            "Unsupported core download platform: {os}-{arch}"
        ))),
    }
}

#[cfg(all(test, unix))]
pub(crate) fn target_spec() -> LanpathResult<(&'static str, bool)> {
    target_spec_for(CoreType::SingBox)
}

fn mihomo_asset_base(arch_hint: &str, version: &str) -> String {
    let alpha = version.starts_with("alpha-");
    match (arch_hint, alpha) {
        ("windows-amd64", _) => "mihomo-windows-amd64-v2".into(),
        ("windows-arm64", _) => "mihomo-windows-arm64".into(),
        ("linux-amd64", _) => "mihomo-linux-amd64-v2".into(),
        ("linux-arm64", _) => "mihomo-linux-arm64".into(),
        ("darwin-amd64", true) => "mihomo-darwin-amd64-v1-go122".into(),
        ("darwin-amd64", false) => "mihomo-darwin-amd64-v2-go122".into(),
        ("darwin-arm64", _) => "mihomo-darwin-arm64-go122".into(),
        _ => "mihomo-unknown".into(),
    }
}

/// Both upstream projects use `v`-prefixed release tags.
pub(super) fn github_tag(version: &str) -> String {
    if version.starts_with('v') {
        version.to_string()
    } else {
        format!("v{version}")
    }
}

pub(super) fn github_tag_for(core_type: CoreType, version: &str) -> String {
    match core_type {
        CoreType::SingBox => github_tag(version),
        CoreType::Mihomo if version.starts_with("alpha-") => "Prerelease-Alpha".into(),
        CoreType::Mihomo => github_tag(version),
    }
}

/// Exact upstream release asset name.
///
/// sing-box: `sing-box-<version>-<os>-<arch>.tar.gz` (`.zip` on Windows).
/// Mihomo stable: `mihomo-<variant>-v<version>.gz` (`.zip` on Windows).
/// Mihomo Alpha: `mihomo-<variant>-alpha-<hash>.gz` (`.zip` on Windows).
pub(super) fn release_asset_name(
    core_type: CoreType,
    version: &str,
    arch_hint: &str,
    is_windows: bool,
) -> String {
    let version = version.strip_prefix('v').unwrap_or(version);
    match core_type {
        CoreType::SingBox => format!(
            "sing-box-{version}-{arch_hint}.{}",
            if is_windows { "zip" } else { "tar.gz" }
        ),
        CoreType::Mihomo => {
            let base = mihomo_asset_base(arch_hint, version);
            let suffix = if is_windows { "zip" } else { "gz" };
            if version.starts_with("alpha-") {
                format!("{base}-{version}.{suffix}")
            } else {
                format!("{base}-v{version}.{suffix}")
            }
        }
    }
}

pub(super) fn asset_matches(
    core_type: CoreType,
    version: &str,
    arch_hint: &str,
    is_windows: bool,
    name: &str,
) -> bool {
    name == release_asset_name(core_type, version, arch_hint, is_windows)
}

/// Try `version` subcommand / `--version` / `-v` in sequence, taking the first
/// successful output (stdout and stderr concatenated).
#[cfg(test)]
pub(crate) fn binary_output(binary: &Path) -> String {
    binary_output_for(CoreType::SingBox, binary)
}

pub(crate) fn binary_output_for(core_type: CoreType, binary: &Path) -> String {
    let args: &[&str] = match core_type {
        CoreType::SingBox => &["version", "--version", "-v"],
        CoreType::Mihomo => &["-v"],
    };
    for arg in args {
        if let Ok(output) = lanpath_common::no_window(Command::new(binary).arg(arg)).output() {
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            if output.status.success() {
                return text;
            }
        }
    }
    String::new()
}

/// Details displayed by core-management surfaces, derived from a real version probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CoreVersionDetails {
    pub version: String,
    pub environment: Option<String>,
    pub tags: Vec<String>,
}

/// Parse a core's exact reported version.
pub(crate) fn parse_version_for(core_type: CoreType, output: &str) -> Option<String> {
    let pattern = match core_type {
        CoreType::SingBox => r"(?i)sing-box\s+version\s+v?([0-9][0-9A-Za-z.\-]*)",
        CoreType::Mihomo => r"(?i)\bmihomo(?:\s+meta)?\s+v?(alpha-[0-9a-z.\-]+|[0-9][0-9a-z.\-]*)",
    };
    let re = regex::Regex::new(pattern).ok()?;
    if core_type == CoreType::SingBox
        && let Some(line) = output.lines().find(|line| line.contains("version"))
        && let Some(version) = capture_version(&re, line)
    {
        return Some(version);
    }
    capture_version(&re, output)
}

fn capture_version(re: &regex::Regex, output: &str) -> Option<String> {
    re.captures(output)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().to_string())
}

/// Sing-box compatibility wrapper used by the bundled seed and existing callers.
#[cfg(test)]
pub(crate) fn parse_version_from_output(output: &str) -> Option<String> {
    parse_version_for(CoreType::SingBox, output)
}

/// Parse version, Environment and Tags without requiring optional detail lines.
pub(crate) fn parse_details_from_output(
    core_type: CoreType,
    output: &str,
) -> Option<CoreVersionDetails> {
    Some(CoreVersionDetails {
        version: parse_version_for(core_type, output)?,
        environment: parse_environment(output),
        tags: parse_tags(output),
    })
}

pub(crate) fn probe_details(binary: &Path, core_type: CoreType) -> Option<CoreVersionDetails> {
    parse_details_from_output(core_type, &binary_output_for(core_type, binary))
}

fn parse_environment(output: &str) -> Option<String> {
    let mut lines = output.lines();
    while let Some(line) = lines.next() {
        let Some(value) = line.trim().strip_prefix("Environment:") else {
            continue;
        };
        let value = value.trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
        return lines
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(str::to_string);
    }
    None
}

fn parse_tags(output: &str) -> Vec<String> {
    output
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("Tags:")
                .or_else(|| line.trim().strip_prefix("Use tags:"))
        })
        .map(|tags| {
            tags.split(',')
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Sing-box compatibility wrapper used by the bundled seed and existing callers.
pub(crate) fn verify_version(binary: &Path, version: &str) -> LanpathResult<()> {
    verify_version_for(binary, CoreType::SingBox, version)
}

/// Verify the parsed core version exactly matches the requested identity.
pub(crate) fn verify_version_for(
    binary: &Path,
    core_type: CoreType,
    version: &str,
) -> LanpathResult<()> {
    let text = binary_output_for(core_type, binary);
    let parsed = parse_version_for(core_type, &text).unwrap_or_default();
    if !version.is_empty() && parsed == version {
        return Ok(());
    }
    Err(LanpathError::Core(format!(
        "Core {core_type} version verification failed: requested {version}, version probe output: {text}"
    )))
}

/// Semantic version comparison (for [`ClientCoreInventory::preferred_binary`] sorting downloaded cores).
///
/// Rules:
/// - Numeric segments (`.` separated) compared numerically: `1.14.0` > `1.13.15`;
/// - Versions with same numeric segments but prerelease suffix are lower than stable
///   (`1.14.0-beta.4` < `1.14.0`);
/// - Strings that cannot be parsed into version segments (e.g. `unknown`) are treated as empty
///   segments → oldest.
pub(crate) fn compare_core_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let (a_pre, a_core) = split_version_identity(a);
    let (b_pre, b_core) = split_version_identity(b);

    let a_nums = parse_numeric_segments(&a_core);
    let b_nums = parse_numeric_segments(&b_core);
    // Compare numeric segments; shorter one is smaller when numerically equal (1.14 < 1.14.0).
    for (x, y) in a_nums.iter().zip(b_nums.iter()) {
        match x.cmp(y) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
    }
    let len_cmp = a_nums.len().cmp(&b_nums.len());
    if len_cmp != std::cmp::Ordering::Equal {
        return len_cmp;
    }
    // Numeric segments equal: stable > prerelease; prereleases compared by marker + tail.
    match (a_pre, b_pre) {
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, None) => std::cmp::Ordering::Equal,
        (Some(p), Some(q)) => p.cmp(&q).then_with(|| a_core.cmp(&b_core)),
    }
}

/// Split version into (prerelease prefix, numeric core).
///
/// `1.14.0-beta.4` → `(Some("beta.4"), "1.14.0")`;
/// `1.13.15` → `(None, "1.13.15")`.
fn split_version_identity(v: &str) -> (Option<String>, String) {
    let v = v.trim();
    // Standard `number[.number].prerelease`: after `-` is prerelease marker.
    if let Some(idx) = v.find('-') {
        let (core, pre) = v.split_at(idx);
        let pre = pre.trim_start_matches('-');
        if !core.is_empty() && core.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return (Some(pre.to_ascii_lowercase()), core.to_string());
        }
    }
    (None, v.to_string())
}

/// Parse `.` separated numeric segments (non-numeric segments truncated and ignored).
fn parse_numeric_segments(s: &str) -> Vec<u64> {
    s.split('.')
        .map(|seg| seg.trim_end_matches(|c: char| !c.is_ascii_digit()))
        .filter(|seg| !seg.is_empty())
        .filter_map(|seg| seg.parse::<u64>().ok())
        .collect()
}

#[cfg(test)]
mod version_tests {
    use super::{
        CoreType, compare_core_versions, parse_details_from_output, parse_version_for,
        release_asset_name,
    };
    use std::cmp::Ordering;

    #[test]
    fn parses_mihomo_version_and_build_details() {
        assert_eq!(
            parse_version_for(CoreType::Mihomo, "Mihomo Meta alpha-69c1631 windows amd64"),
            Some("alpha-69c1631".into())
        );
        assert_eq!(
            parse_version_for(CoreType::Mihomo, "Mihomo Meta v1.19.30 darwin arm64"),
            Some("1.19.30".into())
        );
        let mihomo = parse_details_from_output(
            CoreType::Mihomo,
            "Mihomo Meta v1.19.33 linux amd64\nUse tags: with_gvisor",
        )
        .unwrap();
        assert_eq!(mihomo.tags, ["with_gvisor"]);

        let details = parse_details_from_output(
            CoreType::SingBox,
            "sing-box version 1.15.0-alpha.13\nEnvironment: go1.25 linux/amd64\nTags: with_quic,with_dhcp,with_gvisor",
        )
        .unwrap();
        assert_eq!(details.version, "1.15.0-alpha.13");
        assert_eq!(details.environment.as_deref(), Some("go1.25 linux/amd64"));
        assert_eq!(details.tags, ["with_quic", "with_dhcp", "with_gvisor"]);
    }

    #[test]
    fn mihomo_release_assets_use_variant_and_rolling_alpha_names() {
        assert_eq!(
            release_asset_name(CoreType::Mihomo, "1.19.29", "windows-amd64", true),
            "mihomo-windows-amd64-v2-v1.19.29.zip"
        );
        assert_eq!(
            release_asset_name(CoreType::Mihomo, "alpha-c0e43eb", "linux-amd64", false),
            "mihomo-linux-amd64-v2-alpha-c0e43eb.gz"
        );
        assert_eq!(
            release_asset_name(CoreType::SingBox, "1.13.15", "linux-amd64", false),
            "sing-box-1.13.15-linux-amd64.tar.gz"
        );
    }

    #[test]
    fn compares_numeric_segments() {
        assert_eq!(compare_core_versions("1.13.15", "1.14.0"), Ordering::Less);
        assert_eq!(
            compare_core_versions("1.14.0", "1.13.15"),
            Ordering::Greater
        );
        assert_eq!(compare_core_versions("1.14.0", "1.14.0"), Ordering::Equal);
        // Trailing segment semantics: 1.14 < 1.14.0.
        assert_eq!(compare_core_versions("1.14", "1.14.0"), Ordering::Less);
    }

    #[test]
    fn prerelease_sorts_below_same_base_stable() {
        assert_eq!(
            compare_core_versions("1.14.0-beta.4", "1.14.0"),
            Ordering::Less
        );
        assert_eq!(
            compare_core_versions("1.14.0", "1.14.0-beta.4"),
            Ordering::Greater
        );
        // Prerelease base version higher than old stable: 1.14.0-beta.4 > 1.13.15.
        assert_eq!(
            compare_core_versions("1.13.15", "1.14.0-beta.4"),
            Ordering::Less
        );
    }

    #[test]
    fn unknown_versions_fallback_lowest() {
        // Strings that cannot be parsed into version segments (e.g. `unknown`) are treated as empty
        // segments → oldest; two unknowns are equal.
        assert_eq!(compare_core_versions("unknown", "1.19.29"), Ordering::Less);
        assert_eq!(compare_core_versions("unknown", "unknown"), Ordering::Equal);
    }
}
