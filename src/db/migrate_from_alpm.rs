use std::path::Path;

use crate::error::Result;

use super::Database;
use crate::format::alpm::local_db::read_local_db;

pub fn migrate_from_alpm(db: &mut Database, pacman_local: &Path) -> Result<i64> {
    let entries = read_local_db(pacman_local)?;

    let gen_id = db.create_generation(&format!(
        "migrated from pacman ({} packages)",
        entries.len()
    ))?;

    for entry in &entries {
        let files: Vec<std::path::PathBuf> = entry.files.iter().cloned().collect();
        db.insert_installed_package(
            gen_id,
            &entry.info,
            &files,
            &format!("migrated-{}", entry.info.name),
        )?;
    }

    Ok(gen_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_real_pacman_db() {
        let local = Path::new("/var/lib/pacman/local");
        if !local.is_dir() {
            eprintln!("skipping: no pacman local DB");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut db = Database::open(&db_path).unwrap();
        let gen_id = migrate_from_alpm(&mut db, local).unwrap();
        assert!(gen_id > 0);

        let pkgs = db.list_installed(gen_id).unwrap();
        assert!(!pkgs.is_empty(), "should have migrated packages");
    }
}
