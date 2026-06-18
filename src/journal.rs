use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{BulbError, Result};
use crate::util::fs::{atomic_rename, fsync_dir};

#[derive(Debug, Serialize, Deserialize)]
pub enum TransactionKind {
    Install { name: String, version: String },
    Remove { name: String },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Transaction {
    pub id: u64,
    pub kind: TransactionKind,
    pub files: Vec<PathBuf>,
    pub status: TransactionStatus,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum TransactionStatus {
    Pending,
    Applied,
    RolledBack,
}

pub struct Journal {
    dir: PathBuf,
}

impl Journal {
    pub fn new(db_dir: &Path) -> Self {
        let dir = db_dir.join("journal");
        Self { dir }
    }

    pub fn init(&self) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        Ok(())
    }

    pub fn next_id(&self) -> Result<u64> {
        let mut max_id = 0;
        if let Ok(entries) = fs::read_dir(&self.dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let name = entry.file_name().to_string_lossy().into_owned();
                if let Some(id_str) = name.strip_suffix(".json") {
                    if let Ok(id) = id_str.parse::<u64>() {
                        max_id = max_id.max(id);
                    }
                }
            }
        }
        Ok(max_id + 1)
    }

    pub fn begin(&self, kind: TransactionKind, files: Vec<PathBuf>) -> Result<Transaction> {
        let id = self.next_id()?;
        let txn = Transaction {
            id,
            kind,
            files,
            status: TransactionStatus::Pending,
        };
        self.write(&txn)?;
        Ok(txn)
    }

    pub fn commit(&self, txn: &Transaction) -> Result<()> {
        let mut committed = txn.clone();
        committed.status = TransactionStatus::Applied;
        self.write(&committed)?;
        Ok(())
    }

    pub fn rollback(&self, txn: &Transaction) -> Result<()> {
        let mut rolled = txn.clone();
        rolled.status = TransactionStatus::RolledBack;
        self.write(&rolled)?;
        Ok(())
    }

    pub fn pending(&self) -> Result<Vec<Transaction>> {
        let mut txns = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Ok(data) = fs::read_to_string(&path) {
                        if let Ok(txn) = serde_json::from_str::<Transaction>(&data) {
                            if matches!(txn.status, TransactionStatus::Pending) {
                                txns.push(txn);
                            }
                        }
                    }
                }
            }
        }
        txns.sort_by_key(|t| t.id);
        Ok(txns)
    }

    pub fn recover(&self, root: &Path) -> Result<Vec<String>> {
        let pending = self.pending()?;
        let mut recovered = Vec::new();

        for txn in &pending {
            match &txn.kind {
                TransactionKind::Install { .. } => {
                    for file in &txn.files {
                        let dest = root.join(file);
                        if dest.exists() || dest.symlink_metadata().is_ok() {
                            fs::remove_file(&dest).ok();
                        }
                    }
                    self.rollback(txn)?;
                    recovered.push(format!("rolled back transaction {}", txn.id));
                }
                TransactionKind::Remove { .. } => {
                    self.rollback(txn)?;
                    recovered.push(format!("skipped incomplete remove {}", txn.id));
                }
            }
        }

        Ok(recovered)
    }

    fn write(&self, txn: &Transaction) -> Result<()> {
        let path = self.dir.join(format!("{}.json", txn.id));
        let tmp = path.with_extension("tmp");

        let data = serde_json::to_string_pretty(txn)
            .map_err(|e| BulbError::Config(format!("serialize journal: {e}")))?;

        fs::write(&tmp, data)?;

        fs::File::open(&tmp)?.sync_all()?;

        if let Some(parent) = path.parent() {
            fsync_dir(parent)?;
        }

        atomic_rename(&tmp, &path)?;

        Ok(())
    }
}

impl Clone for Transaction {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            kind: match &self.kind {
                TransactionKind::Install { name, version } => TransactionKind::Install {
                    name: name.clone(),
                    version: version.clone(),
                },
                TransactionKind::Remove { name } => TransactionKind::Remove { name: name.clone() },
            },
            files: self.files.clone(),
            status: match &self.status {
                TransactionStatus::Pending => TransactionStatus::Pending,
                TransactionStatus::Applied => TransactionStatus::Applied,
                TransactionStatus::RolledBack => TransactionStatus::RolledBack,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn journal_crud() {
        let dir = tempdir().unwrap();
        let journal = Journal::new(dir.path());
        journal.init().unwrap();

        let txn = journal.begin(
            TransactionKind::Install {
                name: "test".into(),
                version: "1.0".into(),
            },
            vec!["usr/bin/test".into()],
        )
        .unwrap();

        assert_eq!(txn.id, 1);
        assert!(matches!(txn.status, TransactionStatus::Pending));

        let pending = journal.pending().unwrap();
        assert_eq!(pending.len(), 1);

        journal.commit(&txn).unwrap();
        let pending = journal.pending().unwrap();
        assert_eq!(pending.len(), 0);
    }
}
