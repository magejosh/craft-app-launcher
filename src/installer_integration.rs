//! Test-only transport and deterministic cancellation; never compiled into releases.
use super::*;
use std::{cell::RefCell,io::Cursor};
struct Fixture { bytes:Vec<u8>, cancel_at:&'static str }
thread_local! { static FIXTURE:RefCell<Option<Fixture>>=const {RefCell::new(None)}; }
struct FixtureGuard;
impl Drop for FixtureGuard { fn drop(&mut self){FIXTURE.with(|f|*f.borrow_mut()=None);} }
fn configure(bytes:Vec<u8>,cancel_at:&'static str)->FixtureGuard{FIXTURE.with(|f|*f.borrow_mut()=Some(Fixture{bytes,cancel_at}));FixtureGuard}
pub(super) fn checkpoint(point:&str,cancel:&AtomicBool){FIXTURE.with(|f|{if f.borrow().as_ref().is_some_and(|v|v.cancel_at==point){cancel.store(true,Ordering::Relaxed);}});}
pub(super) fn download(size:u64,file:&mut File,cancel:&AtomicBool,progress:&WorkProgress)->Result<(),String>{
    assert_eq!(progress.snapshot().phase,Phase::Downloading);assert_eq!(progress.snapshot().total_bytes,Some(size));
    // Tests fail closed without a fixture: never fall through to live networking.
    let bytes=FIXTURE.with(|f|f.borrow().as_ref().map(|v|v.bytes.clone())).ok_or("Offline test fixture is not configured")?;
    let mut total=0u64;
    for chunk in bytes.chunks(1024){stop(cancel)?;total+=chunk.len()as u64;if total>size{return Err("Download exceeded official asset size".into());}file.write_all(chunk).map_err(|e|e.to_string())?;progress.store(total,Ordering::Relaxed);checkpoint("download",cancel);}
    stop(cancel)?;if total!=size{return Err("Download length does not match official asset metadata".into());}file.sync_all().map_err(|e|e.to_string())
}
const PE:&[u8]=include_bytes!("../tests/fixtures/sentinel.exe");
const FOLDER:&str="filmcraft-0.6.0-windows-x64-portable";
fn archive(entries:&[(&str,&[u8])])->Vec<u8>{let mut writer=zip::ZipWriter::new(Cursor::new(Vec::new()));for(name,bytes)in entries{writer.start_file(*name,zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)).unwrap();writer.write_all(bytes).unwrap();}writer.finish().unwrap().into_inner()}
fn root(case:&str)->PathBuf{let path=std::env::temp_dir().join(format!("craft-installer-integration-{}-{}-{case}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir(&path).unwrap();let old=path.join("filmcraft-0.5.0-windows-x64-portable");fs::create_dir(&old).unwrap();fs::write(old.join("user-sentinel.txt"),b"older version user data must survive").unwrap();path}
fn preserved(root:&Path){assert_eq!(fs::read(root.join("filmcraft-0.5.0-windows-x64-portable/user-sentinel.txt")).unwrap(),b"older version user data must survive");}
fn asset(root:&Path,bytes:&[u8])->Asset{let path=root.join("fixture-source.zip");fs::write(&path,bytes).unwrap();let hash=native::sha256_file(&mut File::open(path).unwrap(),&AtomicBool::new(false)).unwrap();let name=format!("{FOLDER}.zip");Asset{name:name.clone(),url:format!("https://github.com/storytold/filmcraft/releases/download/v0.6.0/{name}"),size:bytes.len()as u64,digest:format!("sha256:{hash}")}}
fn stages(root:&Path)->Vec<PathBuf>{fs::read_dir(root).unwrap().map(|v|v.unwrap().path()).filter(|v|v.file_name().unwrap().to_string_lossy().starts_with(".craft-stage-")).collect()}
fn rejected(case:&str,bytes:Vec<u8>,cancel_at:&'static str,change:impl FnOnce(&mut Asset),expected:&str){
    let root=root(case);let mut asset=asset(&root,&bytes);change(&mut asset);let _guard=configure(bytes,cancel_at);let cancel=AtomicBool::new(cancel_at=="before");let progress=WorkProgress::new(0);
    let error=install(&root,4,"v0.6.0",&asset,&cancel,&progress).unwrap_err();assert_eq!(progress.snapshot().phase,Phase::Failed);assert!(progress.snapshot().message.contains(expected));assert!(error.contains(expected),"{case}: {error}");assert!(!root.join(FOLDER).exists());preserved(&root);
    let stages=stages(&root);if cancel_at=="before"{assert!(stages.is_empty());}else{assert_eq!(stages.len(),1);assert!(stages[0].join("download.zip").is_file());if case=="digest"||case=="short-size"||case=="long-size"{assert!(!stages[0].join("payload").exists());}if case=="unsafe"{assert_eq!(fs::read_dir(stages[0].join("payload")).unwrap().count(),0);}if cancel_at=="extract"{assert!(stages[0].join("payload/filmcraft.exe").is_file());}if cancel_at=="publish"{assert_eq!(native::file_version(&stages[0].join("payload/filmcraft.exe")).as_deref(),Some("0.6.0.0"));}}
    eprintln!("OFFLINE {case}: rejected as expected; old data preserved; retained fixture {}",root.display());
}
#[test]fn installer_offline_success_and_no_replace(){
    let root=root("success");let bytes=archive(&[("filmcraft.exe",PE),("data/new.txt",b"new version data")]);let asset=asset(&root,&bytes);let _guard=configure(bytes,"");let cancel=AtomicBool::new(false);let progress=WorkProgress::new(0);let installed=install(&root,4,"v0.6.0",&asset,&cancel,&progress).unwrap();
    assert_eq!(installed.folder,core::checked_path(&root).unwrap().join(FOLDER));assert_eq!(fs::read(installed.folder.join("filmcraft.exe")).unwrap(),PE);assert_eq!(native::file_version(&installed.folder.join("filmcraft.exe")).as_deref(),Some("0.6.0.0"));assert_eq!(progress.load(Ordering::Relaxed),asset.size);assert_eq!(progress.snapshot().phase,Phase::Installed);assert_eq!(progress.snapshot().percent,None);assert_eq!(installed.retained_archive_bytes,0);assert!(!installed.stage.join("download.zip").exists());assert!(!installed.stage.exists());assert!(installed.cleanup_warning.is_none());preserved(&root);
    fs::write(installed.folder.join("user-sentinel.txt"),b"do not replace").unwrap();let before=stages(&root).len();assert!(install(&root,4,"v0.6.0",&asset,&cancel,&progress).unwrap_err().contains("already exists"));assert_eq!(stages(&root).len(),before);assert_eq!(fs::read(installed.folder.join("user-sentinel.txt")).unwrap(),b"do not replace");preserved(&root);eprintln!("OFFLINE success/no-replace: retained fixture {}",root.display());
}
#[test]fn installer_offline_integrity_and_metadata_rejections(){
    let valid=archive(&[("filmcraft.exe",PE)]);
    rejected("digest",valid.clone(),"",|a|a.digest=format!("sha256:{}","0".repeat(64)),"SHA-256 mismatch");
    rejected("short-size",valid.clone(),"",|a|a.size-=1,"exceeded official asset size");
    rejected("long-size",valid,"",|a|a.size+=1,"length does not match");
    rejected("unsafe",archive(&[("filmcraft.exe",PE),("../escape.txt",b"escape")]),"",|_|{},"Unsafe ZIP path");
    rejected("no-pe",archive(&[("filmcraft.exe",b"not PE")]),"",|_|{},"no PE version");
    rejected("wrong-name",archive(&[("unrelated.exe",PE)]),"",|_|{},"no expected GUI executable");
    let mut wrong=PE.to_vec();let signature=0xFEEF04BDu32.to_le_bytes();let offset=wrong.windows(4).position(|v|v==signature).unwrap();wrong[offset+8..offset+12].copy_from_slice(&0x00000007u32.to_le_bytes());
    rejected("wrong-version",archive(&[("filmcraft.exe",&wrong)]),"",|_|{},"does not match release");
}
#[test]fn installer_offline_cancellation_at_all_boundaries(){
    // Multiple chunks ensure cancellation interrupts both transport and extraction.
    let mut payload=PE.to_vec();payload.extend((0..200_000u32).flat_map(|v|v.wrapping_mul(2654435761).to_le_bytes()));let bytes=archive(&[("filmcraft.exe",&payload)]);
    for point in ["before","download","extract","publish"]{rejected(point,bytes.clone(),point,|_|{},"cancelled");}
}
#[test]fn owned_archive_cleanup_rejects_wrong_identity_and_preserves_file(){
    let root=root("cleanup-identity");let archive_path=root.join("owned.zip");
    let file=OpenOptions::new().read(true).write(true).create_new(true).share_mode(0).access_mode(0xC0010000).custom_flags(0x00200000).open(&archive_path).unwrap();
    let mut wrong=native::archive_identity(&file).unwrap();wrong[2]^=1;
    assert!(native::delete_owned_archive(&file,&wrong).is_err());drop(file);assert!(archive_path.is_file());preserved(&root);
}
pub(super) fn cleanup_failure()->bool{FIXTURE.with(|f|f.borrow().as_ref().is_some_and(|v|v.cancel_at=="cleanup-failure"))}
#[test]fn publication_survives_cleanup_failure(){
    let root=root("cleanup-warning");let bytes=archive(&[("filmcraft.exe",PE)]);let asset=asset(&root,&bytes);let _guard=configure(bytes,"cleanup-failure");
    let installed=install(&root,4,"v0.6.0",&asset,&AtomicBool::new(false),&WorkProgress::new(0)).unwrap();
    assert!(installed.folder.join("filmcraft.exe").is_file());assert!(installed.cleanup_warning.is_some());assert!(installed.stage.join("download.zip").is_file());assert_eq!(installed.retained_archive_bytes,asset.size);preserved(&root);
}
#[test]fn bulk_guard_prevents_publication_after_root_change(){
    let root=root("bulk-root-guard");let bytes=archive(&[("filmcraft.exe",PE)]);let asset=asset(&root,&bytes);let _fixture=configure(bytes,"");let calls=std::cell::Cell::new(0);
    let guard=||{calls.set(calls.get()+1);if calls.get()>1{Err("Apps root changed".into())}else{Ok(())}};
    let error=install_guarded(&root,4,"v0.6.0",&asset,&AtomicBool::new(false),&WorkProgress::new(0),&guard).unwrap_err();
    assert!(error.contains("Apps root changed"));assert!(!root.join(FOLDER).exists());assert!(stages(&root)[0].join("download.zip").exists());preserved(&root);
}
#[test]fn bulk_run_offline_prefers_new_version_retains_rollback_and_skips_latest(){
    struct OfficialGuard;impl Drop for OfficialGuard{fn drop(&mut self){crate::actions::fixture_official(None);}}
    let _official=OfficialGuard;let root=root("bulk-pipeline");let mut older=PE.to_vec();let signature=0xFEEF04BDu32.to_le_bytes();let offset=older.windows(4).position(|v|v==signature).unwrap();older[offset+8..offset+12].copy_from_slice(&0x00000005u32.to_le_bytes());
    let old=root.join("filmcraft-0.5.0-windows-x64-portable");fs::write(old.join("filmcraft.exe"),&older).unwrap();assert_eq!(native::file_version(&old.join("filmcraft.exe")).as_deref(),Some("0.5.0.0"));
    let bytes=archive(&[("filmcraft.exe",PE)]);let asset=asset(&root,&bytes);let _fixture=configure(bytes,"");
    let release=updates::parse_release_for(4,&serde_json::to_vec(&serde_json::json!({"tag_name":"v0.6.0","draft":false,"prerelease":false,"assets":[{"name":asset.name,"browser_download_url":asset.url,"size":asset.size,"digest":asset.digest}]})).unwrap()).unwrap();crate::actions::fixture_official(Some(release));
    let mut config=core::Config::default();config.root=Some(root.clone());config.selections.insert("filmcraft".into(),core::portable_path(&root,&old));config.save(&root).unwrap();let before=fs::read(root.join("craft-launcher.conf")).unwrap();let progress=std::sync::Mutex::new(crate::bulk::Progress::default());
    let result=crate::bulk::run(&root,&config,&AtomicBool::new(false),&WorkProgress::new(0),&progress).unwrap();assert_eq!(result["bulk"]["rows"][0]["status"],"installed");assert_eq!(result["bulk"]["completed"],1);assert_eq!(fs::read(root.join(FOLDER).join("filmcraft.exe")).unwrap(),PE);preserved(&root);assert_eq!(fs::read(old.join("filmcraft.exe")).unwrap(),older);assert_ne!(fs::read(root.join("craft-launcher.conf")).unwrap(),before);let before=fs::read(root.join("craft-launcher.conf")).unwrap();assert_eq!(crate::actions::load(&root).unwrap().0.selections.get("filmcraft"),Some(&PathBuf::from(FOLDER)));assert!(stages(&root).is_empty());
    let plans=fs::read(root.join("craft-launcher.plans.json")).unwrap();let stored:serde_json::Value=serde_json::from_slice(&plans).unwrap();assert_eq!(stored.as_array().unwrap().len(),1);assert_eq!(stored[0]["used"],true);assert_eq!(stored[0]["plan"]["app"],"filmcraft");let count=stages(&root).len();
    let again=crate::bulk::run(&root,&config,&AtomicBool::new(false),&WorkProgress::new(0),&progress).unwrap();assert_eq!(again["bulk"]["rows"][0]["status"],"skipped");assert_eq!(stages(&root).len(),count);assert_eq!(fs::read(root.join("craft-launcher.plans.json")).unwrap(),plans);assert_eq!(fs::read(root.join("craft-launcher.conf")).unwrap(),before);
}
#[test]fn nested_success_removes_only_empty_owned_scaffolding(){
    for extra in [false,true]{
        let root=root("nested-cleanup");let name=format!("{FOLDER}/filmcraft.exe");
        let mut entries=vec![(name.as_str(),PE)];if extra{entries.push(("keep-user-file.txt",b"preserve"));}
        let bytes=archive(&entries);let asset=asset(&root,&bytes);let _fixture=configure(bytes,"");
        let installed=install(&root,4,"v0.6.0",&asset,&AtomicBool::new(false),&WorkProgress::new(0)).unwrap();
        assert!(installed.folder.join("filmcraft.exe").is_file());preserved(&root);
        if extra{assert!(installed.cleanup_warning.is_some());assert_eq!(fs::read(installed.stage.join("payload/keep-user-file.txt")).unwrap(),b"preserve");}
        else{assert!(installed.cleanup_warning.is_none(),"{:?}",installed.cleanup_warning);assert!(!installed.stage.exists());}
    }
}
#[test]fn owned_directory_cleanup_refuses_wrong_identity_nonempty_and_file(){
    let root=root("directory-identity");let directory=root.join("owned");fs::create_dir(&directory).unwrap();
    let(handle,identity)=native::owned_directory(&directory).unwrap();let mut wrong=identity;wrong[2]^=1;
    assert!(native::delete_owned_empty_directory(&handle,&wrong).is_err());
    fs::write(directory.join("user.txt"),b"keep").unwrap();assert!(native::delete_owned_empty_directory(&handle,&identity).is_err());drop(handle);
    assert_eq!(fs::read(directory.join("user.txt")).unwrap(),b"keep");
    // A regular file is also refused by the directory-only helper.
    assert!(native::owned_directory(&directory.join("user.txt")).is_err());preserved(&root);
}
#[test]fn failed_install_preserves_preference_and_success_merges_multiple_apps(){
    let root=root("preference-merge");let root=core::checked_path(&root).unwrap();let mut config=core::Config::default();config.root=Some(root.clone());
    for app in [0,4]{let id=core::APPS[app].0;let old=root.join(format!("{id}-old"));let new=root.join(format!("{id}-new"));fs::create_dir(&old).unwrap();fs::create_dir(&new).unwrap();fs::write(old.join(format!("{id}.exe")),PE).unwrap();fs::write(new.join(format!("{id}.exe")),PE).unwrap();config.selections.insert(id.into(),core::portable_path(&root,&old));}
    config.save(&root).unwrap();let mut latest=config.clone();latest.theme=core::ThemeMode::Dark;latest.save(&root).unwrap();
    crate::actions::prefer_installed(&root,&config,&root,0,"v0.6.0",&root.join("cadcraft-new")).unwrap();
    crate::actions::prefer_installed(&root,&config,&root,4,"v0.6.0",&root.join("filmcraft-new")).unwrap();
    let(latest,_)=crate::actions::load(&root).unwrap();assert_eq!(latest.theme,core::ThemeMode::Dark);assert_eq!(latest.selections["cadcraft"],PathBuf::from("cadcraft-new"));assert_eq!(latest.selections["filmcraft"],PathBuf::from("filmcraft-new"));
    let before=fs::read(root.join("craft-launcher.conf")).unwrap();assert!(crate::actions::prefer_installed(&root,&config,&root,4,"v0.6.0",&root.join("filmcraft-new")).unwrap_err().contains("changed"));assert_eq!(fs::read(root.join("craft-launcher.conf")).unwrap(),before);
    let bytes=archive(&[("filmcraft.exe",PE)]);let mut bad=asset(&root,&bytes);bad.digest=format!("sha256:{}","0".repeat(64));let _fixture=configure(bytes,"");
    assert!(install(&root,4,"v0.6.0",&bad,&AtomicBool::new(false),&WorkProgress::new(0)).is_err());assert_eq!(fs::read(root.join("craft-launcher.conf")).unwrap(),before);
    let other=root.join("other-root");fs::create_dir(&other).unwrap();let mut changed=latest.clone();changed.root=Some(other);changed.save(&root).unwrap();let before=fs::read(root.join("craft-launcher.conf")).unwrap();
    assert!(crate::actions::prefer_installed(&root,&latest,&root,4,"v0.6.0",&root.join("filmcraft-new")).unwrap_err().contains("root changed"));assert_eq!(fs::read(root.join("craft-launcher.conf")).unwrap(),before);
}

pub(super) fn mixed_update_fixture(root:&Path)->(core::Config,serde_json::Value){
    let root=core::checked_path(root).unwrap();let mut older=PE.to_vec();let offset=older.windows(4).position(|v|v==0xFEEF04BDu32.to_le_bytes()).unwrap();older[offset+8..offset+12].copy_from_slice(&0x00000005u32.to_le_bytes());
    let mut config=core::Config::default();config.root=Some(root.clone());
    for id in ["cadcraft","filmcraft"]{let folder=root.join(format!("{id}-0.5.0-windows-x64-portable"));fs::create_dir(&folder).unwrap();fs::write(folder.join(format!("{id}.exe")),&older).unwrap();fs::write(folder.join("rollback.txt"),b"keep rollback").unwrap();}
    config.selections.insert("cadcraft".into(),PathBuf::from("cadcraft-0.5.0-windows-x64-portable"));config.save(&root).unwrap();
    // A setting changed after the operation's configuration snapshot must survive.
    let mut latest=config.clone();latest.theme=core::ThemeMode::Dark;latest.save(&root).unwrap();
    let bytes=archive(&[("filmcraft.exe",PE)]);let asset=asset(&root,&bytes);let _fixture=configure(bytes,"");
    let release=updates::parse_release_for(4,&serde_json::to_vec(&serde_json::json!({"tag_name":"v0.6.0","draft":false,"prerelease":false,"assets":[{"name":asset.name,"browser_download_url":asset.url,"size":asset.size,"digest":asset.digest}]})).unwrap()).unwrap();
    crate::actions::fixture_official(Some(release));
    let result=crate::bulk::run(&root,&config,&AtomicBool::new(false),&WorkProgress::new(0),&std::sync::Mutex::new(crate::bulk::Progress::default())).unwrap();crate::actions::fixture_official(None);
    assert_eq!(result["bulk"]["rows"][0]["status"],"failed");assert_eq!(result["bulk"]["rows"][1]["status"],"installed");(config,result)
}

pub(super) fn installing_phase(progress:&WorkProgress){assert_eq!(progress.snapshot().phase,Phase::Installing);assert_eq!(progress.snapshot().percent,None);assert!(progress.snapshot().bar().1);}
