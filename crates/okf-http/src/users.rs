use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::Serialize;
use zeroize::Zeroizing;

const SCHEMA_VERSION: i64 = 3;
const PASSWORD_MIN_LENGTH: usize = 12;
const PASSWORD_MAX_LENGTH: usize = 1024;
const FAILURE_DELAY: Duration = Duration::from_millis(250);
const ARGON_MEMORY_KIB: u32 = 65_536;
const ARGON_ITERATIONS: u32 = 3;
const ARGON_LANES: u32 = 1;
const ARGON_OUTPUT_LENGTH: usize = 32;
const ROLES: &[&str] = &["admin", "editor", "voyage"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UserCommand {
    Add { name: String, password_stdin: bool },
    Passwd { name: String, password_stdin: bool },
    Disable { name: String },
    Remove { name: String },
    List,
    Grant { name: String, role: String },
    Revoke { name: String, role: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct UserSummary {
    pub name: String,
    pub enabled: bool,
    pub roles: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserAuthorization {
    pub name: String,
    pub auth_epoch: i64,
    pub capabilities: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserStore {
    database: PathBuf,
}

impl UserStore {
    pub fn from_environment() -> Result<Self, String> {
        Self::open(crate::platform::auth_database()?)
    }

    pub fn open(database: impl Into<PathBuf>) -> Result<Self, String> {
        let store = Self {
            database: database.into(),
        };
        store.initialize()?;
        Ok(store)
    }

    pub fn database_path(&self) -> &Path {
        &self.database
    }

    pub fn add_user(&self, name: &str, password: &str) -> Result<UserSummary, String> {
        validate_password(password)?;
        let name = normalize_username(name)?;
        let password_hash = hash_password(password)?;
        let now = timestamp()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(sql_error)?;
        transaction
            .execute(
                "INSERT INTO users(name, password_hash, enabled, created_at, updated_at) VALUES (?1, ?2, 1, ?3, ?3)",
                params![name, password_hash, now],
            )
            .map_err(|error| {
                if is_constraint(&error) {
                    format!("user already exists: {name}")
                } else {
                    sql_error(error)
                }
            })?;
        audit(&transaction, "user_added", Some(&name))?;
        transaction.commit().map_err(sql_error)?;
        self.user(&name)
    }

    pub fn change_password(&self, name: &str, password: &str) -> Result<(), String> {
        validate_password(password)?;
        let name = normalize_username(name)?;
        let password_hash = hash_password(password)?;
        let now = timestamp()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(sql_error)?;
        let changed = transaction
            .execute(
                "UPDATE users SET password_hash = ?1, updated_at = ?2, auth_epoch = auth_epoch + 1 WHERE name = ?3",
                params![password_hash, now, name],
            )
            .map_err(sql_error)?;
        require_user(changed, &name)?;
        audit(&transaction, "password_changed", Some(&name))?;
        transaction.commit().map_err(sql_error)
    }

    pub fn disable_user(&self, name: &str) -> Result<(), String> {
        let name = normalize_username(name)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(sql_error)?;
        let changed = transaction
            .execute(
                "UPDATE users SET enabled = 0, updated_at = ?1, auth_epoch = auth_epoch + 1 WHERE name = ?2",
                params![timestamp()?, name],
            )
            .map_err(sql_error)?;
        require_user(changed, &name)?;
        audit(&transaction, "user_disabled", Some(&name))?;
        transaction.commit().map_err(sql_error)
    }

    pub fn remove_user(&self, name: &str) -> Result<(), String> {
        let name = normalize_username(name)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(sql_error)?;
        let changed = transaction
            .execute("DELETE FROM users WHERE name = ?1", [&name])
            .map_err(sql_error)?;
        require_user(changed, &name)?;
        audit(&transaction, "user_removed", Some(&name))?;
        transaction.commit().map_err(sql_error)
    }

    pub fn grant_role(&self, name: &str, role: &str) -> Result<(), String> {
        self.change_role(name, role, true)
    }

    pub fn revoke_role(&self, name: &str, role: &str) -> Result<(), String> {
        self.change_role(name, role, false)
    }

    pub fn list_users(&self) -> Result<Vec<UserSummary>, String> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare("SELECT name, enabled, created_at, updated_at FROM users ORDER BY name")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok(UserSummary {
                    name: row.get(0)?,
                    enabled: row.get(1)?,
                    roles: Vec::new(),
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            })
            .map_err(sql_error)?;
        let mut users = rows.collect::<Result<Vec<_>, _>>().map_err(sql_error)?;
        for user in &mut users {
            user.roles = roles_for(&connection, &user.name)?;
        }
        Ok(users)
    }

    pub fn authenticate(&self, name: &str, password: &str) -> Result<bool, String> {
        self.authenticate_authorization(name, password)
            .map(|authorization| authorization.is_some())
    }

    pub fn authenticate_authorization(
        &self,
        name: &str,
        password: &str,
    ) -> Result<Option<UserAuthorization>, String> {
        let started = Instant::now();
        let normalized = normalize_username(name).ok();
        let connection = self.connection()?;
        let record = match normalized.as_deref() {
            Some(name) => connection
                .query_row(
                    "SELECT password_hash, enabled, auth_epoch FROM users WHERE name = ?1",
                    [name],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, bool>(1)?,
                            row.get::<_, i64>(2)?,
                        ))
                    },
                )
                .optional()
                .map_err(sql_error)?,
            None => None,
        };
        let candidate_hash = match record.as_ref() {
            Some((hash, _, _)) => hash.clone(),
            None => connection
                .query_row(
                    "SELECT value FROM auth_settings WHERE key = 'dummy_password_hash'",
                    [],
                    |row| row.get(0),
                )
                .map_err(sql_error)?,
        };
        let parsed = PasswordHash::new(&candidate_hash)
            .map_err(|error| format!("stored password hash is invalid: {error}"))?;
        let verified = argon2()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok();
        let success = verified && record.as_ref().is_some_and(|(_, enabled, _)| *enabled);
        if success && password_hash_needs_upgrade(&candidate_hash) {
            let upgraded = hash_password(password)?;
            connection
                .execute(
                    "UPDATE users SET password_hash = ?1, updated_at = ?2 WHERE name = ?3",
                    params![
                        upgraded,
                        timestamp()?,
                        normalized.as_deref().unwrap_or_default()
                    ],
                )
                .map_err(sql_error)?;
        }
        audit_connection(
            &connection,
            if success {
                "login_succeeded"
            } else {
                "login_failed"
            },
            success.then_some(normalized.as_deref().unwrap_or_default()),
        )?;
        if !success {
            thread::sleep(FAILURE_DELAY.saturating_sub(started.elapsed()));
        }
        if !success {
            return Ok(None);
        }
        let name = normalized.expect("successful authentication has a normalized username");
        let auth_epoch = record
            .expect("successful authentication has a user record")
            .2;
        Ok(Some(UserAuthorization {
            capabilities: capabilities_for_roles(&roles_for(&connection, &name)?),
            name,
            auth_epoch,
        }))
    }

    pub fn current_authorization(&self, name: &str) -> Result<Option<UserAuthorization>, String> {
        let name = match normalize_username(name) {
            Ok(name) => name,
            Err(_) => return Ok(None),
        };
        let connection = self.connection()?;
        let record = connection
            .query_row(
                "SELECT enabled, auth_epoch FROM users WHERE name = ?1",
                [&name],
                |row| Ok((row.get::<_, bool>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let Some((true, auth_epoch)) = record else {
            return Ok(None);
        };
        Ok(Some(UserAuthorization {
            capabilities: capabilities_for_roles(&roles_for(&connection, &name)?),
            name,
            auth_epoch,
        }))
    }

    pub fn record_security_event(&self, event: &str, subject: Option<&str>) -> Result<(), String> {
        if !event
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
        {
            return Err("security audit event name is invalid".to_string());
        }
        let normalized_subject = subject.map(normalize_username).transpose()?;
        let connection = self.connection()?;
        audit_connection(&connection, event, normalized_subject.as_deref())
    }

    fn change_role(&self, name: &str, role: &str, grant: bool) -> Result<(), String> {
        let name = normalize_username(name)?;
        let role = normalize_role(role)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(sql_error)?;
        let exists = transaction
            .query_row("SELECT 1 FROM users WHERE name = ?1", [&name], |_| Ok(()))
            .optional()
            .map_err(sql_error)?
            .is_some();
        if !exists {
            return Err(format!("user does not exist: {name}"));
        }
        if grant {
            transaction
                .execute(
                    "INSERT OR IGNORE INTO user_roles(user_name, role) VALUES (?1, ?2)",
                    params![name, role],
                )
                .map_err(sql_error)?;
            audit(&transaction, "role_granted", Some(&name))?;
        } else {
            transaction
                .execute(
                    "DELETE FROM user_roles WHERE user_name = ?1 AND role = ?2",
                    params![name, role],
                )
                .map_err(sql_error)?;
            audit(&transaction, "role_revoked", Some(&name))?;
        }
        transaction
            .execute(
                "UPDATE users SET auth_epoch = auth_epoch + 1, updated_at = ?1 WHERE name = ?2",
                params![timestamp()?, name],
            )
            .map_err(sql_error)?;
        transaction.commit().map_err(sql_error)
    }

    fn user(&self, name: &str) -> Result<UserSummary, String> {
        self.list_users()?
            .into_iter()
            .find(|user| user.name == name)
            .ok_or_else(|| format!("user does not exist: {name}"))
    }

    fn initialize(&self) -> Result<(), String> {
        let parent = self.database.parent().unwrap_or_else(|| Path::new("."));
        create_private_directory(parent)?;
        let existed = self.database.exists();
        if !existed {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            options.open(&self.database).map_err(|error| {
                format!("failed to create {}: {error}", self.database.display())
            })?;
        }
        validate_private_file(&self.database)?;
        let connection = Connection::open(&self.database).map_err(sql_error)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = DELETE;")
            .map_err(sql_error)?;
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(sql_error)?;
        match version {
            0 => connection
                .execute_batch(
                    "BEGIN IMMEDIATE;
                     CREATE TABLE IF NOT EXISTS users (
                         name TEXT PRIMARY KEY,
                         password_hash TEXT NOT NULL,
                         enabled INTEGER NOT NULL CHECK(enabled IN (0, 1)),
                         auth_epoch INTEGER NOT NULL DEFAULT 1,
                         created_at INTEGER NOT NULL,
                         updated_at INTEGER NOT NULL
                     );
                     CREATE TABLE IF NOT EXISTS user_roles (
                         user_name TEXT NOT NULL REFERENCES users(name) ON DELETE CASCADE,
                         role TEXT NOT NULL,
                         PRIMARY KEY(user_name, role)
                     );
                     CREATE TABLE IF NOT EXISTS audit_events (
                         id INTEGER PRIMARY KEY AUTOINCREMENT,
                         created_at INTEGER NOT NULL,
                         event TEXT NOT NULL,
                         subject TEXT
                     );
                     CREATE TABLE IF NOT EXISTS auth_settings (
                         key TEXT PRIMARY KEY,
                         value TEXT NOT NULL
                     );
                     PRAGMA user_version = 3;
                     COMMIT;",
                )
                .map_err(sql_error)?,
            1 => connection
                .execute_batch(
                    "BEGIN IMMEDIATE;
                     CREATE TABLE auth_settings (
                         key TEXT PRIMARY KEY,
                         value TEXT NOT NULL
                     );
                     ALTER TABLE users ADD COLUMN auth_epoch INTEGER NOT NULL DEFAULT 1;
                     PRAGMA user_version = 3;
                     COMMIT;",
                )
                .map_err(sql_error)?,
            2 => connection
                .execute_batch(
                    "BEGIN IMMEDIATE;
                     ALTER TABLE users ADD COLUMN auth_epoch INTEGER NOT NULL DEFAULT 1;
                     PRAGMA user_version = 3;
                     COMMIT;",
                )
                .map_err(sql_error)?,
            SCHEMA_VERSION => {}
            other => return Err(format!("unsupported authentication schema version {other}")),
        }
        let dummy_exists = connection
            .query_row(
                "SELECT 1 FROM auth_settings WHERE key = 'dummy_password_hash'",
                [],
                |_| Ok(()),
            )
            .optional()
            .map_err(sql_error)?
            .is_some();
        if !dummy_exists {
            let dummy_hash = hash_password("okf-internal-dummy-password-not-an-account")?;
            connection
                .execute(
                    "INSERT INTO auth_settings(key, value) VALUES ('dummy_password_hash', ?1)",
                    [&dummy_hash],
                )
                .map_err(sql_error)?;
        }
        Ok(())
    }

    fn connection(&self) -> Result<Connection, String> {
        validate_private_file(&self.database)?;
        let connection = Connection::open(&self.database).map_err(sql_error)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = DELETE;")
            .map_err(sql_error)?;
        Ok(connection)
    }
}

pub fn read_password(password_stdin: bool) -> Result<Zeroizing<String>, String> {
    if password_stdin {
        let mut password = Zeroizing::new(String::new());
        std::io::stdin()
            .read_line(&mut password)
            .map_err(|error| format!("failed to read password from stdin: {error}"))?;
        while password.ends_with(['\n', '\r']) {
            password.pop();
        }
        validate_password(&password)?;
        return Ok(password);
    }
    let first = Zeroizing::new(
        rpassword::prompt_password("Password: ")
            .map_err(|error| format!("failed to read password from terminal: {error}"))?,
    );
    let second = Zeroizing::new(
        rpassword::prompt_password("Confirm password: ")
            .map_err(|error| format!("failed to confirm password from terminal: {error}"))?,
    );
    if *first != *second {
        return Err("passwords do not match".to_string());
    }
    validate_password(&first)?;
    Ok(first)
}

fn argon2() -> Argon2<'static> {
    let params = Params::new(
        ARGON_MEMORY_KIB,
        ARGON_ITERATIONS,
        ARGON_LANES,
        Some(ARGON_OUTPUT_LENGTH),
    )
    .expect("fixed Argon2 parameters are valid");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

fn hash_password(password: &str) -> Result<String, String> {
    let mut salt_bytes = [0u8; 16];
    getrandom::getrandom(&mut salt_bytes)
        .map_err(|error| format!("failed to generate password salt: {error}"))?;
    let salt = SaltString::encode_b64(&salt_bytes)
        .map_err(|error| format!("failed to encode password salt: {error}"))?;
    argon2()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| format!("failed to hash password: {error}"))
}

fn password_hash_needs_upgrade(hash: &str) -> bool {
    !hash.starts_with("$argon2id$v=19$m=65536,t=3,p=1$")
}

fn validate_password(password: &str) -> Result<(), String> {
    if password.len() < PASSWORD_MIN_LENGTH {
        return Err(format!(
            "password must contain at least {PASSWORD_MIN_LENGTH} bytes"
        ));
    }
    if password.len() > PASSWORD_MAX_LENGTH || password.contains('\0') {
        return Err("password is too long or contains a NUL byte".to_string());
    }
    Ok(())
}

fn normalize_username(name: &str) -> Result<String, String> {
    let normalized = name.trim().to_ascii_lowercase();
    if !(3..=32).contains(&normalized.len())
        || !normalized.is_ascii()
        || !normalized.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphanumeric() || (index > 0 && b"._-".contains(&byte))
        })
        || !normalized.as_bytes()[0].is_ascii_alphanumeric()
    {
        return Err(
            "username must be 3-32 ASCII characters, start with a letter or digit, and contain only letters, digits, '.', '_' or '-'"
                .to_string(),
        );
    }
    Ok(normalized)
}

fn normalize_role(role: &str) -> Result<String, String> {
    let role = role.trim().to_ascii_lowercase();
    if !ROLES.contains(&role.as_str()) {
        return Err(format!(
            "unknown role {role:?}; expected one of: {}",
            ROLES.join(", ")
        ));
    }
    Ok(role)
}

fn roles_for(connection: &Connection, name: &str) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare("SELECT role FROM user_roles WHERE user_name = ?1 ORDER BY role")
        .map_err(sql_error)?;
    let rows = statement
        .query_map([name], |row| row.get(0))
        .map_err(sql_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql_error)
}

fn capabilities_for_roles(roles: &[String]) -> Vec<String> {
    let mut capabilities = std::collections::BTreeSet::from([
        "content.read",
        "password.change",
        "session.logout",
        "session.recover",
    ]);
    if roles
        .iter()
        .any(|role| matches!(role.as_str(), "editor" | "admin"))
    {
        capabilities.extend([
            "content.initialize",
            "content.write",
            "derived.rebuild",
            "review.decide",
            "roots.configure",
            "roots.propose",
            "session.logout",
            "session.recover",
        ]);
    }
    if roles.iter().any(|role| role == "voyage") {
        capabilities.insert("voyage.spend");
    }
    if roles.iter().any(|role| role == "admin") {
        capabilities.extend(["security.manage", "users.manage"]);
    }
    capabilities.into_iter().map(str::to_string).collect()
}

fn audit(transaction: &Transaction<'_>, event: &str, subject: Option<&str>) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO audit_events(created_at, event, subject) VALUES (?1, ?2, ?3)",
            params![timestamp()?, event, subject],
        )
        .map(|_| ())
        .map_err(sql_error)
}

fn audit_connection(
    connection: &Connection,
    event: &str,
    subject: Option<&str>,
) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO audit_events(created_at, event, subject) VALUES (?1, ?2, ?3)",
            params![timestamp()?, event, subject],
        )
        .map(|_| ())
        .map_err(sql_error)
}

fn require_user(changed: usize, name: &str) -> Result<(), String> {
    (changed != 0)
        .then_some(())
        .ok_or_else(|| format!("user does not exist: {name}"))
}

fn timestamp() -> Result<i64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .map_err(|_| "system clock is before the Unix epoch".to_string())
}

fn create_private_directory(path: &Path) -> Result<(), String> {
    if path.exists() {
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            format!(
                "failed to inspect private state {}: {error}",
                path.display()
            )
        })?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            return Err(format!(
                "private state path must be a real directory, not a symlink: {}",
                path.display()
            ));
        }
    }
    fs::create_dir_all(path)
        .map_err(|error| format!("failed to create private state {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("failed to protect private state: {error}"))?;
    }
    Ok(())
}

fn validate_private_file(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(format!(
            "credential database must be a regular file: {}",
            path.display()
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.mode() & 0o077 != 0 {
            return Err(
                "credential database permissions must be owner-only (mode 600)".to_string(),
            );
        }
    }
    Ok(())
}

fn is_constraint(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(code, _)
            if code.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

fn sql_error(error: rusqlite::Error) -> String {
    format!("authentication database error: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        root: PathBuf,
        store: UserStore,
    }

    impl Fixture {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("okf-users-{unique}"));
            let store = UserStore::open(root.join("okf/auth.sqlite")).expect("open user store");
            Self { root, store }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn user_lifecycle_stores_hashes_roles_and_disablement() {
        let fixture = Fixture::new();
        let user = fixture
            .store
            .add_user("Alice", "correct horse battery staple")
            .expect("add user");
        assert_eq!(user.name, "alice");
        assert!(fixture
            .store
            .authenticate("ALICE", "correct horse battery staple")
            .unwrap());
        fixture.store.grant_role("alice", "editor").unwrap();
        fixture.store.grant_role("alice", "voyage").unwrap();
        assert_eq!(
            fixture.store.list_users().unwrap()[0].roles,
            ["editor", "voyage"]
        );
        fixture.store.disable_user("alice").unwrap();
        assert!(!fixture
            .store
            .authenticate("alice", "correct horse battery staple")
            .unwrap());
        fixture.store.remove_user("alice").unwrap();
        assert!(fixture.store.list_users().unwrap().is_empty());
        assert!(!fixture
            .store
            .authenticate("alice", "correct horse battery staple")
            .unwrap());

        let bytes = fs::read(fixture.store.database_path()).unwrap();
        assert!(!bytes
            .windows(b"correct horse battery staple".len())
            .any(|window| window == b"correct horse battery staple"));
        let connection = fixture.store.connection().unwrap();
        let audit_events: i64 = connection
            .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
            .unwrap();
        assert!(audit_events >= 7);
        let failed_subject: Option<String> = connection
            .query_row(
                "SELECT subject FROM audit_events WHERE event = 'login_failed' ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(failed_subject.is_none());
    }

    #[test]
    fn duplicate_case_and_confusable_names_are_rejected() {
        let fixture = Fixture::new();
        fixture
            .store
            .add_user("Alice", "correct horse battery staple")
            .unwrap();
        assert!(fixture
            .store
            .add_user("alice", "another secure password")
            .is_err());
        assert!(fixture
            .store
            .add_user("al\u{0438}ce", "another secure password")
            .is_err());
    }

    #[test]
    fn password_change_and_unknown_login_have_uniform_public_result() {
        let fixture = Fixture::new();
        fixture
            .store
            .add_user("alice", "correct horse battery staple")
            .unwrap();
        fixture
            .store
            .change_password("alice", "new correct horse battery staple")
            .unwrap();
        assert!(!fixture
            .store
            .authenticate("alice", "correct horse battery staple")
            .unwrap());
        assert!(fixture
            .store
            .authenticate("alice", "new correct horse battery staple")
            .unwrap());
        let started = Instant::now();
        assert!(!fixture
            .store
            .authenticate("missing", "new correct horse battery staple")
            .unwrap());
        assert!(started.elapsed() >= FAILURE_DELAY);
    }

    #[test]
    fn schema_zero_migrates_and_future_schema_is_rejected() {
        let fixture = Fixture::new();
        let connection = fixture.store.connection().unwrap();
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            SCHEMA_VERSION
        );
        connection
            .execute("ALTER TABLE users DROP COLUMN auth_epoch", [])
            .unwrap();
        connection.pragma_update(None, "user_version", 2).unwrap();
        drop(connection);
        UserStore::open(fixture.store.database_path()).expect("migrate schema two to three");
        let connection = fixture.store.connection().unwrap();
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            SCHEMA_VERSION
        );
        connection
            .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        drop(connection);
        assert!(UserStore::open(fixture.store.database_path()).is_err());
    }

    #[test]
    fn successful_login_upgrades_older_argon2_parameters() {
        let fixture = Fixture::new();
        fixture
            .store
            .add_user("alice", "correct horse battery staple")
            .unwrap();
        let weak_params = Params::new(8_192, 1, 1, Some(32)).unwrap();
        let weak_argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, weak_params);
        let salt = SaltString::encode_b64(&[7u8; 16]).unwrap();
        let weak_hash = weak_argon2
            .hash_password(b"correct horse battery staple", &salt)
            .unwrap()
            .to_string();
        let connection = fixture.store.connection().unwrap();
        connection
            .execute(
                "UPDATE users SET password_hash = ?1 WHERE name = 'alice'",
                [&weak_hash],
            )
            .unwrap();
        assert!(fixture
            .store
            .authenticate("alice", "correct horse battery staple")
            .unwrap());
        let upgraded: String = connection
            .query_row(
                "SELECT password_hash FROM users WHERE name = 'alice'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(upgraded.starts_with("$argon2id$v=19$m=65536,t=3,p=1$"));
    }

    #[cfg(unix)]
    #[test]
    fn credential_state_rejects_symlinked_directory_and_broad_database_permissions() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("okf-users-security-{unique}"));
        let target = root.join("target");
        let linked = root.join("linked");
        fs::create_dir_all(&target).unwrap();
        symlink(&target, &linked).unwrap();
        assert!(UserStore::open(linked.join("auth.sqlite")).is_err());

        let store = UserStore::open(root.join("real/auth.sqlite")).unwrap();
        fs::set_permissions(store.database_path(), fs::Permissions::from_mode(0o644)).unwrap();
        assert!(store.list_users().is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
