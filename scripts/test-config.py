import pathlib
import re
import subprocess
import sys

src = pathlib.Path(sys.argv[1]).resolve()
out = pathlib.Path(sys.argv[2]).resolve()
out.mkdir(parents=True, exist_ok=True)
(out / "src").mkdir(exist_ok=True)
config = (src / "src/rustdesk/config.rs").read_text()
config = re.sub(r"^use typeshare::typeshare;\n", "", config, flags=re.M)
config = re.sub(r"^\s*#\[typeshare(?:\(skip\))?\]\n", "", config, flags=re.M)
types = (src / "src/web/handlers/config/types.rs").read_text()
a = types.index("pub struct RustDeskConfigUpdate {")
z = types.index("\n#[typeshare]", a)
update = "#[derive(Debug, serde::Deserialize)]\n" + types[a:z]
lib = (
    "mod error { #[derive(Debug)] pub enum AppError { BadRequest(String) } "
    "pub type Result<T> = std::result::Result<T,AppError>; }\n"
    "mod rustdesk { pub mod config { " + config + " } }\n"
    "use rustdesk::config::RustDeskConfig;\nuse error::AppError;\n" + update
)
lib += r'''
#[cfg(test)]
mod custom_tests {
 use super::*;
 fn request(v:serde_json::Value)->RustDeskConfigUpdate{serde_json::from_value(v).unwrap()}
 #[test] fn arbitrary_values_roundtrip_without_normalization(){
  for value in ["home-mac", "123", "", " 非ASCII ID \n", "custom-key-9", "a=b+/ not base64", " x\n y\r\n"]{
   let req=request(serde_json::json!({"device_id":value,"relay_key":value}));
   req.validate().unwrap();
   let mut c=RustDeskConfig::default();
   c.uuid=Some("fixture-uuid".into());c.private_key=Some("fixture-private-key".into());
   c.signing_private_key=Some("fixture-signing-key".into());let old=c.clone();
   req.apply_to(&mut c);
   assert_eq!(c.device_id,value);
   assert_eq!(c.relay_key.as_deref(),if value.is_empty(){None}else{Some(value)});
   let restored:RustDeskConfig=serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
   assert_eq!(restored.device_id,value);assert_eq!(restored.relay_key,c.relay_key);
   assert_eq!(restored.uuid,old.uuid);assert_eq!(restored.private_key,old.private_key);
   assert_eq!(restored.signing_private_key,old.signing_private_key);
   assert_eq!(restored.device_password,old.device_password);
  }
 }
 #[test] fn omitted_fields_preserve_current_values(){
  let mut c=RustDeskConfig::default();c.device_id="original".into();c.relay_key=Some(" key ".into());
  request(serde_json::json!({})).apply_to(&mut c);
  assert_eq!(c.device_id,"original");assert_eq!(c.relay_key.as_deref(),Some(" key "));
 }
}
'''
(out / "src/lib.rs").write_text(lib)
(out / "Cargo.toml").write_text(
    '[package]\nname="custom-config-tests"\nversion="0.1.0"\nedition="2021"\n'
    '[dependencies]\nserde={version="1",features=["derive"]}\nserde_json="1"\n'
    'rand="0.10"\nuuid={version="1",features=["v4"]}\n'
)
subprocess.run(["cargo", "test", "--manifest-path", str(out / "Cargo.toml")], check=True)
assert "rr.licence_key = licence_key.to_string();" in (src / "src/rustdesk/protocol.rs").read_text()
assert "let relay_key = rustdesk_relay_key(&config);" in (src / "src/rustdesk/mod.rs").read_text()
print("Actual config/update roundtrip tests and wire constructor check passed")
