//! Reader for ALPM sync databases — `/var/lib/pacman/sync/<repo>.db`.
//!
//! A sync DB is a **gzip**-compressed tarball (verified empirically on pacman
//! 7.1; do not assume zstd). Each top-level entry is a directory named
//! `<pkgname>-<version>` containing exactly one file, `desc`. We decompress
//! with `flate2`, walk the tar with the `tar` crate, parse each `desc`, and
//! produce [`PackageInfo`] values tagged with the repo name.

use std::fs::File;
use std::path::Path;

use flate2::read::GzDecoder;
use tar::Archive;

use crate::core::pkginfo::PackageInfo;
use crate::error::Result;

use super::convert::package_info_from_desc;
use super::desc::Desc;

/// Parse a sync database file (`<repo>.db`) into [`PackageInfo`] entries.
///
/// The `repo` name is attached to each entry's [`PackageSource`]. Entries
/// with no usable `desc` are skipped (sync DBs are occasionally noisy).
pub fn parse_sync_db(path: &Path, repo: &str) -> Result<Vec<PackageInfo>> {
    let file = File::open(path)?;
    parse_sync_db_from_reader(file, repo)
}

pub fn parse_sync_db_from_reader<R: std::io::Read>(reader: R, repo: &str) -> Result<Vec<PackageInfo>> {
    let gz = GzDecoder::new(reader);
    let mut archive = Archive::new(gz);
    let mut out = Vec::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        // Only the `desc` file inside each package dir interests us. Directory
        // entries have no content; skip them.
        let path = entry.path()?.into_owned();
        if entry.header().entry_type().is_dir() {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str());
        if name != Some("desc") {
            continue;
        }
        let mut text = String::new();
        entry.read_to_string_end(&mut text)?;
        let desc = Desc::parse(&text);
        // Skip entries missing the essential name field — malformed noise.
        if desc.get("name").is_none() {
            continue;
        }
        out.push(package_info_from_desc(&desc, Some(repo)));
    }

    Ok(out)
}

/// Helper to read an entire entry into a String, abstracting the
/// `tar::Entry::read_to_string` lifetime dance.
trait ReadToEnd {
    fn read_to_string_end(&mut self, out: &mut String) -> std::io::Result<()>;
}

impl<R: std::io::Read> ReadToEnd for tar::Entry<'_, R> {
    fn read_to_string_end(&mut self, out: &mut String) -> std::io::Result<()> {
        use std::io::Read;
        self.read_to_string(out)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_core_db() {
        // Integration test against the live system. Skipped if the file is
        // absent (non-Arch CI / sandboxed environments).
        let path = Path::new("/var/lib/pacman/sync/core.db");
        if !path.exists() {
            eprintln!("skipping: {path:?} not present");
            return;
        }
        let pkgs = parse_sync_db(path, "core").expect("core.db parses");
        assert!(!pkgs.is_empty(), "core.db should contain packages");
        // Every entry must have a name and a version.
        for p in &pkgs {
            assert!(!p.name.is_empty(), "entry has empty name");
            assert!(!p.version.is_empty(), "entry {} has empty version", p.name);
        }
        // Sanity: core.db on Arch always contains `filesystem` and `pacman`.
        let names: Vec<&str> = pkgs.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"filesystem"), "expected `filesystem` in core.db, got: {names:?}");
    }
}
