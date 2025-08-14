use std::{
    cell::LazyCell,
    fs,
    path::{Path, PathBuf}
};

use anyhow::Result;

use crate::error::FileError;


pub const WORK_DIR: LazyCell<PathBuf> = LazyCell::new(|| {
    directories::BaseDirs::new()
        .expect("should be valid home directory")
        .home_dir()
        .join(format!(".{}", env!("CARGO_PKG_NAME")))
});

pub const CONFIG_PATH: LazyCell<Box<str>> = LazyCell::new(|| {
    WORK_DIR
        .join("config.yml")
        .to_str()
        .expect("config path must be valid UTF-8")
        .into()
});

pub const CACHE_PATH: LazyCell<Box<str>> = LazyCell::new(|| {
    WORK_DIR
        .join(".cache.json")
        .to_str()
        .expect("cache path must be valid UTF-8")
        .into()
});

pub trait StorageProvider: Default {
    const WORK_FILE: LazyCell<Box<str>>;

    fn serialize(&self) -> Result<String>;
    fn deserialize(data: &str) -> Result<Self>;

    fn save_to_file(&self) -> Result<()> {
        let file_path = &**Self::WORK_FILE;
        if !Path::new(file_path).exists() {
            ensure_work_dir()?;
        }
        let data = self.serialize()?;
        fs::write(file_path, data).map_err(FileError::Std)?;

        Ok(())
    }

    fn load_from_file() -> Result<Self> {
        let file_path = &**Self::WORK_FILE;
        if !Path::new(file_path).exists() {
            return Ok(Self::default())
        }
        let content = fs::read_to_string(file_path).map_err(FileError::Std)?;
        let obj = Self::deserialize(&content)?;

        Ok(obj)
    }
}

pub fn get_full_path(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        return path
    }
    if path.starts_with("~/") {
        return directories::BaseDirs::new()
            .expect("should be valid home directory")
            .home_dir()
            .join(path.strip_prefix("~/").unwrap())
    }

    panic!("Path should be absolute or start with ~/")
}

pub fn ensure_work_dir() -> Result<()> {
    fs::create_dir_all(&*WORK_DIR)?;

    Ok(())
}
