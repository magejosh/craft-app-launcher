//! Local-only ownership and authenticated named pipe. Handles never inherit.
use crate::{actions::{self,Request,Response},core,native::wide};
use serde::Deserialize;
use std::{ffi::c_void,fs::{self,File,OpenOptions},io,os::windows::{fs::OpenOptionsExt,ffi::OsStringExt},path::{Path,PathBuf},ptr,sync::{Arc,atomic::{AtomicBool,AtomicU8,Ordering},mpsc::{self,Receiver,SyncSender}},thread,time::{Duration,Instant}};
type H=isize;
#[repr(C)] struct Security {length:u32,descriptor:*mut c_void,inherit:i32}
#[repr(C)] struct Overlapped {internal:usize,internal_high:usize,offset:u32,offset_high:u32,event:H}
#[repr(C)] struct SidAttributes {sid:*mut c_void,attributes:u32}
#[repr(C)] struct ProcessEntry {size:u32,usage:u32,pid:u32,heap:usize,module:u32,threads:u32,parent:u32,priority:i32,flags:u32,exe:[u16;260]}
#[link(name="kernel32")] extern "system" {
 fn CloseHandle(h:H)->i32;fn GetCurrentProcess()->H;fn OpenProcess(access:u32,inherit:i32,pid:u32)->H;
 fn QueryFullProcessImageNameW(h:H,flags:u32,path:*mut u16,size:*mut u32)->i32;
 fn CreateNamedPipeW(name:*const u16,open:u32,mode:u32,instances:u32,out:u32,input:u32,timeout:u32,sa:*const Security)->H;
 fn CreateFileW(name:*const u16,access:u32,share:u32,sa:*const Security,creation:u32,flags:u32,template:H)->H;
 fn ConnectNamedPipe(h:H,overlapped:*mut Overlapped)->i32;fn DisconnectNamedPipe(h:H)->i32;
 fn ReadFile(h:H,buffer:*mut c_void,size:u32,read:*mut u32,overlapped:*mut Overlapped)->i32;
 fn WriteFile(h:H,buffer:*const c_void,size:u32,written:*mut u32,overlapped:*mut Overlapped)->i32;
 fn CreateEventW(sa:*const Security,manual:i32,initial:i32,name:*const u16)->H;
 fn WaitForSingleObject(h:H,millis:u32)->u32;fn GetOverlappedResult(h:H,o:*mut Overlapped,bytes:*mut u32,wait:i32)->i32;
 fn CancelIoEx(h:H,o:*mut Overlapped)->i32;
 fn GetNamedPipeClientProcessId(h:H,pid:*mut u32)->i32;fn GetNamedPipeServerProcessId(h:H,pid:*mut u32)->i32;
 fn LocalFree(p:*mut c_void)->*mut c_void;
 fn CreateToolhelp32Snapshot(flags:u32,pid:u32)->H;fn Process32FirstW(h:H,e:*mut ProcessEntry)->i32;fn Process32NextW(h:H,e:*mut ProcessEntry)->i32;
}
#[link(name="advapi32")] extern "system"{
 fn OpenProcessToken(h:H,access:u32,token:*mut H)->i32;
 fn GetTokenInformation(token:H,class:u32,data:*mut c_void,size:u32,needed:*mut u32)->i32;
 fn ConvertSidToStringSidW(sid:*const c_void,text:*mut *mut u16)->i32;
 fn ConvertStringSecurityDescriptorToSecurityDescriptorW(text:*const u16,revision:u32,descriptor:*mut *mut c_void,size:*mut u32)->i32;
}
struct Handle(H);impl Drop for Handle{fn drop(&mut self){unsafe{CloseHandle(self.0);}}}
fn error()->String{io::Error::last_os_error().to_string()}
fn owned(h:H)->Result<Handle,String>{if h==0||h==-1{Err(error())}else{Ok(Handle(h))}}
fn token_data(token:H,class:u32)->Result<Vec<usize>,String>{unsafe{let mut n=0;GetTokenInformation(token,class,ptr::null_mut(),0,&mut n);if n==0||n>65536{return Err("Invalid token information size".into());}let mut data=vec![0usize;(n as usize+std::mem::size_of::<usize>()-1)/std::mem::size_of::<usize>()];if GetTokenInformation(token,class,data.as_mut_ptr().cast(),n,&mut n)==0{return Err(error());}Ok(data)}}
fn sid_string(sid:*mut c_void)->Result<String,String>{unsafe{let mut text=ptr::null_mut();if ConvertSidToStringSidW(sid,&mut text)==0{return Err(error());}let mut n=0;while n<256&&*text.add(n)!=0{n+=1;}let result=String::from_utf16(std::slice::from_raw_parts(text,n)).map_err(|e|e.to_string());LocalFree(text.cast());result}}
#[derive(Clone,Debug,PartialEq)] struct Identity{user:String,logon:String,session:u32}
fn identity(process:H)->Result<Identity,String>{unsafe{
 let mut t=0;if OpenProcessToken(process,8,&mut t)==0{return Err(error());}let t=owned(t)?;
 let user=token_data(t.0,1)?;let user=sid_string((*(user.as_ptr().cast::<SidAttributes>())).sid)?;
 let session=token_data(t.0,12)?;let session=*(session.as_ptr().cast::<u32>());
 let groups=token_data(t.0,2)?;let count=*(groups.as_ptr().cast::<u32>())as usize;
 let offset=std::mem::align_of::<SidAttributes>();if count>1024||offset+count*std::mem::size_of::<SidAttributes>()>groups.len()*std::mem::size_of::<usize>(){return Err("Invalid token groups".into());}
 let entries=std::slice::from_raw_parts((groups.as_ptr().cast::<u8>()).add(offset).cast::<SidAttributes>(),count);
 let logon=entries.iter().find(|g|g.attributes&0xC0000000==0xC0000000).ok_or("Token has no logon SID")?;
 Ok(Identity{user,logon:sid_string(logon.sid)?,session})
}}
fn process_image(h:H)->Result<PathBuf,String>{unsafe{let mut p=vec![0u16;32768];let mut n=p.len()as u32;if QueryFullProcessImageNameW(h,0,p.as_mut_ptr(),&mut n)==0{return Err(error());}core::checked_path(&PathBuf::from(std::ffi::OsString::from_wide(&p[..n as usize])))}}
fn hash_file(p:&Path)->Result<String,String>{crate::native::sha256_file(&mut File::open(p).map_err(|e|e.to_string())?,&AtomicBool::new(false))}
#[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Manifest{protocol:u32,gui_sha256:String,agent_sha256:String}
pub fn verify_pair(base:&Path)->Result<String,String>{
 let path=base.join("craft-launcher-pair.json");if fs::metadata(&path).map_err(|_|"Missing companion manifest; stage the verified GUI/agent pair")?.len()>4096{return Err("Companion manifest too large".into());}
 core::checked_path(&path)?;
 let m:Manifest=serde_json::from_slice(&fs::read(path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
 let gui=core::checked_path(&base.join("craft-app-launcher.exe"))?;let agent=core::checked_path(&base.join("craft-launcher-agent.exe"))?;
 if m.protocol!=1||hash_file(&gui)?!=m.gui_sha256||hash_file(&agent)?!=m.agent_sha256{return Err("Companion protocol/hash mismatch; no action executed".into());}Ok(m.gui_sha256)
}
pub fn base()->Result<PathBuf,String>{let p=std::env::current_exe().map_err(|e|e.to_string())?;core::checked_path(p.parent().ok_or("Missing executable directory")?)}
pub struct OwnerLock{_file:File}
impl OwnerLock{pub fn acquire(base:&Path)->Result<Self,String>{
 core::checked_path(base)?;
 let file=OpenOptions::new().read(true).write(true).create(true).share_mode(0).custom_flags(0x00200000).open(base.join("craft-launcher.owner")).map_err(|e|format!("OwnerBusy: another owner holds this portable base ({e})"))?;
 use std::os::windows::fs::MetadataExt;if file.metadata().map_err(|e|e.to_string())?.file_attributes()&(0x10|0x400)!=0{return Err("Invalid owner lock".into());}Ok(Self{_file:file})
}}
pub fn refuse_running_gui(base:&Path)->Result<(),String>{unsafe{
 let snapshot=owned(CreateToolhelp32Snapshot(2,0))?;let mut e:ProcessEntry=std::mem::zeroed();e.size=std::mem::size_of::<ProcessEntry>()as u32;
 let mut next=Process32FirstW(snapshot.0,&mut e);while next!=0{
 let n=e.exe.iter().position(|v|*v==0).unwrap_or(e.exe.len());let name=String::from_utf16_lossy(&e.exe[..n]);
 if name.eq_ignore_ascii_case("craft-app-launcher.exe")&&e.pid!=std::process::id(){
 let p=owned(OpenProcess(0x1000,0,e.pid)).map_err(|_|"Cannot verify a running launcher identity; refusing headless action")?;
 let image=process_image(p.0)?;if image.parent()==Some(base){return Err("OwnerBusy: target GUI is running without an available authenticated endpoint; legacy launchers cannot cooperate".into());}
 }next=Process32NextW(snapshot.0,&mut e);
 }Ok(())
}}
fn endpoint(base:&Path,id:&Identity)->Result<String,String>{
 use std::os::windows::ffi::OsStrExt;let base=core::checked_path(base)?;let mut b=Vec::new();for u in base.as_os_str().encode_wide(){b.extend(u.to_le_bytes());}b.extend(id.logon.as_bytes());Ok(format!(r"\\.\pipe\craft-launcher-{}",actions::hash(&b)?))
}
fn verify_peer(pipe:H,server:bool,identity_here:&Identity,base:&Path,gui_hash:&str)->Result<Handle,String>{unsafe{
 let mut pid=0;let ok=if server{GetNamedPipeServerProcessId(pipe,&mut pid)}else{GetNamedPipeClientProcessId(pipe,&mut pid)};
 if ok==0||pid==0{return Err("Cannot verify pipe peer PID".into());}let process=owned(OpenProcess(0x1000,0,pid))?;
 if identity(process.0)?!=*identity_here{return Err("Pipe peer user/logon/session mismatch".into());}
 if server{let p=process_image(process.0)?;if p!=core::checked_path(&base.join("craft-app-launcher.exe"))?||hash_file(&p)?!=gui_hash{return Err("Pipe server executable identity mismatch".into());}}
 Ok(process)
}}
fn pending(pipe:H,o:&mut Overlapped,started:i32,deadline:Instant,stop:&AtomicBool)->Result<u32,String>{unsafe{
 if started==0&&io::Error::last_os_error().raw_os_error()!=Some(997){return Err(error());}
 loop{if stop.load(Ordering::Relaxed)||Instant::now()>=deadline{CancelIoEx(pipe,o);let mut ignored=0;GetOverlappedResult(pipe,o,&mut ignored,1);return Err("Timeout/disconnect: outcome may be Unknown; do not replay automatically".into());}
 match WaitForSingleObject(o.event,50){0=>{let mut n=0;if GetOverlappedResult(pipe,o,&mut n,0)==0{return Err(error());}return Ok(n)},258=>(),_=>return Err(error())}
 }
}}
fn transfer(pipe:H,bytes:&mut[u8],write:bool,deadline:Instant,stop:&AtomicBool)->Result<(),String>{
 let event=owned(unsafe{CreateEventW(ptr::null(),1,0,ptr::null())})?;
 let mut done=0;while done<bytes.len(){let mut o=Overlapped{internal:0,internal_high:0,offset:0,offset_high:0,event:event.0};let mut n=0;
 let ok=unsafe{if write{WriteFile(pipe,bytes[done..].as_ptr().cast(),(bytes.len()-done)as u32,&mut n,&mut o)}else{ReadFile(pipe,bytes[done..].as_mut_ptr().cast(),(bytes.len()-done)as u32,&mut n,&mut o)}};
 let n=pending(pipe,&mut o,ok,deadline,stop)? as usize;if n==0{return Err("Peer disconnected; side-effect outcome may be Unknown".into());}done+=n;}Ok(())
}
fn read_frame(pipe:H,limit:usize,deadline:Instant,stop:&AtomicBool)->Result<Vec<u8>,String>{let mut head=[0u8;4];transfer(pipe,&mut head,false,deadline,stop)?;let n=u32::from_le_bytes(head)as usize;if n==0||n>limit{return Err("Frame size outside protocol bounds".into());}let mut b=vec![0u8;n];transfer(pipe,&mut b,false,deadline,stop)?;Ok(b)}
fn write_frame(pipe:H,mut bytes:Vec<u8>,deadline:Instant,stop:&AtomicBool)->Result<(),String>{let mut h=(bytes.len()as u32).to_le_bytes();transfer(pipe,&mut h,true,deadline,stop)?;transfer(pipe,&mut bytes,true,deadline,stop)}
/// A single atomic claim distinguishes a proven unstarted cancellation from an
/// uncertain operation. Expired queue entries can never start on a later wake.
pub struct Dispatch {deadline:Instant,state:AtomicU8}
impl Dispatch{
    pub fn new(deadline:Instant)->Self{Self{deadline,state:AtomicU8::new(0)}}
    pub fn try_start(&self)->bool{
        if Instant::now()>=self.deadline{self.cancel_before_start();return false;}
        self.state.compare_exchange(0,1,Ordering::AcqRel,Ordering::Acquire).is_ok()
    }
    pub fn cancel_before_start(&self)->bool{
        match self.state.compare_exchange(0,2,Ordering::AcqRel,Ordering::Acquire){Ok(_)|Err(2)=>true,Err(_)=>false}
    }
    pub fn stopped_response(&self,id:&str,reason:&str)->Response{
        if self.cancel_before_start(){Response::error(id,"not_executed",format!("{reason}; the command was canceled before it started"))}
        else{Response::error(id,"unknown",format!("{reason}; the command had started, reconcile before retrying"))}
    }
}
pub struct Command{pub request:Request,pub reply:SyncSender<Response>,pub dispatch:Arc<Dispatch>}
pub struct Server{pub receiver:Receiver<Command>,stop:Arc<AtomicBool>,thread:Option<thread::JoinHandle<()>>}
impl Drop for Server{fn drop(&mut self){self.stop.store(true,Ordering::Relaxed);if let Some(t)=self.thread.take(){let _=t.join();}}}
impl Server{pub fn start(base:&Path,hwnd:isize)->Result<Self,String>{
 let gui_hash=verify_pair(base)?;let id=identity(unsafe{GetCurrentProcess()})?;let name=endpoint(base,&id)?;
 let mut descriptor=ptr::null_mut();unsafe{if ConvertStringSecurityDescriptorToSecurityDescriptorW(wide(format!("D:P(A;;GA;;;{})",id.logon)).as_ptr(),1,&mut descriptor,ptr::null_mut())==0{return Err(error());}}
 let sa=Security{length:std::mem::size_of::<Security>()as u32,descriptor,inherit:0};
 let pipe=owned(unsafe{CreateNamedPipeW(wide(name).as_ptr(),3|0x40000000|0x00080000,8,1,actions::RESPONSE_LIMIT as u32,actions::REQUEST_LIMIT as u32,2000,&sa)});
 unsafe{LocalFree(descriptor);}let pipe=pipe?;
 let(sender,receiver)=mpsc::sync_channel::<Command>(8);let stop=Arc::new(AtomicBool::new(false));let stopped=stop.clone();let base=base.to_owned();
 let thread=thread::Builder::new().name("craft-local-pipe".into()).spawn(move||{
 let pipe=pipe; // Move the owning RAII handle, not only its Copy raw field, into the thread.
 let mut reported_error=false;
 while !stopped.load(Ordering::Relaxed){
 let event=match owned(unsafe{CreateEventW(ptr::null(),1,0,ptr::null())}){Ok(v)=>v,Err(_)=>break};let mut o=Overlapped{internal:0,internal_high:0,offset:0,offset_high:0,event:event.0};
 let ok=unsafe{ConnectNamedPipe(pipe.0,&mut o)};let connected=ok!=0||io::Error::last_os_error().raw_os_error()==Some(535)||pending(pipe.0,&mut o,ok,Instant::now()+Duration::from_secs(30),&stopped).map_err(|error|{if !reported_error{let _=core::log(&base,&format!("Agent pipe connect failed: {error}"));reported_error=true;}error}).is_ok();
 if connected{
 let handled=(||->Result<(),String>{
 let _peer=verify_peer(pipe.0,false,&id,&base,&gui_hash)?;
 let bytes=read_frame(pipe.0,actions::REQUEST_LIMIT,Instant::now()+Duration::from_secs(3),&stopped)?;
 let request=actions::decode(&bytes)?;let request_id=request.id.clone();let(tx,rx)=mpsc::sync_channel(1);
 let deadline=Instant::now()+Duration::from_secs(15);let dispatch=Arc::new(Dispatch::new(deadline));
 let response=if sender.try_send(Command{request,reply:tx,dispatch:dispatch.clone()}).is_err(){dispatch.stopped_response(&request_id,"Owner command queue is full")}else if unsafe{crate::native::PostMessageW(hwnd,0x804d,0,0)}==0{
     dispatch.stopped_response(&request_id,"Could not wake the owner")
 }else{
     loop{
         if stopped.load(Ordering::Relaxed)||Instant::now()>=deadline{break dispatch.stopped_response(&request_id,"Owner response deadline expired");}
         match rx.recv_timeout(Duration::from_millis(50)){
             Ok(v)=>break v,Err(mpsc::RecvTimeoutError::Timeout)=>(),
             Err(_)=>break dispatch.stopped_response(&request_id,"Owner stopped"),
         }
     }
 };
 write_frame(pipe.0,actions::encode(&response)?,Instant::now()+Duration::from_secs(3),&stopped)?;
 // DisconnectNamedPipe discards unread output. A bounded consumed-response ACK
 // retains it until the client has read it without an unbounded FlushFileBuffers.
 let mut ack=[0u8;1];transfer(pipe.0,&mut ack,false,Instant::now()+Duration::from_secs(3),&stopped)?;
 if ack!=[1]{return Err("Invalid consumed-response acknowledgement".into());}Ok(())
 })();if let Err(error)=handled{if !reported_error{let _=core::log(&base,&format!("Agent pipe rejected a request: {error}"));reported_error=true;}}
 }unsafe{DisconnectNamedPipe(pipe.0);}
 }
 }).map_err(|e|e.to_string())?;
 Ok(Self{receiver,stop,thread:Some(thread)})
}}
pub fn request(base:&Path,r:&Request)->Result<Option<Response>,String>{
 let hash=verify_pair(base)?;let id=identity(unsafe{GetCurrentProcess()})?;let name=endpoint(base,&id)?;
 let raw=unsafe{CreateFileW(wide(name).as_ptr(),0xC0000000,0,ptr::null(),3,0x40000000,0)};
 if raw==-1{let e=io::Error::last_os_error();return if e.raw_os_error()==Some(2){Ok(None)}else{Err(format!("OwnerBusy or inaccessible endpoint: {e}"))};}
 let pipe=owned(raw)?;let _peer=verify_peer(pipe.0,true,&id,base,&hash)?;let stop=AtomicBool::new(false);
 write_frame(pipe.0,serde_json::to_vec(r).map_err(|e|e.to_string())?,Instant::now()+Duration::from_secs(3),&stop).map_err(|e|format!("Pipe request write: {e}"))?;
 let bytes=read_frame(pipe.0,actions::RESPONSE_LIMIT,Instant::now()+Duration::from_secs(20),&stop).map_err(|e|format!("Pipe response read: {e}"))?;
 let response:Response=serde_json::from_slice(&bytes).map_err(|e|e.to_string())?;if response.protocol!=1||response.id!=r.id{return Err("Response envelope mismatch; outcome Unknown".into());}
 // A verified complete response remains authoritative even if its ACK is lost.
 let _=transfer(pipe.0,&mut[1u8],true,Instant::now()+Duration::from_secs(3),&stop);Ok(Some(response))
}
#[cfg(test)] mod tests{
    use super::*;
    fn base()->PathBuf{let p=Path::new(env!("CARGO_MANIFEST_DIR")).join(".build/temp").join(format!("craft-owner-{}",crate::native::random_id().unwrap()));fs::create_dir_all(&p).unwrap();core::checked_path(&p).unwrap()}
    #[test]fn physical_owner_excludes_second_writer_and_releases(){
        let b=base();let first=OwnerLock::acquire(&b).unwrap();assert!(OwnerLock::acquire(&b.join(".")).is_err());drop(first);assert!(OwnerLock::acquire(&b).is_ok());
    }
    #[test]fn pair_missing_or_unrecognized_manifest_fails_closed(){
        let b=base();assert!(verify_pair(&b).is_err());fs::write(b.join("craft-launcher-pair.json"),br#"{"protocol":99,"gui_sha256":"fake","agent_sha256":"fake"}"#).unwrap();assert!(verify_pair(&b).is_err());
    }
    #[test]fn server_retains_pipe_handle_after_start_returns(){
        let b=base();fs::write(b.join("craft-app-launcher.exe"),b"fixture-gui").unwrap();fs::write(b.join("craft-launcher-agent.exe"),b"fixture-agent").unwrap();
        let manifest=serde_json::json!({"protocol":1,"gui_sha256":hash_file(&b.join("craft-app-launcher.exe")).unwrap(),"agent_sha256":hash_file(&b.join("craft-launcher-agent.exe")).unwrap()});
        fs::write(b.join("craft-launcher-pair.json"),serde_json::to_vec(&manifest).unwrap()).unwrap();
        let server=Server::start(&b,isize::MAX).unwrap();let id=identity(unsafe{GetCurrentProcess()}).unwrap();let name=endpoint(&b,&id).unwrap();
        let client=owned(unsafe{CreateFileW(wide(&name).as_ptr(),0xC0000000,0,ptr::null(),3,0x40000000,0)}).expect("server must retain its pipe after returning");
        let stop=AtomicBool::new(false);let request=Request{protocol:1,id:"delayed-reader".into(),action:actions::Action::State};
        write_frame(client.0,serde_json::to_vec(&request).unwrap(),Instant::now()+Duration::from_secs(3),&stop).unwrap();
        thread::sleep(Duration::from_millis(100)); // Force the server to finish writing before this client reads.
        let bytes=read_frame(client.0,actions::RESPONSE_LIMIT,Instant::now()+Duration::from_secs(3),&stop).unwrap();
        let response:Response=serde_json::from_slice(&bytes).unwrap();assert_eq!(response.id,"delayed-reader");assert_eq!(response.code,"not_executed");
        transfer(client.0,&mut[1u8],true,Instant::now()+Duration::from_secs(3),&stop).unwrap();
        drop(client);thread::sleep(Duration::from_millis(100));
        let withheld=owned(unsafe{CreateFileW(wide(&name).as_ptr(),0xC0000000,0,ptr::null(),3,0x40000000,0)}).unwrap();
        write_frame(withheld.0,serde_json::to_vec(&request).unwrap(),Instant::now()+Duration::from_secs(3),&stop).unwrap();
        let bytes=read_frame(withheld.0,actions::RESPONSE_LIMIT,Instant::now()+Duration::from_secs(3),&stop).unwrap();
        let verified:Response=serde_json::from_slice(&bytes).unwrap();assert_eq!(verified.id,request.id);
        thread::sleep(Duration::from_millis(3300)); // Withhold ACK; server must release this connection.
        let next=owned(unsafe{CreateFileW(wide(&name).as_ptr(),0xC0000000,0,ptr::null(),3,0x40000000,0)}).expect("withheld ACK must not strand the server");
        assert_eq!(verified.code,"not_executed");drop(withheld);drop(next);
        let before=Instant::now();drop(server);assert!(before.elapsed()<Duration::from_secs(2),"stop must cancel pending pipe I/O");
    }
    #[test]fn endpoint_normalizes_gui_plain_and_agent_extended_base(){
        let b=base();let plain=PathBuf::from(b.to_string_lossy().strip_prefix(r"\\?\").unwrap().to_owned());
        let current=identity(unsafe{GetCurrentProcess()}).unwrap();
        assert_eq!(endpoint(&plain,&current).unwrap(),endpoint(&b,&current).unwrap());
        assert_eq!(endpoint(&plain.join("."),&current).unwrap(),endpoint(&b,&current).unwrap());
    }
    #[test]fn token_endpoint_is_logon_scoped(){
        let b=base();let current=identity(unsafe{GetCurrentProcess()}).unwrap();assert!(current.logon.starts_with("S-1-5-5-"));assert!(current.user.starts_with("S-1-"));let mut other=current.clone();other.logon.push_str("-other");assert_ne!(endpoint(&b,&current).unwrap(),endpoint(&b,&other).unwrap());
    }
}
#[cfg(test)] impl Server{
    pub(crate) fn fixture(receiver:Receiver<Command>)->Self{Self{receiver,stop:Arc::new(AtomicBool::new(false)),thread:None}}
}
#[cfg(test)] mod dispatch_regressions{
    use super::*;
    #[test]fn expired_or_failed_wake_cannot_execute_on_later_dispatch(){
        let expired=Dispatch::new(Instant::now()-Duration::from_secs(1));assert!(!expired.try_start());assert_eq!(expired.stopped_response("expired","deadline").code,"not_executed");assert!(!expired.try_start());
        let failed_wake=Dispatch::new(Instant::now()+Duration::from_secs(15));assert_eq!(failed_wake.stopped_response("wake","Could not wake the owner").code,"not_executed");assert!(!failed_wake.try_start());
    }
    #[test]fn timeout_after_execution_claim_is_unknown_never_not_executed(){
        let started=Dispatch::new(Instant::now()+Duration::from_secs(15));assert!(started.try_start());assert_eq!(started.stopped_response("started","deadline").code,"unknown");assert!(!started.try_start());
    }
}