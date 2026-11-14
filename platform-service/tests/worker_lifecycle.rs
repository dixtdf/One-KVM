// SPDX-License-Identifier: AGPL-3.0-only
#![cfg(unix)]
use one_kvm_platform_service::{host::Host,discovery::Candidate,model::{Channel,Resource},gateway};
use serde_json::json;
use sha2::{Digest,Sha256};
use std::{fs,os::unix::fs::PermissionsExt};

// This is a real child process and SQLite/HTTP boundary, not the hardware
// worker. No video or input device is opened by the fixture.
const WORKER:&str=r#"#!/usr/bin/python3
import sys,os,sqlite3,json,signal
from http.server import BaseHTTPRequestHandler,HTTPServer
p=sys.argv[sys.argv.index('--data-dir')+1]
assert os.environ['ONE_KVM_PLATFORM_WORKER']=='1'
assert os.environ['ONE_KVM_OTG_OWNERSHIP']=='none'
c=json.loads(sqlite3.connect(p+'/one-kvm.db').execute("select value from config where key='app_config'").fetchone()[0])
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*a):pass
 def do_POST(self):
  body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
  if self.path=='/api/auth/login':
   assert body['username']=='platform-worker'
   assert len(body['password'])==64
   result={'next':'authenticated'}
  elif self.path=='/api/platform/input':
   assert self.headers.get('Cookie')=='one_kvm_session=worker-token'
   assert body==[]
   open(p+'/neutral-confirmed','w').write('yes')
   result={'accepted':True}
  else:raise AssertionError('unexpected path')
  payload=json.dumps(result).encode();self.send_response(200)
  self.send_header('Content-Length',str(len(payload)))
  if self.path=='/api/auth/login':self.send_header('Set-Cookie','one_kvm_session=worker-token; HttpOnly')
  self.end_headers();self.wfile.write(payload)
signal.signal(signal.SIGTERM,lambda *a:sys.exit(0))
HTTPServer(('127.0.0.1',c['web']['http_port']),Handler).serve_forever()
"#;

#[tokio::test]
async fn host_launches_authenticates_resets_waits_and_can_restart_the_same_channel(){
    let dir=tempfile::tempdir().unwrap();let program=dir.path().join("worker");fs::write(&program,WORKER).unwrap();
    fs::set_permissions(&program,fs::Permissions::from_mode(0o700)).unwrap();
    let hash:String=Sha256::digest(WORKER.as_bytes()).iter().map(|b|format!("{b:02x}")).collect();
    let root=dir.path().join("workers");let host=Host::new(root.clone(),program,hash);
    let channel=Channel{id:uuid::Uuid::new_v4().to_string(),name:"fixture".into(),enabled:true,video:"v".into(),control:Some("c".into()),rustdesk_id:Some("custom-id".into())};
    let resource=|id:&str,kind:&str|Resource{id:id.into(),name:id.into(),kind:kind.into(),selector_kind:"port".into(),selector:id.into()};
    let resources=vec![resource("v","video"),resource("c","control")];
    let candidate=|id:&str,kind:&str|Candidate{id:id.into(),label:id.into(),kind:kind.into(),selector_kind:"port".into(),selector:id.into(),physical_interface:format!("interface-{id}"),current_path:format!("/dev/fixture-{id}")};
    let inventory=vec![candidate("v","video"),candidate("c","control")];
    let template=json!({"video":{},"hid":{},"rustdesk":{"enabled":false},"web":{}});
    let endpoint=host.start(&channel,&resources,&inventory,&template).await.unwrap();
    assert_eq!(host.ids().await,vec![channel.id.clone()]);
    assert!(host.start(&channel,&resources,&inventory,&template).await.is_err());
    gateway::input(endpoint.clone(),vec![]).await.unwrap();
    host.stop(&channel.id,endpoint,true).await.unwrap();
    assert!(root.join(&channel.id).join("neutral-confirmed").is_file());
    assert!(host.ids().await.is_empty());
    let endpoint=host.start(&channel,&resources,&inventory,&template).await.unwrap();
    host.stop(&channel.id,endpoint,true).await.unwrap();
    assert!(host.start(&channel,&resources,&[],&template).await.is_err());
}
