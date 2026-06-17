use std::collections::BTreeMap;

use crate::error::Result;

#[derive(Debug, Clone, Default)]
pub struct BuildInfo {
    pub fields: BTreeMap<String, Vec<String>>,
}

impl BuildInfo {
    pub fn parse(text: &str) -> Result<Self> {
        let mut fields: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if line.starts_with('[') {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                fields
                    .entry(key.trim().to_string())
                    .or_default()
                    .push(value.trim().to_string());
            }
        }

        Ok(BuildInfo { fields })
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .get(key)
            .and_then(|v| v.first())
            .map(String::as_str)
    }

    pub fn get_vec(&self, key: &str) -> &[String] {
        self.fields.get(key).map(Vec::as_slice).unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
[BUILDINFO]
buildenv=base-devel
options=debug !strip
installed=true
";

    #[test]
    fn parses_buildinfo() {
        let bi = BuildInfo::parse(SAMPLE).unwrap();
        assert_eq!(bi.get("buildenv"), Some("base-devel"));
        assert_eq!(bi.get("options"), Some("debug !strip"));
    }

    #[test]
    fn parses_real_buildinfo() {
        let cache = std::path::Path::new("/var/cache/pacman/pkg");
        if !cache.is_dir() {
            return;
        }
        for entry in std::fs::read_dir(cache).unwrap().flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.ends_with(".pkg.tar.zst") {
                if let Ok(text) = std::fs::read_to_string(entry.path()) {
                    if text.contains("[BUILDINFO]") {
                        let bi = BuildInfo::parse(&text).unwrap();
                        assert!(bi.get("buildenv").is_some());
                        return;
                    }
                }
            }
        }
    }
}
