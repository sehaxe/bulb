use serde::Deserialize;

use crate::error::{BulbError, Result};

#[derive(Debug, Deserialize)]
pub struct BuildManifest {
    pub package: BuildPackage,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BuildPackage {
    pub name: String,
    pub version: String,
    pub release: String,
    pub arch: String,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub packager: Option<String>,
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

pub fn parse_manifest(text: &str) -> Result<BuildManifest> {
    toml::from_str(text).map_err(|e| BulbError::InvalidMetadata(format!("invalid Bulb.toml: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_manifest() {
        let manifest = parse_manifest(
            r#"
            [package]
            name = "hello"
            version = "1.0"
            release = "1"
            arch = "x86_64"
            desc = "Hello world"
            depends = ["glibc"]
            "#,
        )
        .unwrap();
        assert_eq!(manifest.package.name, "hello");
        assert_eq!(manifest.package.version, "1.0");
        assert_eq!(manifest.package.depends, vec!["glibc"]);
    }
}
