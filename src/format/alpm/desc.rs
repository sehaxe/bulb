//! Parser for ALPM `desc` files — the `%KEY%`-delimited format used in both
//! the sync database (`/var/lib/pacman/sync/<repo>.db` entries) and the local
//! database (`/var/lib/pacman/local/<pkg>/desc`).
//!
//! Format: each field is a line `%KEY%`, immediately followed by one or more
//! value lines, terminated by a blank line. Multi-value keys list one value
//! per line. Example:
//!
//! ```text
//! %NAME%
//! acl
//!
//! %DEPENDS%
//! glibc
//! attr
//! ```
//!
//! This module produces an ordered, case-insensitive map of key → list of
//! values. Conversion to [`PackageInfo`] is done by the caller (sync and
//! local DBs share the parser but produce slightly different metadata —
//! notably local has `%INSTALLDATE%`/`%SIZE%`/`%VALIDATION%`, sync has
//! `%CSIZE%`/`%SHA256SUM%`/`%PGPSIG%`).

use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct Desc {
    fields: HashMap<String, Vec<String>>,
}

impl Desc {
    pub fn parse(text: &str) -> Self {
        let mut fields: HashMap<String, Vec<String>> = HashMap::with_capacity(16);
        let mut lines = text.lines().peekable();

        while let Some(line) = lines.next() {
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(key) = trimmed
                .strip_prefix('%')
                .and_then(|s| s.strip_suffix('%'))
            {
                let key = key.to_ascii_lowercase();
                let mut values = Vec::new();
                while let Some(next) = lines.peek() {
                    if next.trim().is_empty() {
                        lines.next();
                        break;
                    }
                    values.push(lines.next().unwrap().to_string());
                }
                fields.entry(key).or_default().extend(values);
            }
        }

        Desc { fields }
    }

    #[inline]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .get(key)
            .and_then(|v| v.first())
            .map(String::as_str)
    }

    #[inline]
    pub fn get_vec(&self, key: &str) -> &[String] {
        self.fields
            .get(key)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    #[inline]
    pub fn get_ci(&self, key: &str) -> Option<&str> {
        self.fields
            .get(&key.to_ascii_lowercase())
            .and_then(|v| v.first())
            .map(String::as_str)
    }

    #[inline]
    pub fn get_vec_ci(&self, key: &str) -> &[String] {
        self.fields
            .get(&key.to_ascii_lowercase())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESC: &str = "\
%NAME%
acl

%VERSION%
2.3.2-2

%DESC%
Access control list utilities

%DEPENDS%
glibc
attr

%PROVIDES%
libacl.so=1-64
";

    #[test]
    fn parses_single_and_multi() {
        let d = Desc::parse(DESC);
        assert_eq!(d.get("name"), Some("acl"));
        assert_eq!(d.get("version"), Some("2.3.2-2"));
        assert_eq!(d.get_vec("depends"), &["glibc", "attr"]);
        assert_eq!(d.get_vec("provides"), &["libacl.so=1-64"]);
    }

    #[test]
    fn case_insensitive() {
        let d = Desc::parse("%NAME%\nx\n\n");
        assert_eq!(d.get("name"), Some("x"));
        assert_eq!(d.get_ci("Name"), Some("x"));
    }

    #[test]
    fn missing_key_is_empty() {
        let d = Desc::parse(DESC);
        assert!(d.get("nonexistent").is_none());
        assert!(d.get_vec("nonexistent").is_empty());
    }
}
