use super::*;
use std::ptr::null_mut;
use windows_sys::Win32::{
    Foundation::{CRYPT_E_EXISTS, CRYPT_E_NOT_FOUND, GetLastError},
    Security::Cryptography::*,
};
// Application-owned user property; value includes a product marker plus a random installation token.
const OWNER_PROPERTY: u32 = CERT_FIRST_USER_PROP_ID + 0x4c50;
struct Store(HCERTSTORE);
impl Store {
    fn open(writable: bool) -> LanpathResult<Self> {
        let root: Vec<u16> = "Root\0".encode_utf16().collect();
        let flags = CERT_SYSTEM_STORE_CURRENT_USER
            | if writable {
                0
            } else {
                CERT_STORE_OPEN_EXISTING_FLAG | CERT_STORE_READONLY_FLAG
            };
        // Registry provider is deliberately precise HKCU, not a logical collection that includes HKLM.
        let store = unsafe {
            CertOpenStore(
                CERT_STORE_PROV_SYSTEM_REGISTRY_W,
                0,
                0,
                flags,
                root.as_ptr().cast(),
            )
        };
        if store.is_null() {
            Err(native_error("open CurrentUser Root"))
        } else {
            Ok(Self(store))
        }
    }
    fn find(&self, der: &[u8]) -> LanpathResult<Option<Context>> {
        let mut previous = null_mut();
        loop {
            let current = unsafe { CertEnumCertificatesInStore(self.0, previous) };
            if current.is_null() {
                let code = unsafe { GetLastError() };
                return if code == CRYPT_E_NOT_FOUND as u32 {
                    Ok(None)
                } else {
                    Err(code_error("enumerate CurrentUser Root", code))
                };
            }
            // Enumeration frees previous. A found context is now owned by its RAII wrapper.
            if unsafe { encoded(current) } == der {
                return Ok(Some(Context(current)));
            }
            previous = current;
        }
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        unsafe {
            CertCloseStore(self.0, 0);
        }
    }
}
struct Context(*mut CERT_CONTEXT);
impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            CertFreeCertificateContext(self.0);
        }
    }
}
unsafe fn encoded<'a>(context: *const CERT_CONTEXT) -> &'a [u8] {
    unsafe {
        std::slice::from_raw_parts((*context).pbCertEncoded, (*context).cbCertEncoded as usize)
    }
}
fn code_error(operation: &str, code: u32) -> LanpathError {
    LanpathError::Mitm(format!(
        "Windows CA {operation}: {} (0x{code:08x})",
        std::io::Error::from_raw_os_error(code as i32)
    ))
}
fn native_error(operation: &str) -> LanpathError {
    code_error(operation, unsafe { GetLastError() })
}
pub(super) fn query(der: &[u8]) -> CaTrustView {
    match Store::open(false).and_then(|store| store.find(der)) {
        Ok(Some(_)) => CaTrustView::trusted("Exact CA DER present in Windows CurrentUser Root"),
        Ok(None) => CaTrustView::not_trusted("Exact CA DER absent from Windows CurrentUser Root"),
        Err(error) => CaTrustView::unknown(error.to_string()),
    }
}
pub(super) fn install(der: &[u8]) -> LanpathResult<(CaInstallDisposition, Option<Vec<u8>>)> {
    install_in(&Store::open(true)?, der, add_context)
}
pub(super) fn install_recorded(
    der: &[u8],
    record: impl FnOnce(&[u8]) -> LanpathResult<()>,
) -> LanpathResult<(CaInstallDisposition, Option<Vec<u8>>)> {
    install_in_recorded(&Store::open(true)?, der, add_context, record)
}
fn add_context(store: &Store, context: &Context) -> LanpathResult<bool> {
    if unsafe {
        CertAddCertificateContextToStore(store.0, context.0, CERT_STORE_ADD_NEW, null_mut())
    } != 0
    {
        return Ok(true);
    }
    let code = unsafe { GetLastError() };
    if code == CRYPT_E_EXISTS as u32 {
        Ok(false)
    } else {
        Err(code_error("add exact CA", code))
    }
}
fn install_in(
    store: &Store,
    der: &[u8],
    add: impl FnOnce(&Store, &Context) -> LanpathResult<bool>,
) -> LanpathResult<(CaInstallDisposition, Option<Vec<u8>>)> {
    install_in_recorded(store, der, add, |_| Ok(()))
}
fn install_in_recorded(
    store: &Store,
    der: &[u8],
    add: impl FnOnce(&Store, &Context) -> LanpathResult<bool>,
    record: impl FnOnce(&[u8]) -> LanpathResult<()>,
) -> LanpathResult<(CaInstallDisposition, Option<Vec<u8>>)> {
    // A failed lookup is not absence: never proceed to mutation after an unknown result.
    if store.find(der)?.is_some() {
        return Ok((CaInstallDisposition::AlreadyPresent, None));
    }
    let length = u32::try_from(der.len())
        .map_err(|_| LanpathError::Mitm("CA certificate too large".into()))?;
    let context =
        Context(unsafe { CertCreateCertificateContext(X509_ASN_ENCODING, der.as_ptr(), length) });
    if context.0.is_null() {
        return Err(native_error("create certificate context"));
    }
    let mut token = b"Lanpath GPUI CA ownership v1\0".to_vec();
    token.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    let blob = CRYPT_INTEGER_BLOB {
        cbData: token.len() as u32,
        pbData: token.as_mut_ptr(),
    };
    // Attach before adding: the native API copies user properties together with the DER.
    if unsafe {
        CertSetCertificateContextProperty(
            context.0,
            OWNER_PROPERTY,
            0,
            (&blob as *const CRYPT_INTEGER_BLOB).cast(),
        )
    } == 0
    {
        return Err(native_error("set ownership property"));
    }
    // Persist exact DER/token before mutation. A crash or racing installer cannot grant ownership.
    record(&token)?;
    if !add(store, &context)? {
        // Another installer won the race. Never replace or claim its certificate.
        if store.find(der)?.is_some() {
            return Ok((CaInstallDisposition::AlreadyPresent, None));
        }
        return Err(code_error(
            "add exact CA (a different certificate already exists)",
            CRYPT_E_EXISTS as u32,
        ));
    }
    Ok((CaInstallDisposition::Added, Some(token)))
}
fn owner(context: &Context) -> LanpathResult<Option<Vec<u8>>> {
    let mut length = 0;
    if unsafe {
        CertGetCertificateContextProperty(context.0, OWNER_PROPERTY, null_mut(), &mut length)
    } == 0
    {
        let code = unsafe { GetLastError() };
        return if code == CRYPT_E_NOT_FOUND as u32 {
            Ok(None)
        } else {
            Err(code_error("read CA ownership", code))
        };
    }
    if length > 4096 {
        return Err(LanpathError::Mitm(
            "Unexpected CA ownership property length".into(),
        ));
    }
    let mut token = vec![0; length as usize];
    if unsafe {
        CertGetCertificateContextProperty(
            context.0,
            OWNER_PROPERTY,
            token.as_mut_ptr().cast(),
            &mut length,
        )
    } == 0
    {
        return Err(native_error("read CA ownership value"));
    }
    token.truncate(length as usize);
    Ok(Some(token))
}
pub(super) fn ownership(receipt: &CaInstallReceipt) -> LanpathResult<CaRemovalDisposition> {
    let store = Store::open(false)?;
    let Some(context) = store.find(&receipt.der)? else {
        return Ok(CaRemovalDisposition::AlreadyAbsent);
    };
    let token = owner(&context)?;
    Ok(removal_decision(
        receipt,
        Some(unsafe { encoded(context.0) }),
        token.as_deref(),
    ))
}
pub(super) fn remove(receipt: &CaInstallReceipt) -> LanpathResult<CaRemovalDisposition> {
    if receipt.scope != CaTrustScope::CurrentUserRoot {
        return Err(LanpathError::Mitm(
            "Refusing removal outside CurrentUser Root".into(),
        ));
    }
    if receipt.disposition != CaInstallDisposition::Added {
        return Ok(CaRemovalDisposition::PreservedPreexisting);
    }
    remove_from(&Store::open(true)?, receipt)
}
fn remove_from(store: &Store, receipt: &CaInstallReceipt) -> LanpathResult<CaRemovalDisposition> {
    let Some(context) = store.find(&receipt.der)? else {
        return Ok(CaRemovalDisposition::AlreadyAbsent);
    };
    let token = owner(&context)?;
    let disposition = removal_decision(
        receipt,
        Some(unsafe { encoded(context.0) }),
        token.as_deref(),
    );
    if disposition != CaRemovalDisposition::Removed {
        return Ok(disposition);
    }
    // API consumes context even on failure; prevent the RAII double-free.
    let pointer = context.0;
    std::mem::forget(context);
    if unsafe { CertDeleteCertificateFromStore(pointer) } == 0 {
        return Err(native_error("delete owned exact CA"));
    }
    Ok(CaRemovalDisposition::Removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CaStore;

    fn memory_store() -> Store {
        // Native ephemeral provider exercises real certificate/property APIs without adding trust.
        let handle = unsafe { CertOpenStore(CERT_STORE_PROV_MEMORY, 0, 0, 0, std::ptr::null()) };
        assert!(
            !handle.is_null(),
            "{}",
            native_error("open test memory store")
        );
        Store(handle)
    }
    fn der_fixture() -> Vec<u8> {
        let dir = tempfile::tempdir().unwrap();
        let material = crate::FileCaStore::new(dir.path())
            .load_or_generate()
            .unwrap();
        pem_to_der(&material.cert_pem).unwrap()
    }
    fn receipt(
        der: &[u8],
        disposition: CaInstallDisposition,
        owner_token: Option<Vec<u8>>,
    ) -> CaInstallReceipt {
        CaInstallReceipt {
            source_path: PathBuf::from("memory-test-ca.crt"),
            sha256: sha256_hex(der),
            scope: CaTrustScope::CurrentUserRoot,
            disposition,
            der: der.to_vec(),
            owner_token,
        }
    }
    #[test]
    fn native_memory_add_preserves_preexisting_and_removes_only_its_exact_root() {
        let store = memory_store();
        let first = der_fixture();
        let unrelated = der_fixture(); // Deliberately the same subject but a different key/DER.
        let (added, token) = install_in(&store, &first, add_context).unwrap();
        assert_eq!(added, CaInstallDisposition::Added);
        let own = receipt(&first, added, token);
        assert_eq!(
            owner(&store.find(&first).unwrap().unwrap()).unwrap(),
            own.owner_token
        );
        let (other_added, other_token) = install_in(&store, &unrelated, add_context).unwrap();
        assert_eq!(other_added, CaInstallDisposition::Added);
        let (existing, token) = install_in(&store, &first, |_, _| {
            panic!("must not mutate a preexisting certificate")
        })
        .unwrap();
        assert_eq!(existing, CaInstallDisposition::AlreadyPresent);
        assert!(token.is_none());
        assert_eq!(
            remove_from(&store, &receipt(&first, existing, token)).unwrap(),
            CaRemovalDisposition::PreservedPreexisting
        );
        assert!(store.find(&first).unwrap().is_some());
        assert_eq!(
            remove_from(&store, &own).unwrap(),
            CaRemovalDisposition::Removed
        );
        assert_eq!(
            remove_from(&store, &own).unwrap(),
            CaRemovalDisposition::AlreadyAbsent
        );
        assert!(store.find(&unrelated).unwrap().is_some());
        assert_eq!(
            remove_from(&store, &receipt(&unrelated, other_added, other_token)).unwrap(),
            CaRemovalDisposition::Removed
        );
    }
    #[test]
    fn native_memory_missing_or_replaced_ownership_never_removes_a_root() {
        for replacement in [None, Some(b"another installation".as_slice())] {
            let store = memory_store();
            let der = der_fixture();
            let (disposition, token) = install_in(&store, &der, add_context).unwrap();
            let own = receipt(&der, disposition, token);
            let context = store.find(&der).unwrap().unwrap();
            let mut bytes = replacement.unwrap_or_default().to_vec();
            let blob = CRYPT_INTEGER_BLOB {
                cbData: bytes.len() as u32,
                pbData: bytes.as_mut_ptr(),
            };
            let value = if replacement.is_some() {
                (&blob as *const CRYPT_INTEGER_BLOB).cast()
            } else {
                std::ptr::null()
            };
            assert_ne!(
                unsafe { CertSetCertificateContextProperty(context.0, OWNER_PROPERTY, 0, value) },
                0
            );
            assert_eq!(
                remove_from(&store, &own).unwrap(),
                CaRemovalDisposition::OwnershipChanged
            );
            assert!(store.find(&der).unwrap().is_some());
        }
    }
    #[test]
    fn native_memory_cancelled_add_leaves_prior_roots_untouched_and_no_receipt() {
        let store = memory_store();
        let original = der_fixture();
        let (disposition, token) = install_in(&store, &original, add_context).unwrap();
        let candidate = der_fixture();
        let result = install_in(&store, &candidate, |_, _| {
            Err(code_error(
                "add exact CA",
                windows_sys::Win32::Foundation::ERROR_CANCELLED,
            ))
        });
        assert!(result.unwrap_err().to_string().contains("0x000004c7"));
        assert!(store.find(&candidate).unwrap().is_none());
        assert_eq!(
            owner(&store.find(&original).unwrap().unwrap()).unwrap(),
            token
        );
        assert_eq!(disposition, CaInstallDisposition::Added);
    }
    #[test]
    fn native_memory_install_race_never_claims_the_winners_root() {
        let store = memory_store();
        let der = der_fixture();
        let (disposition, token) = install_in(&store, &der, |store, context| {
            assert!(add_context(store, context)?);
            Ok(false) // Another caller's CERT_STORE_ADD_NEW won before this caller committed.
        })
        .unwrap();
        assert_eq!(disposition, CaInstallDisposition::AlreadyPresent);
        assert!(token.is_none());
        assert_eq!(
            remove_from(&store, &receipt(&der, disposition, token)).unwrap(),
            CaRemovalDisposition::PreservedPreexisting
        );
        assert!(store.find(&der).unwrap().is_some());
    }
    #[test]
    fn native_memory_receipt_failure_prevents_the_trust_addition() {
        let store = memory_store();
        let der = der_fixture();
        let error = install_in_recorded(&store, &der, add_context, |_| {
            Err(LanpathError::Mitm(
                "protected receipt publication failed".into(),
            ))
        })
        .unwrap_err();
        assert!(error.to_string().contains("receipt publication failed"));
        assert!(store.find(&der).unwrap().is_none());
    }
    /// Parent must explicitly opt in on Windows. Never run as part of ambient unit tests.
    #[test]
    #[ignore = "explicit Windows CurrentUser Root temporary-certificate authorization required"]
    fn owned_current_user_root_roundtrip_preserves_preexisting_installation() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::FileCaStore::new(dir.path());
        store.load_or_generate().unwrap();
        let initial = query_ca(&store.cert_path()).unwrap();
        assert_eq!(initial.view.status, CaTrustStatus::NotTrusted);
        let added = install_ca_with_expected(&store.cert_path(), &initial.sha256).unwrap();
        assert_eq!(added.disposition, CaInstallDisposition::Added);
        // Ensure cleanup even if an assertion fails.
        let result = std::panic::catch_unwind(|| {
            assert_eq!(
                query_ca(&store.cert_path()).unwrap().view.status,
                CaTrustStatus::Trusted
            );
            let existing = install_ca(&store.cert_path()).unwrap();
            assert_eq!(existing.disposition, CaInstallDisposition::AlreadyPresent);
            assert_eq!(
                remove_ca(&existing).unwrap().disposition,
                CaRemovalDisposition::PreservedPreexisting
            );
            assert_eq!(
                query_ca(&store.cert_path()).unwrap().view.status,
                CaTrustStatus::Trusted
            );
        });
        assert_eq!(
            remove_ca(&added).unwrap().disposition,
            CaRemovalDisposition::Removed
        );
        assert_eq!(
            remove_ca(&added).unwrap().disposition,
            CaRemovalDisposition::AlreadyAbsent
        );
        assert_eq!(
            query_ca(&store.cert_path()).unwrap().view.status,
            CaTrustStatus::NotTrusted
        );
        result.unwrap();
    }
}
