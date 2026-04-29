use std::path::{Path, PathBuf};

use crate::error::Result;

pub fn absolutize_path(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(normalize_path(&absolute))
}

pub fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        normalized.push(component);
    }
    normalized
}
