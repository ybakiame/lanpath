use super::{check_open_file, check_regular_file, file_options, invalid};
use lanpath_common::LanpathResult;
use std::{
    io::{Read, Write},
    path::Path,
};
use uuid::Uuid;

const FILE: &str = ".lanpath-root-id";

pub(super) fn load_or_create(root: &Path) -> LanpathResult<Uuid> {
    let path = root.join(FILE);
    match std::fs::symlink_metadata(&path) {
        Ok(_) => {
            if check_regular_file(&path)?.len() != 37 {
                return Err(invalid("数据根身份损坏；不得静默重新生成"));
            }
            let file = file_options().read(true).open(&path)?;
            check_open_file(&file, &path)?;
            let mut text = String::with_capacity(37);
            file.take(38).read_to_string(&mut text)?;
            let text = text
                .strip_suffix('\n')
                .filter(|text| text.len() == 36)
                .ok_or_else(|| invalid("数据根身份编码无效"))?;
            let id = Uuid::parse_str(text).map_err(|_| invalid("数据根身份无效"))?;
            if id.is_nil() {
                return Err(invalid("数据根身份不能为 nil"));
            }
            Ok(id)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // 调用方持有根目录租约。身份不属于配置导入/备份的一部分：
            // 导入其他根不得收养其缓存所有者。
            let id = Uuid::new_v4();
            let mut file = file_options().write(true).create_new(true).open(&path)?;
            let mut bytes = [0u8; 36];
            file.write_all(id.hyphenated().encode_lower(&mut bytes).as_bytes())?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            Ok(id)
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_root::DataRoot;

    #[test]
    fn owner_identity_survives_reopen_and_directory_moves() {
        let parent = tempfile::tempdir().unwrap();
        let first = parent.path().join("first");
        let root = DataRoot::open(&first).unwrap();
        let id = root.id();
        drop(root);
        assert_eq!(DataRoot::open(&first).unwrap().id(), id);
        let moved = parent.path().join("moved");
        std::fs::rename(first, &moved).unwrap();
        assert_eq!(DataRoot::open(&moved).unwrap().id(), id);
        assert_ne!(
            DataRoot::open(&parent.path().join("independent"))
                .unwrap()
                .id(),
            id
        );
    }

    #[test]
    fn corrupt_identity_is_preserved_and_cannot_be_replaced_with_another_owner() {
        let parent = tempfile::tempdir().unwrap();
        let root = DataRoot::open(parent.path()).unwrap();
        drop(root);
        let file = parent.path().join(FILE);
        std::fs::write(&file, b"damaged identity").unwrap();
        assert!(DataRoot::open(parent.path()).is_err());
        assert_eq!(std::fs::read(file).unwrap(), b"damaged identity");
    }
}
