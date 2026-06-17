pub mod installed;
pub mod migrate_from_alpm;
pub mod schema;
pub mod store;
pub mod transaction;

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::core::pkginfo::PackageInfo;
use crate::error::Result;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        schema::run_migrations(&conn)?;
        Ok(Self { conn })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    pub fn current_generation(&self) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT id FROM generations WHERE is_current = 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn ensure_generation(&self) -> Result<i64> {
        if let Some(id) = self.current_generation()? {
            return Ok(id);
        }
        self.conn.execute(
            "INSERT INTO generations (parent, is_current, note) VALUES (NULL, 1, 'initial')",
            [],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn create_generation(&self, note: &str) -> Result<i64> {
        let parent = self.current_generation()?;
        self.conn.execute(
            "UPDATE generations SET is_current = 0 WHERE is_current = 1",
            [],
        )?;
        self.conn.execute(
            "INSERT INTO generations (parent, is_current, note) VALUES (?1, 1, ?2)",
            params![parent, note],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn switch_generation(&self, gen_id: i64) -> Result<()> {
        let exists: bool = self.conn.query_row(
            "SELECT COUNT(*) > 0 FROM generations WHERE id = ?1",
            params![gen_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(crate::error::BulbError::GenerationNotFound(gen_id));
        }
        self.conn.execute(
            "UPDATE generations SET is_current = 0 WHERE is_current = 1",
            [],
        )?;
        self.conn.execute(
            "UPDATE generations SET is_current = 1 WHERE id = ?1",
            params![gen_id],
        )?;
        Ok(())
    }

    pub fn list_generations(&self) -> Result<Vec<(i64, Option<i64>, String, bool)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, parent, COALESCE(note, ''), is_current FROM generations ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn insert_installed_package(
        &mut self,
        gen_id: i64,
        info: &PackageInfo,
        files: &[PathBuf],
        store_hash: &str,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;

        tx.execute(
            "INSERT INTO generation_members (generation_id, name, version, arch, source, store_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                gen_id,
                info.name,
                info.version,
                info.arch,
                serde_json::to_string(&info.source).unwrap_or_default(),
                store_hash,
            ],
        )?;

        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO member_files (generation_id, path, name, is_dir)
                 VALUES (?1, ?2, ?3, 0)",
            )?;
            for file in files {
                stmt.execute(params![
                    gen_id,
                    file.to_string_lossy(),
                    file.file_name().unwrap_or_default().to_string_lossy(),
                ])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    pub fn list_installed(&self, gen_id: i64) -> Result<Vec<PackageInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, version, arch, source FROM generation_members WHERE generation_id = ?1 ORDER BY name",
        )?;
        let rows = stmt.query_map(params![gen_id], |row| {
            let name: String = row.get(0)?;
            let version: String = row.get(1)?;
            let arch: String = row.get(2)?;
            let source: String = row.get(3)?;
            Ok((name, version, arch, source))
        })?;
        let mut pkgs = Vec::new();
        for row in rows {
            let (name, version, arch, source_json) = row?;
            let source: crate::core::pkginfo::PackageSource =
                serde_json::from_str(&source_json).unwrap_or_default();
            pkgs.push(PackageInfo {
                name,
                version,
                arch,
                source,
                ..Default::default()
            });
        }
        Ok(pkgs)
    }

    pub fn find_file_owner(&self, gen_id: i64, path: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT m.name FROM member_files f
                 JOIN generation_members m ON f.generation_id = m.generation_id AND f.name = m.name
                 WHERE f.generation_id = ?1 AND f.path = ?2",
                params![gen_id, path],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn remove_package(&mut self, gen_id: i64, name: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM member_files WHERE generation_id = ?1 AND name = ?2",
            params![gen_id, name],
        )?;
        tx.execute(
            "DELETE FROM generation_members WHERE generation_id = ?1 AND name = ?2",
            params![gen_id, name],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn get_generation_files(&self, gen_id: i64) -> Result<Vec<PathBuf>> {
        let mut stmt = self.conn.prepare(
            "SELECT path FROM member_files WHERE generation_id = ?1 ORDER BY path",
        )?;
        let rows = stmt.query_map(params![gen_id], |row| {
            let path: String = row.get(0)?;
            Ok(PathBuf::from(path))
        })?;
        let mut files = Vec::new();
        for row in rows {
            files.push(row?);
        }
        Ok(files)
    }

    pub fn switch_generation_files(
        &self,
        old_gen: i64,
        new_gen: i64,
        root: &Path,
        _store: &store::ContentStore,
    ) -> Result<()> {
        let old_files = self.get_generation_files(old_gen)?;
        let new_files = self.get_generation_files(new_gen)?;

        let old_set: std::collections::HashSet<_> = old_files.iter().collect();
        let new_set: std::collections::HashSet<_> = new_files.iter().collect();

        for file in old_files.iter() {
            if !new_set.contains(file) {
                let dest = root.join(file);
                store::ContentStore::unlink(&dest)?;
            }
        }

        for file in new_files.iter() {
            if !old_set.contains(file) {
                let dest = root.join(file);
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_and_queries_generations() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::open(&db_path).unwrap();

        let gen_id = db.ensure_generation().unwrap();
        assert_eq!(gen_id, 1);

        let current = db.current_generation().unwrap();
        assert_eq!(current, Some(1));

        let gen2 = db.create_generation("test upgrade").unwrap();
        assert_eq!(gen2, 2);
        assert_eq!(db.current_generation().unwrap(), Some(2));

        let gens = db.list_generations().unwrap();
        assert_eq!(gens.len(), 2);
    }
}
