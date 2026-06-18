//! Unified package metadata.
//!
//! Every package backend — ALPM `.pkg.tar.zst`, ALPM local DB entries, and
//! bulb's native `.pkg.tar.zst` — produces the same [`PackageInfo`] struct.
//! This is the single source of truth for "what is this package" that the
//! resolver, DB layer, and UI all consume.

use serde::{Deserialize, Serialize};

use crate::core::dependency::{Depend, Provide};
use crate::core::version::Version;

/// Where a package came from. Determines how to fetch and validate it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PackageSource {
    /// Available in a synced repository (or installed from one). The repo
    /// name records origin (`core`, `extra`, …).
    Alpm {
        repo: String,
        /// Filename inside the repo, e.g. `firefox-120.0-1-x86_64.pkg.tar.zst`.
        filename: Option<String>,
        /// Compressed size in bytes (from `%CSIZE%`).
        csize: Option<u64>,
        /// sha256 of the package file (from `%SHA256SUM%`).
        sha256: Option<String>,
        /// base64 detached PGP sig (from `%PGPSIG%`).
        pgpsig: Option<String>,
    },
    /// bulb's native zstd format, built locally or from a TOML AUR.
    Native {
        /// Commit hash if from the TOML AUR; `None` for locally built.
        commit: Option<String>,
    },
    /// Installed locally only (origin unknown or local-only install).
    Local,
}

impl Default for PackageSource {
    fn default() -> Self {
        PackageSource::Local
    }
}

/// Unified package metadata. See module docs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PackageInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    /// Full `[epoch:]pkgver[-pkgrel]` string, kept verbatim so it round-trips
    /// exactly to/from on-disk forms. Use [`Version::parse`] to compare.
    pub version: String,
    pub arch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packager: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builddate: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installdate: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub license: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends: Vec<Depend>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub optdepends: Vec<Depend>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub makedepends: Vec<Depend>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checkdepends: Vec<Depend>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provides: Vec<Provide>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<Depend>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replaces: Vec<Depend>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub backup: Vec<String>,
    /// Installed size in bytes (`%ISIZE%`/`%SIZE%`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Provenance / how to fetch.
    #[serde(default)]
    pub source: PackageSource,
}

impl PackageInfo {
    /// Parse the version string into a structured [`Version`]. Returns
    /// `None` on parse failure (callers should fall back to string compare).
    pub fn parsed_version(&self) -> Option<Version> {
        Version::parse(&self.version).ok()
    }

    /// `name-version-arch` identifier (no extension). Used as a stable key.
    pub fn nva(&self) -> String {
        format!("{}-{}-{}", self.name, self.version, self.arch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nva_format() {
        let info = PackageInfo {
            name: "firefox".into(),
            version: "120.0-1".into(),
            arch: "x86_64".into(),
            ..Default::default()
        };
        assert_eq!(info.nva(), "firefox-120.0-1-x86_64");
    }

    #[test]
    fn parsed_version_handles_epoch() {
        let info = PackageInfo {
            name: "x".into(),
            version: "2:1.0-3".into(),
            arch: "any".into(),
            ..Default::default()
        };
        let v = info.parsed_version().unwrap();
        assert_eq!(v.epoch, 2);
    }
}
