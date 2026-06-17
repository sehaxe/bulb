use std::fs;
use std::path::{Path, PathBuf};

use crate::core::pkginfo::PackageInfo;
use crate::error::Result;

use super::convert::package_info_from_desc;
use super::desc::Desc;

#[derive(Debug, Clone)]
pub struct LocalEntry {
    pub info: PackageInfo,
    pub files: Vec<PathBuf>,
    pub backup: Vec<String>,
}

pub fn read_local_db(db_path: &Path) -> Result<Vec<LocalEntry>> {
    let mut entries = Vec::new();
    if !db_path.is_dir() {
        return Ok(entries);
    }

    for dir_entry in fs::read_dir(db_path)? {
        let dir_entry = dir_entry?;
        if !dir_entry.file_type()?.is_dir() {
            continue;
        }
        let pkg_dir = dir_entry.path();
        let desc_path = pkg_dir.join("desc");
        if !desc_path.exists() {
            continue;
        }

        let desc_text = fs::read_to_string(&desc_path)?;
        let desc = Desc::parse(&desc_text);
        if desc.get("name").is_none() {
            continue;
        }

        let files = read_files_list(&pkg_dir);
        let backup = read_backup_list(&pkg_dir);
        let mut info = package_info_from_desc(&desc, None);
        info.installdate = desc.get("installdate").and_then(|s| s.parse().ok());

        entries.push(LocalEntry {
            info,
            files,
            backup,
        });
    }

    Ok(entries)
}

pub fn read_local_package(pkg_dir: &Path) -> Result<Option<LocalEntry>> {
    let desc_path = pkg_dir.join("desc");
    if !desc_path.exists() {
        return Ok(None);
    }

    let desc_text = fs::read_to_string(&desc_path)?;
    let desc = Desc::parse(&desc_text);
    if desc.get("name").is_none() {
        return Ok(None);
    }

    let files = read_files_list(pkg_dir);
    let backup = read_backup_list(pkg_dir);
    let mut info = package_info_from_desc(&desc, None);
    info.installdate = desc.get("installdate").and_then(|s| s.parse().ok());

    Ok(Some(LocalEntry {
        info,
        files,
        backup,
    }))
}

fn read_files_list(pkg_dir: &Path) -> Vec<PathBuf> {
    let files_path = pkg_dir.join("files");
    let Ok(text) = fs::read_to_string(files_path) else {
        return Vec::new();
    };

    let mut files = Vec::new();
    let mut in_files = false;
    for line in text.lines() {
        if line.trim() == "%FILES%" {
            in_files = true;
            continue;
        }
        if in_files && !line.trim().is_empty() {
            files.push(PathBuf::from(line.trim()));
        }
    }
    files
}

fn read_backup_list(pkg_dir: &Path) -> Vec<String> {
    let files_path = pkg_dir.join("files");
    let Ok(text) = fs::read_to_string(files_path) else {
        return Vec::new();
    };

    let mut backup = Vec::new();
    let mut in_backup = false;
    for line in text.lines() {
        if line.trim() == "%BACKUP%" {
            in_backup = true;
            continue;
        }
        if in_backup && !line.trim().is_empty() {
            backup.push(line.trim().to_string());
        }
    }
    backup
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_local_db_if_present() {
        let path = Path::new("/var/lib/pacman/local");
        if !path.exists() {
            eprintln!("skipping: no local DB");
            return;
        }
        let entries = read_local_db(path).unwrap();
        assert!(!entries.is_empty(), "local DB should have packages");
        for e in &entries {
            assert!(!e.info.name.is_empty());
        }
    }
}
