//! Validated, recoverable CA identity storage. Loading never rotates an identity or installs trust.
use lanpath_common::error::{LanpathError, LanpathResult};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub(crate) mod protection;
#[cfg(test)]
mod tests;
pub(crate) mod validation;
pub use validation::validate_ca_material;

#[derive(Clone, PartialEq, Eq)]
pub struct CaMaterial {
    pub cert_pem: String,
    pub key_pem: String,
}
impl std::fmt::Debug for CaMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaMaterial")
            .field("cert_pem", &self.cert_pem)
            .field("key_pem", &"[REDACTED]")
            .finish()
    }
}
pub trait CaStore: Send + Sync {
    /// Generate only when no identity or interrupted transaction exists.
    fn load_or_generate(&self) -> LanpathResult<CaMaterial>;
}
#[derive(Debug, Clone)]
pub struct FileCaStore {
    dir: PathBuf,
}
impl FileCaStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }
    pub fn cert_path(&self) -> PathBuf {
        self.dir.join("ca.crt")
    }
    pub fn key_path(&self) -> PathBuf {
        self.dir.join("ca.key")
    }
    /// Read an existing complete identity without generating or recovering another one.
    pub fn load_existing(&self) -> LanpathResult<CaMaterial> {
        require_directory(&self.dir)?;
        protection::protect(&self.dir, true)?;
        load_material(&self.cert_path(), &self.key_path())
    }
}
fn error(message: impl Into<String>) -> LanpathError {
    LanpathError::Mitm(message.into())
}
fn io_error(operation: &str, e: std::io::Error) -> LanpathError {
    error(format!("CA {operation}: {e}"))
}
pub(crate) fn exists(path: &Path) -> LanpathResult<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            Err(error("CA paths must not be symbolic links"))
        }
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(io_error("inspect path", e)),
    }
}
pub(crate) fn require_directory(path: &Path) -> LanpathResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(|e| io_error("inspect directory", e))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(error(
            "CA storage and transaction paths must be real directories",
        ));
    }
    Ok(())
}
pub(crate) fn require_file(path: &Path) -> LanpathResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(|e| io_error("inspect material", e))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(error(
            "CA certificate, private key and lock must be regular files",
        ));
    }
    Ok(())
}
impl CaStore for FileCaStore {
    fn load_or_generate(&self) -> LanpathResult<CaMaterial> {
        if !exists(&self.dir)? {
            fs::create_dir_all(&self.dir).map_err(|e| io_error("create directory", e))?;
        }
        require_directory(&self.dir)?;
        // Restrict before creating any private material, including portable storage.
        protection::protect(&self.dir, true)?;
        let lock_path = self.dir.join(".ca.lock");
        if exists(&lock_path)? {
            require_file(&lock_path)?;
        }
        let lock = private_options()
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| io_error("open process lock", e))?;
        protection::protect(&lock_path, false)?;
        fs2::FileExt::lock_exclusive(&lock).map_err(|e| io_error("lock identity", e))?;
        let transaction = self.dir.join(".ca-transaction");
        let staging = self.dir.join(".ca-staging");
        if exists(&staging)? {
            require_directory(&staging)?;
            if exists(&transaction)? {
                return Err(error(
                    "Multiple CA transactions preserved; explicit recovery required",
                ));
            }
            // A crash after the second file flush but before rename can recover the exact pair.
            load_material(&staging.join("ca.crt"), &staging.join("ca.key"))?;
            fs::rename(&staging, &transaction)
                .map_err(|e| io_error("recover staged identity", e))?;
            sync_directory(&self.dir)?;
        }
        if exists(&transaction)? {
            require_directory(&transaction)?;
            publish_transaction(&self.dir, &transaction)?;
        }
        let cert_exists = exists(&self.cert_path())?;
        let key_exists = exists(&self.key_path())?;
        if cert_exists != key_exists {
            if key_exists {
                require_file(&self.key_path())?;
                protection::protect(&self.key_path(), false)?;
            }
            return Err(error(
                "CA identity is incomplete; restore the original matching certificate AND private key. No replacement was generated.",
            ));
        }
        if cert_exists {
            return load_material(&self.cert_path(), &self.key_path());
        }
        for entry in
            fs::read_dir(&self.dir).map_err(|e| io_error("inspect recovery journals", e))?
        {
            let entry = entry.map_err(|e| io_error("inspect recovery journal", e))?;
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with(".ca-retired-")
            {
                return Err(error(
                    "An earlier CA recovery journal remains; restore its original identity explicitly. No replacement was generated.",
                ));
            }
        }
        let material = generate_ca_material()?;
        fs::create_dir(&staging).map_err(|e| io_error("create staging directory", e))?;
        protection::protect(&staging, true)?;
        write_private(&staging.join("ca.crt"), &material.cert_pem)?;
        write_private(&staging.join("ca.key"), &material.key_pem)?;
        sync_directory(&staging)?;
        // This rename is the commit point: only a complete validated pair becomes recoverable.
        fs::rename(&staging, &transaction)
            .map_err(|e| io_error("commit identity transaction", e))?;
        sync_directory(&self.dir)?;
        publish_transaction(&self.dir, &transaction)?;
        load_material(&self.cert_path(), &self.key_path())
    }
}
fn publish_transaction(dir: &Path, transaction: &Path) -> LanpathResult<()> {
    let material = load_material(&transaction.join("ca.crt"), &transaction.join("ca.key"))?;
    // Check *both* existing destinations before publishing either. Never overwrite user bytes.
    for (name, expected) in [
        ("ca.crt", &material.cert_pem),
        ("ca.key", &material.key_pem),
    ] {
        let destination = dir.join(name);
        if exists(&destination)?
            && fs::read(&destination).map_err(|e| io_error("read destination", e))?
                != expected.as_bytes()
        {
            return Err(error(
                "CA transaction conflicts with existing identity; all material preserved for explicit recovery",
            ));
        }
    }
    for name in ["ca.crt", "ca.key"] {
        let destination = dir.join(name);
        if !exists(&destination)? {
            // Atomic no-clobber publication. Hard links keep the journal available through any crash.
            fs::hard_link(transaction.join(name), destination).map_err(|e| {
                io_error("publish identity (filesystem must support hard links)", e)
            })?;
        }
    }
    protection::protect(&dir.join("ca.key"), false)?;
    sync_directory(dir)?;
    // Atomic retirement prevents a crash during cleanup from leaving a half journal.
    let retired = dir.join(format!(".ca-retired-{}", uuid::Uuid::new_v4()));
    fs::rename(transaction, &retired).map_err(|e| io_error("retire transaction", e))?;
    sync_directory(dir)?;
    fs::remove_dir_all(retired).map_err(|e| io_error("remove retired transaction", e))?;
    Ok(())
}
fn load_material(cert: &Path, key: &Path) -> LanpathResult<CaMaterial> {
    require_file(key)?;
    // Invalid/expired and incomplete material is preserved too, but its key is still protected.
    protection::protect(key, false)?;
    require_file(cert)?;
    let material = CaMaterial {
        cert_pem: fs::read_to_string(cert).map_err(|e| io_error("read certificate", e))?,
        key_pem: fs::read_to_string(key).map_err(|e| io_error("read private key", e))?,
    };
    validate_ca_material(&material)?;
    Ok(material)
}
pub(crate) fn private_options() -> fs::OpenOptions {
    let mut options = fs::OpenOptions::new();
    options.write(true).read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}
pub(crate) fn write_private(path: &Path, contents: &str) -> LanpathResult<()> {
    let mut file = private_options()
        .create_new(true)
        .open(path)
        .map_err(|e| io_error("create private file", e))?;
    protection::protect(path, false)?;
    file.write_all(contents.as_bytes())
        .map_err(|e| io_error("write private file", e))?;
    file.sync_all()
        .map_err(|e| io_error("sync private file", e))
}
pub(crate) fn sync_directory(path: &Path) -> LanpathResult<()> {
    #[cfg(unix)]
    {
        fs::File::open(path)
            .and_then(|file| file.sync_all())
            .map_err(|e| io_error("sync directory", e))?;
    }
    // Windows MoveFile/NTFS journal recovery is completed on next load; individual files are flushed.
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
fn ca_params() -> LanpathResult<rcgen::CertificateParams> {
    let mut params = rcgen::CertificateParams::new(Vec::<String>::new())
        .map_err(|e| error(format!("CA parameters: {e}")))?;
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "Lanpath MITM CA");
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params.key_usages = vec![
        rcgen::KeyUsagePurpose::KeyCertSign,
        rcgen::KeyUsagePurpose::CrlSign,
        rcgen::KeyUsagePurpose::DigitalSignature,
    ];
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now - time::Duration::days(1);
    params.not_after = now + time::Duration::days(3650);
    Ok(params)
}
fn generate_ca_material() -> LanpathResult<CaMaterial> {
    let key = rcgen::KeyPair::generate().map_err(|e| error(format!("CA key generation: {e}")))?;
    let cert = ca_params()?
        .self_signed(&key)
        .map_err(|e| error(format!("CA signing: {e}")))?;
    let material = CaMaterial {
        cert_pem: cert.pem(),
        key_pem: key.serialize_pem(),
    };
    validate_ca_material(&material)?;
    Ok(material)
}
