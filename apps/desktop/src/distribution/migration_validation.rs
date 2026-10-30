use super::{
    CaScope, MigrationOptions,
    files::{normalize_history, relocate},
    invalid,
};
use lanpath_common::LanpathResult;
use std::path::Path;

pub(super) fn validate_stage(
    stage: &Path,
    source: &Path,
    target: &Path,
    options: MigrationOptions,
) -> LanpathResult<()> {
    if options.configuration {
        relocate(stage, source, target, true)?;
    }
    if options.history {
        normalize_history(stage)?;
    }
    if options.appearance && stage.join("appearance.json").try_exists()? {
        // Desktop preferences are typed; do not copy an invalid appearance file.
        crate::appearance::AppearanceStore::new(stage).load()?;
    }
    if options.ca == CaScope::IdentityAndPrivateKey {
        let cert = stage.join("certs/ca.crt");
        let key = stage.join("certs/ca.key");
        if cert.try_exists()? != key.try_exists()? {
            return Err(invalid(
                "CA import requires both certificate and private key",
            ));
        }
        if cert.try_exists()? {
            validate_ca(&cert, &key)?;
        }
    }
    Ok(())
}
fn validate_ca(cert: &Path, key: &Path) -> LanpathResult<()> {
    let cert = std::fs::read_to_string(cert)?;
    let key = std::fs::read_to_string(key)?;
    lanpath_mitm::validate_ca_material(&lanpath_mitm::CaMaterial {
        cert_pem: cert,
        key_pem: key,
    })?;
    Ok(())
}
