use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use okf::{
    analyze_compliance, scan_document_root, AdmissionInventory, AdmissionLimits, BrowserRoot,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const SCHEMA_VERSION: i64 = 1;

#[derive(Clone, Debug)]
pub(crate) struct RootMonitor {
    path: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct InventoryEntry {
    pub(crate) path: String,
    pub(crate) state: String,
    pub(crate) content_hash: Option<String>,
    pub(crate) detail: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ChangeEntry {
    pub(crate) kind: String,
    pub(crate) path: String,
    pub(crate) previous_path: Option<String>,
    pub(crate) detail: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct PendingChanges {
    pub(crate) root_id: String,
    pub(crate) snapshot_digest: String,
    pub(crate) baseline_digest: Option<String>,
    pub(crate) scanned_at: u64,
    pub(crate) changes: Vec<ChangeEntry>,
    pub(crate) inventory: Vec<InventoryEntry>,
    pub(crate) confirmable: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct MonitorStatus {
    pub(crate) root_id: String,
    pub(crate) enabled: bool,
    pub(crate) initialized: bool,
    pub(crate) pending: bool,
    pub(crate) change_count: usize,
    pub(crate) last_scanned_at: Option<u64>,
    pub(crate) snapshot_digest: Option<String>,
}

impl RootMonitor {
    pub(crate) fn open(state_dir: &Path) -> Result<Self, String> {
        if fs::symlink_metadata(state_dir)
            .ok()
            .is_some_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err("root monitoring state directory must not be a symlink".to_string());
        }
        fs::create_dir_all(state_dir).map_err(|error| {
            format!("failed to create root monitoring state directory: {error}")
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(state_dir, fs::Permissions::from_mode(0o700))
                .map_err(|error| format!("failed to protect monitoring state: {error}"))?;
        }
        let monitor = Self {
            path: state_dir.join("root-monitor.sqlite"),
        };
        if let Ok(metadata) = fs::symlink_metadata(&monitor.path) {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err("root monitoring database must be a regular file".to_string());
            }
        }
        monitor.migrate().map_err(|error| error.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&monitor.path, fs::Permissions::from_mode(0o600))
                .map_err(|error| format!("failed to protect monitoring database: {error}"))?;
        }
        Ok(monitor)
    }

    fn connection(&self) -> rusqlite::Result<Connection> {
        let connection = Connection::open(&self.path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
        Ok(connection)
    }

    fn migrate(&self) -> rusqlite::Result<()> {
        let connection = self.connection()?;
        connection.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS monitor_schema (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS root_baselines (
                root_id TEXT PRIMARY KEY,
                snapshot_digest TEXT NOT NULL,
                inventory_json TEXT NOT NULL,
                scanned_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS root_pending_changes (
                root_id TEXT PRIMARY KEY,
                snapshot_digest TEXT NOT NULL,
                baseline_digest TEXT,
                pending_json TEXT NOT NULL,
                scanned_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS root_monitor_status (
                root_id TEXT PRIMARY KEY,
                last_scanned_at INTEGER,
                last_error TEXT
            );
            INSERT OR IGNORE INTO monitor_schema (version) VALUES (1);
            ",
        )?;
        let version =
            connection.query_row("SELECT MAX(version) FROM monitor_schema", [], |row| {
                row.get::<_, i64>(0)
            })?;
        if version != SCHEMA_VERSION {
            return Err(rusqlite::Error::InvalidQuery);
        }
        Ok(())
    }

    pub(crate) fn scan(&self, root: &BrowserRoot) -> Result<PendingChanges, String> {
        let root_id = root.root_id.to_string();
        let inventory = build_inventory(&root.path)?;
        let snapshot_digest = inventory_digest(&inventory);
        let scanned_at = now();
        let connection = self.connection().map_err(|error| error.to_string())?;
        let baseline = connection
            .query_row(
                "SELECT snapshot_digest, inventory_json FROM root_baselines WHERE root_id = ?1",
                [&root_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;

        if baseline.is_none() {
            connection
                .execute(
                    "INSERT INTO root_baselines (root_id, snapshot_digest, inventory_json, scanned_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![root_id, snapshot_digest, json(&inventory)?, scanned_at as i64],
                )
                .map_err(|error| error.to_string())?;
            connection
                .execute(
                    "INSERT INTO root_monitor_status (root_id, last_scanned_at, last_error)
                     VALUES (?1, ?2, NULL)
                     ON CONFLICT(root_id) DO UPDATE SET last_scanned_at = excluded.last_scanned_at,
                         last_error = NULL",
                    params![root_id, scanned_at as i64],
                )
                .map_err(|error| error.to_string())?;
            return Ok(PendingChanges {
                root_id,
                snapshot_digest,
                baseline_digest: None,
                scanned_at,
                changes: Vec::new(),
                inventory,
                confirmable: true,
            });
        }

        let (baseline_digest, baseline_json) = baseline.expect("checked baseline");
        let baseline_inventory: Vec<InventoryEntry> =
            serde_json::from_str(&baseline_json).map_err(|error| error.to_string())?;
        let changes = compare_inventory(&baseline_inventory, &inventory);
        let pending = PendingChanges {
            root_id: root_id.clone(),
            snapshot_digest: snapshot_digest.clone(),
            baseline_digest: Some(baseline_digest.clone()),
            scanned_at,
            changes,
            inventory,
            confirmable: true,
        };
        let mut connection = connection;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        if pending.changes.is_empty() {
            transaction
                .execute(
                    "DELETE FROM root_pending_changes WHERE root_id = ?1",
                    [&root_id],
                )
                .map_err(|error| error.to_string())?;
        } else {
            transaction
                .execute(
                    "INSERT INTO root_pending_changes
                        (root_id, snapshot_digest, baseline_digest, pending_json, scanned_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(root_id) DO UPDATE SET
                        snapshot_digest = excluded.snapshot_digest,
                        baseline_digest = excluded.baseline_digest,
                        pending_json = excluded.pending_json,
                        scanned_at = excluded.scanned_at",
                    params![
                        root_id,
                        snapshot_digest,
                        baseline_digest,
                        json(&pending)?,
                        scanned_at as i64
                    ],
                )
                .map_err(|error| error.to_string())?;
        }
        transaction
            .execute(
                "INSERT INTO root_monitor_status (root_id, last_scanned_at, last_error)
                 VALUES (?1, ?2, NULL)
                 ON CONFLICT(root_id) DO UPDATE SET last_scanned_at = excluded.last_scanned_at,
                     last_error = NULL",
                params![root_id, scanned_at as i64],
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(pending)
    }

    pub(crate) fn seed_reviewed_baseline(
        &self,
        root_id: &str,
        inventory: &AdmissionInventory,
    ) -> Result<(), String> {
        let entries = inventory_entries(inventory);
        let digest = inventory_digest(&entries);
        self.connection()
            .map_err(|error| error.to_string())?
            .execute(
                "INSERT INTO root_baselines (root_id, snapshot_digest, inventory_json, scanned_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(root_id) DO NOTHING",
                params![root_id, digest, json(&entries)?, now() as i64],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub(crate) fn replace_reviewed_baseline(&self, root: &BrowserRoot) -> Result<(), String> {
        let entries = build_inventory(&root.path)?;
        let digest = inventory_digest(&entries);
        let mut connection = self.connection().map_err(|error| error.to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO root_baselines (root_id, snapshot_digest, inventory_json, scanned_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(root_id) DO UPDATE SET
                    snapshot_digest = excluded.snapshot_digest,
                    inventory_json = excluded.inventory_json,
                    scanned_at = excluded.scanned_at",
                params![
                    root.root_id.to_string(),
                    digest,
                    json(&entries)?,
                    now() as i64
                ],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "DELETE FROM root_pending_changes WHERE root_id = ?1",
                [root.root_id.to_string()],
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())
    }

    pub(crate) fn pending(&self, root_id: &str) -> Result<Option<PendingChanges>, String> {
        self.connection()
            .map_err(|error| error.to_string())?
            .query_row(
                "SELECT pending_json FROM root_pending_changes WHERE root_id = ?1",
                [root_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .map(|source| serde_json::from_str(&source).map_err(|error| error.to_string()))
            .transpose()
    }

    pub(crate) fn status(&self, roots: &[BrowserRoot]) -> Result<Vec<MonitorStatus>, String> {
        let connection = self.connection().map_err(|error| error.to_string())?;
        roots
            .iter()
            .map(|root| {
                let id = root.root_id.to_string();
                let initialized = connection
                    .query_row(
                        "SELECT 1 FROM root_baselines WHERE root_id = ?1",
                        [&id],
                        |_| Ok(()),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?
                    .is_some();
                let pending = self.pending(&id)?;
                let last_scanned_at = connection
                    .query_row(
                        "SELECT last_scanned_at FROM root_monitor_status WHERE root_id = ?1",
                        [&id],
                        |row| row.get::<_, Option<i64>>(0),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?
                    .flatten()
                    .map(|value| value as u64);
                Ok(MonitorStatus {
                    root_id: id,
                    enabled: root.check_for_changes,
                    initialized,
                    pending: pending.is_some(),
                    change_count: pending.as_ref().map_or(0, |value| value.changes.len()),
                    last_scanned_at,
                    snapshot_digest: pending.map(|value| value.snapshot_digest),
                })
            })
            .collect()
    }

    pub(crate) fn accept(
        &self,
        root: &BrowserRoot,
        expected_digest: &str,
    ) -> Result<PendingChanges, String> {
        let pending = self
            .pending(&root.root_id.to_string())?
            .ok_or_else(|| "no pending change set".to_string())?;
        if pending.snapshot_digest != expected_digest {
            return Err("pending snapshot digest does not match".to_string());
        }
        let current = build_inventory(&root.path)?;
        let current_digest = inventory_digest(&current);
        if current_digest != expected_digest {
            return Err("document root changed after review; run Check now again".to_string());
        }
        let mut connection = self.connection().map_err(|error| error.to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO root_baselines (root_id, snapshot_digest, inventory_json, scanned_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(root_id) DO UPDATE SET
                    snapshot_digest = excluded.snapshot_digest,
                    inventory_json = excluded.inventory_json,
                    scanned_at = excluded.scanned_at",
                params![
                    root.root_id.to_string(),
                    current_digest,
                    json(&current)?,
                    now() as i64
                ],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "DELETE FROM root_pending_changes WHERE root_id = ?1",
                [root.root_id.to_string()],
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(pending)
    }

    pub(crate) fn dismiss(&self, root_id: &str, expected_digest: &str) -> Result<(), String> {
        let pending = self
            .pending(root_id)?
            .ok_or_else(|| "no pending change set".to_string())?;
        if pending.snapshot_digest != expected_digest {
            return Err("pending snapshot digest does not match".to_string());
        }
        self.connection()
            .map_err(|error| error.to_string())?
            .execute(
                "DELETE FROM root_pending_changes WHERE root_id = ?1",
                [root_id],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub(crate) fn remove(&self, root_id: &str) -> Result<(), String> {
        let mut connection = self.connection().map_err(|error| error.to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        for table in [
            "root_pending_changes",
            "root_baselines",
            "root_monitor_status",
        ] {
            transaction
                .execute(
                    &format!("DELETE FROM {table} WHERE root_id = ?1"),
                    [root_id],
                )
                .map_err(|error| error.to_string())?;
        }
        transaction.commit().map_err(|error| error.to_string())
    }

    pub(crate) fn allows(
        &self,
        root_id: &str,
        root: &Path,
        relative_path: &str,
    ) -> Result<bool, String> {
        let inventory = self
            .connection()
            .map_err(|error| error.to_string())?
            .query_row(
                "SELECT inventory_json FROM root_baselines WHERE root_id = ?1",
                [root_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let Some(inventory) = inventory else {
            return Ok(true);
        };
        let entries: Vec<InventoryEntry> =
            serde_json::from_str(&inventory).map_err(|error| error.to_string())?;
        let Some(entry) = entries
            .iter()
            .find(|entry| entry.path == relative_path && entry.state == "accepted")
        else {
            return Ok(false);
        };
        let portable = okf::PortablePath::parse(relative_path.to_string())
            .map_err(|_| "invalid monitored document path".to_string())?;
        let bytes = fs::read(root.join(portable.as_str()))
            .map_err(|_| "monitored document is unavailable".to_string())?;
        Ok(entry.content_hash.as_deref() == Some(&format!("{:x}", Sha256::digest(bytes))))
    }
}

fn build_inventory(root: &Path) -> Result<Vec<InventoryEntry>, String> {
    let scanned =
        scan_document_root(root, AdmissionLimits::default()).map_err(|error| error.to_string())?;
    Ok(inventory_entries(&scanned))
}

fn inventory_entries(scanned: &AdmissionInventory) -> Vec<InventoryEntry> {
    let compliance = analyze_compliance(scanned);
    let metadata = compliance
        .diagnostics()
        .iter()
        .filter(|item| !item.path().is_empty())
        .fold(BTreeMap::<String, Vec<String>>::new(), |mut map, item| {
            map.entry(item.path().to_string())
                .or_default()
                .push(format!("{}: {}", item.code().code(), item.detail()));
            map
        });
    let mut entries = scanned
        .accepted()
        .iter()
        .map(|file| InventoryEntry {
            path: file.path().as_str().to_string(),
            state: if metadata.contains_key(file.path().as_str()) {
                "metadata_invalid".to_string()
            } else {
                "accepted".to_string()
            },
            content_hash: Some(file.content_hash().to_string()),
            detail: metadata
                .get(file.path().as_str())
                .map(|items| items.join("; ")),
        })
        .collect::<Vec<_>>();
    entries.extend(scanned.rejected().iter().map(|entry| {
        InventoryEntry {
            path: entry.display_path().to_string(),
            state: match entry.reason().code() {
                "hidden_path" => "hidden",
                "path_collision" => "conflict",
                _ => "rejected",
            }
            .to_string(),
            content_hash: None,
            detail: Some(entry.reason().code().to_string()),
        }
    }));
    entries.sort_by(|left, right| (&left.path, &left.state).cmp(&(&right.path, &right.state)));
    entries
}

fn compare_inventory(previous: &[InventoryEntry], current: &[InventoryEntry]) -> Vec<ChangeEntry> {
    let old = previous
        .iter()
        .map(|entry| (&entry.path, entry))
        .collect::<BTreeMap<_, _>>();
    let new = current
        .iter()
        .map(|entry| (&entry.path, entry))
        .collect::<BTreeMap<_, _>>();
    let mut changes = Vec::new();
    let mut deleted = BTreeSet::new();
    let mut added = BTreeSet::new();
    for (path, entry) in &old {
        match new.get(path) {
            None if entry.state == "accepted" => {
                deleted.insert((*path).clone());
            }
            Some(next) if entry.state == "accepted" && next.state != "accepted" => changes.push(
                change(classify_nonaccepted(next), path, None, next.detail.clone()),
            ),
            Some(next) if entry.content_hash != next.content_hash => changes.push(change(
                if next.state == "metadata_invalid" {
                    "metadata-invalid"
                } else {
                    "modified"
                },
                path,
                None,
                next.detail.clone(),
            )),
            Some(next) if entry.state != next.state => changes.push(change(
                classify_nonaccepted(next),
                path,
                None,
                next.detail.clone(),
            )),
            _ => {}
        }
    }
    for (path, entry) in &new {
        if !old.contains_key(path) {
            if entry.state == "accepted" {
                added.insert((*path).clone());
            } else {
                changes.push(change(
                    classify_nonaccepted(entry),
                    path,
                    None,
                    entry.detail.clone(),
                ));
            }
        }
    }
    let old_hashes = deleted
        .iter()
        .filter_map(|path| {
            old.get(path)
                .and_then(|entry| entry.content_hash.as_ref())
                .map(|hash| (hash.clone(), path.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    for path in added {
        let renamed = new
            .get(&path)
            .and_then(|entry| entry.content_hash.as_ref())
            .and_then(|hash| old_hashes.get(hash).cloned());
        if let Some(previous_path) = renamed {
            deleted.remove(&previous_path);
            changes.push(change(
                "renamed_candidate",
                &path,
                Some(previous_path),
                None,
            ));
        } else {
            changes.push(change("added", &path, None, None));
        }
    }
    for path in deleted {
        changes.push(change("deleted", &path, None, None));
    }
    changes.sort_by(|left, right| (&left.path, &left.kind).cmp(&(&right.path, &right.kind)));
    changes
}

fn classify_nonaccepted(entry: &InventoryEntry) -> &'static str {
    match entry.state.as_str() {
        "hidden" => "newly_hidden",
        "conflict" => "conflict",
        "metadata_invalid" => "metadata-invalid",
        _ => "newly_rejected",
    }
}

fn change(
    kind: &str,
    path: &str,
    previous_path: Option<String>,
    detail: Option<String>,
) -> ChangeEntry {
    ChangeEntry {
        kind: kind.to_string(),
        path: path.to_string(),
        previous_path,
        detail: detail.unwrap_or_else(|| "filesystem snapshot changed".to_string()),
    }
}

fn inventory_digest(entries: &[InventoryEntry]) -> String {
    let source = serde_json::to_vec(entries).unwrap_or_default();
    format!("{:x}", Sha256::digest(source))
}

fn json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use okf::RootId;

    fn root(path: &Path) -> BrowserRoot {
        BrowserRoot {
            root_id: RootId::parse("urn:okf:root:123e4567-e89b-42d3-a456-426614174000").unwrap(),
            mount: Some("test".to_string()),
            path: path.to_path_buf(),
            enabled: true,
            priority: 0,
            check_for_changes: true,
        }
    }

    #[test]
    fn baseline_pending_accept_and_dismiss_are_explicit() {
        let temp = std::env::temp_dir().join(format!("okf-monitor-{}", now()));
        let docs = temp.join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("index.md"), "# Index\n").unwrap();
        let monitor = RootMonitor::open(&temp.join("state")).unwrap();
        let configured = root(&docs);
        assert!(monitor.scan(&configured).unwrap().changes.is_empty());
        fs::write(
            docs.join("note.md"),
            "---\ntitle: Note\ntype: concept\n---\n# Note\n",
        )
        .unwrap();
        let pending = monitor.scan(&configured).unwrap();
        assert!(pending.changes.iter().any(|change| change.kind == "added"));
        assert!(!monitor
            .allows(&configured.root_id.to_string(), &docs, "note.md")
            .unwrap());
        monitor
            .dismiss(&configured.root_id.to_string(), &pending.snapshot_digest)
            .unwrap();
        assert!(monitor
            .pending(&configured.root_id.to_string())
            .unwrap()
            .is_none());
        let pending = monitor.scan(&configured).unwrap();
        monitor
            .accept(&configured, &pending.snapshot_digest)
            .unwrap();
        assert!(monitor
            .allows(&configured.root_id.to_string(), &docs, "note.md")
            .unwrap());
        fs::write(
            docs.join("note.md"),
            "---\ntitle: Changed\ntype: concept\n---\n# Changed\n",
        )
        .unwrap();
        assert!(!monitor
            .allows(&configured.root_id.to_string(), &docs, "note.md")
            .unwrap());
        fs::remove_dir_all(temp).unwrap();
    }
}
