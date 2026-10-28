// SPDX-License-Identifier: AGPL-3.0-only
use crate::{model::{Channel, Principal, Resource, User}, store};
use axum::{Json, Router, extract::{Path, State, Request, DefaultBodyLimit},
    http::StatusCode, middleware::{self, Next}, response::{IntoResponse, Response, Html},
    routing::{get, post, put, delete}, Extension};
use one_kvm_platform_core::{self as core, Permission};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::{Mutex, Semaphore};
use uuid::Uuid;

pub struct Service {
    pub pool: SqlitePool,
    mutation: Mutex<()>,
    login_failures: Mutex<BTreeMap<String, (i64, usize)>>,
    hashing_slots: Semaphore,
    dummy_hash: String,
    inventory: crate::discovery::Inventory,
    workers:Mutex<BTreeMap<String,crate::gateway::WorkerEndpoint>>,
    video_sessions:Mutex<BTreeMap<String,VideoOwner>>,
}
struct VideoOwner { user:String, token_hash:String, channel:String, endpoint:crate::gateway::WorkerEndpoint }

impl Service {
    pub fn new(pool: SqlitePool) -> Arc<Self> {
        Self::with_inventory(pool, crate::discovery::Inventory::System {
            sys: "/sys".into(), dev: "/dev".into(),
        })
    }
    pub fn with_inventory(pool: SqlitePool, inventory: crate::discovery::Inventory) -> Arc<Self> {
        Arc::new(Self { pool, mutation: Mutex::new(()), login_failures: Mutex::new(BTreeMap::new()),
            inventory,
            workers:Mutex::new(BTreeMap::new()),video_sessions:Mutex::new(BTreeMap::new()),
            hashing_slots: Semaphore::new(4),
            dummy_hash: store::password_hash(&Uuid::new_v4().to_string()).expect("Hash initialization failed") })
    }
    /// Called only by the trusted supervisor after worker readiness/login.
    pub async fn attach_worker(&self,channel:String,endpoint:crate::gateway::WorkerEndpoint) {
        self.workers.lock().await.insert(channel,endpoint);
    }
}

type Result<T> = std::result::Result<T, Error>;
pub struct Error(StatusCode, &'static str);
impl IntoResponse for Error {
    fn into_response(self) -> Response { (self.0, Json(json!({"error": self.1}))).into_response() }
}
impl From<sqlx::Error> for Error {
    fn from(_: sqlx::Error) -> Self { Self(StatusCode::INTERNAL_SERVER_ERROR, "Database operation failed") }
}
fn bad(message: &'static str) -> Error { Error(StatusCode::BAD_REQUEST, message) }
fn forbidden() -> Error { Error(StatusCode::FORBIDDEN, "Access denied") }
fn admin(p: &Principal) -> Result<()> { if p.0.admin { Ok(()) } else { Err(forbidden()) } }

pub fn router(service: Arc<Service>) -> Router {
    let protected = Router::new()
        .route("/auth/me", get(me))
        .route("/auth/logout", post(logout))
        .route("/users", get(users).post(create_user))
        .route("/users/{id}", put(update_user))
        .route("/resources", get(resources).post(create_resource))
        .route("/discovery", get(discovery))
        .route("/resources/register", post(register_resource))
        .route("/resources/{id}", delete(delete_resource))
        .route("/channels", get(channels).post(create_channel))
        .route("/channels/{id}", put(update_channel).delete(delete_channel))
        .route("/grants/{user}/{channel}", put(grant).delete(revoke))
        .route("/channels/{id}/authorize/{action}", get(authorize_action))
        .route("/channels/{id}/video/offer", post(video_offer))
        .route("/channels/{id}/video/ice", post(video_ice))
        .route("/channels/{id}/video/close", post(video_close))
        .route("/channels/{id}/video/status", get(video_status))
        .route_layer(middleware::from_fn_with_state(service.clone(), authenticate));
    Router::new()
        .route("/",get(|| async {Html(include_str!("../web/index.html"))}))
        .route("/platform.js",get(||async { ([("content-type","text/javascript; charset=utf-8")],include_str!("../web/platform.js")) }))
        .route("/platform.css",get(||async { ([("content-type","text/css; charset=utf-8")],include_str!("../web/platform.css")) }))
        .route("/health", get(|| async { Json(json!({"status":"ok","stage":"metadata-only"})) }))
        .route("/auth/login", post(login)).merge(protected)
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(middleware::from_fn(response_headers)).with_state(service)
}

async fn response_headers(req:Request,next:Next)->Response {
    let mut response=next.run(req).await;
    for (name,value) in [
        ("cache-control","no-store"),("x-content-type-options","nosniff"),
        ("referrer-policy","no-referrer"),
        ("content-security-policy","default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"),
    ] {response.headers_mut().insert(axum::http::HeaderName::from_static(name),axum::http::HeaderValue::from_static(value));}
    response
}

async fn authenticate(State(s): State<Arc<Service>>, mut req: Request, next: Next) -> Result<Response> {
    let token = req.headers().get("authorization").and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer ")).filter(|t| t.len() <= 256)
        .ok_or(Error(StatusCode::UNAUTHORIZED, "Authentication required"))?;
    let row = sqlx::query("SELECT u.id,u.username,u.admin,u.active FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.token_hash=? AND s.expires>? AND u.active=1")
        .bind(store::token_hash(token)).bind(store::now()).fetch_optional(&s.pool).await?
        .ok_or(Error(StatusCode::UNAUTHORIZED, "Session expired"))?;
    let token_hash=store::token_hash(token);
    req.extensions_mut().insert(Principal(store::user(&row)));
    req.extensions_mut().insert(AuthenticatedToken(token_hash));
    Ok(next.run(req).await)
}
#[derive(Clone)]
struct AuthenticatedToken(String);

async fn authorized_video(s:&Service,p:&Principal,id:&str)->Result<crate::gateway::WorkerEndpoint> {
    let channel=store::channels(&s.pool).await.map_err(|_|bad("Invalid channel data"))?.into_iter().find(|c|c.id==id).ok_or_else(forbidden)?;
    if !channel.enabled || !allowed(s,p,&channel,Permission::View).await? {return Err(forbidden());}
    s.workers.lock().await.get(id).cloned().ok_or(Error(StatusCode::SERVICE_UNAVAILABLE,"Channel worker is offline"))
}
async fn video_offer(State(s):State<Arc<Service>>,Extension(p):Extension<Principal>,Extension(token):Extension<AuthenticatedToken>,Path(id):Path<String>,Json(mut body):Json<Value>)->Result<Json<Value>> {
    let endpoint=authorized_video(&s,&p,&id).await?;
    if !body.is_object() || !body.get("sdp").is_some_and(Value::is_string) {return Err(bad("Missing video offer"));}
    if let Some(object)=body.as_object_mut(){object.remove("client_id");}
    // A viewer cannot request reuse of another peer's WebRTC session.
    let reply=crate::gateway::video(endpoint.clone(),crate::gateway::VideoAction::Offer,body).await.map_err(|_|Error(StatusCode::BAD_GATEWAY,"Worker offer failed"))?;
    let sid=reply.get("session_id").and_then(Value::as_str).ok_or(Error(StatusCode::BAD_GATEWAY,"Worker returned no session"))?;
    s.video_sessions.lock().await.insert(sid.into(),VideoOwner{user:p.0.id,token_hash:token.0,channel:id,endpoint});
    Ok(Json(reply))
}
async fn video_session_action(s:Arc<Service>,p:Principal,token:AuthenticatedToken,id:String,body:Value,action:crate::gateway::VideoAction)->Result<Json<Value>> {
    let _=authorized_video(&s,&p,&id).await?;
    let sid=body.get("session_id").and_then(Value::as_str).ok_or_else(forbidden)?.to_owned();
    let endpoint={let sessions=s.video_sessions.lock().await;let owner=sessions.get(&sid).ok_or_else(forbidden)?;
        if owner.user!=p.0.id || owner.token_hash!=token.0 || owner.channel!=id {return Err(forbidden());} owner.endpoint.clone()};
    let reply=crate::gateway::video(endpoint,action,body).await.map_err(|_|Error(StatusCode::BAD_GATEWAY,"Worker signaling failed"))?;
    if matches!(action,crate::gateway::VideoAction::Close){s.video_sessions.lock().await.remove(&sid);}
    Ok(Json(reply))
}
async fn video_ice(State(s):State<Arc<Service>>,Extension(p):Extension<Principal>,Extension(token):Extension<AuthenticatedToken>,Path(id):Path<String>,Json(body):Json<Value>)->Result<Json<Value>> {
    video_session_action(s,p,token,id,body,crate::gateway::VideoAction::Ice).await
}
async fn video_close(State(s):State<Arc<Service>>,Extension(p):Extension<Principal>,Extension(token):Extension<AuthenticatedToken>,Path(id):Path<String>,Json(body):Json<Value>)->Result<Json<Value>> {
    video_session_action(s,p,token,id,body,crate::gateway::VideoAction::Close).await
}
async fn video_status(State(s):State<Arc<Service>>,Extension(p):Extension<Principal>,Path(id):Path<String>)->Result<Json<Value>> {
    let endpoint=authorized_video(&s,&p,&id).await?;
    Ok(Json(crate::gateway::video(endpoint,crate::gateway::VideoAction::Status,Value::Null).await.map_err(|_|Error(StatusCode::BAD_GATEWAY,"Worker status unavailable"))?))
}

impl Service {
    /// Close media sessions after logout, expiration, user disable or grant
    /// revocation. A failed close remains tracked and is retried next sweep.
    pub async fn sweep_video_sessions(&self)->std::result::Result<(),String> {
        let snapshot:Vec<_>=self.video_sessions.lock().await.iter().map(|(id,o)|(id.clone(),o.user.clone(),o.token_hash.clone(),o.channel.clone(),o.endpoint.clone())).collect();
        for (sid,user,token,channel,endpoint) in snapshot {
            let row=sqlx::query("SELECT u.id,u.username,u.admin,u.active FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.token_hash=? AND u.id=? AND s.expires>? AND u.active=1")
                .bind(token).bind(user).bind(store::now()).fetch_optional(&self.pool).await.map_err(|_|"Session verification unavailable")?;
            let valid=if let Some(row)=row {authorized_video(self,&Principal(store::user(&row)),&channel).await.is_ok()}else{false};
            if !valid && crate::gateway::video(endpoint,crate::gateway::VideoAction::Close,json!({"session_id":sid})).await.is_ok(){self.video_sessions.lock().await.remove(&sid);}
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Login { username: String, password: String }
async fn login(State(s): State<Arc<Service>>, Json(req): Json<Login>) -> Result<Json<Value>> {
    if req.username.len() > 128 || req.password.len() > 1024 { return Err(bad("Invalid credentials")); }
    {
        let mut failures = s.login_failures.lock().await;
        let now = store::now(); failures.retain(|_, v| now - v.0 < 60);
        if failures.len() > 4096 { return Err(Error(StatusCode::TOO_MANY_REQUESTS, "Login throttled")); }
        let entry = failures.entry(req.username.clone()).or_insert((now, 0));
        if entry.1 >= 5 { return Err(Error(StatusCode::TOO_MANY_REQUESTS, "Login throttled")); }
        entry.1 += 1;
    }
    let row = sqlx::query("SELECT id,username,password_hash,admin,active FROM users WHERE username=? AND active=1")
        .bind(&req.username).fetch_optional(&s.pool).await?;
    let _slot = s.hashing_slots.try_acquire().map_err(|_| Error(StatusCode::TOO_MANY_REQUESTS,"Login throttled"))?;
    let hash: String = row.as_ref().map(|r| r.get("password_hash")).unwrap_or_else(|| s.dummy_hash.clone());
    let password = req.password;
    let accepted = tokio::task::spawn_blocking(move || store::verify_password(&password, &hash)).await.unwrap_or(false) && row.is_some();
    if !accepted { return Err(Error(StatusCode::UNAUTHORIZED, "Invalid username or password")); }
    s.login_failures.lock().await.remove(&req.username);
    let user = store::user(row.as_ref().unwrap());
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    sqlx::query("DELETE FROM sessions WHERE expires<=?").bind(store::now()).execute(&s.pool).await?;
    sqlx::query("INSERT INTO sessions(token_hash,user_id,expires) VALUES(?,?,?)")
        .bind(store::token_hash(&token)).bind(&user.id).bind(store::now() + 3600 * 8).execute(&s.pool).await?;
    Ok(Json(json!({"token":token,"user":user,"expires_in":28800})))
}
async fn me(Extension(p): Extension<Principal>) -> Json<User> { Json(p.0) }
async fn logout(State(s): State<Arc<Service>>, req: Request) -> Result<Json<Value>> {
    let token = req.headers().get("authorization").and_then(|h| h.to_str().ok()).and_then(|h| h.strip_prefix("Bearer ")).unwrap_or("");
    sqlx::query("DELETE FROM sessions WHERE token_hash=?").bind(store::token_hash(token)).execute(&s.pool).await?;
    Ok(Json(json!({"ok":true})))
}

async fn users(State(s): State<Arc<Service>>, Extension(p): Extension<Principal>) -> Result<Json<Vec<User>>> {
    admin(&p)?;
    let rows = sqlx::query("SELECT id,username,admin,active FROM users ORDER BY username").fetch_all(&s.pool).await?;
    Ok(Json(rows.iter().map(store::user).collect()))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UserCreate { username: String, password: String, #[serde(default)] admin: bool }
async fn create_user(State(s): State<Arc<Service>>, Extension(p): Extension<Principal>, Json(req): Json<UserCreate>) -> Result<Json<User>> {
    admin(&p)?;
    if req.username.trim().is_empty() || req.username.len() > 128 { return Err(bad("Invalid username")); }
    let hash = tokio::task::spawn_blocking(move || store::password_hash(&req.password)).await
        .map_err(|_| bad("Password hashing failed"))?.map_err(|_| bad("Password must contain 8 to 1024 UTF-8 bytes"))?;
    let _guard = s.mutation.lock().await;
    let user = User { id: Uuid::new_v4().to_string(), username: req.username, admin: req.admin, active: true };
    let mut tx = s.pool.begin().await?;
    sqlx::query("INSERT INTO users(id,username,password_hash,admin,active) VALUES(?,?,?,?,1)")
        .bind(&user.id).bind(&user.username).bind(hash).bind(user.admin).execute(&mut *tx).await
        .map_err(|_| bad("Username already exists or invalid user"))?;
    store::audit(&mut tx, &p.0.id, "create_user", &user.id).await?; tx.commit().await?;
    Ok(Json(user))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UserUpdate { active: Option<bool>, password: Option<String> }
async fn update_user(State(s): State<Arc<Service>>, Extension(p): Extension<Principal>, Path(id): Path<String>, Json(req): Json<UserUpdate>) -> Result<Json<Value>> {
    admin(&p)?;
    if id == p.0.id && req.active == Some(false) { return Err(bad("Cannot disable the current administrator")); }
    let hash = if let Some(password) = req.password {
        Some(tokio::task::spawn_blocking(move || store::password_hash(&password)).await.map_err(|_| bad("Hash failed"))?
            .map_err(|_| bad("Password must contain 8 to 1024 UTF-8 bytes"))?)
    } else { None };
    let _guard = s.mutation.lock().await;
    let mut tx = s.pool.begin().await?;
    let found: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE id=?").bind(&id).fetch_one(&mut *tx).await?;
    if found == 0 { return Err(Error(StatusCode::NOT_FOUND,"User not found")); }
    if let Some(active) = req.active { sqlx::query("UPDATE users SET active=? WHERE id=?").bind(active).bind(&id).execute(&mut *tx).await?; }
    if let Some(hash) = hash { sqlx::query("UPDATE users SET password_hash=? WHERE id=?").bind(hash).bind(&id).execute(&mut *tx).await?; }
    sqlx::query("DELETE FROM sessions WHERE user_id=?").bind(&id).execute(&mut *tx).await?;
    store::audit(&mut tx,&p.0.id,"update_user",&id).await?; tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}

async fn resources(State(s): State<Arc<Service>>, Extension(p): Extension<Principal>) -> Result<Json<Vec<Resource>>> {
    admin(&p)?; Ok(Json(store::resources(&s.pool).await.map_err(|_| bad("Invalid resource data"))?))
}
async fn discovery(State(s): State<Arc<Service>>, Extension(p): Extension<Principal>) -> Result<Json<Vec<crate::discovery::Candidate>>> {
    admin(&p)?;
    Ok(Json(s.inventory.read()))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegisterResource { candidate_id: String, name: String }
async fn register_resource(State(s): State<Arc<Service>>, Extension(p): Extension<Principal>, Json(req): Json<RegisterResource>) -> Result<Json<Resource>> {
    admin(&p)?;
    let inventory=s.inventory.read();
    let candidates: Vec<_>=inventory.iter().filter(|c|c.id==req.candidate_id).collect();
    if candidates.len()!=1 {return Err(bad("Device is missing or ambiguous"));}
    let resource=candidates[0].resource(req.name,Uuid::new_v4().to_string());
    create_resource(State(s),Extension(p),Json(resource)).await
}
async fn create_resource(State(s): State<Arc<Service>>, Extension(p): Extension<Principal>, Json(mut resource): Json<Resource>) -> Result<Json<Resource>> {
    admin(&p)?; resource.id = Uuid::new_v4().to_string();
    resource.domain().map_err(bad)?;
    let inventory=s.inventory.read();
    let matched:Vec<_>=inventory.iter().filter(|c|c.kind==resource.kind
        && c.selector_kind==resource.selector_kind && c.selector==resource.selector).collect();
    if matched.len()!=1 {return Err(bad("Resource must match one discovered device"));}
    let _guard = s.mutation.lock().await;
    let mut tx = s.pool.begin().await?;
    sqlx::query("INSERT INTO resources(id,body) VALUES(?,?)").bind(&resource.id)
        .bind(serde_json::to_string(&resource).unwrap()).execute(&mut *tx).await?;
    store::audit(&mut tx,&p.0.id,"create_resource",&resource.id).await?; tx.commit().await?;
    Ok(Json(resource))
}
async fn delete_resource(State(s): State<Arc<Service>>, Extension(p): Extension<Principal>, Path(id): Path<String>) -> Result<Json<Value>> {
    admin(&p)?; let _guard = s.mutation.lock().await;
    let channels = store::channels(&s.pool).await.map_err(|_| bad("Invalid channel data"))?;
    if channels.iter().any(|c| c.video == id || c.control.as_ref() == Some(&id)) { return Err(bad("Resource is assigned to a channel")); }
    let mut tx = s.pool.begin().await?;
    sqlx::query("DELETE FROM resources WHERE id=?").bind(&id).execute(&mut *tx).await?;
    store::audit(&mut tx,&p.0.id,"delete_resource",&id).await?;tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}

async fn allowed(s: &Service, p: &Principal, c: &Channel, action: Permission) -> Result<bool> {
    if p.0.admin { return Ok(action != Permission::Control || (c.enabled && c.control.is_some())); }
    let role: Option<String> = sqlx::query_scalar("SELECT role FROM grants WHERE user_id=? AND channel_id=?")
        .bind(&p.0.id).bind(&c.id).fetch_optional(&s.pool).await?;
    let mut policy = core::AccessPolicy::default();
    if let Some(role) = role.and_then(|r| crate::model::role(&r)) { policy.grant(p.0.id.clone(),c.domain().id,role); }
    Ok(policy.allows(&p.0.id,&c.domain(),action))
}
async fn channels(State(s): State<Arc<Service>>, Extension(p): Extension<Principal>) -> Result<Json<Vec<Channel>>> {
    let all = store::channels(&s.pool).await.map_err(|_| bad("Invalid channel data"))?;
    let mut result = vec![];
    for channel in all { if allowed(&s,&p,&channel,Permission::View).await? { result.push(channel); } }
    Ok(Json(result))
}
async fn validate(s: &Service, proposed: &Channel) -> Result<()> {
    if proposed.name.is_empty() || proposed.name.len() > 128 { return Err(bad("Invalid channel name")); }
    let mut channels = store::channels(&s.pool).await.map_err(|_| bad("Invalid channel data"))?;
    channels.retain(|c| c.id != proposed.id); channels.push(proposed.clone());
    let resources = store::resources(&s.pool).await.map_err(|_| bad("Invalid resource data"))?;
    let bindings: Vec<_> = resources.iter().map(Resource::domain).collect::<std::result::Result<_,_>>().map_err(bad)?;
    core::validate_config(&channels.iter().map(Channel::domain).collect::<Vec<_>>(),&bindings)
        .map_err(|_| bad("Invalid or conflicting channel binding"))
}
async fn create_channel(State(s): State<Arc<Service>>, Extension(p): Extension<Principal>, Json(mut channel): Json<Channel>) -> Result<Json<Channel>> {
    admin(&p)?; let _guard = s.mutation.lock().await;
    channel.id = Uuid::new_v4().to_string(); validate(&s,&channel).await?;
    let mut tx = s.pool.begin().await?;
    sqlx::query("INSERT INTO channels(id,body) VALUES(?,?)").bind(&channel.id)
        .bind(serde_json::to_string(&channel).unwrap()).execute(&mut *tx).await?;
    store::audit(&mut tx,&p.0.id,"create_channel",&channel.id).await?;tx.commit().await?;
    Ok(Json(channel))
}
async fn update_channel(State(s): State<Arc<Service>>, Extension(p): Extension<Principal>, Path(id): Path<String>, Json(mut channel): Json<Channel>) -> Result<Json<Channel>> {
    // Resource reassignment remains a global administrator operation.
    admin(&p)?; let _guard = s.mutation.lock().await;
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM channels WHERE id=?").bind(&id).fetch_one(&s.pool).await?;
    if exists == 0 { return Err(Error(StatusCode::NOT_FOUND,"Channel not found")); }
    channel.id = id;validate(&s,&channel).await?;
    let mut tx = s.pool.begin().await?;
    sqlx::query("UPDATE channels SET body=? WHERE id=?").bind(serde_json::to_string(&channel).unwrap()).bind(&channel.id).execute(&mut *tx).await?;
    store::audit(&mut tx,&p.0.id,"update_channel",&channel.id).await?;tx.commit().await?;
    Ok(Json(channel))
}
async fn delete_channel(State(s): State<Arc<Service>>, Extension(p): Extension<Principal>, Path(id): Path<String>) -> Result<Json<Value>> {
    admin(&p)?;let _guard = s.mutation.lock().await;let mut tx = s.pool.begin().await?;
    sqlx::query("DELETE FROM channels WHERE id=?").bind(&id).execute(&mut *tx).await?;
    store::audit(&mut tx,&p.0.id,"delete_channel",&id).await?;tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Grant { role: String }
async fn grant(State(s): State<Arc<Service>>, Extension(p): Extension<Principal>, Path((user, channel)): Path<(String,String)>, Json(req): Json<Grant>) -> Result<Json<Value>> {
    admin(&p)?;if crate::model::role(&req.role).is_none() { return Err(bad("Unknown role")); }
    let _guard = s.mutation.lock().await; let mut tx = s.pool.begin().await?;
    sqlx::query("INSERT INTO grants(user_id,channel_id,role) VALUES(?,?,?) ON CONFLICT(user_id,channel_id) DO UPDATE SET role=excluded.role")
        .bind(&user).bind(&channel).bind(&req.role).execute(&mut *tx).await.map_err(|_| bad("Unknown user or channel"))?;
    store::audit(&mut tx,&p.0.id,"grant",&format!("{user}/{channel}")).await?;tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
async fn revoke(State(s): State<Arc<Service>>, Extension(p): Extension<Principal>, Path((user, channel)): Path<(String,String)>) -> Result<Json<Value>> {
    admin(&p)?;let _guard = s.mutation.lock().await;let mut tx = s.pool.begin().await?;
    sqlx::query("DELETE FROM grants WHERE user_id=? AND channel_id=?").bind(&user).bind(&channel).execute(&mut *tx).await?;
    store::audit(&mut tx,&p.0.id,"revoke",&format!("{user}/{channel}")).await?;tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
async fn authorize_action(State(s): State<Arc<Service>>, Extension(p): Extension<Principal>, Path((id,action)): Path<(String,String)>) -> Result<Json<Value>> {
    let action = match action.as_str() { "view"=>Permission::View,"control"=>Permission::Control,"configure"=>Permission::Configure,_=>return Err(bad("Unknown permission")) };
    let channel = store::channels(&s.pool).await.map_err(|_| bad("Invalid channel data"))?
        .into_iter().find(|c| c.id == id).ok_or_else(forbidden)?;
    if !allowed(&s,&p,&channel,action).await? { return Err(forbidden()); }
    // Capability probe only. No input/video endpoint exists at this stage.
    Ok(Json(json!({"allowed":true,"runtime_available":false})))
}
