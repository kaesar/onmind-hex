//! Role aggregate → SQLite via rusqlite (`db` feature), mirroring hex4w
//! `R2dbcRoleRepository` + `roles` table. Bundled SQLite, no network needed.
//!
//! Schema (id, name, created_at) seeded with ADMIN/USER/MODERATOR.
//!
//! Env: `HEX_DB_PATH` (default `hex.db`)

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::{params, Connection};

use crate::application::ports::RoleRepositoryPort;
use crate::domain::{DomainError, Role};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS roles (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT OR IGNORE INTO roles (name) VALUES ('ADMIN'), ('USER'), ('MODERATOR');
";

pub struct SqliteRoleRepository {
    conn: Mutex<Connection>,
}

fn err(what: impl std::fmt::Display) -> DomainError {
    DomainError::Internal(format!("roles: {what}"))
}

impl SqliteRoleRepository {
    /// Open (creating if needed) the SQLite file under `HEX_DB_PATH` and apply
    /// the schema + seed data.
    pub fn open() -> Result<Self, DomainError> {
        let path = std::env::var("HEX_DB_PATH").unwrap_or_else(|_| "hex.db".into());
        let conn = Connection::open(PathBuf::from(&path))
            .map_err(|e| err(format!("open {path}: {e}")))?;
        conn.execute_batch(SCHEMA).map_err(|e| err(format!("schema: {e}")))?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    fn row_to_role(row: &rusqlite::Row<'_>) -> rusqlite::Result<Role> {
        Ok(Role {
            id: row.get(0)?,
            name: row.get(1)?,
            created_at: row.get::<_, Option<String>>(2)?,
        })
    }
}

impl RoleRepositoryPort for SqliteRoleRepository {
    fn list(&self) -> Result<Vec<Role>, DomainError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, name, created_at FROM roles ORDER BY id")
            .map_err(|e| err(e))?;
        let rows = stmt
            .query_map([], Self::row_to_role)
            .map_err(|e| err(e))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| err(e))?)
    }

    fn find(&self, id: i64) -> Result<Option<Role>, DomainError> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, name, created_at FROM roles WHERE id = ?1",
            params![id],
            Self::row_to_role,
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(err(other)),
        })
    }

    fn search_by_name(&self, name: &str) -> Result<Vec<Role>, DomainError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, name, created_at FROM roles WHERE name LIKE ?1 ESCAPE '\\' ORDER BY id")
            .map_err(|e| err(e))?;
        let pattern = format!("%{}%", name.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        let rows = stmt.query_map(params![pattern], Self::row_to_role).map_err(|e| err(e))?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| err(e))
    }

    fn save(&self, role: &Role) -> Result<Role, DomainError> {
        let conn = self.conn.lock().unwrap();
        if role.id == 0 {
            conn.execute("INSERT INTO roles (name) VALUES (?1)", params![role.name])
                .map_err(|e| err(e))?;
            let id = conn.last_insert_rowid();
            conn.query_row(
                "SELECT id, name, created_at FROM roles WHERE id = ?1",
                params![id],
                Self::row_to_role,
            )
            .map_err(|e| err(e))
        } else {
            conn.execute("UPDATE roles SET name = ?1 WHERE id = ?2", params![role.name, role.id])
                .map_err(|e| err(e))?;
            conn.query_row(
                "SELECT id, name, created_at FROM roles WHERE id = ?1",
                params![role.id],
                Self::row_to_role,
            )
            .map_err(|e| err(e))
        }
    }

    fn exists_by_name(&self, name: &str) -> Result<bool, DomainError> {
        let conn = self.conn.lock().unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM roles WHERE name = ?1", params![name], |r| r.get(0))
            .map_err(|e| err(e))?;
        Ok(n > 0)
    }

    fn count(&self) -> Result<i64, DomainError> {
        let conn = self.conn.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM roles", [], |r| r.get(0))
            .map_err(|e| err(e))
    }

    fn delete_by_id(&self, id: i64) -> Result<(), DomainError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM roles WHERE id = ?1", params![id])
            .map(|_| ())
            .map_err(|e| err(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    static SEQ: AtomicUsize = AtomicUsize::new(0);

    fn repo() -> SqliteRoleRepository {
        let path = format!(
            "/tmp/hex_role_test_{}_{}.db",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        );
        std::env::set_var("HEX_DB_PATH", &path);
        let _ = std::fs::remove_file(&path);
        SqliteRoleRepository::open().expect("open temp db")
    }

    #[test]
    fn seeds_and_search() {
        let repo: Arc<dyn RoleRepositoryPort> = Arc::new(repo());
        let roles = repo.list().expect("list");
        let names: Vec<_> = roles.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"ADMIN") && names.contains(&"USER") && names.contains(&"MODERATOR"));
        assert!(repo.search_by_name("AD").expect("search").iter().any(|r| r.name == "ADMIN"));
    }

    #[test]
    fn saves_new_role() {
        let repo: Arc<dyn RoleRepositoryPort> = Arc::new(repo());
        let saved = repo.save(&Role {
            id: 0,
            name: "GUEST".into(),
            created_at: None,
        }).expect("save");
        assert!(saved.id > 0);
        assert_eq!(repo.find(saved.id).expect("find").unwrap().name, "GUEST");
    }
}