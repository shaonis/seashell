use std::{
    fs,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use anyhow::Result;

use crate::error::FileError;

const CONFIG_FILENAME: &str = "config.yml";
const CACHE_FILENAME: &str = ".cache.json";

pub static WORK_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    directories::BaseDirs::new()
        .expect("Must be valid home directory")
        .home_dir()
        .join(format!(".{}", env!("CARGO_PKG_NAME")))
});

pub static CONFIG_PATH: LazyLock<Box<str>> = LazyLock::new(|| {
    WORK_DIR
        .join(CONFIG_FILENAME)
        .to_str()
        .expect("Config path must be valid UTF-8")
        .into()
});

pub static CACHE_PATH: LazyLock<Box<str>> = LazyLock::new(|| {
    WORK_DIR
        .join(CACHE_FILENAME)
        .to_str()
        .expect("Cache path must be valid UTF-8")
        .into()
});

pub trait StorageProvider: Default {
    fn work_file() -> &'static LazyLock<Box<str>>;
    fn serialize(&self) -> Result<String>;
    fn deserialize(data: &str) -> Result<Self>;

    fn save_to_file(&self) -> Result<()> {
        let file_path = &***Self::work_file();
        if !Path::new(file_path).exists() {
            ensure_work_dir()?;
        }
        let data = self.serialize()?;
        fs::write(file_path, data).map_err(FileError::Std)?;

        Ok(())
    }

    fn load_from_file() -> Result<Self> {
        let file_path = &***Self::work_file();
        if !Path::new(file_path).exists() {
            let self_default = Self::default();
            Self::save_to_file(&self_default)?;
            return Ok(self_default);
        }
        let content = fs::read_to_string(file_path).map_err(FileError::Std)?;
        let obj = Self::deserialize(&content)?;

        Ok(obj)
    }
}

pub fn get_full_path(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        return path;
    }

    directories::BaseDirs::new()
        .expect("should be valid home directory")
        .home_dir()
        .join(
            path.strip_prefix("~/")
                .expect("Each path in the configuration must be absolute or begin with ~/"),
        )
}

pub fn ensure_work_dir() -> Result<()> {
    fs::create_dir_all(&*WORK_DIR)?;

    Ok(())
}
