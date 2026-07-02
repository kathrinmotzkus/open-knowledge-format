use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(crate) struct ReadOnlyStorageSummary {
    pub(crate) path: PathBuf,
    pub(crate) schema_version: i64,
    pub(crate) documents: i64,
    pub(crate) chunks: i64,
    pub(crate) embeddings: i64,
}

pub(crate) fn inspect_read_only(
    index_root: &Path,
) -> rusqlite::Result<Option<ReadOnlyStorageSummary>> {
    let path = index_root.join("okf.sqlite");
    if !path.is_file() {
        return Ok(None);
    }
    let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    Ok(Some(ReadOnlyStorageSummary {
        path,
        schema_version: connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get::<_, Option<i64>>(0)
            })?
            .unwrap_or(0),
        documents: count(&connection, "documents")?,
        chunks: count(&connection, "chunks")?,
        embeddings: count(&connection, "embeddings")?,
    }))
}

fn count(connection: &Connection, table: &str) -> rusqlite::Result<i64> {
    connection.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })
}
