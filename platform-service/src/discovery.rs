// SPDX-License-Identifier: AGPL-3.0-only
//! Read-only discovery. No device is opened, configured or written here.
use crate::model::Resource;
use one_kvm_platform_core::{Observation, ResourceKind};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, path::{Path, PathBuf}};

#[derive(Clone, Debug, Serialize)]
pub struct Candidate {
    pub id: String,
    pub label: String,
    pub kind: String,
    #[serde(skip_serializing)]
    pub selector_kind: String,
    #[serde(skip_serializing)]
    pub selector: String,
    #[serde(skip_serializing)]
    pub physical_interface: String,
    #[serde(skip_serializing)]
    pub current_path: String,
}

impl Candidate {
    pub fn resource(&self, name: String, id: String) -> Resource {
        Resource { id, name, kind:self.kind.clone(),selector_kind:self.selector_kind.clone(),selector:self.selector.clone() }
    }
    pub fn observation(&self) -> Observation {
        let binding=self.resource(self.label.clone(),self.id.clone()).domain().expect("Trusted discovery candidate");
        Observation { physical_interface:self.physical_interface.clone(), selectors:vec![binding.selector],
            kind:if self.kind=="video" {ResourceKind::Video}else{ResourceKind::Control},current_path:self.current_path.clone() }
    }
}

fn digest(value: &str) -> String { format!("{:x}",Sha256::digest(value.as_bytes())) }
fn text(path: &Path) -> Option<String> { fs::read_to_string(path).ok().map(|s|s.trim().to_owned()).filter(|s|!s.is_empty()) }
fn usb_model(interface: &Path) -> String {
    // Include model and interface in the port binding. A different device plugged
    // into the same port must not inherit the old channel's control authority.
    let ifnum=text(&interface.join("bInterfaceNumber")).unwrap_or_default();
    for p in interface.ancestors() {
        if let (Some(v),Some(d))=(text(&p.join("idVendor")),text(&p.join("idProduct"))) {
            return format!("{v}:{d}:{ifnum}");
        }
    }
    "builtin".into()
}
fn add(items: &mut Vec<Candidate>, kind:&str,label:String,selector_kind:&str,selector:String,physical:String,path:String) {
    let id=digest(&format!("{kind}:{selector_kind}:{selector}"));
    items.push(Candidate {id,label,kind:kind.into(),selector_kind:selector_kind.into(),selector,physical_interface:physical,current_path:path});
}

pub fn discover(sys: &Path, dev: &Path) -> Vec<Candidate> {
    let mut items=vec![];
    for (kind,class,alias_dir) in [("video","video4linux","v4l/by-path"),("control","tty","serial/by-path")] {
        let Ok(entries)=fs::read_dir(dev.join(alias_dir)) else {continue};
        for entry in entries.flatten() {
            let Ok(node)=entry.path().canonicalize() else {continue};
            // Only nodes beneath this machine's /dev are eligible.
            let Ok(root)=dev.canonicalize() else {continue};
            if !node.starts_with(&root) {continue;}
            let Some(name)=node.file_name().and_then(|s|s.to_str()) else {continue};
            let class_path=sys.join("class").join(class).join(name);
            let Ok(interface)=class_path.join("device").canonicalize() else {continue};
            let label=text(&class_path.join("name")).unwrap_or_else(||name.into());
            let selector=digest(&format!("{}:{}",entry.file_name().to_string_lossy(),usb_model(&interface)));
            add(&mut items,kind,label,"port",selector,interface.join(name).to_string_lossy().into(),node.to_string_lossy().into());
        }
    }
    // Built-in CSI/loopback devices may have no udev by-path links. Their stable
    // card name is matched exactly; duplicate names remain ambiguous.
    if let Ok(entries)=fs::read_dir(sys.join("class/video4linux")) {
        for entry in entries.flatten() {
            let Some(label)=text(&entry.path().join("name")) else {continue};
            let Ok(physical)=entry.path().canonicalize() else {continue};
            let name=entry.file_name();
            if items.iter().any(|i|i.current_path==dev.join(&name).to_string_lossy()) {continue;}
            if physical.ancestors().any(|p|p.join("idVendor").exists()) {continue;}
            add(&mut items,"video",label.clone(),"builtin",digest(&format!("card:{label}")),physical.to_string_lossy().into(),dev.join(name).to_string_lossy().into());
        }
    }
    if let Ok(entries)=fs::read_dir(sys.join("class/udc")) {
        for entry in entries.flatten() {
            let name=entry.file_name().to_string_lossy().into_owned();
            add(&mut items,"control",format!("OTG {name}"),"builtin",format!("udc:{name}"),format!("udc:{name}"),format!("udc:{name}"));
        }
    }
    items.sort_by(|a,b|a.id.cmp(&b.id).then(a.physical_interface.cmp(&b.physical_interface)));
    // udev may expose equivalent aliases for one interface. Merge those only;
    // identical identities on different interfaces must remain ambiguous.
    let mut seen=BTreeSet::new();
    items.retain(|i|seen.insert((i.id.clone(),i.physical_interface.clone())));
    items
}

pub enum Inventory {
    System { sys:PathBuf,dev:PathBuf },
    Fixed(Vec<Candidate>),
}
impl Inventory {
    pub fn read(&self)->Vec<Candidate> {
        match self {Self::System{sys,dev}=>discover(sys,dev),Self::Fixed(items)=>items.clone()}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    fn link(from:&Path,to:&Path) {fs::create_dir_all(to.parent().unwrap()).unwrap();std::os::unix::fs::symlink(from,to).unwrap();}

    #[test]
    #[cfg(unix)]
    fn port_identity_survives_node_number_changes_but_not_model_changes() {
        let t=tempfile::tempdir().unwrap();let sys=t.path().join("sys");let dev=t.path().join("dev");
        fs::create_dir_all(&dev).unwrap();let usb=sys.join("devices/usb1/1-2");let interface=usb.join("1-2:1.0");
        fs::create_dir_all(&interface).unwrap();fs::write(usb.join("idVendor"),"1a86").unwrap();fs::write(usb.join("idProduct"),"7523").unwrap();
        fs::write(interface.join("bInterfaceNumber"),"00").unwrap();
        let alias=dev.join("serial/by-path/pci-usb-0:2:1.0-port0");
        fs::write(dev.join("ttyUSB0"),"").unwrap();link(&dev.join("ttyUSB0"),&alias);
        link(&interface,&sys.join("class/tty/ttyUSB0/device"));
        let first=discover(&sys,&dev);assert_eq!(first.len(),1);
        fs::remove_file(&alias).unwrap();fs::write(dev.join("ttyUSB9"),"").unwrap();link(&dev.join("ttyUSB9"),&alias);
        link(&interface,&sys.join("class/tty/ttyUSB9/device"));
        let next=discover(&sys,&dev);assert_eq!(first[0].id,next[0].id);assert_ne!(first[0].current_path,next[0].current_path);
        fs::write(usb.join("idProduct"),"9999").unwrap();assert_ne!(next[0].id,discover(&sys,&dev)[0].id);
        fs::remove_file(&alias).unwrap();assert!(discover(&sys,&dev).is_empty());
    }

    #[test]
    #[cfg(unix)]
    fn aliases_outside_dev_are_ignored_and_raw_paths_are_not_serialized() {
        let t=tempfile::tempdir().unwrap();let sys=t.path().join("sys");let dev=t.path().join("dev");
        fs::create_dir_all(&dev).unwrap();let external=t.path().join("secret");fs::write(&external,"secret").unwrap();
        link(&external,&dev.join("serial/by-path/escape"));assert!(discover(&sys,&dev).is_empty());
        fs::create_dir_all(sys.join("class/udc/fe980000.usb")).unwrap();
        let items=discover(&sys,&dev);let json=serde_json::to_string(&items).unwrap();
        assert!(!json.contains("current_path"));assert!(!json.contains("physical_interface"));assert!(!json.contains("selector"));
    }
}
