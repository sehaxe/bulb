use rusqlite::Connection;

use crate::error::Result;

pub fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY
        );",
    )?;

    let current: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if current < 1 {
        migrate_v1(conn)?;
    }

    Ok(())
}

fn migrate_v1(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS generations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            parent INTEGER,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            is_current INTEGER NOT NULL DEFAULT 0,
            note TEXT,
            FOREIGN KEY (parent) REFERENCES generations(id)
        );

        CREATE TABLE IF NOT EXISTS generation_members (
            generation_id INTEGER NOT NULL,
            name TEXT NOT NULL,
            version TEXT NOT NULL,
            arch TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT '{}',
            store_hash TEXT NOT NULL DEFAULT '',
            PRIMARY KEY (generation_id, name),
            FOREIGN KEY (generation_id) REFERENCES generations(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_gen_members_name
            ON generation_members(name);

        CREATE TABLE IF NOT EXISTS store_objects (
            hash TEXT PRIMARY KEY,
            size INTEGER NOT NULL DEFAULT 0,
            refcount INTEGER NOT NULL DEFAULT 1,
            added_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS repo_packages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            repo TEXT NOT NULL,
            name TEXT NOT NULL,
            base TEXT,
            version TEXT NOT NULL,
            arch TEXT NOT NULL,
            filename TEXT,
            csize INTEGER,
            isize INTEGER,
            sha256 TEXT,
            pgpsig TEXT,
            UNIQUE(repo, name, version, arch)
        );

        CREATE INDEX IF NOT EXISTS idx_repo_pkgs_name
            ON repo_packages(name);

        CREATE TABLE IF NOT EXISTS repo_package_relations (
            package_id INTEGER NOT NULL,
            kind TEXT NOT NULL,
            value TEXT NOT NULL,
            PRIMARY KEY (package_id, kind, value),
            FOREIGN KEY (package_id) REFERENCES repo_packages(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS member_files (
            generation_id INTEGER NOT NULL,
            path TEXT NOT NULL,
            name TEXT NOT NULL,
            is_dir INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (generation_id, path),
            FOREIGN KEY (generation_id) REFERENCES generations(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_member_files_name
            ON member_files(generation_id, name);

        CREATE TABLE IF NOT EXISTS member_backup (
            generation_id INTEGER NOT NULL,
            name TEXT NOT NULL,
            path TEXT NOT NULL,
            hash TEXT,
            PRIMARY KEY (generation_id, name, path),
            FOREIGN KEY (generation_id) REFERENCES generations(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS transactions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            generation_id INTEGER NOT NULL,
            started_at TEXT NOT NULL DEFAULT (datetime('now')),
            finished_at TEXT,
            status TEXT NOT NULL DEFAULT 'pending',
            kind TEXT NOT NULL DEFAULT 'install',
            summary TEXT,
            FOREIGN KEY (generation_id) REFERENCES generations(id)
        );

        CREATE TABLE IF NOT EXISTS install_scripts (
            name TEXT NOT NULL,
            version TEXT NOT NULL,
            script TEXT NOT NULL,
            has_pre_install INTEGER NOT NULL DEFAULT 0,
            has_post_install INTEGER NOT NULL DEFAULT 0,
            has_pre_upgrade INTEGER NOT NULL DEFAULT 0,
            has_post_upgrade INTEGER NOT NULL DEFAULT 0,
            has_pre_remove INTEGER NOT NULL DEFAULT 0,
            has_post_remove INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (name, version)
        );

        CREATE TABLE IF NOT EXISTS config_state (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        INSERT OR IGNORE INTO schema_version (version) VALUES (1);
        ",
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_schema() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let version: i32 = conn
            .query_row("SELECT version FROM schema_version", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, 1);

        let tables: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .unwrap();
            let rows = stmt.query_map([], |row| row.get(0)).unwrap();
            rows.filter_map(|r| r.ok()).collect()
        };
        assert!(tables.contains(&"generations".to_string()));
        assert!(tables.contains(&"generation_members".to_string()));
        assert!(tables.contains(&"store_objects".to_string()));
    }
}
