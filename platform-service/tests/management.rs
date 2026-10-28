// SPDX-License-Identifier: AGPL-3.0-only
use axum::{body::Body, http::{Request, StatusCode}, Router};
use http_body_util::BodyExt;
use one_kvm_platform_service::{router, store, Service};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tower::ServiceExt;

struct Fixture { app: Router, pool: SqlitePool, _dir: tempfile::TempDir, admin: String }
async fn call(app: &Router, method: &str, path: &str, token: &str, data: Value) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(path).header("content-type","application/json");
    if !token.is_empty() { request = request.header("authorization",format!("Bearer {token}")); }
    let body = if data.is_null() { Body::empty() } else { Body::from(serde_json::to_vec(&data).unwrap()) };
    let response = app.clone().oneshot(request.body(body).unwrap()).await.unwrap();
    let status = response.status();let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status,serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}
async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();let pool = store::open(&dir.path().join("test.db")).await.unwrap();
    store::bootstrap(&pool,"admin","admin-test-password").await.unwrap();
    let inventory=(0..32).map(|n|one_kvm_platform_service::discovery::Candidate {
        id:format!("candidate-{n}"),label:format!("device-{n}"),
        kind:if n%2==0 {"video"}else{"control"}.into(),selector_kind:"port".into(),
        selector:format!("port-{n}"),physical_interface:format!("physical-{n}"),current_path:format!("/dev/fixture-{n}"),
    }).collect();
    let app = router(Service::with_inventory(pool.clone(),one_kvm_platform_service::discovery::Inventory::Fixed(inventory)));
    let (status,response)=call(&app,"POST","/auth/login","",json!({"username":"admin","password":"admin-test-password"})).await;
    assert_eq!(status,StatusCode::OK);
    Fixture { app,pool,_dir:dir,admin:response["token"].as_str().unwrap().into() }
}
async fn resource(f: &Fixture, n: usize, kind: &str) -> String {
    let (status,r)=call(&f.app,"POST","/resources",&f.admin,json!({"id":"ignored","name":format!("resource-{n}"),"kind":kind,"selector_kind":"port","selector":format!("port-{n}")})).await;
    assert_eq!(status,StatusCode::OK);r["id"].as_str().unwrap().into()
}
async fn channel(f: &Fixture,n: usize) -> Value {
    let video=resource(f,n*2,"video").await;let control=resource(f,n*2+1,"control").await;
    let (status,c)=call(&f.app,"POST","/channels",&f.admin,json!({"id":"ignored","name":format!("channel-{n}"),"enabled":true,"video":video,"control":control,"rustdesk_id":format!(" 固定ID-{n} ")})).await;
    assert_eq!(status,StatusCode::OK);c
}
async fn viewer(f: &Fixture) -> (String,String) {
    let (status,user)=call(&f.app,"POST","/users",&f.admin,json!({"username":"viewer","password":"viewer-test-password"})).await;
    assert_eq!(status,StatusCode::OK);
    let (status,r)=call(&f.app,"POST","/auth/login","",json!({"username":"viewer","password":"viewer-test-password"})).await;
    assert_eq!(status,StatusCode::OK);(user["id"].as_str().unwrap().into(),r["token"].as_str().unwrap().into())
}

#[tokio::test]
async fn sessions_are_hashed_logout_and_expiration_apply_to_every_request() {
    let f=fixture().await;
    let stored: String=sqlx::query_scalar("SELECT token_hash FROM sessions").fetch_one(&f.pool).await.unwrap();
    assert_ne!(stored,f.admin);assert_eq!(stored,store::token_hash(&f.admin));
    assert_eq!(call(&f.app,"GET","/users","",Value::Null).await.0,StatusCode::UNAUTHORIZED);
    assert_eq!(call(&f.app,"POST","/auth/logout",&f.admin,Value::Null).await.0,StatusCode::OK);
    assert_eq!(call(&f.app,"GET","/auth/me",&f.admin,Value::Null).await.0,StatusCode::UNAUTHORIZED);
    let (_,login)=call(&f.app,"POST","/auth/login","",json!({"username":"admin","password":"admin-test-password"})).await;
    let token=login["token"].as_str().unwrap();
    sqlx::query("UPDATE sessions SET expires=0").execute(&f.pool).await.unwrap();
    assert_eq!(call(&f.app,"GET","/auth/me",token,Value::Null).await.0,StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn user_grants_prevent_cross_channel_access_and_revoke_live_session() {
    let f=fixture().await;let a=channel(&f,1).await;let b=channel(&f,2).await;
    let (user,token)=viewer(&f).await;
    let a=a["id"].as_str().unwrap();let b=b["id"].as_str().unwrap();
    let grant=format!("/grants/{user}/{a}");
    assert_eq!(call(&f.app,"PUT",&grant,&f.admin,json!({"role":"viewer"})).await.0,StatusCode::OK);
    assert_eq!(call(&f.app,"GET",&format!("/channels/{a}/authorize/view"),&token,Value::Null).await.0,StatusCode::OK);
    assert_eq!(call(&f.app,"GET",&format!("/channels/{a}/authorize/control"),&token,Value::Null).await.0,StatusCode::FORBIDDEN);
    assert_eq!(call(&f.app,"GET",&format!("/channels/{b}/authorize/view"),&token,Value::Null).await.0,StatusCode::FORBIDDEN);
    assert_eq!(call(&f.app,"PUT",&grant,&token,json!({"role":"manager"})).await.0,StatusCode::FORBIDDEN);
    assert_eq!(call(&f.app,"GET","/resources",&token,Value::Null).await.0,StatusCode::FORBIDDEN);
    let (_,visible)=call(&f.app,"GET","/channels",&token,Value::Null).await;assert_eq!(visible.as_array().unwrap().len(),1);
    call(&f.app,"DELETE",&grant,&f.admin,Value::Null).await;
    assert_eq!(call(&f.app,"GET",&format!("/channels/{a}/authorize/view"),&token,Value::Null).await.0,StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn disable_or_password_change_invalidates_existing_user_sessions() {
    let f=fixture().await;let (user,token)=viewer(&f).await;
    call(&f.app,"PUT",&format!("/users/{user}"),&f.admin,json!({"password":"replacement-password"})).await;
    assert_eq!(call(&f.app,"GET","/auth/me",&token,Value::Null).await.0,StatusCode::UNAUTHORIZED);
    let (_,r)=call(&f.app,"POST","/auth/login","",json!({"username":"viewer","password":"replacement-password"})).await;
    let token=r["token"].as_str().unwrap();
    call(&f.app,"PUT",&format!("/users/{user}"),&f.admin,json!({"active":false})).await;
    assert_eq!(call(&f.app,"GET","/auth/me",token,Value::Null).await.0,StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn conflicts_and_bound_resource_deletion_fail_without_partial_write() {
    let f=fixture().await;let channel=channel(&f,1).await;let video=channel["video"].as_str().unwrap();
    assert_eq!(call(&f.app,"DELETE",&format!("/resources/{video}"),&f.admin,Value::Null).await.0,StatusCode::BAD_REQUEST);
    let mut conflict=channel.clone();conflict["id"]=json!("arbitrary");
    assert_eq!(call(&f.app,"POST","/channels",&f.admin,conflict).await.0,StatusCode::BAD_REQUEST);
    let count: i64=sqlx::query_scalar("SELECT COUNT(*) FROM channels").fetch_one(&f.pool).await.unwrap();assert_eq!(count,1);
    let audit: String=sqlx::query_scalar("SELECT group_concat(action,' ') FROM audit").fetch_one(&f.pool).await.unwrap();
    assert!(!audit.contains("admin-test-password"));
}

#[tokio::test]
async fn restart_retains_channels_grants_and_manual_rustdesk_id() {
    let f=fixture().await;let c=channel(&f,5).await;let (user,token)=viewer(&f).await;
    let id=c["id"].as_str().unwrap();
    call(&f.app,"PUT",&format!("/grants/{user}/{id}"),&f.admin,json!({"role":"operator"})).await;
    let reopened=store::open(&f._dir.path().join("test.db")).await.unwrap();
    let app=router(Service::new(reopened));
    let (_,visible)=call(&app,"GET","/channels",&token,Value::Null).await;
    assert_eq!(visible[0]["rustdesk_id"],json!(" 固定ID-5 "));
    assert_eq!(call(&app,"GET",&format!("/channels/{id}/authorize/control"),&token,Value::Null).await.0,StatusCode::OK);
}

#[tokio::test]
async fn bootstrap_cannot_overwrite_existing_accounts_or_future_schema() {
    let f=fixture().await;
    assert!(store::bootstrap(&f.pool,"another","another-password").await.is_err());
    sqlx::query("PRAGMA user_version=99").execute(&f.pool).await.unwrap();
    assert!(store::open(&f._dir.path().join("test.db")).await.is_err());
}

#[tokio::test]
async fn discovery_requires_admin_and_registration_rejects_arbitrary_paths() {
    let f=fixture().await;let (_,token)=viewer(&f).await;
    assert_eq!(call(&f.app,"GET","/discovery",&token,Value::Null).await.0,StatusCode::FORBIDDEN);
    let (status,items)=call(&f.app,"GET","/discovery",&f.admin,Value::Null).await;
    assert_eq!(status,StatusCode::OK);assert!(!items.to_string().contains("current_path"));
    assert_eq!(call(&f.app,"POST","/resources/register",&f.admin,json!({"candidate_id":"missing","name":"bad"})).await.0,StatusCode::BAD_REQUEST);
    assert_eq!(call(&f.app,"POST","/resources/register",&f.admin,json!({"candidate_id":"candidate-0","name":"camera"})).await.0,StatusCode::OK);
    assert_eq!(call(&f.app,"POST","/resources",&f.admin,json!({"id":"x","name":"bad","kind":"video","selector_kind":"port","selector":"/etc/shadow"})).await.0,StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn management_shell_is_public_but_private_data_requires_authentication() {
    let f=fixture().await;
    let response=f.app.clone().oneshot(Request::builder().uri("/").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(),StatusCode::OK);
    assert_eq!(response.headers()["cache-control"],"no-store");
    assert!(response.headers()["content-security-policy"].to_str().unwrap().contains("frame-ancestors 'none'"));
    let html=String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(html.contains("platform.js"));assert!(!html.contains(&f.admin));
    assert_eq!(call(&f.app,"GET","/channels","",Value::Null).await.0,StatusCode::UNAUTHORIZED);
    assert_eq!(call(&f.app,"GET","/discovery","",Value::Null).await.0,StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn video_sessions_are_bound_to_channel_and_login_and_closed_after_revoke() {
    use std::{net::TcpListener,io::{Read,Write}};
    let f=fixture().await;let c=channel(&f,1).await;let id=c["id"].as_str().unwrap();
    let (user,token)=viewer(&f).await;
    let (_,login)=call(&f.app,"POST","/auth/login","",json!({"username":"viewer","password":"viewer-test-password"})).await;
    let second_token=login["token"].as_str().unwrap();
    let listener=TcpListener::bind("127.0.0.1:0").unwrap();let port=listener.local_addr().unwrap().port();
    let server=std::thread::spawn(move || {
        for index in 0..2 {
            let (mut stream,_)=listener.accept().unwrap();stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
            let mut bytes=vec![];let mut byte=[0];
            while !bytes.ends_with(b"\r\n\r\n") {stream.read_exact(&mut byte).unwrap();bytes.push(byte[0]);}
            let head=String::from_utf8(bytes).unwrap();
            assert!(head.contains("Cookie: one_kvm_session=worker-private"));
            let len:usize=head.lines().find_map(|line|line.strip_prefix("Content-Length: ")).unwrap().trim().parse().unwrap();
            let mut payload=vec![0;len];stream.read_exact(&mut payload).unwrap();let body:Value=serde_json::from_slice(&payload).unwrap();
            let reply=if index==0 {assert!(!body.as_object().unwrap().contains_key("client_id"));json!({"session_id":"owned-session","sdp":"answer"})}
                else {assert!(head.starts_with("POST /api/webrtc/close "));assert_eq!(body["session_id"],"owned-session");json!({"success":true})};
            let reply=reply.to_string();write!(stream,"HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n{}",reply.len(),reply).unwrap();
        }
    });
    // Rebuild the router on the same database to attach the trusted test worker.
    let service=Service::new(f.pool.clone());service.attach_worker(id.into(),one_kvm_platform_service::gateway::WorkerEndpoint{port,session:"worker-private".into()}).await;
    let app=router(service.clone());
    let path=format!("/channels/{id}/video/offer");
    assert_eq!(call(&app,"POST",&path,&token,json!({"sdp":"offer"})).await.0,StatusCode::FORBIDDEN);
    call(&app,"PUT",&format!("/grants/{user}/{id}"),&f.admin,json!({"role":"viewer"})).await;
    let (status,reply)=call(&app,"POST",&path,&token,json!({"sdp":"offer","client_id":"someone-else"})).await;
    assert_eq!(status,StatusCode::OK);assert_eq!(reply["session_id"],"owned-session");
    assert_eq!(call(&app,"POST",&format!("/channels/{id}/video/close"),second_token,json!({"session_id":"owned-session"})).await.0,StatusCode::FORBIDDEN);
    call(&app,"DELETE",&format!("/grants/{user}/{id}"),&f.admin,Value::Null).await;
    service.sweep_video_sessions().await.unwrap();server.join().unwrap();
    assert_eq!(call(&app,"POST",&format!("/channels/{id}/video/ice"),&token,json!({"session_id":"owned-session","candidate":{}})).await.0,StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn input_requires_operator_and_exclusive_lease_and_revoke_sends_neutral_reset() {
    use std::{net::TcpListener,io::{Read,Write}};
    let f=fixture().await;let c=channel(&f,1).await;let id=c["id"].as_str().unwrap();let (user,token)=viewer(&f).await;
    let listener=TcpListener::bind("127.0.0.1:0").unwrap();let port=listener.local_addr().unwrap().port();
    let server=std::thread::spawn(move ||{
        let mut packets=vec![];
        for _ in 0..3 {
            let (mut stream,_)=listener.accept().unwrap();stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
            let mut bytes=vec![];let mut byte=[0];while !bytes.ends_with(b"\r\n\r\n"){stream.read_exact(&mut byte).unwrap();bytes.push(byte[0]);}
            let head=String::from_utf8(bytes).unwrap();assert!(head.starts_with("POST /api/platform/input "));
            let len:usize=head.lines().find_map(|line|line.strip_prefix("Content-Length: ")).unwrap().trim().parse().unwrap();
            let mut data=vec![0;len];stream.read_exact(&mut data).unwrap();packets.push(serde_json::from_slice::<Value>(&data).unwrap());
            stream.write_all(b"HTTP/1.0 200 OK\r\nContent-Length: 17\r\n\r\n{\"accepted\":true}").unwrap();
        }
        packets
    });
    let service=Service::new(f.pool.clone());service.attach_worker(id.into(),one_kvm_platform_service::gateway::WorkerEndpoint{port,session:"worker-private".into()}).await;let app=router(service.clone());
    let acquire=format!("/channels/{id}/control/acquire");let input=format!("/channels/{id}/control/input");let grant=format!("/grants/{user}/{id}");
    call(&app,"PUT",&grant,&f.admin,json!({"role":"viewer"})).await;
    assert_eq!(call(&app,"POST",&acquire,&token,Value::Null).await.0,StatusCode::FORBIDDEN);
    call(&app,"PUT",&grant,&f.admin,json!({"role":"operator"})).await;
    assert_eq!(call(&app,"POST",&input,&token,json!([1,0,4,0])).await.0,StatusCode::FORBIDDEN);
    assert_eq!(call(&app,"POST",&acquire,&token,Value::Null).await.0,StatusCode::OK);
    assert_eq!(call(&app,"POST",&acquire,&f.admin,Value::Null).await.0,StatusCode::CONFLICT);
    assert_eq!(call(&app,"POST",&input,&token,json!([1,0])).await.0,StatusCode::BAD_REQUEST);
    assert_eq!(call(&app,"POST",&input,&token,json!([1,0,4,0])).await.0,StatusCode::OK);
    call(&app,"DELETE",&grant,&f.admin,Value::Null).await;
    assert_eq!(call(&app,"POST",&input,&token,json!([1,1,4,0])).await.0,StatusCode::FORBIDDEN);
    service.sweep_controls().await.unwrap();
    assert_eq!(server.join().unwrap(),vec![json!([]),json!([1,0,4,0]),json!([])]);
}

#[tokio::test]
async fn runtime_configuration_preserves_arbitrary_key_without_returning_secrets() {
    let f=fixture().await;let channel=channel(&f,1).await;let id=channel["id"].as_str().unwrap();
    let (user,token)=viewer(&f).await;let path=format!("/channels/{id}/runtime/config");
    assert_eq!(call(&f.app,"GET",&path,&token,Value::Null).await.0,StatusCode::FORBIDDEN);
    call(&f.app,"PUT",&format!("/grants/{user}/{id}"),&f.admin,json!({"role":"manager"})).await;
    let update=json!({"fps":25,"relay_key":" arbitrary key \n","device_password":"manual-device-password","rendezvous_server":"example.invalid:21116"});
    assert_eq!(call(&f.app,"PATCH",&path,&token,update).await.0,StatusCode::OK);
    let (_,response)=call(&f.app,"GET",&path,&token,Value::Null).await;
    assert_eq!(response["rustdesk"]["key_set"],true);assert_eq!(response["rustdesk"]["password_set"],true);
    assert!(!response.to_string().contains("manual-device-password"));assert!(!response.to_string().contains("arbitrary key"));
    call(&f.app,"PATCH",&path,&token,json!({"fps":30,"device_password":""})).await;
    let raw:String=sqlx::query_scalar("SELECT value FROM metadata WHERE key=?").bind(format!("worker_template:{id}")).fetch_one(&f.pool).await.unwrap();
    let template:Value=serde_json::from_str(&raw).unwrap();assert_eq!(template["rustdesk"]["relay_key"]," arbitrary key \n");
    assert_eq!(template["rustdesk"]["device_password"],"manual-device-password");
    assert_eq!(call(&f.app,"PATCH",&path,&token,json!({"device_password":"bad"})).await.0,StatusCode::BAD_REQUEST);
    assert_eq!(call(&f.app,"PATCH",&path,&token,json!({"fps":0})).await.0,StatusCode::BAD_REQUEST);
    assert_eq!(call(&f.app,"PATCH",&path,&token,json!({"executable":"/bin/sh"})).await.0,StatusCode::UNPROCESSABLE_ENTITY);
    call(&f.app,"PATCH",&path,&token,json!({"relay_key":""})).await;
    assert_eq!(call(&f.app,"GET",&path,&token,Value::Null).await.1["rustdesk"]["key_set"],false);
    assert_eq!(call(&f.app,"POST",&format!("/channels/{id}/runtime/start"),&token,Value::Null).await.0,StatusCode::FORBIDDEN);
    assert_eq!(call(&f.app,"POST",&format!("/channels/{id}/runtime/start"),&f.admin,Value::Null).await.0,StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn login_throttle_and_unknown_payloads_do_not_escalate_privileges() {
    let f=fixture().await;
    for _ in 0..5 {
        assert_eq!(call(&f.app,"POST","/auth/login","",json!({"username":"unknown","password":"bad-password"})).await.0,StatusCode::UNAUTHORIZED);
    }
    assert_eq!(call(&f.app,"POST","/auth/login","",json!({"username":"unknown","password":"bad-password"})).await.0,StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(call(&f.app,"POST","/auth/login","",json!({"username":"unknown","password":"bad-password","admin":true})).await.0,StatusCode::UNPROCESSABLE_ENTITY);
    let (user,_)=viewer(&f).await;
    let (_,me)=call(&f.app,"GET","/auth/me",&f.admin,Value::Null).await;
    assert_eq!(call(&f.app,"PUT",&format!("/users/{}",me["id"].as_str().unwrap()),&f.admin,json!({"active":false})).await.0,StatusCode::BAD_REQUEST);
    let (_,users)=call(&f.app,"GET","/users",&f.admin,Value::Null).await;
    assert!(!users.to_string().contains("password_hash"));assert!(!user.is_empty());
}
