use super::{CaMaterial, error};
use lanpath_common::error::LanpathResult;
use rcgen::PublicKeyData;
/// Accept one unencrypted PEM object only; decoders otherwise silently skip unrelated objects.
pub(crate) fn validate_pem_envelope(pem: &str, label: &str) -> LanpathResult<()> {
    let pem = pem.trim();
    let body = pem
        .strip_prefix("-----BEGIN ")
        .and_then(|body| body.strip_prefix(label))
        .and_then(|body| body.strip_prefix("-----"))
        .and_then(|body| body.strip_suffix("-----"))
        .and_then(|body| body.strip_suffix(label))
        .and_then(|body| body.strip_suffix("-----END "))
        .ok_or_else(|| error(format!("Expected exactly one {label} PEM object")))?;
    if body.is_empty()
        || !body.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || b"+/=".contains(&byte) || byte.is_ascii_whitespace()
        })
    {
        return Err(error(format!("Invalid or multiple {label} PEM objects")));
    }
    Ok(())
}

/// Validate without changing bytes or identity. Old product subjects are deliberately accepted.
pub fn validate_ca_material(material: &CaMaterial) -> LanpathResult<()> {
    let der = crate::ca_trust::pem_to_der(&material.cert_pem)?;
    let cert = validate_ca_der(&der)?;
    let key_label = [
        ("-----BEGIN PRIVATE KEY-----", "PRIVATE KEY"),
        ("-----BEGIN RSA PRIVATE KEY-----", "RSA PRIVATE KEY"),
        ("-----BEGIN EC PRIVATE KEY-----", "EC PRIVATE KEY"),
    ]
    .into_iter()
    .find_map(|(prefix, label)| material.key_pem.trim().starts_with(prefix).then_some(label))
    .ok_or_else(|| error("Expected one supported unencrypted CA private key"))?;
    validate_pem_envelope(&material.key_pem, key_label)?;
    let key = rcgen::KeyPair::from_pem(&material.key_pem)
        .map_err(|_| error("Invalid or unsupported CA private key; original material preserved"))?;
    if key.subject_public_key_info() != cert.public_key().raw {
        return Err(error(
            "CA certificate/private key mismatch; restore the original matching pair, not a regenerated key",
        ));
    }
    Ok(())
}
pub(crate) fn validate_ca_der(
    der: &[u8],
) -> LanpathResult<x509_parser::certificate::X509Certificate<'_>> {
    let (remaining, cert) = x509_parser::parse_x509_certificate(der)
        .map_err(|e| error(format!("Invalid CA certificate: {e}")))?;
    if !remaining.is_empty() {
        return Err(error("Trailing data in CA DER"));
    }
    if !cert.validity().is_valid() {
        return Err(error(
            "CA is expired or not yet valid; explicit identity migration is required",
        ));
    }
    let constraints = cert
        .basic_constraints()
        .map_err(|e| error(format!("CA constraints: {e}")))?
        .ok_or_else(|| error("CA basic constraints are missing"))?;
    if !constraints.value.ca {
        return Err(error("Certificate is not a CA"));
    }
    let usage = cert
        .key_usage()
        .map_err(|e| error(format!("CA key usage: {e}")))?
        .ok_or_else(|| error("CA key usage is missing"))?;
    if !usage.value.key_cert_sign() {
        return Err(error("CA does not permit certificate signing"));
    }
    if cert.subject() != cert.issuer() {
        return Err(error("CA must be a self-issued root"));
    }
    cert.verify_signature(None)
        .map_err(|e| error(format!("CA self signature is invalid or unsupported: {e}")))?;
    Ok(cert)
}
