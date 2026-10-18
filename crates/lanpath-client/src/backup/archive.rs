use super::*;
use age::secrecy::SecretString;
use lanpath_common::LanpathResult;
use rand::{Rng, distr::Alphanumeric};
use std::io::{Cursor, Read, Write};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

static CRYPTO_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn generate_password() -> String {
    rand::rng()
        .sample_iter(Alphanumeric)
        .take(24)
        .map(char::from)
        .collect()
}

pub fn encode(snapshot: &Snapshot, password: &str) -> LanpathResult<Vec<u8>> {
    if password.chars().count() < 12 || password.len() > 1024 {
        return Err(invalid(
            "密码长度不符合要求：至少 12 个字符；过长时请使用随机生成的密码",
        ));
    }
    check_size(&snapshot.files)?;
    if snapshot.files.values().map(Vec::len).sum::<usize>() > MAX_EXPANDED - 65536 {
        return Err(invalid("备份内容过大，需要为备份目录保留空间"));
    }
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(3))
        .unix_permissions(0o600);
    zip.start_file("manifest.json", options)
        .map_err(|_| invalid("无法创建备份目录"))?;
    zip.write_all(&serde_json::to_vec(&snapshot.summary)?)?;
    for (name, bytes) in &snapshot.files {
        if !allowed(name) {
            return Err(invalid("备份包含不支持的文件"));
        }
        zip.start_file(name, options)
            .map_err(|_| invalid("无法压缩备份文件"))?;
        zip.write_all(bytes)?;
    }
    let compressed = zip
        .finish()
        .map_err(|_| invalid("无法完成压缩"))?
        .into_inner();
    if compressed.len() > MAX_ARCHIVE - 8192 {
        return Err(invalid("压缩后的备份超过 16 MiB"));
    }
    let _guard = CRYPTO_LOCK
        .lock()
        .map_err(|_| invalid("加密服务暂不可用，请重试"))?;
    let mut recipient = age::scrypt::Recipient::new(SecretString::from(password.to_owned()));
    recipient.set_work_factor(17);
    let encryptor =
        age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
            .map_err(|_| invalid("无法初始化备份加密"))?;
    let mut encrypted = Vec::new();
    let mut writer = encryptor
        .wrap_output(&mut encrypted)
        .map_err(|_| invalid("无法加密备份"))?;
    writer.write_all(&compressed)?;
    writer.finish()?;
    if encrypted.len() > MAX_ARCHIVE {
        return Err(invalid("加密备份超过 16 MiB"));
    }
    Ok(encrypted)
}

pub fn decode(encrypted: &[u8], password: &str) -> LanpathResult<Snapshot> {
    if encrypted.len() > MAX_ARCHIVE || password.len() > 1024 {
        return Err(invalid("备份文件或密码超过限制"));
    }
    let _guard = CRYPTO_LOCK
        .lock()
        .map_err(|_| invalid("解密服务暂不可用"))?;
    let decryptor =
        age::Decryptor::new(encrypted).map_err(|_| invalid("不是受支持的加密备份文件"))?;
    let mut identity = age::scrypt::Identity::new(SecretString::from(password.to_owned()));
    identity.set_max_work_factor(17);
    let reader = decryptor
        .decrypt(std::iter::once(&identity as &dyn age::Identity))
        .map_err(|_| invalid("密码错误、备份损坏或加密参数不受支持"))?;
    let mut compressed = Vec::new();
    reader
        .take((MAX_ARCHIVE + 1) as u64)
        .read_to_end(&mut compressed)
        .map_err(|_| invalid("密码错误或备份已损坏"))?;
    if compressed.len() > MAX_ARCHIVE {
        return Err(invalid("备份压缩体超过限制"));
    }
    unpack(compressed)
}

pub(super) fn unpack(compressed: Vec<u8>) -> LanpathResult<Snapshot> {
    let mut archive =
        ZipArchive::new(Cursor::new(compressed)).map_err(|_| invalid("备份压缩体无效"))?;
    if archive.len() > MAX_FILES + 1 {
        return Err(invalid("备份文件数量超过限制"));
    }
    let mut files = BTreeMap::new();
    let mut manifest = None;
    let mut expanded = 0;
    for i in 0..archive.len() {
        let file = archive
            .by_index(i)
            .map_err(|_| invalid("无法读取备份条目"))?;
        let name = file.name().to_owned();
        if file.is_dir()
            || file.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
            || (name != "manifest.json" && !allowed(&name))
        {
            return Err(invalid("备份包含非法路径、链接或不支持的条目"));
        }
        if file.size() > (MAX_EXPANDED - expanded) as u64 {
            return Err(invalid("解压数据超过 64 MiB"));
        }
        let mut bytes = Vec::new();
        file.take((MAX_EXPANDED - expanded + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("备份条目已损坏"))?;
        expanded += bytes.len();
        if expanded > MAX_EXPANDED {
            return Err(invalid("解压数据超过 64 MiB"));
        }
        if name == "manifest.json" {
            if manifest.is_some() {
                return Err(invalid("备份目录重复"));
            }
            manifest = Some(serde_json::from_slice::<BackupSummary>(&bytes)?);
        } else if files.insert(name, bytes).is_some() {
            return Err(invalid("备份条目重复"));
        }
    }
    let summary = manifest.ok_or_else(|| invalid("缺少备份目录"))?;
    if !matches!(summary.version, 1 | 2) {
        return Err(invalid("备份版本不受支持，请更新客户端"));
    }
    let mut snapshot = Snapshot { summary, files };
    snapshot::validate(&mut snapshot)?;
    Ok(snapshot)
}
