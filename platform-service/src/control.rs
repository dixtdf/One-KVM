// SPDX-License-Identifier: AGPL-3.0-only
//! Per-channel control lease state. Holding the manager's control mutex across
//! authorization, delivery and release prevents competing owners interleaving.
use crate::gateway::WorkerEndpoint;
use std::{collections::BTreeMap,time::{Duration,Instant}};

pub struct Lease {
    pub user:String,
    pub token:String,
    pub endpoint:WorkerEndpoint,
    pub deadline:Instant,
}
#[derive(Default)]
pub struct Controls {pub leases:BTreeMap<String,Lease>}
impl Controls {
    pub fn owns(&self,channel:&str,user:&str,token:&str)->bool {
        self.leases.get(channel).is_some_and(|lease|lease.user==user && lease.token==token && lease.deadline>Instant::now())
    }
    pub fn renew(&mut self,channel:&str){
        if let Some(lease)=self.leases.get_mut(channel){lease.deadline=Instant::now()+Duration::from_secs(15);}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lease_expires_and_cannot_be_reused_by_another_login_or_channel(){
        let mut controls=Controls::default();controls.leases.insert("a".into(),Lease{user:"u".into(),token:"login1".into(),endpoint:WorkerEndpoint{port:1,session:"secret".into()},deadline:Instant::now()+Duration::from_secs(1)});
        assert!(controls.owns("a","u","login1"));assert!(!controls.owns("a","u","login2"));assert!(!controls.owns("b","u","login1"));
        controls.leases.get_mut("a").unwrap().deadline=Instant::now()-Duration::from_secs(1);assert!(!controls.owns("a","u","login1"));
    }
}
