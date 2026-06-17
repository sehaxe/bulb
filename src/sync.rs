use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::db::Database;
use crate::error::{BulbError, Result};
use crate::format::alpm::desc::Desc;

pub struct SyncDb {
    pub mirror: String,
    pub repos: Vec<String>,
    pub cache_dir: PathBuf,
}

impl SyncDb {
    pub fn new(mirror: String, repos: Vec<String>, cache_dir: PathBuf) -> Self {
        Self { mirror, repos, cache_dir }
    }

    pub fn db_filename(repo: &str) -> String {
        format!("{repo}.db")
    }

    pub fn db_url(&self, repo: &str) -> String {
        format!("{}/{}", self.mirror, Self::db_filename(repo))
    }

    pub async fn sync_repos(&self) -> Result<Vec<(String, PathBuf)>> {
        let client = reqwest::Client::builder()
            .user_agent("bulb/0.1")
            .build()
            .map_err(|e| BulbError::Config(format!("http client: {e}")))?;

        let mut results = Vec::new();

        for repo in &self.repos {
            let url = self.db_url(repo);
            let dest = self.cache_dir.join(Self::db_filename(repo));

            let response = client.get(&url).send().await
                .map_err(|e| BulbError::Config(format!("sync failed for {repo}: {e}")))?;

            if !response.status().is_success() {
                return Err(BulbError::Config(format!(
                    "sync failed: HTTP {} for {}",
                    response.status(), url
                )));
            }

            let bytes = response.bytes().await
                .map_err(|e| BulbError::Config(format!("read failed for {repo}: {e}")))?;

            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&dest, &bytes)?;

            results.push((repo.clone(), dest));
        }

        Ok(results)
    }

    pub fn parse_sync_db(db_path: &Path) -> Result<Vec<SyncPackage>> {
        let bytes = fs::read(db_path)?;

        let is_zstd = bytes.len() >= 4 && bytes[0] == 0x28 && bytes[1] == 0xB5 && bytes[2] == 0x2F && bytes[3] == 0xFD;

        let mut packages = Vec::new();

        if is_zstd {
            let decoder = zstd::stream::Decoder::new(&bytes[..])
                .map_err(|e| BulbError::Decompress(e.to_string()))?;
            let mut archive = tar::Archive::new(decoder);
            Self::extract_desc_entries(&mut archive, &mut packages)?;
        } else {
            let gz = flate2::read::GzDecoder::new(&bytes[..]);
            let mut archive = tar::Archive::new(gz);
            Self::extract_desc_entries(&mut archive, &mut packages)?;
        }

        Ok(packages)
    }

    fn extract_desc_entries(
        archive: &mut tar::Archive<impl std::io::Read>,
        packages: &mut Vec<SyncPackage>,
    ) -> Result<()> {
        for entry in archive.entries().map_err(|e| BulbError::MalformedTarball(e.to_string()))? {
            let entry = entry.map_err(|e| BulbError::MalformedTarball(e.to_string()))?;
            let path = entry.path().map_err(|e| BulbError::MalformedTarball(e.to_string()))?;

            if path.file_name().and_then(|e| e.to_str()) == Some("desc") {
                let mut content = String::new();
                std::io::Read::read_to_string(&mut std::io::BufReader::new(entry), &mut content)
                    .map_err(|e| BulbError::MalformedTarball(e.to_string()))?;

                if let Some(pkg) = parse_desc_to_sync(&content) {
                    packages.push(pkg);
                }
            }
        }
        Ok(())
    }

    pub fn load_installed(db: &Database, gen_id: i64) -> Result<HashMap<String, Version>> {
        let pkgs = db.list_installed(gen_id)?;
        let mut installed = HashMap::new();
        for pkg in pkgs {
            if let Ok(v) = Version::parse(&pkg.version) {
                installed.insert(pkg.name, v);
            }
        }
        Ok(installed)
    }
}

use crate::core::version::Version;

#[derive(Debug, Clone)]
pub struct SyncPackage {
    pub name: String,
    pub version: Version,
    pub description: Option<String>,
    pub arch: String,
    pub base: Option<String>,
    pub filename: Option<String>,
    pub csize: Option<u64>,
    pub isize: Option<u64>,
    pub sha256: Option<String>,
    pub deps: Vec<String>,
    pub provides: Vec<String>,
    pub conflicts: Vec<String>,
    pub replaces: Vec<String>,
}

pub fn parse_desc_to_sync(content: &str) -> Option<SyncPackage> {
    let desc = Desc::parse(content);

    let name = desc.get("name")?.to_string();
    let version_str = desc.get("version")?.to_string();
    let version = Version::parse(&version_str).ok()?;

    let filename = desc.get("filename").map(|s| s.to_string());
    let csize = desc.get("csize").and_then(|s| s.parse().ok());
    let isize = desc.get("isize").and_then(|s| s.parse().ok());
    let sha256 = desc.get("sha256sum").map(|s| s.to_string());
    let arch = desc.get("arch").map(|s| s.to_string()).unwrap_or_else(|| "x86_64".into());
    let base = desc.get("base").map(|s| s.to_string());
    let description = desc.get("desc").map(|s| s.to_string());

    let deps = desc.get_vec("depends").to_vec();
    let provides = desc.get_vec("provides").to_vec();
    let conflicts = desc.get_vec("conflicts").to_vec();
    let replaces = desc.get_vec("replaces").to_vec();

    Some(SyncPackage {
        name,
        version,
        description,
        arch,
        base,
        filename,
        csize,
        isize,
        sha256,
        deps,
        provides,
        conflicts,
        replaces,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_desc_content() {
        let content = "%FILENAME%\nfoo-1.0.pkg.tar.zst\n\n%NAME%\nfoo\n\n%VERSION%\n1.0-1\n\n%CSIZE%\n12345\n\n%DEPENDS%\nbar\nbaz\n\n";
        let pkg = parse_desc_to_sync(content).unwrap();
        assert_eq!(pkg.name, "foo");
        assert_eq!(pkg.filename, Some("foo-1.0.pkg.tar.zst".into()));
        assert_eq!(pkg.deps, vec!["bar", "baz"]);
    }
}
