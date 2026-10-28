// SPDX-License-Identifier: AGPL-3.0-only
use crate::{discovery::Candidate,model::{Channel,Resource},runtime::{self,Reservations},supervisor::Supervisor,gateway::{self,WorkerEndpoint},worker_db};
use serde_json::Value;
use std::{collections::BTreeSet,path::PathBuf,sync::Arc};
use tokio::sync::Mutex;

pub struct Host {base:PathBuf,supervisor:Mutex<Supervisor>}
impl Host {
    pub fn new(base:PathBuf,program:PathBuf,sha256:String)->Self {
        Self{base,supervisor:Mutex::new(Supervisor::new(program,sha256,Arc::new(Reservations::default())))}
    }
    pub async fn ids(&self)->Vec<String>{self.supervisor.lock().await.running_ids()}
    pub async fn start(&self,channel:&Channel,resources:&[Resource],inventory:&[Candidate],template:&Value)->Result<WorkerEndpoint,String>{
        uuid::Uuid::parse_str(&channel.id).map_err(|_|"Invalid internal channel ID")?;
        let mut supervisor=self.supervisor.lock().await;
        if !supervisor.can_prepare(&channel.id){return Err("Channel is running or awaiting recovery".into());}
        let web=std::net::TcpListener::bind("127.0.0.1:0").map_err(|_|"Cannot allocate web port")?;
        let direct=std::net::TcpListener::bind("0.0.0.0:0").map_err(|_|"Cannot allocate RustDesk port")?;
        let web_port=web.local_addr().map_err(|_|"Cannot inspect web port")?.port();
        let direct_port=direct.local_addr().map_err(|_|"Cannot inspect RustDesk port")?.port();
        let plan=runtime::plan(channel,resources,inventory,template,web_port,direct_port)?;
        let owns_otg=plan.physical_interfaces.iter().any(|s|s.starts_with("udc:"));
        let credentials=worker_db::prepare(&self.base.join(&channel.id),&plan).await?;
        drop(web);drop(direct);
        supervisor.start(&channel.id,&self.base.join(&channel.id),plan.physical_interfaces,owns_otg)?;
        // Readiness requires an authenticated response, not merely an open port.
        for _ in 0..20 {
            if let Ok(endpoint)=gateway::login(web_port,credentials.username.clone(),credentials.password.clone()).await{return Ok(endpoint);}
            if supervisor.reap()?.iter().any(|id|id==&channel.id){return Err("Worker exited during initialization".into());}
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        supervisor.kill_and_wait(&channel.id)?;
        Err("Worker authentication did not become ready; resource recovery required".into())
    }
    pub async fn stop(&self,id:&str,endpoint:WorkerEndpoint,has_control:bool)->Result<(),String>{
        // A confirmed backend reset must precede graceful worker shutdown.
        if has_control {gateway::input(endpoint,vec![]).await?;}
        let mut supervisor=self.supervisor.lock().await;
        supervisor.graceful_stop(id)?;
        supervisor.confirm_input_recovered(id)?;Ok(())
    }
    pub async fn reconcile(&self,inventory:&[Candidate])->Result<Vec<String>,String>{
        let present:BTreeSet<_>=inventory.iter().map(|c|c.physical_interface.clone()).collect();
        let mut supervisor=self.supervisor.lock().await;
        let mut lost=supervisor.stop_missing(&present)?;lost.extend(supervisor.reap()?);Ok(lost)
    }
}
