use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use bzip3::read::Bz3Decoder;
use tar::Archive;

use crate::core::dependency::{Depend, Provide};
use crate::core::pkginfo::PackageInfo;
use crate::error::{BulbError, Result};

use super::manifest::BuildManifest;

pub struct NativePkgFile {
    pub info: PackageInfo,
    pub entries: Vec<PathBuf>,
}

pub fn bzip3_decoder<R: Read>(reader: R) -> std::result::Result<Bz3Decoder<R>, bzip3::Error> {
    Ok(Bz3Decoder::new(reader)?)
}

pub fn read_native_pkg(path: &Path) -> Result<NativePkgFile> {
    let file = File::open(path)?;
    let decoder = bzip3_decoder(file)?;
    let mut archive = Archive::new(decoder);

    let mut pkginfo_text = None;
    let mut entries = Vec::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?.into_owned();
        let file_name = entry_path.file_name().and_then(|n| n.to_str());

        if file_name == Some(".PKGINFO") {
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            pkginfo_text = Some(text);
        } else if let Some(name) = file_name {
            if !name.is_empty() {
                entries.push(entry_path);
            }
        }
    }

    let pkginfo_text = pkginfo_text.ok_or_else(|| {
        BulbError::InvalidMetadata(format!(
            "{}: missing .PKGINFO",
            path.display()
        ))
    })?;

    let info = parse_native_pkginfo(&pkginfo_text)?;

    Ok(NativePkgFile { info, entries })
}

fn parse_native_pkginfo(text: &str) -> Result<PackageInfo> {
    let mut fields = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            fields
                .entry(key.trim().to_string())
                .or_insert_with(Vec::new)
                .push(value.trim().to_string());
        }
    }

    let get = |key: &str| fields.get(key).and_then(|v| v.first()).cloned();

    Ok(PackageInfo {
        name: get("pkgname")
            .ok_or_else(|| BulbError::InvalidMetadata("missing pkgname".into()))?,
        base: get("pkgbase"),
        version: get("pkgver")
            .ok_or_else(|| BulbError::InvalidMetadata("missing pkgver".into()))?,
        arch: get("arch")
            .ok_or_else(|| BulbError::InvalidMetadata("missing arch".into()))?,
        description: get("pkgdesc"),
        url: get("url"),
        packager: get("packager"),
        builddate: get("builddate").and_then(|v| v.parse().ok()),
        installdate: None,
        license: fields
            .get("license")
            .cloned()
            .unwrap_or_default(),
        groups: fields
            .get("groups")
            .cloned()
            .unwrap_or_default(),
        depends: fields
            .get("depend")
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|v| Depend::parse(v))
            .collect(),
        optdepends: fields
            .get("optdepend")
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|v| Depend::parse(v))
            .collect(),
        makedepends: Vec::new(),
        checkdepends: Vec::new(),
        provides: fields
            .get("provides")
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|v| Provide::parse(v))
            .collect(),
        conflicts: fields
            .get("conflict")
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|v| Depend::parse(v))
            .collect(),
        replaces: fields
            .get("replaces")
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|v| Depend::parse(v))
            .collect(),
        backup: fields
            .get("backup")
            .cloned()
            .unwrap_or_default(),
        size: get("size").and_then(|v| v.parse().ok()),
        source: crate::core::pkginfo::PackageSource::Native { commit: None },
    })
}

pub fn manifest_to_pkginfo(manifest: &BuildManifest) -> PackageInfo {
    let pkg = &manifest.package;
    PackageInfo {
        name: pkg.name.clone(),
        base: None,
        version: format!("{}-{}", pkg.version, pkg.release),
        arch: pkg.arch.clone(),
        description: pkg.desc.clone(),
        url: pkg.url.clone(),
        packager: pkg.packager.clone(),
        builddate: None,
        installdate: None,
        license: pkg.license.clone(),
        groups: Vec::new(),
        depends: pkg.depends.iter().map(|v| Depend::parse(v)).collect(),
        optdepends: pkg.optdepends.iter().map(|v| Depend::parse(v)).collect(),
        makedepends: Vec::new(),
        checkdepends: Vec::new(),
        provides: pkg.provides.iter().map(|v| Provide::parse(v)).collect(),
        conflicts: pkg.conflicts.iter().map(|v| Depend::parse(v)).collect(),
        replaces: pkg.replaces.iter().map(|v| Depend::parse(v)).collect(),
        backup: pkg.backup.clone(),
        size: None,
        source: crate::core::pkginfo::PackageSource::Native { commit: None },
    }
}

pub fn package_file_name(info: &PackageInfo) -> String {
    let version = &info.version;
    format!("{}-{}-{}.pkg.tar.bz3", info.name, version, info.arch)
}

pub fn normalize_archive_path(path: &Path) -> Result<PathBuf> {
    use std::path::Component;
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::RootDir | Component::ParentDir | Component::Prefix(_) => {
                return Err(BulbError::UnsafeArchivePath(path.display().to_string()));
            }
        }
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_safe_paths() {
        assert_eq!(
            normalize_archive_path(Path::new("./usr/bin/hello")).unwrap(),
            PathBuf::from("usr/bin/hello")
        );
        assert!(normalize_archive_path(Path::new("../etc/passwd")).is_err());
    }
}
