use super::*;
use ed25519_dalek::{Signer, SigningKey};

fn manifest() -> Manifest {
    Manifest {
        schema: 1,
        product: crate::product::APP_ID.into(),
        channel: crate::product::UPDATE_CHANNEL.into(),
        version: "2.3.4".into(),
        arch: std::env::consts::ARCH.into(),
        artifact: Artifact {
            url: format!(
                "{}/releases/download/gpui-v2.3.4/{}",
                crate::product::REPOSITORY,
                crate::product::BINARY_NAME
            ),
            size: 7,
            sha256: "a".repeat(64),
        },
        files: crate::product::TRUSTED_BUNDLE_FILES
            .iter()
            .map(|path| BundleFile {
                path: (*path).into(),
                size: 7,
                sha256: "a".repeat(64),
            })
            .collect(),
    }
}
fn signed(value: &Manifest, key: &SigningKey) -> (Vec<u8>, String, String) {
    let bytes = serde_json::to_vec(value).unwrap();
    let signature = STANDARD.encode(key.sign(&bytes).to_bytes());
    let public = STANDARD.encode(key.verifying_key().as_bytes());
    (bytes, signature, public)
}

#[test]
fn only_the_trusted_key_can_authorize_the_exact_payload_bytes() {
    let key = SigningKey::from_bytes(&[19; 32]);
    let (mut bytes, signature, public) = signed(&manifest(), &key);
    assert!(verify_manifest(&bytes, &signature, &public).is_ok());
    let unrelated = SigningKey::from_bytes(&[29; 32]);
    let untrusted = STANDARD.encode(unrelated.sign(&bytes).to_bytes());
    assert!(verify_manifest(&bytes, &untrusted, &public).is_err());
    // Still valid JSON with identical values, but not the signed byte sequence.
    bytes.push(b'\n');
    assert!(verify_manifest(&bytes, &signature, &public).is_err());
}

#[test]
fn even_a_trusted_signature_cannot_cross_product_architecture_or_file_namespaces() {
    let key = SigningKey::from_bytes(&[19; 32]);
    let mut cases = Vec::new();
    let mut value = manifest();
    value.product = "com.ybakiame.lanpath".into();
    cases.push(value);
    let mut value = manifest();
    value.arch = "foreign-architecture".into();
    cases.push(value);
    let mut value = manifest();
    value.channel = "tauri-stable".into();
    cases.push(value);
    let mut value = manifest();
    value.artifact.url = "https://example.invalid/foreign.exe".into();
    cases.push(value);
    let mut value = manifest();
    value.files[0].path = "../foreign.exe".into();
    cases.push(value);
    let mut value = manifest();
    value.artifact.url = format!(
        "{}/releases/download/gpui-v2.3.4/LanpathGPUI_2.3.4_windows_{}-setup.exe",
        crate::product::REPOSITORY,
        std::env::consts::ARCH
    );
    cases.push(value);
    for value in cases {
        let (bytes, signature, public) = signed(&value, &key);
        assert!(verify_manifest(&bytes, &signature, &public).is_err());
    }
}

#[test]
fn payload_identity_binds_release_and_files_not_outer_artifact_kind() {
    let bundle = manifest();
    let identity = bundle_identity(&bundle).unwrap();
    let mut descriptor = bundle.clone();
    descriptor.artifact.url = format!(
        "{}/releases/download/gpui-v2.3.4/LanpathGPUI_2.3.4_windows_{}-portable.zip",
        crate::product::REPOSITORY,
        std::env::consts::ARCH
    );
    descriptor.artifact.sha256 = "b".repeat(64);
    assert_eq!(identity, bundle_identity(&descriptor).unwrap());
    descriptor.files[0].sha256 = "c".repeat(64);
    assert_ne!(identity, bundle_identity(&descriptor).unwrap());
    let mut foreign = bundle;
    foreign.version = "2.3.5".into();
    assert_ne!(identity, bundle_identity(&foreign).unwrap());
}
