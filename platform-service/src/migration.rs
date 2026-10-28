// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit offline import. The legacy database is opened read-only and never
//! modified. New data and an idempotency marker commit in one transaction.
use crate::{discovery::Candidate, model::Channel, store};
use argon2::PasswordHash;
use serde_json::Value;
#[cfg(test)]
use serde_json::json;
use sqlx::{Row, SqlitePool, sqlite::{SqliteConnectOptions,SqlitePoolOptions}};
use std::path::Path;
use uuid::Uuid;

pub async fn import_legacy(pool:&SqlitePool, source:&Path, backup_dir:&Path, inventory:&[Candidate]) -> Result<String,String> {
    let existing:Option<String>=sqlx::query_scalar("SELECT value FROM metadata WHERE key='legacy_import_channel'")
        .fetch_optional(pool).await.map_err(|_|"Cannot read migration state")?;
    if let Some(id)=existing {return Ok(id);}
    let legacy=SqlitePoolOptions::new().max_connections(1).connect_with(
        SqliteConnectOptions::new().filename(source).read_only(true).create_if_missing(false))
        .await.map_err(|_|"Cannot open legacy database read-only")?;
    std::fs::create_dir_all(backup_dir).map_err(|_|"Cannot create backup directory")?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(backup_dir,std::fs::Permissions::from_mode(0o700)).map_err(|_|"Cannot secure backup directory")?;
    }
    let backup=backup_dir.join(format!("legacy-{}.db",Uuid::new_v4()));
    sqlx::query("VACUUM INTO ?").bind(backup.to_str().ok_or("Invalid backup path")?).execute(&legacy).await.map_err(|_|"Cannot snapshot legacy database")?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&backup,std::fs::Permissions::from_mode(0o600)).map_err(|_|"Cannot secure legacy snapshot")?;
    }
    legacy.close().await;
    let legacy=SqlitePoolOptions::new().max_connections(1).connect_with(
        SqliteConnectOptions::new().filename(&backup).read_only(true).create_if_missing(false))
        .await.map_err(|_|"Cannot open consistent legacy snapshot")?;
    let users=sqlx::query("SELECT id,username,password_hash FROM users").fetch_all(&legacy).await.map_err(|_|"Invalid legacy users")?;
    if users.len()!=1 {return Err("Legacy import requires exactly one account".into());}
    let user=&users[0];let user_id:String=user.get("id");let username:String=user.get("username");let hash:String=user.get("password_hash");
    let parsed=PasswordHash::new(&hash).map_err(|_|"Unsupported legacy password hash")?;
    if !matches!(parsed.algorithm.as_str(),"argon2id"|"argon2i"|"argon2d"){return Err("Unsupported legacy password algorithm".into());}
    let totp_table:Option<String>=sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' AND name='user_totp_credentials'")
        .fetch_optional(&legacy).await.map_err(|_|"Cannot inspect legacy schema")?;
    if totp_table.is_some() {
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM user_totp_credentials").fetch_one(&legacy).await.map_err(|_|"Cannot inspect MFA state")?;
        if count!=0 {return Err("MFA migration is required before importing this account".into());}
    }
    let raw:String=sqlx::query_scalar("SELECT value FROM config WHERE key='app_config'").fetch_one(&legacy).await.map_err(|_|"Missing legacy config")?;
    let config:Value=serde_json::from_str(&raw).map_err(|_|"Invalid legacy config")?;
    let video=config.pointer("/video/device").and_then(Value::as_str).ok_or("Legacy video source must be explicit")?;
    let choose=|kind:&str,path:&str|->Result<&Candidate,String>{
        let devices:Vec<_>=inventory.iter().filter(|d|d.kind==kind && d.current_path==path).collect();
        if devices.len()!=1 {return Err("Legacy device is missing or ambiguous".into());}
        Ok(devices[0])
    };
    let video=choose("video",video)?;
    let backend=config.pointer("/hid/backend").and_then(Value::as_str).unwrap_or("none");
    let control=match backend {
        "none"=>None,
        "otg"=>{
            let udc=config.pointer("/hid/otg_udc").and_then(Value::as_str).ok_or("Legacy OTG UDC must be explicit")?;
            Some(choose("control",&format!("udc:{udc}"))?)
        },
        "ch9329"=>Some(choose("control",config.pointer("/hid/ch9329_port").and_then(Value::as_str).ok_or("Missing serial port")?)?),
        _=>return Err("Unsupported legacy control backend".into()),
    };
    let channel_id=Uuid::new_v4().to_string();let video=video.resource("Imported video".into(),Uuid::new_v4().to_string());
    let control=control.map(|c|c.resource("Imported control".into(),Uuid::new_v4().to_string()));
    let channel=Channel {id:channel_id.clone(),name:"Imported computer".into(),enabled:false,video:video.id.clone(),
        control:control.as_ref().map(|c|c.id.clone()),rustdesk_id:config.pointer("/rustdesk/device_id").and_then(Value::as_str).map(str::to_owned)};
    let mut tx=pool.begin().await.map_err(|_|"Cannot start migration")?;
    let users_count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&mut *tx).await.map_err(|_|"Cannot inspect target")?;
    let channel_count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM channels").fetch_one(&mut *tx).await.map_err(|_|"Cannot inspect target")?;
    let resource_count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM resources").fetch_one(&mut *tx).await.map_err(|_|"Cannot inspect target")?;
    if users_count!=0 || channel_count!=0 || resource_count!=0 {return Err("Legacy import requires an empty target".into());}
    sqlx::query("INSERT INTO users(id,username,password_hash,admin,active) VALUES(?,?,?,1,1)")
        .bind(&user_id).bind(&username).bind(&hash).execute(&mut *tx).await.map_err(|_|"Cannot import administrator")?;
    for r in std::iter::once(&video).chain(control.iter()) {
        sqlx::query("INSERT INTO resources(id,body) VALUES(?,?)").bind(&r.id).bind(serde_json::to_string(r).unwrap())
            .execute(&mut *tx).await.map_err(|_|"Cannot import resource")?;
    }
    sqlx::query("INSERT INTO channels(id,body) VALUES(?,?)").bind(&channel_id).bind(serde_json::to_string(&channel).unwrap())
        .execute(&mut *tx).await.map_err(|_|"Cannot import channel")?;
    // Private runtime template: never returned by channel/config HTTP responses.
    for (key,value) in [("legacy_import_channel".to_owned(),channel_id.clone()),
        (format!("worker_template:{channel_id}"),raw),
        ("legacy_backup".to_owned(),backup.to_string_lossy().into_owned())] {
        sqlx::query("INSERT INTO metadata(key,value) VALUES(?,?)").bind(key).bind(value).execute(&mut *tx).await.map_err(|_|"Cannot save migration state")?;
    }
    store::audit(&mut tx,&user_id,"import_legacy",&channel_id).await.map_err(|_|"Cannot audit migration")?;
    tx.commit().await.map_err(|_|"Cannot commit migration")?;
    legacy.close().await;
    Ok(channel_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn fixture()->(tempfile::TempDir,SqlitePool,std::path::PathBuf,Vec<Candidate>) {
        let dir=tempfile::tempdir().unwrap();let source=dir.path().join("legacy.db");let old=store::open(&source).await.unwrap();
        store::bootstrap(&old,"legacy-admin","original-password").await.unwrap();
        sqlx::query("CREATE TABLE config(key TEXT PRIMARY KEY,value TEXT)").execute(&old).await.unwrap();
        let config=json!({"video":{"device":"/dev/video42"},"hid":{"backend":"otg","otg_udc":"fe980000.usb"},
            "rustdesk":{"device_id":" 鎵嬪～ID ","relay_key":"custom-secret","device_password":"device-secret"}});
        sqlx::query("INSERT INTO config VALUES('app_config',?)").bind(config.to_string()).execute(&old).await.unwrap();old.close().await;
        let pool=store::open(&dir.path().join("platform.db")).await.unwrap();
        let inventory=vec![Candidate{id:"video".into(),label:"CSI".into(),kind:"video".into(),selector_kind:"builtin".into(),selector:"csi".into(),physical_interface:"csi".into(),current_path:"/dev/video42".into()},
            Candidate{id:"otg".into(),label:"OTG".into(),kind:"control".into(),selector_kind:"builtin".into(),selector:"udc:fe980000.usb".into(),physical_interface:"udc".into(),current_path:"udc:fe980000.usb".into()}];
        (dir,pool,source,inventory)
    }
    #[tokio::test]
    async fn import_preserves_password_and_secrets_without_exposing_them_or_starting_channel() {
        let (dir,pool,source,inventory)=fixture().await;let before=std::fs::read(&source).unwrap();
        let id=import_legacy(&pool,&source,&dir.path().join("backup"),&inventory).await.unwrap();
        assert_eq!(id,import_legacy(&pool,&source,&dir.path().join("backup"),&[]).await.unwrap());
        assert_eq!(before,std::fs::read(&source).unwrap());
        let channels=store::channels(&pool).await.unwrap();assert_eq!(channels.len(),1);assert!(!channels[0].enabled);
        assert_eq!(channels[0].rustdesk_id.as_deref(),Some(" 鎵嬪～ID "));
        assert!(!serde_json::to_string(&channels).unwrap().contains("secret"));
        let hash:String=sqlx::query_scalar("SELECT password_hash FROM users").fetch_one(&pool).await.unwrap();assert!(store::verify_password("original-password",&hash));
        let template:String=sqlx::query_scalar("SELECT value FROM metadata WHERE key LIKE 'worker_template:%'").fetch_one(&pool).await.unwrap();
        assert!(template.contains("custom-secret"));assert!(template.contains("device-secret"));
        let backup:String=sqlx::query_scalar("SELECT value FROM metadata WHERE key='legacy_backup'").fetch_one(&pool).await.unwrap();assert!(Path::new(&backup).exists());
    }
    #[tokio::test]
    async fn missing_or_ambiguous_hardware_causes_no_partial_import() {
        let (dir,pool,source,mut inventory)=fixture().await;
        assert!(import_legacy(&pool,&source,&dir.path().join("backup"),&[]).await.is_err());
        inventory.push(inventory[0].clone());assert!(import_legacy(&pool,&source,&dir.path().join("backup"),&inventory).await.is_err());
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&pool).await.unwrap();assert_eq!(count,0);
    }
    #[tokio::test]
    async fn mfa_is_never_silently_downgraded() {
        let (dir,pool,source,inventory)=fixture().await;let old=store::open(&source).await.unwrap();
        sqlx::query("CREATE TABLE user_totp_credentials(user_id TEXT)").execute(&old).await.unwrap();
        sqlx::query("INSERT INTO user_totp_credentials VALUES('mfa-user')").execute(&old).await.unwrap();old.close().await;
        assert!(import_legacy(&pool,&source,&dir.path().join("backup"),&inventory).await.unwrap_err().contains("MFA"));
    }
}
