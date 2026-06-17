use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use tar::Archive as TarArchive;

use crate::core::pkginfo::PackageInfo;
use crate::error::{BulbError, Result};

use super::buildinfo::BuildInfo;
use super::convert::package_info_from_pkginfo;
use super::install_script::InstallScript;
use super::pkginfo::PkgInfo;

pub struct PkgFileReader {
    pub info: PackageInfo,
    pub buildinfo: Option<BuildInfo>,
    pub install_script: Option<InstallScript>,
    pub entries: Vec<PathBuf>,
}

pub fn read_pkg_zst(path: &Path) -> Result<PkgFileReader> {
    let file = File::open(path)?;
    let decoder = zstd::stream::Decoder::new(file)?;
    read_pkg_tar(decoder, path)
}

pub fn read_pkg_tarzst(path: &Path) -> Result<PkgFileReader> {
    let file = File::open(path)?;
    let decoder = zstd::stream::Decoder::new(file)?;
    read_pkg_tar(decoder, path)
}

fn read_pkg_tar<R: Read>(reader: R, path: &Path) -> Result<PkgFileReader> {
    let mut archive = TarArchive::new(reader);

    let mut pkginfo_text = None;
    let mut buildinfo_text = None;
    let mut install_text = None;
    let mut entries = Vec::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?.into_owned();
        let file_name = entry_path.file_name().and_then(|n| n.to_str());

        match file_name {
            Some(".PKGINFO") => {
                let mut text = String::new();
                entry.read_to_string(&mut text)?;
                pkginfo_text = Some(text);
            }
            Some(".BUILDINFO") => {
                let mut text = String::new();
                entry.read_to_string(&mut text)?;
                buildinfo_text = Some(text);
            }
            Some("install") => {
                let mut text = String::new();
                entry.read_to_string(&mut text)?;
                install_text = Some(text);
            }
            _ => {
                if let Some(name) = file_name {
                    if !name.is_empty() {
                        entries.push(entry_path);
                    }
                }
            }
        }
    }

    let pkginfo_text = pkginfo_text.ok_or_else(|| {
        BulbError::InvalidMetadata(format!(
            "{}: missing .PKGINFO",
            path.display()
        ))
    })?;

    let pkginfo = PkgInfo::parse(&pkginfo_text);
    let info = package_info_from_pkginfo(&pkginfo);
    let buildinfo = buildinfo_text.and_then(|t| BuildInfo::parse(&t).ok());
    let install_script = install_text.and_then(|t| InstallScript::parse(&t));

    Ok(PkgFileReader {
        info,
        buildinfo,
        install_script,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sample_pkg() {
        let cache = Path::new("/var/cache/pacman/pkg");
        if !cache.is_dir() {
            eprintln!("skipping: no pacman cache");
            return;
        }
        for entry in std::fs::read_dir(cache).unwrap().flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.ends_with(".pkg.tar.zst") {
                let result = read_pkg_zst(&entry.path());
                match result {
                    Ok(pkg) => {
                        assert!(!pkg.info.name.is_empty());
                        return;
                    }
                    Err(_) => continue,
                }
            }
        }
        eprintln!("skipping: no .pkg.tar.zst files found");
    }
}
