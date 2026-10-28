// SPDX-License-Identifier: AGPL-3.0-only
use one_kvm_platform_service::{router, store, Service};
use std::{fs::{self, OpenOptions}, io::{self, Read}, path::PathBuf};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 3 || args[1] != "--data-dir" {
        return Err("Usage: one-kvm-platform-service --data-dir DIRECTORY [--bootstrap USERNAME]; bootstrap password is read from stdin".into());
    }
    let dir = PathBuf::from(&args[2]);
    fs::create_dir_all(&dir)?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    }
    let lock = OpenOptions::new().create(true).truncate(false).write(true).open(dir.join("manager.lock"))?;
    lock.try_lock().map_err(|_| "Another manager owns this data directory")?;
    let db = dir.join("platform.db");
    let pool = store::open(&db).await?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&db, fs::Permissions::from_mode(0o600))?;
    }
    if args.len() == 5 && args[3] == "--bootstrap" {
        let mut password = String::new(); io::stdin().take(1025).read_to_string(&mut password)?;
        let password = password.trim_end_matches(['\r','\n']);
        store::bootstrap(&pool,&args[4],password).await.map_err(io::Error::other)?;
        println!("Administrator initialized; no listener started");
        return Ok(());
    }
    if args.len() != 3 { return Err("Unknown arguments".into()); }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE active=1 AND admin=1").fetch_one(&pool).await?;
    if count == 0 { return Err("Initialize an administrator with --bootstrap before serving".into()); }
    // No network exposure until gateway/runtime isolation is complete.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:18080").await?;
    println!("Experimental metadata manager listening on 127.0.0.1:18080");
    axum::serve(listener, router(Service::new(pool))).with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    }).await?;
    drop(lock);
    Ok(())
}
