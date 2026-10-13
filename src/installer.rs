//! Explicit selected portable install. All writes stay in a fresh root-local stage.
use crate::progress::{WorkProgress,Phase};
use std::{collections::{BTreeMap,BTreeSet},fs::{self,File,OpenOptions},io::{Read,Write,Seek,SeekFrom},os::windows::fs::OpenOptionsExt,path::{Path,PathBuf},sync::atomic::{AtomicBool,Ordering},time::{SystemTime,UNIX_EPOCH}};
use crate::{core,native,updates::{self,Asset}};
pub fn download_origin(url:&str,first:bool)->Result<(&str,&str),String>{
    if !url.is_ascii()||url.bytes().any(|c|c<=32||c==127)||url.contains('#')||url.contains('\\'){return Err("Invalid asset HTTPS URL".into());}
    let rest=url.strip_prefix("https://").ok_or("Only HTTPS asset URLs are supported")?;let split=rest.find('/').ok_or("Asset URL has no path")?;let(host,path)=rest.split_at(split);
    if first {if host!="github.com"||!path.starts_with("/storytold/")||!path.contains("/releases/download/")||path.contains('?'){return Err("Initial asset origin is not official GitHub".into());}}
    else if host!="release-assets.githubusercontent.com"{return Err("Asset redirect is outside the allowed GitHub release-asset host".into());}
    Ok((host,path))
}
pub fn safe_entry(name:&str)->Result<PathBuf,String>{
    if name.is_empty()||name.len()>240||!name.is_ascii()||name.contains('\\')||name.starts_with('/') {return Err("Unsupported or unsafe ZIP path; use the official release page".into());}
    let name=name.strip_suffix('/').unwrap_or(name);let mut result=PathBuf::new();
    for part in name.split('/') {let base=part.split('.').next().unwrap_or("").to_ascii_uppercase();
        if part.is_empty()||matches!(part,"."|"..")||part.ends_with(['.',' '])||part.bytes().any(|c|c<32||c==127||b":<>\"|?*".contains(&c))||matches!(base.as_str(),"CON"|"PRN"|"AUX"|"NUL"|"CLOCK$")||(base.len()==4&&(base.starts_with("COM")||base.starts_with("LPT"))&&matches!(base.as_bytes()[3],b'1'..=b'9')) {return Err(format!("Unsafe ZIP path: {name}"));}result.push(part);
    }Ok(result)
}
fn stop(cancel:&AtomicBool)->Result<(),String>{if cancel.load(Ordering::Relaxed){Err("Installation cancelled; existing versions are unchanged".into())}else{Ok(())}}
fn ensure_directory(stage:&Path,path:&Path)->Result<(),String>{
    let relative=path.strip_prefix(stage).map_err(|_|"Extraction path escapes staging")?;let mut current=stage.to_path_buf();
    for component in relative.components(){current.push(component);match fs::create_dir(&current){Ok(())=>(),Err(e)if e.kind()==std::io::ErrorKind::AlreadyExists=>(),Err(e)=>return Err(e.to_string())};let checked=core::checked_path(&current)?;if !checked.starts_with(stage)||!checked.is_dir(){return Err("Extraction directory changed or escaped staging".into());}}
    Ok(())
}
fn extract(file:File,stage:&Path,cancel:&AtomicBool)->Result<(),String>{
    let mut archive=zip::ZipArchive::new(file).map_err(|e|format!("Unsupported ZIP: {e}"))?;
    if archive.len()==0||archive.len()>50_000{return Err("ZIP entry count is outside supported limits".into());}
    let mut entries=Vec::new();let mut explicit=BTreeSet::new();let mut paths=BTreeMap::new();let mut total=0u64;
    // Validate the complete directory before creating extracted files.
    for index in 0..archive.len(){stop(cancel)?;let file=archive.by_index(index).map_err(|e|e.to_string())?;
        if file.encrypted()||file.is_symlink()||!matches!(file.compression(),zip::CompressionMethod::Stored|zip::CompressionMethod::Deflated){return Err("Encrypted, linked, or unsupported-compression ZIP entry".into());}
        let mode=file.unix_mode().unwrap_or(0)&0o170000;if !matches!(mode,0|0o100000|0o040000){return Err("Special ZIP filesystem entries are not supported".into());}
        let name=std::str::from_utf8(file.name_raw()).map_err(|_|"Unsupported ZIP filename encoding")?;let path=safe_entry(name)?;if file.enclosed_name().is_none(){return Err("ZIP path escapes archive".into());}
        let key=path.to_string_lossy().to_ascii_lowercase();if !explicit.insert(key.clone()){return Err("Duplicate/case-colliding ZIP entries".into());}
        let directory=file.is_dir();if paths.get(&key).is_some_and(|existing|*existing!=directory){return Err("ZIP file/directory collision".into());}paths.insert(key,directory);
        for parent in path.ancestors().skip(1).filter(|p|!p.as_os_str().is_empty()){let key=parent.to_string_lossy().to_ascii_lowercase();if paths.get(&key)==Some(&false){return Err("ZIP file is used as a directory".into());}paths.insert(key,true);}
        if file.size()>512*1024*1024||(!directory&&file.size()>file.compressed_size().max(1).saturating_mul(1000)){return Err("ZIP entry exceeds size/expansion limits".into());}
        total=total.checked_add(file.size()).ok_or("ZIP size overflow")?;if total>2*1024*1024*1024{return Err("ZIP exceeds 2 GiB extracted size limit".into());}
        entries.push((path,directory,file.size()));
    }
    for(index,(path,directory,expected))in entries.into_iter().enumerate(){stop(cancel)?;let destination=stage.join(path);if directory{ensure_directory(stage,&destination)?;continue;}
        ensure_directory(stage,destination.parent().ok_or("Missing extraction parent")?)?;let parent=core::checked_path(destination.parent().unwrap())?;if !parent.starts_with(stage){return Err("Staging containment changed".into());}
        let mut output=OpenOptions::new().write(true).create_new(true).share_mode(0).open(&destination).map_err(|e|e.to_string())?;
        let mut input=archive.by_index(index).map_err(|e|e.to_string())?;let mut buffer=[0u8;65536];let mut written=0u64;
        loop{stop(cancel)?;let count=input.read(&mut buffer).map_err(|e|format!("ZIP read/CRC validation failed: {e}"))?;if count==0{break;}written+=count as u64;if written>expected{return Err("ZIP expanded beyond its declared size".into());}output.write_all(&buffer[..count]).map_err(|e|e.to_string())?;
            #[cfg(test)] integration::checkpoint("extract",cancel);
        }
        if written!=expected{return Err("ZIP extracted size mismatch".into());}output.sync_all().map_err(|e|e.to_string())?;
    }Ok(())
}
#[derive(Debug)] pub struct Installed {pub folder:PathBuf,pub stage:PathBuf,pub retained_archive_bytes:u64,pub cleanup_warning:Option<String>}
pub fn install(root:&Path,app:usize,tag:&str,asset:&Asset,cancel:&AtomicBool,progress:&WorkProgress)->Result<Installed,String>{
    install_guarded(root,app,tag,asset,cancel,progress,&||Ok(()))
}
pub fn install_guarded(root:&Path,app:usize,tag:&str,asset:&Asset,cancel:&AtomicBool,progress:&WorkProgress,guard:&dyn Fn()->Result<(),String>)->Result<Installed,String>{
    progress.begin(app,Phase::Downloading,Some(asset.size),"Downloading official portable archive");
    let result=install_inner(root,app,tag,asset,cancel,progress,guard);
    match &result{Ok(_)=>progress.phase(Phase::Installed,"Installed; existing versions retained"),Err(error)=>progress.phase(Phase::Failed,error)}result
}
fn install_inner(root:&Path,app:usize,tag:&str,asset:&Asset,cancel:&AtomicBool,progress:&WorkProgress,guard:&dyn Fn()->Result<(),String>)->Result<Installed,String>{
    guard()?;updates::validate_asset(app,tag,asset)?;stop(cancel)?;let root=core::checked_path(root)?;if !root.is_dir(){return Err("Choose an existing apps folder".into());}
    let folder=asset.name.strip_suffix(".zip").ok_or("Not a portable ZIP")?;safe_entry(folder)?;let destination=root.join(folder);if fs::symlink_metadata(&destination).is_ok(){return Err("That version folder already exists; nothing was replaced".into());}
    let unique=SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos();let stage=root.join(format!(".craft-stage-{}-{unique}",std::process::id()));fs::create_dir(&stage).map_err(|e|e.to_string())?;let stage=core::checked_path(&stage)?;let (stage_handle,stage_identity)=native::owned_directory(&stage)?;
    let result:Result<(PathBuf,Option<String>),String>=(||{
        let mut archive=OpenOptions::new().read(true).write(true).create_new(true).share_mode(0).access_mode(0xC0010000).custom_flags(0x00200000).open(stage.join("download.zip")).map_err(|e|e.to_string())?;
        let archive_identity=native::archive_identity(&archive)?;
        #[cfg(not(test))]
        native::download_asset(&asset.url,asset.size,&mut archive,cancel,progress)?;
        #[cfg(test)]
        integration::download(asset.size,&mut archive,cancel,progress)?;
        stop(cancel)?;
        progress.phase(Phase::Installing,"Verifying SHA-256 and extracting the portable archive");
        #[cfg(test)] integration::installing_phase(progress);
        let actual=native::sha256_file(&mut archive,cancel)?;if !asset.digest[7..].eq_ignore_ascii_case(&actual){return Err("SHA-256 mismatch; archive was not extracted".into());}archive.seek(SeekFrom::Start(0)).map_err(|e|e.to_string())?;
        let payload=stage.join("payload");fs::create_dir(&payload).map_err(|e|e.to_string())?;let payload=core::checked_path(&payload)?;let (payload_handle,payload_identity)=native::owned_directory(&payload)?;drop(payload_handle);extract(archive.try_clone().map_err(|e|e.to_string())?,&payload,cancel)?;stop(cancel)?;
        let id=core::APPS[app].0;let flat=payload.join(format!("{id}.exe"));let nested=payload.join(folder);
        let nested_publish=!flat.is_file();
        let publish=if !nested_publish{payload.clone()}else if nested.join(format!("{id}.exe")).is_file(){nested}else{return Err("ZIP has no expected GUI executable at its root or exact portable folder".into());};
        let executable=core::validate_executable(&stage,&publish.join(format!("{id}.exe")),app,false)?;
        let version=native::file_version(&executable).ok_or("Downloaded GUI executable has no PE version evidence")?;
        let expected=tag.trim_start_matches(['v','V']).split('+').next().unwrap_or("");if version!=format!("{expected}.0"){return Err(format!("Downloaded PE version {version} does not match release {tag}"));}
        #[cfg(test)] integration::checkpoint("publish",cancel);
        if core::checked_path(&root)?!=root||!core::checked_path(&publish)?.starts_with(&stage){return Err("Root or staging changed before publishing".into());}stop(cancel)?;
        guard()?;native::publish_directory(&publish,&destination)?;
        // Publication is committed. Cleanup cannot turn success into retryable failure.
        let verified=core::validate_executable(&root,&destination.join(format!("{id}.exe")),app,false).and_then(|p|if native::file_version(&p).as_deref()==Some(&format!("{expected}.0")){Ok(())}else{Err("Published version could not be reverified; archive retained".into())});
        #[cfg(test)] let cleanup=if integration::cleanup_failure(){Some("Injected archive cleanup failure".into())}else{verified.and_then(|_|native::delete_owned_archive(&archive,&archive_identity)).err()};
        #[cfg(not(test))] let cleanup=verified.and_then(|_|native::delete_owned_archive(&archive,&archive_identity)).err();
        drop(archive);
        let warning=cleanup.or_else(||{
            // Delete only the exact empty wrapper created by this run. A nested ZIP
            // may contain other root files; Windows refuses deletion if any remain.
            let cleaned=(||{
                if nested_publish{let (handle,identity)=native::owned_directory(&payload)?;if identity!=payload_identity{return Err("Payload identity changed; staging retained".into());}native::delete_owned_empty_directory(&handle,&payload_identity)?;drop(handle);}
                native::delete_owned_empty_directory(&stage_handle,&stage_identity)
            })();cleaned.err()
        });
        if let Some(warning)=&warning{
            // Never overwrite a file another process added to the stage.
            if let Ok(mut report)=OpenOptions::new().write(true).create_new(true).open(stage.join("recovery.txt")){let _=writeln!(report,"Installed: {}\nCleanup warning: {warning}\nExisting versions retained.",destination.display());}
        }
        Ok((destination,warning))
    })();
    drop(stage_handle);
    result.map(|(folder,cleanup_warning)|Installed{folder,stage:stage.clone(),retained_archive_bytes:if stage.join("download.zip").exists(){asset.size}else{0},cleanup_warning}).map_err(|error|format!("{error}\r\nStaging was retained for inspection: {}\r\nExisting versions and settings were preserved.",stage.display()))
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn extraction_paths_reject_windows_escapes(){for name in ["../bad","/root","C:/bad","a:ads","a\\b","CON.txt","dir/NUL","COM1","a./b","a /b","a//b","./a","x/../../z"]{assert!(safe_entry(name).is_err(),"{name}");}assert_eq!(safe_entry("data/fonts/a.ttf").unwrap(),PathBuf::from(r"data\fonts\a.ttf"));}
    #[test]fn redirects_are_exact_https_asset_host(){assert!(download_origin("https://release-assets.githubusercontent.com/path?token=a",false).is_ok());for url in ["http://release-assets.githubusercontent.com/x","https://release-assets.githubusercontent.com.evil/x","https://user@release-assets.githubusercontent.com/x","https://github.com/elsewhere/x","https://release-assets.githubusercontent.com:443/x"]{assert!(download_origin(url,false).is_err());}}
    #[test]fn cancelled_install_writes_nothing(){
        let base=std::env::temp_dir().join(format!("craft-cancel-validation-{}",std::process::id()));fs::create_dir(&base).unwrap();let cancelled=AtomicBool::new(true);let progress=WorkProgress::new(0);let name="filmcraft-0.6.0-windows-x64-portable.zip";let asset=Asset{name:name.into(),url:format!("https://github.com/storytold/filmcraft/releases/download/v0.6.0/{name}"),size:123,digest:format!("sha256:{}","a".repeat(64))};assert!(install(&base,4,"v0.6.0",&asset,&cancelled,&progress).unwrap_err().contains("cancelled"));assert_eq!(fs::read_dir(&base).unwrap().count(),0);fs::remove_dir(base).unwrap();
    }
    fn zip_fixture(base:&Path,names:&[&str])->File {
        let path=base.join("fixture.zip");let output=File::create(&path).unwrap();let mut writer=zip::ZipWriter::new(output);
        for name in names {writer.start_file(*name,zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored)).unwrap();writer.write_all(b"fixture").unwrap();}writer.finish().unwrap();File::open(path).unwrap()
    }
    #[test]fn archive_validation_precedes_any_extracted_file(){
        let base=std::env::temp_dir().join(format!("craft-zip-validation-{}",std::process::id()));fs::create_dir(&base).unwrap();let stage=base.join("payload");fs::create_dir(&stage).unwrap();let stage=core::checked_path(&stage).unwrap();let cancel=AtomicBool::new(false);
        for names in [&["valid.txt","../escape.txt"][..],&["same.txt","SAME.txt"][..],&["file","file/child.txt"][..],&["a/b.txt","a"][..]] {let file=zip_fixture(&base,names);assert!(extract(file,&stage,&cancel).is_err());assert_eq!(fs::read_dir(&stage).unwrap().count(),0);}
        let file=zip_fixture(&base,&["data/ok.txt"]);extract(file,&stage,&cancel).unwrap();assert_eq!(fs::read(stage.join("data/ok.txt")).unwrap(),b"fixture");fs::remove_dir_all(base).unwrap();
    }
    #[test]fn publication_never_replaces_existing_directory(){
        let base=std::env::temp_dir().join(format!("craft-publish-validation-{}",std::process::id()));fs::create_dir(&base).unwrap();let from=base.join("staged");let to=base.join("existing");fs::create_dir(&from).unwrap();fs::create_dir(&to).unwrap();fs::write(to.join("user-data.txt"),b"keep").unwrap();assert!(native::publish_directory(&from,&to).is_err());assert_eq!(fs::read(to.join("user-data.txt")).unwrap(),b"keep");assert!(from.exists());fs::remove_dir_all(base).unwrap();
    }
}
#[cfg(test)]
#[path = "installer_integration.rs"]
mod integration;

#[cfg(test)]pub(crate) fn mixed_update_fixture(base:&Path)->(core::Config,serde_json::Value){integration::mixed_update_fixture(base)}
