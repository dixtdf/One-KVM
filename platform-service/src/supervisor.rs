// SPDX-License-Identifier: AGPL-3.0-only
//! Private process lifecycle. Only a trusted manager supplies executable, digest,
//! arguments and prepared directories. This module is not an HTTP launch API.
use crate::runtime::Reservations;
use sha2::{Digest,Sha256};
use std::{collections::{BTreeMap,BTreeSet},fs, path::{Path,PathBuf}, process::{Child,Command,Stdio},sync::Arc};

struct Running { child:Child, interfaces:BTreeSet<String> }
pub struct Supervisor {
    executable:PathBuf,
    sha256:String,
    reservations:Arc<Reservations>,
    running:BTreeMap<String,Running>,
    recovery_required:BTreeSet<String>,
}
impl Supervisor {
    pub fn new(executable:PathBuf,sha256:String,reservations:Arc<Reservations>)->Self {
        Self{executable,sha256,reservations,running:BTreeMap::new(),recovery_required:BTreeSet::new()}
    }
    pub fn start(&mut self,id:&str,data_dir:&Path,interfaces:BTreeSet<String>,owns_otg:bool)->Result<(),String> {
        let bytes=fs::read(&self.executable).map_err(|_|"Cannot read worker executable")?;
        let actual:String=Sha256::digest(&bytes).iter().map(|b|format!("{b:02x}")).collect();
        if actual!=self.sha256 {return Err("Worker executable digest mismatch".into());}
        if !data_dir.is_absolute() || !data_dir.join("one-kvm.db").is_file(){return Err("Private worker database is not prepared".into());}
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            if fs::metadata(data_dir).map_err(|_|"Cannot inspect worker directory")?.permissions().mode()&0o077!=0 {
                return Err("Worker directory is not private".into());
            }
        }
        let has_udc=interfaces.iter().any(|i|i.starts_with("udc:"));
        if owns_otg!=has_udc {return Err("OTG ownership does not match reserved resources".into());}
        self.reservations.acquire(id,interfaces.clone()).map_err(str::to_owned)?;
        let spawned=Command::new(&self.executable).args(["--data-dir"]).arg(data_dir)
            .env_clear().env("PATH","/usr/sbin:/usr/bin:/sbin:/bin")
            .env("ONE_KVM_OTG_OWNERSHIP",if owns_otg{"owner"}else{"none"})
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
        match spawned {
            Ok(child)=>{self.running.insert(id.into(),Running{child,interfaces});Ok(())},
            Err(_)=>{self.reservations.release_after_exit(id).map_err(str::to_owned)?;Err("Worker spawn failed".into())}
        }
    }
    pub fn reap(&mut self)->Result<Vec<String>,String> {
        let mut exited=vec![];
        for (id,worker) in &mut self.running {
            if worker.child.try_wait().map_err(|_|"Cannot inspect worker")?.is_some(){exited.push(id.clone());}
        }
        for id in &exited {self.running.remove(id);self.recovery_required.insert(id.clone());}
        Ok(exited)
    }
    /// A hard kill does not prove HID reports have been released. The caller must
    /// run the control adapter's neutral-report recovery before starting a new
    /// controller on the released hardware. Gateway stop will use graceful exit.
    pub fn kill_and_wait(&mut self,id:&str)->Result<(),String> {
        let Some(worker)=self.running.get_mut(id) else{return Ok(())};
        if worker.child.try_wait().map_err(|_|"Cannot inspect worker")?.is_none(){
            worker.child.kill().map_err(|_|"Cannot stop worker")?;
        }
        worker.child.wait().map_err(|_|"Cannot confirm worker exit")?;
        self.running.remove(id);self.recovery_required.insert(id.into());Ok(())
    }
    /// Only the trusted hardware adapter calls this after neutral HID reports
    /// and gadget state recovery. Process exit alone cannot prove key release.
    pub fn confirm_input_recovered(&mut self,id:&str)->Result<(),String> {
        if !self.recovery_required.contains(id){return Err("Channel is not awaiting recovery".into());}
        self.reservations.release_after_exit(id).map_err(str::to_owned)?;
        self.recovery_required.remove(id);Ok(())
    }
    /// Offline/replaced devices stop only their owning workers; no automatic
    /// fallback to another source, and no global OTG restart.
    pub fn stop_missing(&mut self,present:&BTreeSet<String>)->Result<Vec<String>,String> {
        let lost:Vec<_>=self.running.iter().filter(|(_,w)|!w.interfaces.is_subset(present)).map(|(id,_)|id.clone()).collect();
        for id in &lost {self.kill_and_wait(id)?;}Ok(lost)
    }
}
impl Drop for Supervisor {
    fn drop(&mut self){
        for id in self.running.keys().cloned().collect::<Vec<_>>() {
            // Failed stop retains reservation rather than allowing double use.
            let _=self.kill_and_wait(&id);
        }
    }
}

#[cfg(all(test,unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn fixture()->(tempfile::TempDir,Supervisor,Arc<Reservations>){
        let dir=tempfile::tempdir().unwrap();fs::set_permissions(dir.path(),fs::Permissions::from_mode(0o700)).unwrap();
        let script=dir.path().join("worker");
        fs::write(&script,"#!/bin/sh\n[ \"$ONE_KVM_OTG_OWNERSHIP\" = none ] || exit 2\nexec /bin/sleep 30\n").unwrap();
        fs::set_permissions(&script,fs::Permissions::from_mode(0o700)).unwrap();fs::write(dir.path().join("one-kvm.db"),"").unwrap();
        let hash:String=Sha256::digest(fs::read(&script).unwrap()).iter().map(|b|format!("{b:02x}")).collect();
        let reservations=Arc::new(Reservations::default());let s=Supervisor::new(script,hash,reservations.clone());(dir,s,reservations)
    }
    #[test]
    fn device_loss_stops_only_owner_and_waits_before_reuse(){
        let (dir,mut s,r)=fixture();
        s.start("a",dir.path(),BTreeSet::from(["camera-a".into()]),false).unwrap();
        s.start("b",dir.path(),BTreeSet::from(["camera-b".into()]),false).unwrap();
        assert!(r.acquire("c",BTreeSet::from(["camera-a".into()])).is_err());
        assert_eq!(s.stop_missing(&BTreeSet::from(["camera-b".into()])).unwrap(),vec!["a"]);
        assert!(r.acquire("c",BTreeSet::from(["camera-a".into()])).is_err());
        s.confirm_input_recovered("a").unwrap();r.acquire("c",BTreeSet::from(["camera-a".into()])).unwrap();
        assert!(r.acquire("d",BTreeSet::from(["camera-b".into()])).is_err());
        s.kill_and_wait("b").unwrap();assert!(r.acquire("d",BTreeSet::from(["camera-b".into()])).is_err());
        s.confirm_input_recovered("b").unwrap();r.acquire("d",BTreeSet::from(["camera-b".into()])).unwrap();
    }
    #[test]
    fn executable_change_and_wrong_otg_claim_fail_before_launch(){
        let (dir,mut s,_)=fixture();
        assert!(s.start("a",dir.path(),BTreeSet::from(["serial".into()]),true).is_err());
        fs::write(&s.executable,"changed").unwrap();
        assert!(s.start("a",dir.path(),BTreeSet::from(["serial".into()]),false).unwrap_err().contains("digest"));
    }
}
