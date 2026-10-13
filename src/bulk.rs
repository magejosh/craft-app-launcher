//! Sequential, conservative updates through owner-issued install plans.
use crate::progress::{WorkProgress,Phase};
use crate::{actions,core::{self,Config,Install,APPS},updates};
use serde::Serialize;
use serde_json::{json,Value};
use std::{path::Path,sync::{Mutex,atomic::{AtomicBool,Ordering}}};
#[derive(Clone,Debug,Serialize)]
pub struct Row {pub app:String,pub status:String,pub message:String,pub progress:Option<crate::progress::Snapshot>}
#[derive(Clone,Debug,Serialize)]
pub struct Progress {pub status:String,pub completed:usize,pub total:usize,pub rows:Vec<Row>,pub stop_reason:Option<String>}
impl Default for Progress {fn default()->Self{Self{status:"preparing".into(),completed:0,total:0,rows:Vec::new(),stop_reason:None}}}
impl Progress {
    pub fn summary(&self)->String {
        let installed=self.rows.iter().filter(|r|r.status=="installed").count();let failed=self.rows.iter().filter(|r|r.status=="failed").count();let skipped=self.rows.iter().filter(|r|r.status=="skipped").count();
        format!("Update All {}: {}/{} processed; {} installed, {} failed, {} skipped{}",self.status,self.completed,self.total,installed,failed,skipped,self.stop_reason.as_ref().map(|r|format!(". {r}")).unwrap_or_default())
    }
    pub fn details(&self)->String {format!("{}\r\n{}",self.summary(),self.rows.iter().map(|r|format!("{}: {} — {}",r.app,r.status,r.message)).collect::<Vec<_>>().join("\r\n"))}
}
pub fn snapshot(progress:&Mutex<Progress>)->Progress{progress.lock().unwrap_or_else(|p|p.into_inner()).clone()}
pub fn snapshot_live(shared:&Mutex<Progress>,transfer:&WorkProgress)->Progress{
    let mut batch=snapshot(shared);let current=transfer.snapshot();
    if let Some(row)=batch.rows.iter_mut().find(|row|Some(row.app.as_str())==current.app.as_deref()&&matches!(row.status.as_str(),"checking"|"downloading"|"installing")){
        row.status=match current.phase{Phase::Downloading=>"downloading",Phase::Installing=>"installing",Phase::Installed=>"installed",Phase::Failed=>"failed",_=>row.status.as_str()}.into();row.progress=Some(current);
    }batch
}
fn publish(shared:&Mutex<Progress>,progress:&Progress){*shared.lock().unwrap_or_else(|p|p.into_inner())=progress.clone();}
/// Every installed version must be comparable; preference never hides a current copy.
pub fn eligible(app:usize,installs:&[Install],release:&updates::Release)->Result<(),String>{
    let versions:Vec<_>=installs.iter().filter(|i|i.app==app).map(|i|updates::compare_version(&i.version,release)).collect();
    if versions.is_empty(){return Err("Not installed".into());}
    if versions.iter().any(|v|matches!(v,updates::Comparison::Current|updates::Comparison::Ahead)){return Err("Latest or newer version already installed; preference preserved".into());}
    if versions.iter().any(|v|matches!(v,updates::Comparison::Unknown)){return Err("Unknown installed version; review manually".into());}
    if release.assets.len()!=1{return Err("No unique verified portable asset; review manually".into());}
    updates::validate_asset(app,&release.tag,&release.assets[0])
}
pub fn guard(base:&Path,config:&Config,expected:&Path,cancel:&AtomicBool)->Result<(),String>{
    if cancel.load(Ordering::Relaxed){return Err("Cancellation requested; remaining apps were not attempted".into());}
    let current=actions::apps_folder(base,config)?;
    let (disk,_) = actions::load(base)?;
    if current!=expected||actions::apps_folder(base,&disk)?!=expected{return Err("Apps root changed; remaining apps were not attempted".into());}
    Ok(())
}
// Callbacks make selection, partial failure and stop behavior testable without downloads.
fn process<F,G,H>(installs:&[Install],shared:&Mutex<Progress>,bytes:&WorkProgress,mut check:F,mut install:G,mut guard:H)->Value
where F:FnMut(usize)->Result<updates::Release,String>,G:FnMut(usize,&updates::Release)->Result<Value,String>,H:FnMut()->Result<(),String>{
    let mut p=Progress{status:"running".into(),rows:APPS.iter().enumerate().filter(|(a,_)|installs.iter().any(|i|i.app==*a)).map(|(_, (id,_))|Row{app:(*id).into(),status:"queued".into(),message:String::new(),progress:None}).collect(),..Progress::default()};p.total=p.rows.len();publish(shared,&p);
    for row in 0..p.total {
        if let Err(e)=guard(){p.stop_reason=Some(e);break;}
        let app=actions::app_id(&p.rows[row].app).expect("catalogue app");bytes.begin(app,Phase::Checking,None,"Checking official release metadata");p.rows[row].status="checking".into();p.rows[row].message="Checking latest official metadata".into();publish(shared,&p);
        let release=check(app);
        if let Err(e)=guard(){p.stop_reason=Some(e);break;}
        match release {
            Err(e)=>{bytes.phase(Phase::Failed,&e);p.rows[row].status="failed".into();p.rows[row].message=format!("Metadata check failed: {e}");},
            Ok(release)=>match eligible(app,installs,&release){
                Err(e)=>{bytes.phase(Phase::Skipped,&e);p.rows[row].status="skipped".into();p.rows[row].message=e;},
                Ok(())=>{
                    p.rows[row].status="downloading".into();p.rows[row].message=format!("{}; downloading verified portable archive",release.tag);bytes.begin(app,Phase::Downloading,Some(release.assets[0].size),&p.rows[row].message);publish(shared,&p);
                    match install(app,&release){
                        Ok(value)=>{bytes.phase(Phase::Installed,"Installed");p.rows[row].status="installed".into();p.rows[row].message=value.to_string();},
                        Err(e)=>{bytes.phase(Phase::Failed,&e);p.rows[row].status="failed".into();p.rows[row].message=e;},
                    }
                }
            }
        }
        p.rows[row].progress=Some(bytes.snapshot());
        p.completed+=1;publish(shared,&p);
        if let Err(e)=guard(){p.stop_reason=Some(e);break;}
    }
    p.status=if p.stop_reason.is_some(){"stopped"}else{"completed"}.into();
    if p.stop_reason.is_some(){for row in &mut p.rows {if matches!(row.status.as_str(),"queued"|"checking"|"downloading"|"installing"){row.status="not_attempted".into();row.message="Stopped before a verified result; inspect retained staging if present".into();}}}
    publish(shared,&p);json!({"summary":p.summary(),"details":p.details(),"bulk":p})
}
pub fn run(base:&Path,config:&Config,cancel:&AtomicBool,bytes:&WorkProgress,shared:&Mutex<Progress>)->Result<Value,String>{
    let root=actions::apps_folder(base,config)?;guard(base,config,&root,cancel)?;
    let (installs,notes)=core::discover_timed(&root,&mut core::ScanStats::default())?;
    if notes.iter().any(|n|!core::scan_note_is_informational(n)){return Err("Incomplete discovery; Update All did not install anything. Repair scan warnings first.".into());}
    Ok(process(&installs,shared,bytes,actions::official_release,|app,release|{
        guard(base,config,&root,cancel)?;
        // Refresh discovery before each plan, including versions published outside this owner.
        let (current,notes)=core::discover_timed(&root,&mut core::ScanStats::default())?;
        if notes.iter().any(|n|!core::scan_note_is_informational(n)){return Err("Discovery became incomplete; app was not installed".into());}
        eligible(app,&current,release)?;
        let asset=&release.assets[0];let input=actions::AssetInput{name:asset.name.clone(),url:asset.url.clone(),size:asset.size,digest:asset.digest.clone()};
        let plan=actions::make_plan(base,config,APPS[app].0,&release.tag,&input)?;
        guard(base,config,&root,cancel)?;
        actions::confirm_guarded(base,config,&plan,cancel,bytes,&||guard(base,config,&root,cancel))
    },||guard(base,config,&root,cancel)))
}
#[cfg(test)] mod tests {
    use super::*;
    fn release(app:usize)->updates::Release {let id=APPS[app].0;let name=format!("{id}-1.2.3-windows-x64-portable.zip");updates::parse_release_for(app,&serde_json::to_vec(&json!({"tag_name":"v1.2.3","draft":false,"prerelease":false,"assets":[{"name":name,"browser_download_url":format!("https://github.com/storytold/{id}/releases/download/v1.2.3/{name}"),"size":123,"digest":format!("sha256:{}","a".repeat(64))}]})).unwrap()).unwrap()}
    fn install(app:usize,version:&str)->Install{Install{app,gui:None,cli:None,folder:std::path::PathBuf::from(format!("fixture-{app}")),version:version.into()}}
    #[test] fn selection_ignores_preference_and_skips_unknown_current_ahead_and_assets(){
        let r=release(0);let older=install(0,"1.0.0.0 [PE file version]");assert!(eligible(0,&[older.clone()],&r).is_ok());assert!(eligible(0,&[],&r).is_err());
        for version in ["1.2.3.0 [PE file version]","2.0.0.0 [PE file version]","Unknown; folder hint 1.0.0","1.0.0.1 [PE file version]"]{assert!(eligible(0,&[older.clone(),install(0,version)],&r).is_err());}
        let mut no=r.clone();no.assets.clear();assert!(eligible(0,&[older.clone()],&no).is_err());let mut many=r.clone();many.assets.push(r.assets[0].clone());assert!(eligible(0,&[older],&many).is_err());
    }
    #[test] fn partial_failure_continues_and_no_updates_never_install(){
        let rows=vec![install(0,"1.0.0.0 [PE file version]"),install(1,"1.0.0.0 [PE file version]"),install(2,"1.2.3.0 [PE file version]")];let shared=Mutex::new(Progress::default());let mut calls=Vec::new();
        let value=process(&rows,&shared,&WorkProgress::new(0),|a|Ok(release(a)),|a,_|{assert_eq!(snapshot(&shared).rows[a].status,"downloading");calls.push(a);if a==0{Err("injected failure".into())}else{Ok(json!({"folder":"fixture"}))}},||Ok(()));
        assert_eq!(calls,vec![0,1]);assert_eq!(value["bulk"]["completed"],3);assert_eq!(value["bulk"]["rows"][0]["status"],"failed");assert_eq!(value["bulk"]["rows"][1]["status"],"installed");assert_eq!(value["bulk"]["rows"][2]["status"],"skipped");
        process(&[],&shared,&WorkProgress::new(0),|_|panic!(),|_,_|panic!(),||Ok(()));assert_eq!(snapshot(&shared).total,0);
    }
    #[test]fn live_rows_track_download_install_failure_and_next_app_without_stale_bytes(){
        let rows=vec![install(0,"1.0.0.0 [PE file version]"),install(1,"1.0.0.0 [PE file version]")];let shared=Mutex::new(Progress::default());let transfer=WorkProgress::new(0);
        let value=process(&rows,&shared,&transfer,|a|Ok(release(a)),|app,_|{
            assert_eq!(transfer.snapshot().downloaded_bytes,0);let batch=snapshot_live(&shared,&transfer);assert_eq!(batch.rows[app].status,"downloading");if app==0{assert_eq!(batch.rows[1].status,"queued");}else{assert_eq!(batch.rows[0].status,"failed");assert_eq!(batch.rows[0].progress.as_ref().unwrap().phase,Phase::Failed);}
            transfer.store(61,Ordering::Relaxed);let live=snapshot_live(&shared,&transfer);assert_eq!(live.rows[app].progress.as_ref().unwrap().percent,Some(49));
            transfer.phase(Phase::Installing,"Verifying and extracting");let live=snapshot_live(&shared,&transfer);assert_eq!(live.rows[app].status,"installing");assert_eq!(live.rows[app].progress.as_ref().unwrap().percent,None);
            if app==0{Err("Fixture extraction failure".into())}else{Ok(json!({"installed":true}))}
        },||Ok(()));assert_eq!(value["bulk"]["completed"],2);assert_eq!(value["bulk"]["rows"][0]["progress"]["phase"],"failed");assert_eq!(value["bulk"]["rows"][1]["progress"]["phase"],"installed");assert_eq!(value["bulk"]["rows"][0]["progress"]["message"],"Fixture extraction failure");
    }
    #[test] fn fatal_guard_stops_with_honest_remaining_rows(){
        let rows=vec![install(0,"1.0.0.0 [PE file version]"),install(1,"1.0.0.0 [PE file version]")];let shared=Mutex::new(Progress::default());let checks=std::cell::Cell::new(0);
        let value=process(&rows,&shared,&WorkProgress::new(0),|a|Ok(release(a)),|_,_|Ok(json!({})),||{checks.set(checks.get()+1);if checks.get()>2{Err("root disconnected".into())}else{Ok(())}});
        assert_eq!(value["bulk"]["status"],"stopped");assert_eq!(value["bulk"]["rows"][0]["status"],"installed");assert_eq!(value["bulk"]["rows"][1]["status"],"not_attempted");
    }
}

#[cfg(test)] mod root_tests {
    use super::*;
    #[test] fn folder_validation_cancel_and_root_change_preserve_settings(){
        let base=Path::new(env!("CARGO_MANIFEST_DIR")).join(".build/temp").join(format!("bulk-root-{}",crate::native::random_id().unwrap()));std::fs::create_dir_all(&base).unwrap();let base=core::checked_path(&base).unwrap();
        assert!(actions::apps_folder(&base,&Config::default()).is_err());
        let mut config=Config::default();config.root=Some(base.join("missing"));assert!(actions::apps_folder(&base,&config).is_err());
        let file=base.join("ordinary-file");std::fs::write(&file,b"fixture").unwrap();config.root=Some(file);assert!(actions::apps_folder(&base,&config).is_err());
        config.root=Some(base.clone());config.selections.insert("filmcraft".into(),"older".into());config.save(&base).unwrap();let bytes=std::fs::read(base.join("craft-launcher.conf")).unwrap();
        assert_eq!(actions::apps_folder(&base,&config).unwrap(),base);assert!(guard(&base,&config,&base,&AtomicBool::new(false)).is_ok());assert!(guard(&base,&config,&base,&AtomicBool::new(true)).unwrap_err().contains("Cancellation"));assert_eq!(std::fs::read(base.join("craft-launcher.conf")).unwrap(),bytes);
        let other=base.join("other");std::fs::create_dir(&other).unwrap();let mut changed=config.clone();changed.root=Some(other);changed.save(&base).unwrap();let bytes=std::fs::read(base.join("craft-launcher.conf")).unwrap();assert!(guard(&base,&config,&base,&AtomicBool::new(false)).unwrap_err().contains("changed"));assert_eq!(std::fs::read(base.join("craft-launcher.conf")).unwrap(),bytes);
    }
}
