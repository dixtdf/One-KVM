// SPDX-License-Identifier: AGPL-3.0-only
//! Isolated upstream-compatible worker database. The worker gets a random
//! internal account, never a user's platform session or password.
use crate::{runtime::WorkerPlan,store};
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions,SqlitePoolOptions};
use std::{fs,path::Path};
use uuid::Uuid;

pub struct InternalCredentials {pub username:String,pub password:String}

pub async fn prepare(directory:&Path,plan:&WorkerPlan)->Result<InternalCredentials,String> {
    fs::create_dir_all(directory).map_err(|_|"Cannot create worker directory")?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory,fs::Permissions::from_mode(0o700)).map_err(|_|"Cannot secure worker directory")?;
    }
    // Caller must ensure the previous process is fully stopped before calling.
    let path=directory.join("one-kvm.db");
    let pool=SqlitePoolOptions::new().max_connections(1).connect_with(
        SqliteConnectOptions::new().filename(&path).create_if_missing(true))
        .await.map_err(|_|"Cannot open worker database")?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path,fs::Permissions::from_mode(0o600)).map_err(|_|"Cannot secure worker database")?;
    }
    let credentials=InternalCredentials{username:"platform-worker".into(),password:format!("{}{}",Uuid::new_v4().simple(),Uuid::new_v4().simple())};
    let hash=store::password_hash(&credentials.password)?;
    let mut config=plan.private_config.clone();config["initialized"]=Value::Bool(true);
    let mut tx=pool.begin().await.map_err(|_|"Cannot start worker config transaction")?;
    for statement in [
        "CREATE TABLE IF NOT EXISTS config(key TEXT PRIMARY KEY,value TEXT NOT NULL,updated_at TEXT NOT NULL DEFAULT (datetime('now')))",
        "CREATE TABLE IF NOT EXISTS users(id TEXT PRIMARY KEY,username TEXT NOT NULL UNIQUE,password_hash TEXT NOT NULL,created_at TEXT NOT NULL DEFAULT (datetime('now')),updated_at TEXT NOT NULL DEFAULT (datetime('now')))",
        "DELETE FROM users",
    ] {sqlx::query(statement).execute(&mut *tx).await.map_err(|_|"Cannot initialize worker tables")?;}
    sqlx::query("INSERT INTO config(key,value) VALUES('app_config',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value")
        .bind(serde_json::to_string(&config).map_err(|_|"Invalid worker configuration")?).execute(&mut *tx).await.map_err(|_|"Cannot write worker config")?;
    sqlx::query("INSERT INTO users(id,username,password_hash) VALUES(?,?,?)")
        .bind(Uuid::new_v4().to_string()).bind(&credentials.username).bind(hash)
        .execute(&mut *tx).await.map_err(|_|"Cannot create internal worker account")?;
    tx.commit().await.map_err(|_|"Cannot commit worker database")?;
    pool.close().await;
    Ok(credentials)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    #[tokio::test]
    async fn credentials_are_per_worker_rotated_and_never_in_config() {
        let t=tempfile::tempdir().unwrap();
        let plan=WorkerPlan{private_config:serde_json::json!({"rustdesk":{"device_password":"device-only"},"initialized":false}),physical_interfaces:BTreeSet::new()};
        let first=prepare(&t.path().join("a"),&plan).await.unwrap();
        let other=prepare(&t.path().join("b"),&plan).await.unwrap();assert_ne!(first.password,other.password);
        let next=prepare(&t.path().join("a"),&plan).await.unwrap();assert_ne!(first.password,next.password);
        let pool=SqlitePoolOptions::new().connect_with(SqliteConnectOptions::new().filename(t.path().join("a/one-kvm.db")).read_only(true)).await.unwrap();
        let hash:String=sqlx::query_scalar("SELECT password_hash FROM users").fetch_one(&pool).await.unwrap();
        assert!(store::verify_password(&next.password,&hash));assert!(!store::verify_password(&first.password,&hash));
        let raw:String=sqlx::query_scalar("SELECT value FROM config WHERE key='app_config'").fetch_one(&pool).await.unwrap();
        assert!(!raw.contains(&next.password));let config:Value=serde_json::from_str(&raw).unwrap();
        assert_eq!(config["initialized"],true);assert_eq!(config["rustdesk"]["device_password"],"device-only");
    }
}
