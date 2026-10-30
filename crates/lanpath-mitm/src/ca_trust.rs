//! Explicit trust operations. Windows uses native CurrentUser Root APIs and exact DER ownership.
//! Loading a CA never calls this module's mutation functions.

use lanpath_common::error::{LanpathError, LanpathResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
#[cfg(test)]
mod ownership_tests;
mod receipt_store;
#[cfg(windows)]
mod windows;
pub use receipt_store::{CaOwnedTrustView, CaOwnershipStatus, OwnedCaTrustStore};

/// Linux 系统锚点中的固定安装文件名。
const LINUX_ANCHOR_NAME: &str = "proxypanel-mitm.crt";

/// Linux 锚点候选目录（Debian/Ubuntu 系与 Arch/Fedora p11-kit 系）。
#[cfg(not(windows))]
const LINUX_ANCHOR_DIRS: [&str; 2] = [
    "/usr/local/share/ca-certificates",
    "/etc/ca-certificates/trust-source/anchors",
];

/// CA 系统信任状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaTrustStatus {
    /// 已安装到系统信任库且与本机 CA 匹配。
    Trusted,
    /// 系统信任库中未找到该 CA（或存在同名但指纹不一致的旧 CA）。
    NotTrusted,
    /// 无法判定（检测命令不可用 / 读取失败 / 平台不支持）。
    Unknown,
}

/// CA 信任状态视图（Tauri 命令返回）。
#[derive(Debug, Clone, Serialize)]
pub struct CaTrustView {
    pub status: CaTrustStatus,
    /// 状态说明（面向用户的一行描述）。
    pub detail: String,
}

impl CaTrustView {
    fn trusted(detail: impl Into<String>) -> Self {
        Self {
            status: CaTrustStatus::Trusted,
            detail: detail.into(),
        }
    }

    fn not_trusted(detail: impl Into<String>) -> Self {
        Self {
            status: CaTrustStatus::NotTrusted,
            detail: detail.into(),
        }
    }

    fn unknown(detail: impl Into<String>) -> Self {
        Self {
            status: CaTrustStatus::Unknown,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaTrustScope {
    CurrentUserRoot,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaInstallDisposition {
    Added,
    AlreadyPresent,
    InstalledUntracked,
}

/// Persist this receipt only in the application's protected data root.
/// Private DER/token fields bind removal to this precise addition, not a subject name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaInstallReceipt {
    pub source_path: PathBuf,
    pub sha256: String,
    pub scope: CaTrustScope,
    pub disposition: CaInstallDisposition,
    der: Vec<u8>,
    owner_token: Option<Vec<u8>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaRemovalDisposition {
    Removed,
    AlreadyAbsent,
    PreservedPreexisting,
    OwnershipChanged,
}
#[derive(Debug, Clone, Serialize)]
pub struct CaRemovalReceipt {
    pub sha256: String,
    pub scope: CaTrustScope,
    pub disposition: CaRemovalDisposition,
}
#[derive(Debug, Clone, Serialize)]
pub struct CaQueryReceipt {
    pub source_path: PathBuf,
    pub sha256: String,
    pub scope: CaTrustScope,
    pub view: CaTrustView,
}
fn source_der(path: &Path) -> LanpathResult<(PathBuf, Vec<u8>)> {
    let path = std::fs::canonicalize(path)
        .map_err(|e| LanpathError::Mitm(format!("Resolve CA source: {e}")))?;
    let pem = std::fs::read_to_string(&path)
        .map_err(|e| LanpathError::Mitm(format!("Read CA source: {e}")))?;
    let der = pem_to_der(&pem)?;
    crate::ca::validation::validate_ca_der(&der)?;
    Ok((path, der))
}
pub fn query_ca(ca_path: &Path) -> LanpathResult<CaQueryReceipt> {
    let (source_path, der) = source_der(ca_path)?;
    #[cfg(windows)]
    let view = windows::query(&der);
    #[cfg(not(windows))]
    let view = platform_ca_trust_status(&source_path);
    Ok(CaQueryReceipt {
        source_path,
        sha256: sha256_hex(&der),
        scope: trust_scope(),
        view,
    })
}
fn trust_scope() -> CaTrustScope {
    if cfg!(windows) {
        CaTrustScope::CurrentUserRoot
    } else {
        CaTrustScope::System
    }
}
/// Explicit caller authorization required. No LocalMachine Windows support.
pub fn install_ca(ca_path: &Path) -> LanpathResult<CaInstallReceipt> {
    let (source_path, der) = source_der(ca_path)?;
    install_der(source_path, der)
}
/// Pin authorization to the certificate currently used by the running proxy.
pub fn install_ca_with_expected(
    ca_path: &Path,
    expected_sha256: &str,
) -> LanpathResult<CaInstallReceipt> {
    let (source_path, der) = source_der(ca_path)?;
    if sha256_hex(&der) != expected_sha256 {
        return Err(LanpathError::Mitm(
            "CA source changed since authorization; nothing installed".into(),
        ));
    }
    install_der(source_path, der)
}
fn install_der(source_path: PathBuf, der: Vec<u8>) -> LanpathResult<CaInstallReceipt> {
    #[cfg(windows)]
    let (disposition, owner_token) = windows::install(&der)?;
    #[cfg(not(windows))]
    let (disposition, owner_token) = {
        platform_install_ca(&source_path)?;
        (CaInstallDisposition::InstalledUntracked, None)
    };
    Ok(CaInstallReceipt {
        source_path,
        sha256: sha256_hex(&der),
        scope: trust_scope(),
        disposition,
        der,
        owner_token,
    })
}
/// Remove only an exact certificate added by this receipt, never an existing root.
pub fn remove_ca(receipt: &CaInstallReceipt) -> LanpathResult<CaRemovalReceipt> {
    if receipt.sha256 != sha256_hex(&receipt.der) {
        return Err(LanpathError::Mitm("CA receipt fingerprint mismatch".into()));
    }
    #[cfg(windows)]
    {
        let disposition = windows::remove(receipt)?;
        Ok(CaRemovalReceipt {
            sha256: receipt.sha256.clone(),
            scope: receipt.scope,
            disposition,
        })
    }
    #[cfg(not(windows))]
    Err(LanpathError::Mitm(
        "Owned trust removal is supported only for Windows CurrentUser Root".into(),
    ))
}

#[cfg(any(windows, test))]
fn removal_decision(
    receipt: &CaInstallReceipt,
    found_der: Option<&[u8]>,
    owner: Option<&[u8]>,
) -> CaRemovalDisposition {
    if receipt.disposition != CaInstallDisposition::Added {
        return CaRemovalDisposition::PreservedPreexisting;
    }
    let Some(found) = found_der else {
        return CaRemovalDisposition::AlreadyAbsent;
    };
    if receipt.scope != CaTrustScope::CurrentUserRoot
        || found != receipt.der
        || receipt
            .owner_token
            .as_deref()
            .is_none_or(|token| token.is_empty() || Some(token) != owner)
    {
        return CaRemovalDisposition::OwnershipChanged;
    }
    CaRemovalDisposition::Removed
}
/// Unknown remains distinct from a successful exact-certificate absence query.
pub fn ca_trust_status(ca_path: &Path) -> CaTrustView {
    match query_ca(ca_path) {
        Ok(receipt) => receipt.view,
        Err(error) => CaTrustView::unknown(error.to_string()),
    }
}

// ---------------------------------------------------------------------------
// 纯函数（参数化，供单测；不执行系统命令）
// ---------------------------------------------------------------------------

/// PEM → DER; require exactly one certificate, not a silently truncated chain.
pub fn pem_to_der(pem: &str) -> LanpathResult<Vec<u8>> {
    crate::ca::validation::validate_pem_envelope(pem, "CERTIFICATE")?;
    let certificates: Result<Vec<_>, _> = rustls_pemfile::certs(&mut pem.as_bytes()).collect();
    let certificates =
        certificates.map_err(|e| LanpathError::Mitm(format!("Invalid CA PEM: {e}")))?;
    if certificates.len() != 1 {
        return Err(LanpathError::Mitm(
            "CA PEM must contain exactly one certificate".into(),
        ));
    }
    Ok(certificates[0].to_vec())
}

/// 字节的 SHA-256 指纹（小写 hex，无分隔符）。
pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    format!("{:x}", sha2::Sha256::digest(data))
}

/// Linux 提权安装的 shell 脚本（pkexec `sh -c` 的第三个参数；纯构造函数）。
pub fn linux_install_script(ca_path: &Path) -> String {
    format!(
        "install -m 0644 {} /usr/local/share/ca-certificates/{LINUX_ANCHOR_NAME} && update-ca-certificates",
        shell_quote(&ca_path.to_string_lossy())
    )
}

/// macOS 提权安装的 AppleScript（osascript `-e` 参数；纯构造函数）。
pub fn macos_install_applescript(ca_path: &Path) -> String {
    let path = ca_path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!(
        "do shell script \"security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain \\\"{path}\\\"\" with administrator privileges"
    )
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

// ---------------------------------------------------------------------------
// macOS
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn platform_install_ca(ca_path: &Path) -> LanpathResult<()> {
    let script = macos_install_applescript(ca_path);
    match std::process::Command::new("osascript")
        .args(["-e", &script])
        .status()
    {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(LanpathError::Mitm(format!(
            "安装 CA 失败：osascript 退出码 {status}（若取消了授权弹窗请重试）"
        ))),
        Err(e) => Err(LanpathError::Mitm(format!("无法执行 osascript: {e}"))),
    }
}

#[cfg(target_os = "macos")]
fn platform_ca_trust_status(ca_path: &Path) -> CaTrustView {
    match std::process::Command::new("security")
        .args(["verify-cert", "-c"])
        .arg(ca_path)
        .status()
    {
        Ok(status) if status.success() => CaTrustView::trusted("系统钥匙串已信任该 CA"),
        Ok(status) => CaTrustView::not_trusted(format!(
            "security verify-cert 退出码 {status}：CA 未被系统信任"
        )),
        Err(e) => CaTrustView::unknown(format!("无法执行 security: {e}")),
    }
}

// ---------------------------------------------------------------------------
// Linux
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn platform_install_ca(ca_path: &Path) -> LanpathResult<()> {
    let script = linux_install_script(ca_path);
    match std::process::Command::new("pkexec")
        .args(["sh", "-c", &script])
        .status()
    {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(LanpathError::Mitm(format!(
            "安装 CA 失败：pkexec 退出码 {status}（需要 update-ca-certificates 与 polkit 授权）"
        ))),
        Err(e) => Err(LanpathError::Mitm(format!(
            "无法执行 pkexec（{e}）：请安装 polkit（如 apt install policykit-1）后重试"
        ))),
    }
}

#[cfg(target_os = "linux")]
fn platform_ca_trust_status(ca_path: &Path) -> CaTrustView {
    let ours_pem = match std::fs::read_to_string(ca_path) {
        Ok(p) => p,
        Err(e) => return CaTrustView::unknown(format!("读取本机 CA 失败: {e}")),
    };
    let ours = match pem_to_der(&ours_pem) {
        Ok(d) => d,
        Err(e) => return CaTrustView::unknown(format!("解析本机 CA 失败: {e}")),
    };
    let ours_fp = sha256_hex(&ours);
    let mut found_anchor = false;
    for dir in LINUX_ANCHOR_DIRS {
        let anchor = Path::new(dir).join(LINUX_ANCHOR_NAME);
        match std::fs::read_to_string(&anchor) {
            Ok(pem) => {
                found_anchor = true;
                match pem_to_der(&pem) {
                    Ok(der) if sha256_hex(&der) == ours_fp => {
                        return CaTrustView::trusted(format!(
                            "系统锚点 {} 与本机 CA 指纹一致",
                            anchor.display()
                        ));
                    }
                    // 同名锚点但指纹不一致（旧 CA）：继续检查其他锚点目录。
                    Ok(_) => {}
                    Err(e) => {
                        return CaTrustView::unknown(format!(
                            "解析 {} 失败: {e}",
                            anchor.display()
                        ));
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return CaTrustView::unknown(format!("读取 {} 失败: {e}", anchor.display())),
        }
    }
    if found_anchor {
        CaTrustView::not_trusted("系统锚点存在同名 CA 但指纹不一致（可能为旧 CA，请重新安装）")
    } else {
        CaTrustView::not_trusted("系统锚点中未找到该 CA")
    }
}

// ---------------------------------------------------------------------------
// 兜底（其他平台：可编译，安装报不支持、状态报未知）
// ---------------------------------------------------------------------------

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn platform_install_ca(_ca_path: &Path) -> LanpathResult<()> {
    Err(LanpathError::Mitm(
        "当前平台不支持自动安装 CA 到系统信任库".to_string(),
    ))
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn platform_ca_trust_status(_ca_path: &Path) -> CaTrustView {
    CaTrustView::unknown("当前平台不支持信任状态检测")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CaStore;

    #[test]
    fn pem_to_der_decodes_a_single_certificate() {
        let dir = tempfile::tempdir().unwrap();
        let material = crate::FileCaStore::new(dir.path())
            .load_or_generate()
            .unwrap();
        let der = pem_to_der(&material.cert_pem).unwrap();
        assert!(!der.is_empty());
        assert_eq!(der[0], 0x30, "DER 应以 SEQUENCE 开头");
    }

    #[test]
    fn pem_to_der_rejects_pem_without_cert() {
        let err = pem_to_der("-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n");
        assert!(err.is_err());
    }

    #[test]
    fn sha256_hex_matches_known_vector() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha256_fingerprint_stable_for_same_der() {
        let dir = tempfile::tempdir().unwrap();
        let material = crate::FileCaStore::new(dir.path())
            .load_or_generate()
            .unwrap();
        let a = pem_to_der(&material.cert_pem).unwrap();
        let b = pem_to_der(&material.cert_pem).unwrap();
        assert_eq!(sha256_hex(&a), sha256_hex(&b));
    }

    #[test]
    fn linux_install_script_quotes_path_and_updates_store() {
        let script = linux_install_script(Path::new("/home/user/my dir/ca.crt"));
        assert!(
            script.contains(
                "install -m 0644 '/home/user/my dir/ca.crt' /usr/local/share/ca-certificates/proxypanel-mitm.crt"
            ),
            "应引用并安装到锚点目录: {script}"
        );
        assert!(script.ends_with("&& update-ca-certificates"), "{script}");
    }

    #[test]
    fn linux_install_script_escapes_single_quotes() {
        let script = linux_install_script(Path::new("/tmp/it's/ca.crt"));
        assert!(script.contains("/tmp/it'\\''s/ca.crt"), "{script}");
    }

    #[test]
    fn macos_install_applescript_embeds_path_and_admin_flag() {
        let script = macos_install_applescript(Path::new("/Users/u/ca.crt"));
        assert!(
            script.contains("security add-trusted-cert -d -r trustRoot"),
            "{script}"
        );
        assert!(
            script.contains("/Library/Keychains/System.keychain"),
            "{script}"
        );
        assert!(script.contains("\\\"/Users/u/ca.crt\\\""), "{script}");
        assert!(script.contains("with administrator privileges"), "{script}");
    }

    #[test]
    fn trust_status_serializes_to_frontend_contract() {
        let view = CaTrustView::trusted("ok");
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["status"], "trusted");
        let view = CaTrustView::not_trusted("no");
        assert_eq!(
            serde_json::to_value(&view).unwrap()["status"],
            "not_trusted"
        );
        let view = CaTrustView::unknown("?");
        assert_eq!(serde_json::to_value(&view).unwrap()["status"], "unknown");
    }
}
