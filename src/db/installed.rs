use crate::error::Result;
use crate::core::pkginfo::PackageInfo;
use super::Database;

impl Database {
    pub fn get_installed_package(&self, gen_id: i64, name: &str) -> Result<Option<PackageInfo>> {
        let mut stmt = self.connection().prepare(
            "SELECT name, version, arch, source FROM generation_members
             WHERE generation_id = ?1 AND name = ?2",
        )?;
        let mut rows = stmt.query_map(rusqlite::params![gen_id, name], |row| {
            let name: String = row.get(0)?;
            let version: String = row.get(1)?;
            let arch: String = row.get(2)?;
            let source: String = row.get(3)?;
            Ok((name, version, arch, source))
        })?;
        match rows.next() {
            Some(Ok((name, version, arch, source_json))) => {
                let source: crate::core::pkginfo::PackageSource =
                    serde_json::from_str(&source_json).unwrap_or_default();
                Ok(Some(PackageInfo {
                    name,
                    version,
                    arch,
                    source,
                    ..Default::default()
                }))
            }
            _ => Ok(None),
        }
    }

    pub fn get_installed_files(&self, gen_id: i64, name: &str) -> Result<Vec<std::path::PathBuf>> {
        let mut stmt = self.connection().prepare(
            "SELECT path FROM member_files
             WHERE generation_id = ?1 AND name = ?2
             ORDER BY path",
        )?;
        let rows = stmt.query_map(rusqlite::params![gen_id, name], |row| {
            let path: String = row.get(0)?;
            Ok(std::path::PathBuf::from(path))
        })?;
        let mut files = Vec::new();
        for row in rows {
            files.push(row?);
        }
        Ok(files)
    }

    pub fn list_installed_names(&self, gen_id: i64) -> Result<Vec<String>> {
        let mut stmt = self.connection().prepare(
            "SELECT name FROM generation_members
             WHERE generation_id = ?1
             ORDER BY name",
        )?;
        let rows = stmt.query_map(rusqlite::params![gen_id], |row| row.get(0))?;
        let mut names = Vec::new();
        for row in rows {
            names.push(row?);
        }
        Ok(names)
    }
}
