//! Shared transfer phase and byte snapshots; bytes update cheaply between UI polls.
use serde::Serialize;
use std::sync::{Mutex,atomic::{AtomicU64,Ordering}};
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize)]
#[serde(rename_all="snake_case")]
pub enum Phase {Idle,Queued,Checking,Downloading,Installing,Installed,Failed,Skipped}
impl Phase {pub fn label(self)->&'static str{match self{Self::Idle=>"",Self::Queued=>"Queued",Self::Checking=>"Checking",Self::Downloading=>"Downloading",Self::Installing=>"Installing",Self::Installed=>"Installed",Self::Failed=>"Failed",Self::Skipped=>"Skipped"}}}
#[derive(Clone,Debug,Serialize)]
pub struct Snapshot{pub app:Option<String>,pub phase:Phase,pub downloaded_bytes:u64,pub total_bytes:Option<u64>,pub percent:Option<u64>,pub message:String}
impl Snapshot{
    pub fn label(&self)->String{match self.phase{
        Phase::Downloading=>match self.total_bytes.filter(|n|*n>0){Some(total)=>format!("Downloading {}{}",download_bytes(self.downloaded_bytes,total),self.percent.map(|n|format!(" ({n}%)")).unwrap_or_default()),None=>format!("Downloading {}",bytes(self.downloaded_bytes))},
        Phase::Installing=>format!("Installing - {} downloaded",bytes(self.downloaded_bytes)),_=>self.phase.label().into()}}
    pub fn bar(&self)->(Option<u16>,bool){match self.phase{Phase::Downloading=>(self.percent.map(|v|v as u16*10),self.percent.is_none()),Phase::Installing|Phase::Checking=>(None,true),Phase::Installed=>(Some(1000),false),_=>(Some(0),false)}}
}
fn download_bytes(value:u64,total:u64)->String{if total>=1024*1024{format!("{:.1}/{:.1} MiB",value as f64/(1024.0*1024.0),total as f64/(1024.0*1024.0))}else if total>=1024{format!("{:.1}/{:.1} KiB",value as f64/1024.0,total as f64/1024.0)}else{format!("{value}/{total} B")}}
pub fn bytes(value:u64)->String{if value>=1024*1024{format!("{:.1} MiB",value as f64/(1024.0*1024.0))}else if value>=1024{format!("{:.1} KiB",value as f64/1024.0)}else{format!("{value} B")}}
pub struct WorkProgress{bytes:AtomicU64,state:Mutex<Snapshot>}
impl WorkProgress{
    pub fn new(bytes:u64)->Self{Self{bytes:AtomicU64::new(bytes),state:Mutex::new(Snapshot{app:None,phase:Phase::Idle,downloaded_bytes:0,total_bytes:None,percent:None,message:String::new()})}}
    pub fn load(&self,order:Ordering)->u64{self.bytes.load(order)}
    pub fn store(&self,bytes:u64,order:Ordering){self.bytes.store(bytes,order);}
    pub fn begin(&self,app:usize,phase:Phase,total:Option<u64>,message:&str){let mut state=self.state.lock().unwrap_or_else(|p|p.into_inner());self.bytes.store(0,Ordering::Relaxed);*state=Snapshot{app:crate::core::APPS.get(app).map(|a|a.0.into()),phase,downloaded_bytes:0,total_bytes:total.filter(|n|*n>0),percent:None,message:message.into()};}
    pub fn phase(&self,phase:Phase,message:&str){let mut state=self.state.lock().unwrap_or_else(|p|p.into_inner());state.phase=phase;state.message=message.into();}
    pub fn snapshot(&self)->Snapshot{let guard=self.state.lock().unwrap_or_else(|p|p.into_inner());let mut state=guard.clone();state.downloaded_bytes=self.bytes.load(Ordering::Relaxed);drop(guard);state.percent=if state.phase==Phase::Downloading{state.total_bytes.filter(|n|*n>=state.downloaded_bytes).map(|n|(state.downloaded_bytes as u128*100/n as u128)as u64)}else{None};state}
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn known_unknown_length_and_install_phases_never_invent_percent(){let p=WorkProgress::new(0);p.begin(4,Phase::Downloading,None,"");p.store(512,Ordering::Relaxed);assert_eq!(p.snapshot().percent,None);assert!(p.snapshot().bar().1);p.begin(4,Phase::Downloading,Some(1024),"");p.store(512,Ordering::Relaxed);assert_eq!(p.snapshot().percent,Some(50));assert_eq!(p.snapshot().bar(),(Some(500),false));p.phase(Phase::Installing,"Verifying SHA-256");assert_eq!(p.snapshot().percent,None);assert!(p.snapshot().label().starts_with("Installing"));p.phase(Phase::Installed,"");assert_eq!(p.snapshot().bar(),(Some(1000),false));p.begin(0,Phase::Downloading,Some(0),"");assert_eq!(p.snapshot().downloaded_bytes,0);assert_eq!(p.snapshot().percent,None);p.phase(Phase::Failed,"Network interrupted");assert_eq!(p.snapshot().message,"Network interrupted");assert!(!p.snapshot().bar().1);}
}
