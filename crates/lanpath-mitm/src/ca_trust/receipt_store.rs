use super::*;
#[cfg(any(windows, test))]
use crate::ca::write_private;
use crate::ca::{
    exists, private_options, protection, require_directory, require_file, sync_directory,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaOwnershipStatus {
    Untracked,
    Owned,
    Absent,
    Changed,
    Unknown,
}

/// Public status deliberately excludes the private native ownership token and DER receipt.
#[derive(Debug, Clone, Serialize)]
pub struct CaOwnedTrustView {
    pub status: CaTrustStatus,
    pub detail: String,
    /// None when the current signing source is unavailable; the saved receipt remains independent.
    pub sha256: Option<String>,
    pub scope: CaTrustScope,
    pub ownership: CaOwnershipStatus,
    pub receipt_sha256: Option<String>,
}

/// Receipt and cross-process lock live beside the CA, under its existing protected directory.
/// No trust mutation, CA generation, or receipt deletion happens on construction or query.
pub struct OwnedCaTrustStore {
    dir: PathBuf,
}
impl OwnedCaTrustStore {
    pub fn new(ca_dir: impl Into<PathBuf>) -> Self {
        Self { dir: ca_dir.into() }
    }
    fn source(&self) -> PathBuf {
        self.dir.join("ca.crt")
    }
    fn receipt_path(&self) -> PathBuf {
        self.dir.join("ca-trust-receipt.json")
    }
    pub fn has_receipt(&self) -> LanpathResult<bool> {
        exists(&self.receipt_path())
    }
    fn lock(&self) -> LanpathResult<std::fs::File> {
        require_directory(&self.dir)?;
        protection::protect(&self.dir, true)?;
        let path = self.dir.join(".ca-trust.lock");
        if exists(&path)? {
            require_file(&path)?;
        }
        let file = private_options().create(true).truncate(false).open(&path)?;
        protection::protect(&path, false)?;
        fs2::FileExt::lock_exclusive(&file)?;
        Ok(file)
    }
    fn load(&self) -> LanpathResult<Option<CaInstallReceipt>> {
        let path = self.receipt_path();
        if !exists(&path)? {
            return Ok(None);
        }
        require_file(&path)?;
        protection::protect(&path, false)?;
        let receipt: CaInstallReceipt = serde_json::from_slice(&std::fs::read(path)?)?;
        validate_receipt(&receipt)?;
        Ok(Some(receipt))
    }
    #[cfg(any(windows, test))]
    fn save(&self, receipt: &CaInstallReceipt) -> LanpathResult<()> {
        validate_receipt(receipt)?;
        let temporary = self
            .dir
            .join(format!(".ca-trust-{}.json", uuid::Uuid::new_v4()));
        write_private(&temporary, &serde_json::to_string(receipt)?)?;
        // Publish without overwriting any earlier ownership evidence, including after a crash.
        let published: LanpathResult<()> = (|| {
            std::fs::hard_link(&temporary, self.receipt_path())?;
            sync_directory(&self.dir)
        })();
        let cleanup = std::fs::remove_file(temporary);
        published?;
        cleanup?;
        Ok(())
    }
    pub fn query(&self) -> LanpathResult<CaOwnedTrustView> {
        let _lock = self.lock()?;
        self.query_locked()
    }
    fn query_locked(&self) -> LanpathResult<CaOwnedTrustView> {
        let receipt = self.load()?;
        let (sha256, scope, mut view) = match query_ca(&self.source()) {
            Ok(query) => (Some(query.sha256), query.scope, query.view),
            Err(error) if receipt.is_some() => {
                (None, trust_scope(), CaTrustView::unknown(error.to_string()))
            }
            Err(error) => return Err(error),
        };
        let ownership = if let Some(receipt) = &receipt {
            match receipt_ownership(receipt) {
                Ok(ownership) => ownership,
                Err(error) => {
                    view.detail
                        .push_str(&format!("; ownership query failed: {error}"));
                    CaOwnershipStatus::Unknown
                }
            }
        } else {
            CaOwnershipStatus::Untracked
        };
        Ok(CaOwnedTrustView {
            status: view.status,
            detail: view.detail,
            sha256,
            scope,
            ownership,
            receipt_sha256: receipt.map(|receipt| receipt.sha256),
        })
    }
    pub fn install(&self, expected_sha256: &str) -> LanpathResult<CaOwnedTrustView> {
        let _lock = self.lock()?;
        let (source_path, der) = source_der(&self.source())?;
        let fingerprint = sha256_hex(&der);
        if fingerprint != expected_sha256 {
            return Err(LanpathError::Mitm(
                "CA source changed since authorization; nothing installed".into(),
            ));
        }
        if let Some(receipt) = self.load()? {
            if receipt.sha256 != fingerprint {
                return Err(LanpathError::Mitm("A different CA has a saved trust receipt; preserve it and explicitly remove its owned trust before installing another CA".into()));
            }
            // Never replace an Added receipt with AlreadyPresent and lose its owner token.
            #[cfg(windows)]
            if windows::ownership(&receipt)? == CaRemovalDisposition::Removed {
                return self.query_expected(expected_sha256);
            }
            return Err(LanpathError::Mitm("Saved CA trust ownership is absent or changed; no installation performed. Resolve the existing receipt explicitly".into()));
        }
        #[cfg(windows)]
        {
            windows::install_recorded(&der, |token| {
                self.save(&CaInstallReceipt {
                    source_path,
                    sha256: fingerprint.clone(),
                    scope: CaTrustScope::CurrentUserRoot,
                    disposition: CaInstallDisposition::Added,
                    der: der.clone(),
                    owner_token: Some(token.to_vec()),
                })
            })?;
        }
        #[cfg(not(windows))]
        {
            platform_install_ca(&source_path)?;
        }
        self.query_expected(expected_sha256)
    }
    fn query_expected(&self, expected_sha256: &str) -> LanpathResult<CaOwnedTrustView> {
        let view = self.query_locked()?;
        if view.sha256.as_deref() != Some(expected_sha256) {
            return Err(LanpathError::Mitm("CA source changed during installation; installed identity retained in its ownership receipt".into()));
        }
        Ok(view)
    }
    pub fn remove(&self, expected_sha256: &str) -> LanpathResult<CaRemovalReceipt> {
        let _lock = self.lock()?;
        let receipt = self.load()?.ok_or_else(|| {
            LanpathError::Mitm("No owned CA trust receipt; existing roots are preserved".into())
        })?;
        if receipt.sha256 != expected_sha256 {
            return Err(LanpathError::Mitm(
                "CA trust receipt changed since removal authorization; nothing removed".into(),
            ));
        }
        let removal = remove_ca(&receipt)?;
        if matches!(
            removal.disposition,
            CaRemovalDisposition::Removed | CaRemovalDisposition::AlreadyAbsent
        ) {
            std::fs::remove_file(self.receipt_path())?;
            sync_directory(&self.dir)?;
        }
        // Changed ownership is a refusal, not a successful cleanup. Keep its evidence.
        Ok(removal)
    }
}

fn receipt_ownership(receipt: &CaInstallReceipt) -> LanpathResult<CaOwnershipStatus> {
    #[cfg(windows)]
    {
        Ok(match windows::ownership(receipt)? {
            CaRemovalDisposition::Removed => CaOwnershipStatus::Owned,
            CaRemovalDisposition::AlreadyAbsent => CaOwnershipStatus::Absent,
            CaRemovalDisposition::OwnershipChanged => CaOwnershipStatus::Changed,
            CaRemovalDisposition::PreservedPreexisting => CaOwnershipStatus::Untracked,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = receipt;
        Ok(CaOwnershipStatus::Untracked)
    }
}
fn validate_receipt(receipt: &CaInstallReceipt) -> LanpathResult<()> {
    if receipt.sha256 != sha256_hex(&receipt.der) {
        return Err(LanpathError::Mitm("CA receipt fingerprint mismatch".into()));
    }
    // A receipt is removal evidence, not a currently usable signing CA. Expiry must not
    // prevent exact owned removal; native removal still compares DER and the owner token.
    if receipt.scope != CaTrustScope::CurrentUserRoot
        || receipt.disposition != CaInstallDisposition::Added
        || receipt
            .owner_token
            .as_ref()
            .is_none_or(|token| token.is_empty())
    {
        return Err(LanpathError::Mitm(
            "Invalid owned CurrentUser Root receipt; evidence preserved".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;

#[cfg(all(test, windows))]
mod windows_tests;
