// SPDX-License-Identifier: AGPL-3.0-only
use crate::model::{Channel, Resource, User};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool, sqlite::{SqliteConnectOptions, SqlitePoolOptions}};
use std::{path::Path, time::{SystemTime, UNIX_EPOCH}};
use uuid::Uuid;

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64
}

pub fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

pub fn password_hash(password: &str) -> Result<String, String> {
    if password.len() < 8 || password.len() > 1024 {
        return Err("Password must contain 8 to 1024 UTF-8 bytes".into());
    }
    let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes()).map_err(|_| "Salt generation failed")?;
    Argon2::default().hash_password(password.as_bytes(), &salt)
        .map(|v| v.to_string()).map_err(|_| "Password hashing failed".into())
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).ok().is_some_and(|h| Argon2::default().verify_password(password.as_bytes(), &h).is_ok())
}

pub async fn open(path: &Path) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::new().filename(path).create_if_missing(true)
        .foreign_keys(true).busy_timeout(std::time::Duration::from_secs(5));
    let pool = SqlitePoolOptions::new().max_connections(4).connect_with(options).await?;
    let mut tx = pool.begin().await?;
    let version: i64 = sqlx::query_scalar("PRAGMA user_version").fetch_one(&mut *tx).await?;
    if version > 1 {
        return Err(sqlx::Error::Protocol("Unsupported future platform schema".into()));
    }
    if version == 0 {
        for statement in [
            "CREATE TABLE users (id TEXT PRIMARY KEY, username TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL, admin INTEGER NOT NULL, active INTEGER NOT NULL DEFAULT 1)",
            "CREATE TABLE sessions (token_hash TEXT PRIMARY KEY, user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE, expires INTEGER NOT NULL)",
            "CREATE TABLE resources (id TEXT PRIMARY KEY, body TEXT NOT NULL)",
            "CREATE TABLE channels (id TEXT PRIMARY KEY, body TEXT NOT NULL)",
            "CREATE TABLE grants (user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE, channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE, role TEXT NOT NULL CHECK(role IN ('viewer','operator','manager')), PRIMARY KEY(user_id,channel_id))",
            "CREATE TABLE audit (id INTEGER PRIMARY KEY, at INTEGER NOT NULL, actor TEXT NOT NULL, action TEXT NOT NULL, target TEXT NOT NULL)",
            "CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            "PRAGMA user_version=1",
        ] { sqlx::query(statement).execute(&mut *tx).await?; }
    }
    tx.commit().await?;
    Ok(pool)
}

pub fn user(row: &sqlx::sqlite::SqliteRow) -> User {
    User { id: row.get("id"), username: row.get("username"), admin: row.get("admin"), active: row.get("active") }
}

pub async fn bootstrap(pool: &SqlitePool, username: &str, password: &str) -> Result<(), String> {
    if username.trim().is_empty() || username.len() > 128 { return Err("Invalid username".into()); }
    let hash = password_hash(password)?;
    let mut tx = pool.begin().await.map_err(|_| "Database unavailable")?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&mut *tx).await.map_err(|_| "Database unavailable")?;
    if count != 0 { return Err("Administrator already initialized".into()); }
    sqlx::query("INSERT INTO users(id,username,password_hash,admin) VALUES(?,?,?,1)")
        .bind(Uuid::new_v4().to_string()).bind(username).bind(hash)
        .execute(&mut *tx).await.map_err(|_| "Cannot create administrator")?;
    tx.commit().await.map_err(|_| "Cannot commit administrator")?;
    Ok(())
}

pub async fn resources(pool: &SqlitePool) -> Result<Vec<Resource>, String> {
    let rows: Vec<String> = sqlx::query_scalar("SELECT body FROM resources ORDER BY id")
        .fetch_all(pool).await.map_err(|_| "Resource database error")?;
    rows.into_iter().map(|r| serde_json::from_str(&r).map_err(|_| "Invalid saved resource".into())).collect()
}

pub async fn channels(pool: &SqlitePool) -> Result<Vec<Channel>, String> {
    let rows: Vec<String> = sqlx::query_scalar("SELECT body FROM channels ORDER BY id")
        .fetch_all(pool).await.map_err(|_| "Channel database error")?;
    rows.into_iter().map(|r| serde_json::from_str(&r).map_err(|_| "Invalid saved channel".into())).collect()
}

pub async fn audit(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, actor: &str, action: &str, target: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO audit(at,actor,action,target) VALUES(?,?,?,?)")
        .bind(now()).bind(actor).bind(action).bind(target).execute(&mut **tx).await?;
    Ok(())
}
