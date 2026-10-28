// SPDX-License-Identifier: AGPL-3.0-only
//! Worker preparation and atomic resource reservation. Process launch is not
//! enabled until the gateway and OTG ownership adapter have been verified.
use crate::{discovery::Candidate,model::{Channel,Resource}};
use one_kvm_platform_core as core;
use serde_json::{json,Value};
use std::{collections::{BTreeMap,BTreeSet},sync::Mutex};

#[derive(Default)]
pub struct Reservations(Mutex<BTreeMap<String,BTreeSet<String>>>);

impl Reservations {
    pub fn acquire(&self,channel:&str,interfaces:BTreeSet<String>)->Result<(), &'static str> {
        if interfaces.is_empty(){return Err("Cannot reserve an empty resource set");}
        let mut owners=self.0.lock().map_err(|_|"Reservation state unavailable")?;
        if owners.contains_key(channel){return Err("Channel already owns resources");}
        if owners.values().any(|owned|!owned.is_disjoint(&interfaces)){return Err("Physical resource is busy");}
        owners.insert(channel.into(),interfaces);Ok(())
    }
    /// Supervisor must call only after worker exit and input release, not merely
    /// after requesting shutdown or timing out while waiting for the process.
    pub fn release_after_exit(&self,channel:&str)->Result<(), &'static str> {
        self.0.lock().map_err(|_|"Reservation state unavailable")?.remove(channel);Ok(())
    }
}

pub struct WorkerPlan {
    /// Includes device credentials; never serialize or log this structure.
    pub private_config: Value,
    pub physical_interfaces:BTreeSet<String>,
}

pub fn plan(channel:&Channel,resources:&[Resource],inventory:&[Candidate],template:&Value,
    web_port:u16,rustdesk_port:u16)->Result<WorkerPlan,&'static str> {
    if !channel.enabled{return Err("Channel is disabled");}
    if web_port<1024 || rustdesk_port<1024 || web_port==rustdesk_port{return Err("Invalid or conflicting worker ports");}
    let bindings=resources.iter().map(Resource::domain).collect::<Result<Vec<_>,_>>()?;
    let observations:Vec<_>=inventory.iter().map(Candidate::observation).collect();
    let resolved=core::resolve_enabled(&[channel.domain()],&bindings,&observations).map_err(|_|"Device missing, ambiguous or conflicting")?;
    let video=resolved.get(&core::ResourceId(channel.video.clone())).ok_or("Missing video binding")?;
    let mut config=template.clone();
    if !config.is_object(){return Err("Invalid worker template");}
    // Keep the imported encoder/video geometry and RustDesk keys, replacing only
    // explicitly owned resources. Never infer a fallback device.
    config["video"]["device"]=json!(video.current_path);
    let control=channel.control.as_ref().and_then(|id|resolved.get(&core::ResourceId(id.clone())));
    match control {
        Some(device) if device.current_path.starts_with("udc:")=>{
            config["hid"]["backend"]=json!("otg");
            config["hid"]["otg_udc"]=json!(device.current_path.trim_start_matches("udc:"));
        },
        Some(device)=>{
            config["hid"]["backend"]=json!("ch9329");
            config["hid"]["ch9329_port"]=json!(device.current_path);
        },
        None=>config["hid"]["backend"]=json!("none"),
    }
    config["rustdesk"]["device_id"]=json!(channel.rustdesk_id);
    config["rustdesk"]["direct_access_port"]=json!(rustdesk_port);
    config["web"]["bind_address"]=json!("127.0.0.1");
    config["web"]["bind_addresses"]=json!(["127.0.0.1"]);
    config["web"]["http_port"]=json!(web_port);
    config["web"]["https_enabled"]=json!(false);
    for name in ["msd","atx","audio","uac","vnc","rtsp","redfish","watchdog"] {
        config[name]["enabled"]=json!(false);
    }
    // OTG network is globally visible, so no worker inherits legacy networking.
    config["otg_network"]["enabled"]=json!(false);
    Ok(WorkerPlan{private_config:config,physical_interfaces:resolved.values().map(|d|d.physical_interface.clone()).collect()})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reservations_are_atomic_and_do_not_free_other_channels() {
        let reservations=Reservations::default();
        reservations.acquire("a",BTreeSet::from(["video-a".into(),"otg".into()])).unwrap();
        assert!(reservations.acquire("b",BTreeSet::from(["video-b".into(),"otg".into()])).is_err());
        reservations.acquire("c",BTreeSet::from(["video-b".into(),"serial".into()])).unwrap();
        reservations.release_after_exit("a").unwrap();
        assert!(reservations.acquire("b",BTreeSet::from(["serial".into()])).is_err());
        reservations.acquire("b",BTreeSet::from(["otg".into()])).unwrap();
    }
    #[test]
    fn plans_bind_exact_devices_and_keep_credentials_private() {
        let channel=Channel{id:"a".into(),name:"A".into(),enabled:true,video:"v".into(),control:Some("c".into()),rustdesk_id:Some(" 固定ID ".into())};
        let resources=vec![Resource{id:"v".into(),name:"video".into(),kind:"video".into(),selector_kind:"port".into(),selector:"v".into()},Resource{id:"c".into(),name:"control".into(),kind:"control".into(),selector_kind:"port".into(),selector:"c".into()}];
        let inventory=vec![Candidate{id:"v".into(),label:"video".into(),kind:"video".into(),selector_kind:"port".into(),selector:"v".into(),physical_interface:"physical-video".into(),current_path:"/dev/video9".into()},Candidate{id:"c".into(),label:"control".into(),kind:"control".into(),selector_kind:"port".into(),selector:"c".into(),physical_interface:"physical-serial".into(),current_path:"/dev/ttyUSB8".into()}];
        let template=json!({"video":{"width":1920,"height":1080},"hid":{},"rustdesk":{"relay_key":"private-key","device_password":"private-password"}});
        let p=plan(&channel,&resources,&inventory,&template,18090,21119).unwrap();
        assert_eq!(p.private_config["video"]["device"],"/dev/video9");
        assert_eq!(p.private_config["hid"]["ch9329_port"],"/dev/ttyUSB8");
        assert_eq!(p.private_config["rustdesk"]["device_password"],"private-password");
        assert_eq!(p.private_config["rustdesk"]["device_id"]," 固定ID ");
        assert_eq!(p.private_config["msd"]["enabled"],false);
        assert!(plan(&channel,&resources,&[],&template,18090,21119).is_err());
        assert!(plan(&channel,&resources,&inventory,&template,18090,18090).is_err());
    }
}
