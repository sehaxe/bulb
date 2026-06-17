use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;

use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MtreeEntryType {
    File,
    Dir,
    Symlink { target: PathBuf },
    Link { target: PathBuf },
}

#[derive(Debug, Clone)]
pub struct FileMeta {
    pub path: PathBuf,
    pub entry_type: MtreeEntryType,
    pub mode: Option<u32>,
    pub sha256: Option<String>,
    pub size: Option<u64>,
}

pub fn read_mtree(mtree_path: &Path) -> Result<Vec<FileMeta>> {
    let file = File::open(mtree_path)?;
    let gz = GzDecoder::new(file);
    let mut buf = String::new();
    let mut reader = std::io::BufReader::new(gz);
    reader.read_to_string(&mut buf)?;
    parse_mtree(&buf)
}

pub fn parse_mtree(text: &str) -> Result<Vec<FileMeta>> {
    let mut entries = Vec::new();
    let mut current_dir = PathBuf::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line == "/set" || line.starts_with("type=") {
            continue;
        }

        if line.starts_with('.') && line.contains(" type=") {
            continue;
        }

        if !line.starts_with('/') && !line.starts_with('.') && !line.starts_with("type=") {
            if let Some(dir) = parse_dir_line(line) {
                current_dir = dir;
            }
            continue;
        }

        if let Some(meta) = parse_entry_line(line, &current_dir) {
            entries.push(meta);
        }
    }

    Ok(entries)
}

fn parse_dir_line(line: &str) -> Option<PathBuf> {
    let line = line.trim_end_matches('/');
    if line.is_empty() || line == "." {
        return Some(PathBuf::new());
    }
    Some(PathBuf::from(line))
}

fn parse_entry_line(line: &str, base_dir: &Path) -> Option<FileMeta> {
    let mut parts = line.split_whitespace();
    let name = parts.next()?;
    let path = if name.starts_with('/') || name.starts_with('.') {
        PathBuf::from(name)
    } else {
        base_dir.join(name)
    };

    let mut entry_type = MtreeEntryType::File;
    let mut mode = None;
    let mut sha256 = None;
    let mut size = None;

    for part in parts {
        if let Some((key, val)) = part.split_once('=') {
            match key {
                "type" => {
                    entry_type = match val {
                        "file" => MtreeEntryType::File,
                        "dir" => MtreeEntryType::Dir,
                        "link" => MtreeEntryType::Symlink {
                            target: PathBuf::new(),
                        },
                        "hardlink" => MtreeEntryType::Link {
                            target: PathBuf::new(),
                        },
                        _ => MtreeEntryType::File,
                    };
                }
                "mode" => {
                    mode = u32::from_str_radix(val, 8).ok();
                }
                "sha256digest" | "sha256" => {
                    sha256 = Some(val.to_string());
                }
                "size" => {
                    size = val.parse().ok();
                }
                _ => {}
            }
        }
    }

    Some(FileMeta {
        path,
        entry_type,
        mode,
        sha256,
        size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_real_mtree() {
        let local = Path::new("/var/lib/pacman/local");
        if !local.is_dir() {
            eprintln!("skipping: no local DB");
            return;
        }
        for entry in std::fs::read_dir(local).unwrap().flatten() {
            let mtree = entry.path().join("mtree");
            if mtree.exists() {
                let entries = read_mtree(&mtree);
                if let Ok(entries) = entries {
                    assert!(!entries.is_empty(), "mtree should have entries");
                    return;
                }
            }
        }
        eprintln!("skipping: no mtree files found");
    }
}
