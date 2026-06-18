use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::Result;

#[derive(Debug, Clone)]
pub struct FileConflict {
    pub path: String,
    pub owned_by: String,
}

pub fn check_file_conflicts(
    new_files: &[PathBuf],
    new_pkg_name: &str,
    installed: &[(String, Vec<PathBuf>)],
) -> Result<Vec<FileConflict>> {
    let mut file_map: HashMap<String, String> = HashMap::new();

    for (pkg_name, files) in installed {
        for file in files {
            let path_str = file.to_string_lossy().to_string();
            file_map.entry(path_str).or_insert_with(|| pkg_name.clone());
        }
    }

    let mut conflicts = Vec::new();
    for file in new_files {
        let path_str = file.to_string_lossy().to_string();
        if let Some(owner) = file_map.get(&path_str) {
            if owner != new_pkg_name {
                conflicts.push(FileConflict {
                    path: path_str,
                    owned_by: owner.clone(),
                });
            }
        }
    }

    Ok(conflicts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_file_conflict() {
        let new_files = vec![PathBuf::from("usr/bin/foo"), PathBuf::from("usr/bin/bar")];
        let installed = vec![
            ("pkg-a".into(), vec![PathBuf::from("usr/bin/foo"), PathBuf::from("usr/bin/baz")]),
        ];

        let conflicts = check_file_conflicts(&new_files, "pkg-b", &installed).unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].path, "usr/bin/foo");
        assert_eq!(conflicts[0].owned_by, "pkg-a");
    }

    #[test]
    fn no_conflict_same_package() {
        let new_files = vec![PathBuf::from("usr/bin/foo")];
        let installed = vec![
            ("pkg-a".into(), vec![PathBuf::from("usr/bin/foo")]),
        ];

        let conflicts = check_file_conflicts(&new_files, "pkg-a", &installed).unwrap();
        assert!(conflicts.is_empty());
    }

    #[test]
    fn no_conflict_different_files() {
        let new_files = vec![PathBuf::from("usr/bin/new")];
        let installed = vec![
            ("pkg-a".into(), vec![PathBuf::from("usr/bin/old")]),
        ];

        let conflicts = check_file_conflicts(&new_files, "pkg-b", &installed).unwrap();
        assert!(conflicts.is_empty());
    }
}
