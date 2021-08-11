use std::{ops::Deref, path::Path};

use git2::Oid;
use rusqlite::{params, Connection, OptionalExtension};

mod embedded {
    refinery::embed_migrations!("./migrations");
}

#[derive(Debug)]
pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let mut conn = Connection::open(path)?;
        embedded::migrations::runner().run(&mut conn)?;

        Ok(Self { conn })
    }

    pub fn get_cvs_branch(&self, git: &str) -> anyhow::Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT cvs FROM branch_mappings WHERE git = ?",
                params![git],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn write_branch<I, D>(
        &mut self,
        git_branch: &str,
        cvs_branch: &str,
        commits: I,
    ) -> anyhow::Result<()>
    where
        I: Iterator<Item = D>,
        D: Deref<Target = Oid>,
    {
        let txn = self.conn.transaction()?;

        txn.execute(
            "INSERT OR REPLACE INTO branch_mappings (git, cvs) VALUES (?, ?)",
            params![git_branch, cvs_branch],
        )?;

        txn.execute(
            "DELETE FROM commit_branches WHERE branch = ?",
            params![git_branch],
        )?;

        let mut stmt = txn
            .prepare("INSERT INTO commit_branches (oid, branch, branch_index) VALUES (?, ?, ?)")?;
        for (i, oid) in commits.enumerate() {
            stmt.execute(params![format!("{}", *oid), git_branch, i])?;
        }
        drop(stmt);

        Ok(txn.commit()?)
    }
}
