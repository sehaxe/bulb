use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{BulbError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct PackageInfo {
    pub name: String,
    pub version: String,
    pub release: String,
    pub arch: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub packager: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub license: Vec<String>,
    #[serde(default)]
    pub depends: Vec<String>,
    #[serde(default)]
    pub optdepends: Vec<String>,
    #[serde(default)]
    pub provides: Vec<String>,
    #[serde(default)]
    pub conflicts: Vec<String>,
    #[serde(default)]
    pub replaces: Vec<String>,
    #[serde(default)]
    pub backup: Vec<String>,
}

impl PackageInfo {
    pub fn full_version(&self) -> String {
        format!("{}-{}", self.version, self.release)
    }

    pub fn package_file_name(&self) -> String {
        format!(
            "{}-{}-{}-{}.pkg.tar.zst",
            self.name, self.version, self.release, self.arch
        )
    }
}

pub fn parse_pkginfo(input: &str) -> Result<PackageInfo> {
    let mut values: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (line_no, raw_line) in input.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            return Err(BulbError::InvalidMetadata(format!(
                "line {} is not key = value",
                line_no + 1
            )));
        };

        values
            .entry(key.trim().to_string())
            .or_default()
            .push(value.trim().to_string());
    }

    let get = |key: &str| values.get(key).and_then(|items| items.first()).cloned();
    let get_vec = |key: &str| values.get(key).cloned().unwrap_or_default();

    Ok(PackageInfo {
        name: get("pkgname").ok_or_else(|| BulbError::InvalidMetadata("missing pkgname".into()))?,
        version: get("pkgver")
            .ok_or_else(|| BulbError::InvalidMetadata("missing pkgver".into()))?,
        release: get("pkgrel")
            .ok_or_else(|| BulbError::InvalidMetadata("missing pkgrel".into()))?,
        arch: get("arch").ok_or_else(|| BulbError::InvalidMetadata("missing arch".into()))?,
        description: get("pkgdesc"),
        url: get("url"),
        packager: get("packager"),
        size: get("size").and_then(|value| value.parse().ok()),
        license: get_vec("license"),
        depends: get_vec("depend"),
        optdepends: get_vec("optdepend"),
        provides: get_vec("provides"),
        conflicts: get_vec("conflict"),
        replaces: get_vec("replaces"),
        backup: get_vec("backup"),
    })
}

pub fn render_pkginfo(info: &PackageInfo) -> String {
    let mut lines = Vec::new();
    push(&mut lines, "pkgname", &info.name);
    push(&mut lines, "pkgver", &info.version);
    push(&mut lines, "pkgrel", &info.release);
    push(
        &mut lines,
        "pkgdesc",
        info.description.as_deref().unwrap_or(""),
    );
    push(&mut lines, "url", info.url.as_deref().unwrap_or(""));
    push(&mut lines, "arch", &info.arch);
    if let Some(packager) = &info.packager {
        push(&mut lines, "packager", packager);
    }
    if let Some(size) = info.size {
        push(&mut lines, "size", &size.to_string());
    }
    push_many(&mut lines, "license", &info.license);
    push_many(&mut lines, "depend", &info.depends);
    push_many(&mut lines, "optdepend", &info.optdepends);
    push_many(&mut lines, "provides", &info.provides);
    push_many(&mut lines, "conflict", &info.conflicts);
    push_many(&mut lines, "replaces", &info.replaces);
    push_many(&mut lines, "backup", &info.backup);
    lines.join("\n")
}

fn push(lines: &mut Vec<String>, key: &str, value: &str) {
    if !value.is_empty() {
        lines.push(format!("{key} = {value}"));
    }
}

fn push_many(lines: &mut Vec<String>, key: &str, values: &[String]) {
    for value in values {
        lines.push(format!("{key} = {value}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pkginfo() {
        let info = parse_pkginfo(
            r#"
            pkgname = hello
            pkgver = 1.0
            pkgrel = 1
            pkgdesc = Hello world
            arch = x86_64
            depend = glibc
            depend = bash
            "#,
        )
        .unwrap();

        assert_eq!(info.name, "hello");
        assert_eq!(info.full_version(), "1.0-1");
        assert_eq!(info.depends, vec!["glibc", "bash"]);
    }
}
