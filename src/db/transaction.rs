use std::fs;
use std::path::Path;

use rusqlite::params;

use crate::db::Database;
use crate::db::store::ContentStore;
use crate::error::Result;

pub struct Transaction<'a> {
    db: &'a mut Database,
    gen_id: i64,
    root: &'a Path,
    store: &'a ContentStore,
    staged_files: Vec<String>,
    staged_dirs: Vec<String>,
}

impl<'a> Transaction<'a> {
    pub fn new(
        db: &'a mut Database,
        gen_id: i64,
        root: &'a Path,
        store: &'a ContentStore,
    ) -> Result<Self> {
        Ok(Self {
            db,
            gen_id,
            root,
            store,
            staged_files: Vec::new(),
            staged_dirs: Vec::new(),
        })
    }

    pub fn stage_file(&mut self, relative: &str, hash: &str) -> Result<()> {
        let dest = self.root.join(relative);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        self.store.link(hash, &dest)?;
        self.staged_files.push(relative.to_string());
        Ok(())
    }

    pub fn stage_dir(&mut self, relative: &str) -> Result<()> {
        let dest = self.root.join(relative);
        fs::create_dir_all(&dest)?;
        self.staged_dirs.push(relative.to_string());
        Ok(())
    }

    pub fn commit(mut self) -> Result<()> {
        let tx = self.db.connection_mut().transaction()?;

        for dir in &self.staged_dirs {
            tx.execute(
                "INSERT OR IGNORE INTO member_files (generation_id, path, name, is_dir) VALUES (?1, ?2, ?3, 1)",
                params![self.gen_id, dir, dir.rsplit('/').next().unwrap_or(dir)],
            )?;
        }

        for file in &self.staged_files {
            tx.execute(
                "INSERT OR IGNORE INTO member_files (generation_id, path, name, is_dir) VALUES (?1, ?2, ?3, 0)",
                params![
                    self.gen_id,
                    file,
                    file.rsplit('/').next().unwrap_or(file),
                ],
            )?;
        }

        tx.commit()?;
        self.staged_files.clear();
        self.staged_dirs.clear();
        Ok(())
    }

    pub fn rollback(self) -> Result<()> {
        for file in &self.staged_files {
            let dest = self.root.join(file);
            ContentStore::unlink(&dest)?;
        }
        for dir in self.staged_dirs.iter().rev() {
            let dest = self.root.join(dir);
            let _ = fs::remove_dir(&dest);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::store::ContentStore;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn commit_stages_files_to_db() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let root = dir.path().join("root");
        let store_dir = dir.path().join("content");
        fs::create_dir_all(&root).unwrap();

        let mut db = Database::open(&db_path).unwrap();
        let store = ContentStore::new(store_dir);
        store.init().unwrap();
        let _gen_id = db.ensure_generation().unwrap();
        let new_gen = db.create_generation("test install").unwrap();

        let mut tx = Transaction::new(&mut db, new_gen, &root, &store).unwrap();
        let hash = store.add(b"test content").unwrap();
        tx.stage_file("usr/bin/test", &hash).unwrap();
        tx.commit().unwrap();

        assert!(root.join("usr/bin/test").exists());
        assert_eq!(fs::read(root.join("usr/bin/test")).unwrap(), b"test content");
    }

    #[test]
    fn rollback_removes_staged_files() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let root = dir.path().join("root");
        let store_dir = dir.path().join("content");
        fs::create_dir_all(&root).unwrap();

        let mut db = Database::open(&db_path).unwrap();
        let store = ContentStore::new(store_dir);
        store.init().unwrap();
        let _gen_id = db.ensure_generation().unwrap();
        let new_gen = db.create_generation("test install").unwrap();

        let mut tx = Transaction::new(&mut db, new_gen, &root, &store).unwrap();
        let hash = store.add(b"test content").unwrap();
        tx.stage_file("usr/bin/test", &hash).unwrap();
        tx.rollback().unwrap();

        assert!(!root.join("usr/bin/test").exists());
    }
}