// SPDX-License-Identifier: AGPL-3.0-only
//! A deliberately narrow local JSON transport. No arbitrary URL, header,
//! redirect, terminal, configuration or file proxy is exposed to users.
use serde_json::Value;
use std::{io::{Read,Write},net::{TcpStream,SocketAddrV4,Ipv4Addr},time::Duration};

#[derive(Clone)]
pub struct WorkerEndpoint { pub port:u16, pub session:String }
pub struct Reply {pub body:Value,pub session:Option<String>}

fn exchange(port:u16,path:&'static str,body:Option<Value>,session:Option<String>)->Result<Reply,String> {
    // Every path is a compile-time literal at an authorized call site.
    let address=SocketAddrV4::new(Ipv4Addr::LOCALHOST,port);
    let mut stream=TcpStream::connect_timeout(&address.into(),Duration::from_secs(3)).map_err(|_|"Worker unavailable")?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).map_err(|_|"Worker timeout setup failed")?;
    stream.set_write_timeout(Some(Duration::from_secs(5))).map_err(|_|"Worker timeout setup failed")?;
    let method=if body.is_some(){"POST"}else{"GET"};
    let payload=body.map(|b|b.to_string()).unwrap_or_default();
    if payload.len()>1024*1024{return Err("Worker request too large".into());}
    let cookie=match session {
        Some(s) if !s.is_empty() && s.bytes().all(|b|b.is_ascii_alphanumeric()||b"-_".contains(&b))=>format!("Cookie: one_kvm_session={s}\r\n"),
        Some(_)=>return Err("Invalid internal session".into()),None=>String::new(),
    };
    let request=format!("{method} {path} HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{cookie}\r\n{payload}",payload.len());
    stream.write_all(request.as_bytes()).map_err(|_|"Cannot send worker request")?;
    let mut bytes=vec![];stream.take(2*1024*1024+1).read_to_end(&mut bytes).map_err(|_|"Cannot read worker response")?;
    if bytes.len()>2*1024*1024{return Err("Worker response too large".into());}
    let split=bytes.windows(4).position(|b|b==b"\r\n\r\n").ok_or("Invalid worker HTTP response")?;
    let head=std::str::from_utf8(&bytes[..split]).map_err(|_|"Invalid worker headers")?;
    let mut lines=head.split("\r\n");let status=lines.next().and_then(|s|s.split_whitespace().nth(1)).ok_or("Missing worker status")?;
    if status!="200" {return Err("Worker rejected request".into());}
    let mut received_session=None;
    for line in lines {
        let Some((name,value))=line.split_once(':') else {continue};
        if name.eq_ignore_ascii_case("transfer-encoding") {return Err("Unexpected worker transfer encoding".into());}
        if name.eq_ignore_ascii_case("set-cookie") {
            if let Some(cookie)=value.trim().strip_prefix("one_kvm_session=") {
                received_session=Some(cookie.split(';').next().unwrap_or("").to_owned());
            }
        }
    }
    let body=serde_json::from_slice(&bytes[split+4..]).map_err(|_|"Invalid worker JSON response")?;
    Ok(Reply{body,session:received_session})
}

pub async fn login(port:u16,username:String,password:String)->Result<WorkerEndpoint,String> {
    let reply=tokio::task::spawn_blocking(move ||exchange(port,"/api/auth/login",Some(serde_json::json!({"username":username,"password":password})),None))
        .await.map_err(|_|"Worker login task failed")??;
    let session=reply.session.ok_or("Worker did not issue internal session")?;
    Ok(WorkerEndpoint{port,session})
}

#[derive(Clone,Copy)]
pub enum VideoAction {Offer,Ice,Close,IceServers,Status}
pub async fn video(endpoint:WorkerEndpoint,action:VideoAction,body:Value)->Result<Value,String> {
    let (path,body)=match action {
        VideoAction::Offer=>("/api/webrtc/offer",Some(body)),
        VideoAction::Ice=>("/api/webrtc/ice",Some(body)),
        VideoAction::Close=>("/api/webrtc/close",Some(body)),
        VideoAction::IceServers=>("/api/webrtc/ice-servers",None),
        VideoAction::Status=>("/api/stream/status",None),
    };
    tokio::task::spawn_blocking(move ||exchange(endpoint.port,path,body,Some(endpoint.session)))
        .await.map_err(|_|"Worker video task failed")?.map(|r|r.body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    #[tokio::test]
    async fn upstream_login_cookie_stays_internal_and_transport_does_not_follow_redirects() {
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();let port=listener.local_addr().unwrap().port();
        let server=std::thread::spawn(move || {
            let (mut stream,_)=listener.accept().unwrap();let mut data=[0;4096];let n=stream.read(&mut data).unwrap();
            let request=std::str::from_utf8(&data[..n]).unwrap();assert!(request.starts_with("POST /api/auth/login HTTP/1.0"));
            let payload="{\"next\":\"authenticated\"}";
            write!(stream,"HTTP/1.0 200 OK\r\nSet-Cookie: one_kvm_session=internal-token; HttpOnly\r\nContent-Length: {}\r\n\r\n{}",payload.len(),payload).unwrap();
        });
        let endpoint=login(port,"internal".into(),"not-a-user-password".into()).await.unwrap();assert_eq!(endpoint.session,"internal-token");server.join().unwrap();
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();let port=listener.local_addr().unwrap().port();
        let server=std::thread::spawn(move ||{let (mut stream,_)=listener.accept().unwrap();let mut data=[0;4096];let _=stream.read(&mut data);stream.write_all(b"HTTP/1.0 302 Found\r\nLocation: http://external.invalid/\r\n\r\n").unwrap();});
        assert!(login(port,"internal".into(),"secret".into()).await.is_err());server.join().unwrap();
    }
}
