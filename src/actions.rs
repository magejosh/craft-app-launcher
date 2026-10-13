//! Typed local actions. Neither callers nor workers can replace a Config snapshot.
use crate::progress::{WorkProgress,Phase};
use crate::{core::{self,Config,Install,Profile,ThemeMode,APPS},native,updates,installer};
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use std::{collections::VecDeque,fs,io::{Read,Cursor},path::{Path,PathBuf},os::windows::ffi::OsStrExt,sync::atomic::{AtomicBool},time::{Instant,Duration}};

pub const REQUEST_LIMIT:usize=64*1024;
pub const RESPONSE_LIMIT:usize=256*1024;
const CONFIG_LIMIT:u64=1024*1024;
#[derive(Clone,Debug,Deserialize,Serialize,schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {pub protocol:u32,pub id:String,pub action:Action}
#[derive(Clone,Debug,Deserialize,Serialize,schemars::JsonSchema)]
#[serde(tag="action",rename_all="snake_case",deny_unknown_fields)]
pub enum Action {
    State,
    UpdateAll,
    OpenAppsFolder,
    Installations {#[serde(default)] offset:usize,#[serde(default="page_size")] limit:usize},
    Profiles,
    Patch {fields:Vec<FieldPatch>},
    Reload,
    Launch {app:String,install_id:String,profile_id:String,#[serde(default)] cli:bool},
    ReleasesCheck {app:String},
    InstallPlan {app:String,tag:String,asset:AssetInput},
    InstallConfirm {plan:InstallPlan},
    OperationStatus {request_id:String},
    Cancel {request_id:String},
}
fn page_size()->usize{32}
#[derive(Clone,Debug,Deserialize,Serialize,schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FieldPatch {
    pub field:Field,
    #[serde(default,deserialize_with="present_expected",skip_serializing_if="Option::is_none")]
    pub expected:Option<Value>,
}
fn present_expected<'de,D:serde::Deserializer<'de>>(deserializer:D)->Result<Option<Value>,D::Error>{
    Value::deserialize(deserializer).map(Some)
}
#[derive(Clone,Debug,Deserialize,Serialize,schemars::JsonSchema)]
#[serde(tag="field",rename_all="snake_case",deny_unknown_fields)]
pub enum Field {
    Theme {value:String}, AppsRoot {value:String},
    CreateProfile {profile_id:String}, SelectProfile {profile_id:String},
    Arguments {profile_id:String,app:String,value:String},
    Compatibility {profile_id:String,enabled:bool,obsidian:Option<String>},
    SelectedVersion {app:String,install_id:String},
}
#[derive(Clone,Debug,Deserialize,Serialize,schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssetInput {pub name:String,pub url:String,pub size:u64,pub digest:String}
impl AssetInput {fn asset(&self)->updates::Asset{updates::Asset{name:self.name.clone(),url:self.url.clone(),size:self.size,digest:self.digest.clone()}}}
#[derive(Clone,Debug,Deserialize,Serialize,schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InstallPlan {pub app:String,pub tag:String,pub asset:AssetInput,pub root:String,pub binding:String}
#[derive(Clone,Debug,Deserialize,Serialize,schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Response {pub protocol:u32,pub id:String,pub ok:bool,pub code:String,pub result:Value}
impl Response {
    pub fn success(id:&str,result:Value)->Self{Self{protocol:1,id:id.into(),ok:true,code:"ok".into(),result}}
    pub fn error(id:&str,code:&str,message:impl ToString)->Self{Self{protocol:1,id:id.into(),ok:false,code:code.into(),result:json!({"message":message.to_string()})}}
}
pub fn hash(bytes:&[u8])->Result<String,String>{native::sha256_file(&mut Cursor::new(bytes),&AtomicBool::new(false))}
pub fn config_bytes(base:&Path)->Result<Option<Vec<u8>>,String>{
    let path=base.join("craft-launcher.conf");
    match fs::File::open(path) {
        Ok(f)=>{let mut bytes=Vec::new();f.take(CONFIG_LIMIT+1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;if bytes.len()as u64>CONFIG_LIMIT{return Err("Settings exceed 1 MiB".into());}Ok(Some(bytes))},
        Err(e)if e.kind()==std::io::ErrorKind::NotFound=>Ok(None),Err(e)=>Err(e.to_string())
    }
}
pub fn disk_hash(base:&Path)->Result<String,String>{match config_bytes(base)?{Some(v)=>hash(&v),None=>Ok("missing".into())}}
pub fn load(base:&Path)->Result<(Config,String),String>{
    match config_bytes(base)?{Some(v)=>{let cfg=Config::parse(std::str::from_utf8(&v).map_err(|_|"Settings are not UTF-8")?)?;Ok((cfg,hash(&v)?))},None=>Ok((Config::default(),"missing".into()))}
}
static CONFIG_WRITE_LOCK:std::sync::Mutex<()>=std::sync::Mutex::new(());
pub fn persist(base:&Path,config:&Config,observed:&mut String)->Result<(),String>{
    let _lock=CONFIG_WRITE_LOCK.lock().unwrap_or_else(|p|p.into_inner());
    persist_locked(base,config,observed)
}
fn persist_locked(base:&Path,config:&Config,observed:&mut String)->Result<(),String>{
    if disk_hash(base)?!=*observed{return Err("ExternalConflict: settings changed on disk; reload/rebase before retrying".into());}
    // Cooperative physical owner lock is required by every production caller.
    // This last check detects unrelated editors but cannot make replacement a filesystem CAS.
    config.save(base)?;
    *observed=hash(config.serialize().as_bytes())?;Ok(())
}
pub fn decode(bytes:&[u8])->Result<Request,String>{
    let(mut depth,mut quoted,mut escaped,mut nodes)=(0usize,false,false,0usize);
    for b in bytes{
        if quoted{if escaped{escaped=false;}else if *b==b'\\'{escaped=true;}else if *b==b'"'{quoted=false;}}
        else{match b{b'"'=>{quoted=true;nodes+=1;},b'['|b'{'=>{depth+=1;nodes+=1;if depth>32{return Err("JSON depth exceeds 32".into());}},b']'|b'}'=>{depth=depth.saturating_sub(1);},_=>()}}
        if nodes>4096{return Err("JSON node/string count exceeds 4096".into());}
    }
    if bytes.len()>REQUEST_LIMIT{return Err("Request exceeds 64 KiB".into());}
    // Serde's internally tagged unit variants do not apply deny_unknown_fields
    // to their payload. Keep parameterless actions strict before typed decoding.
    let envelope:Value=serde_json::from_slice(bytes).map_err(|e|e.to_string())?;
    if let Some(action)=envelope.get("action").and_then(Value::as_object){
        if matches!(action.get("action").and_then(Value::as_str),Some("state"|"profiles"|"reload"|"update_all"|"open_apps_folder"))&&action.len()!=1{
            return Err("Parameterless action has unknown fields".into());
        }
    }
    let r:Request=serde_json::from_slice(bytes).map_err(|e|e.to_string())?;
    if r.protocol!=1||r.id.is_empty()||r.id.len()>80||!r.id.bytes().all(|b|b.is_ascii_alphanumeric()||b"-_.".contains(&b)){return Err("Invalid protocol or request ID".into());}
    Ok(r)
}
pub fn encode(response:&Response)->Result<Vec<u8>,String>{
    let bytes=serde_json::to_vec(response).map_err(|e|e.to_string())?;
    if bytes.len()>RESPONSE_LIMIT{return Err("Response exceeds 256 KiB; use a smaller page".into());}Ok(bytes)
}
pub fn app_id(app:&str)->Result<usize,String>{APPS.iter().position(|(id,_)|*id==app).ok_or_else(||"Unknown app ID".into())}
fn profile(config:&Config,id:&str)->Result<(),String>{if id=="Default"||config.devices.contains_key(id){Ok(())}else{Err("Unknown explicit profile ID".into())}}
pub fn install_id(i:&Install)->Result<String,String>{
    let mut bytes=Vec::new();for u in i.folder.as_os_str().encode_wide(){bytes.extend(u.to_le_bytes());}bytes.extend((i.app as u32).to_le_bytes());hash(&bytes)
}
fn root(base:&Path,config:&Config)->Result<PathBuf,String>{core::checked_path(&core::resolve_path(base,config.root.as_ref().ok_or("Choose an apps root first")?)?)}
fn exact_install<'a>(installs:&'a[Install],app:usize,id:&str)->Result<&'a Install,String>{
    installs.iter().find(|i|i.app==app&&install_id(i).ok().as_deref()==Some(id)).ok_or_else(||"Unknown installation ID; refresh discovery".into())
}
pub fn field_value(base:&Path,c:&Config,machine:Option<&str>,installs:&[Install],field:&Field)->Result<Value,String>{
    Ok(match field{
        Field::Theme{..}=>json!(c.theme.name()),
        Field::AppsRoot{..}=>json!(c.root.as_ref().map(|p|core::resolve_path(base,p).map(|p|p.to_string_lossy().into_owned())).transpose()?),
        Field::CreateProfile{profile_id}=>json!(c.device_names().contains(profile_id)),
        Field::SelectProfile{..}=>json!(c.selected_device(machine)),
        Field::Arguments{profile_id,app,..}=>{profile(c,profile_id)?;app_id(app)?;json!(c.device_arguments(profile_id,app))},
        Field::Compatibility{profile_id,..}=>{profile(c,profile_id)?;let p=c.device_compatibility(profile_id,machine,"filmcraft");json!({"enabled":p.enabled,"obsidian":p.obsidian.map(|p|core::resolve_path(base,&p).map(|p|p.to_string_lossy().into_owned())).transpose()?})},
        Field::SelectedVersion{app,..}=>{let app=app_id(app)?;let r=root(base,c)?;json!(core::selected_install(installs,app,&r,c.selections.get(APPS[app].0)).map(|i|install_id(&installs[i])).transpose()?)},
    })
}
pub fn apply_fields(base:&Path,current:&Config,machine:Option<&str>,installs:&[Install],fields:&[FieldPatch])->Result<Config,String>{
    if fields.is_empty()||fields.len()>32{return Err("Provide 1 to 32 field patches".into());}
    let mut next=current.clone();let mut keys=std::collections::BTreeSet::new();
    for patch in fields {
        let key=match &patch.field{Field::Theme{..}=>"theme".into(),Field::AppsRoot{..}=>"root".into(),Field::CreateProfile{profile_id}=>format!("create:{profile_id}"),Field::SelectProfile{..}=>"selected-profile".into(),Field::Arguments{profile_id,app,..}=>format!("args:{profile_id}:{app}"),Field::Compatibility{profile_id,..}=>format!("compat:{profile_id}"),Field::SelectedVersion{app,..}=>format!("version:{app}")};
        if !keys.insert(key){return Err("Duplicate field patch".into());}
        if let Some(expected)=&patch.expected{if &field_value(base,current,machine,installs,&patch.field)?!=expected{return Err("FieldConflict: optional field precondition changed".into());}}
        match &patch.field {
            Field::Theme{value}=>next.theme=ThemeMode::parse(value)?,
            Field::AppsRoot{value}=>{let r=core::checked_path(Path::new(value))?;if !r.is_dir(){return Err("Apps root must be an existing directory".into());}next.root=Some(core::portable_path(base,&r));},
            Field::CreateProfile{profile_id}=>{next.create_device(profile_id)?;},
            Field::SelectProfile{profile_id}=>{profile(&next,profile_id)?;next.machine_devices.insert(machine.ok_or("Machine identity unavailable")?.into(),profile_id.clone());},
            Field::Arguments{profile_id,app,value}=>{profile(&next,profile_id)?;app_id(app)?;if value.len()>16*1024{return Err("Arguments exceed 16 KiB".into());}core::parse_args(value)?;next.set_device_arguments(profile_id,app,value.clone());},
            Field::Compatibility{profile_id,enabled,obsidian}=>{
                profile(&next,profile_id)?;let machine=machine.ok_or("Machine identity unavailable")?;
                let path=obsidian.as_ref().map(|p|core::checked_path(Path::new(p))).transpose()?;
                if path.as_ref().is_some_and(|p|!p.is_dir())||(*enabled&&path.is_none()){return Err("Compatibility requires the exact existing Obsidian directory".into());}
                next.set_device_compatibility(profile_id,machine,"filmcraft",Profile{enabled:*enabled,obsidian:path.map(|p|core::portable_path(base,&p))});
            },
            Field::SelectedVersion{app,install_id:id}=>{let a=app_id(app)?;let i=exact_install(installs,a,id)?;let r=root(base,&next)?;let exe=i.gui.as_ref().or(i.cli.as_ref()).ok_or("Installation has no executable")?;core::validate_executable(&r,exe,a,i.gui.is_none())?;next.selections.insert(app.clone(),core::portable_path(&r,&i.folder));},
        }
    }Ok(next)
}
#[derive(Clone,Debug,Serialize)]
#[serde(tag="status",rename_all="snake_case")]
pub enum Discovery {NotRequested,Ready,Scanning,Unavailable{message:String}}
impl Discovery {pub fn ready(&self)->bool{matches!(self,Self::Ready)}}
#[derive(Clone,Debug,Serialize)]
pub struct View {pub owner:&'static str,pub discovery:Discovery,pub work:Value}
pub fn needs_installations(action:&Action)->bool{
    matches!(action,Action::Installations{..}|Action::Launch{..})||
    matches!(action,Action::Patch{fields}if fields.iter().any(|p|matches!(p.field,Field::SelectedVersion{..})))
}
pub fn discover_for_action(base:&Path,c:&Config,action:&Action)->(Vec<Install>,Discovery){
    if !needs_installations(action)&&!matches!(action,Action::ReleasesCheck{..}){return(Vec::new(),Discovery::NotRequested);}
    match root(base,c).and_then(|r|core::discover_timed(&r,&mut core::ScanStats::default())){
        Ok((installs,notes))=>{let failures=notes.iter().filter(|n|!core::scan_note_is_informational(n)).count();let status=if failures==0{Discovery::Ready}else{Discovery::Unavailable{message:format!("Discovery has {failures} warnings; installation completeness is unknown")}};(installs,status)},
        Err(message)=>(Vec::new(),Discovery::Unavailable{message}),
    }
}
/// Both front ends use the GUI's conservative PE/selected-install status policy.
/// A failed/incomplete/not-requested scan can never prove an app is absent.
pub fn checked_release_value(base:&Path,c:&Config,installs:&[Install],discovery:&Discovery,app:&str,result:&Result<updates::Release,String>)->Result<Value,String>{
    let a=app_id(app)?;let release=result.as_ref().map_err(Clone::clone)?;
    let count=installs.iter().filter(|i|i.app==a).count();
    let checked_root=if discovery.ready(){root(base,c).ok()}else{None};
    let selected=checked_root.as_ref().and_then(|r|core::selected_install(installs,a,r,c.selections.get(app)));
    let mut status=updates::update_status(a,count,selected.map(|i|installs[i].version.as_str()),Some(result));
    if checked_root.is_none(){status.state=updates::UpdateState::Unknown;status.install_available=false;}
    let mut value=release_value(app,release);
    value["comparison"]=json!({"state":status.label(),"install_available":status.install_available,"installed_count":checked_root.as_ref().map(|_|count),"selected_install_id":selected.map(|i|install_id(&installs[i])).transpose()?,"discovery":discovery});
    Ok(value)
}
pub fn state(base:&Path,c:&Config,machine:Option<&str>,revision:&str,view:&View)->Value{
    json!({"known_apps":APPS.iter().map(|(id,name)|json!({"app_id":id,"name":name})).collect::<Vec<_>>(),"owner":view.owner,"discovery":view.discovery,"work":view.work,"revision":revision,"theme":c.theme.name(),"apps_root":c.root.as_ref().map(|p|core::resolve_path(base,p).map(|p|p.to_string_lossy().into_owned()).ok()),"selected_profile_id":c.selected_device(machine),"profiles":c.device_names().into_iter().map(|id|{let args:serde_json::Map<String,Value>=APPS.iter().map(|(a,_)|(a.to_string(),json!(c.device_arguments(&id,a)))).collect();let p=c.device_compatibility(&id,machine,"filmcraft");json!({"profile_id":id,"arguments":args,"compatibility":{"enabled":p.enabled,"obsidian":p.obsidian.map(|p|core::resolve_path(base,&p).map(|p|p.to_string_lossy().into_owned()).ok())}})}).collect::<Vec<_>>()})
}
pub fn installations(installs:&[Install],offset:usize,limit:usize)->Result<Value,String>{
    if limit==0||limit>64||offset>installs.len(){return Err("Invalid page; limit is 1..64".into());}
    let rows=installs.iter().skip(offset).take(limit).map(|i|Ok(json!({"install_id":install_id(i)?,"app":APPS[i.app].0,"version":i.version,"folder_display":i.folder.to_string_lossy(),"gui":i.gui.is_some(),"cli":i.cli.is_some()}))).collect::<Result<Vec<_>,String>>()?;
    Ok(json!({"installations":rows,"next_offset":if offset+limit<installs.len(){Some(offset+limit)}else{None},"total":installs.len()}))
}
pub fn launch(base:&Path,c:&Config,machine:Option<&str>,installs:&[Install],app:&str,id:&str,profile_id:&str,cli:bool)->Result<Value,String>{
    profile(c,profile_id)?;let app=app_id(app)?;let i=exact_install(installs,app,id)?;let r=root(base,c)?;
    let args=core::parse_args(c.device_arguments(profile_id,APPS[app].0))?;let mut p=c.device_compatibility(profile_id,machine,APPS[app].0);
    p.obsidian=p.obsidian.map(|p|core::resolve_path(base,&p)).transpose()?;
    Ok(json!({"pid":core::launch_agent(base,&r,i,cli,&args,&p)?}))
}
pub fn release_value(app:&str,r:&updates::Release)->Value{json!({"app":app,"tag":r.tag,"asset_note":r.asset_note,"assets":r.assets.iter().map(|a|json!({"name":a.name,"url":a.url,"size":a.size,"digest":a.digest})).collect::<Vec<_>>()})}
pub fn release(app:&str)->Result<Value,String>{let r=official_release(app_id(app)?)?;Ok(release_value(app,&r))}
#[derive(Deserialize,Serialize)] #[serde(deny_unknown_fields)]
struct StoredPlan {plan:InstallPlan,created:u64,used:bool}
fn now()->Result<u64,String>{Ok(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?.as_secs())}
fn trusted_asset(app:&str,tag:&str,asset:&AssetInput)->Result<(),String>{
    let a=app_id(app)?;updates::validate_asset(a,tag,&asset.asset())?;
    let official=official_release(a)?;
    if official.tag!=tag||!official.assets.iter().any(|v|v.name==asset.name&&v.url==asset.url&&v.size==asset.size&&v.digest==asset.digest){return Err("Selected asset does not exactly match current trusted official release metadata".into());}
    Ok(())
}
fn read_plans(base:&Path)->Result<Vec<StoredPlan>,String>{
    let path=base.join("craft-launcher.plans.json");
    let f=match fs::File::open(&path){Ok(f)=>f,Err(e)if e.kind()==std::io::ErrorKind::NotFound=>return Ok(Vec::new()),Err(e)=>return Err(e.to_string())};
    core::checked_path(&path)?;let mut data=Vec::new();f.take(65537).read_to_end(&mut data).map_err(|e|e.to_string())?;
    if data.len()>65536{return Err("Plan store exceeds bounds".into());}
    let plans:Vec<StoredPlan>=serde_json::from_slice(&data).map_err(|e|e.to_string())?;
    if plans.len()>32{return Err("Plan store exceeds 32 records".into());}Ok(plans)
}
fn save_plans(base:&Path,plans:&[StoredPlan])->Result<(),String>{
    use std::io::Write;
    let bytes=serde_json::to_vec(plans).map_err(|e|e.to_string())?;if bytes.len()>65536{return Err("Plan store full".into());}
    let path=base.join("craft-launcher.plans.json");let temp=base.join(format!("craft-launcher.plans.{}.tmp",std::process::id()));
    let mut f=fs::OpenOptions::new().write(true).create_new(true).open(&temp).map_err(|e|e.to_string())?;
    let result=(||{f.write_all(&bytes).map_err(|e|e.to_string())?;f.sync_all().map_err(|e|e.to_string())?;drop(f);native::replace_file(&temp,&path)})();
    if result.is_err(){let _=fs::remove_file(&temp);}result
}
pub fn make_plan(base:&Path,c:&Config,app:&str,tag:&str,asset:&AssetInput)->Result<InstallPlan,String>{
    trusted_asset(app,tag,asset)?;
    let r=root(base,c)?;let r=r.to_str().ok_or("Install root is not UTF-8; use the GUI")?.to_string();
    let plan=InstallPlan{app:app.into(),tag:tag.into(),asset:asset.clone(),root:r,binding:native::random_id()?};
    let current=now()?;let mut plans=read_plans(base)?;plans.retain(|p|current.saturating_sub(p.created)<900&&!p.used);
    if plans.len()>=32{plans.remove(0);}plans.push(StoredPlan{plan:plan.clone(),created:current,used:false});save_plans(base,&plans)?;Ok(plan)
}
pub fn confirm(base:&Path,c:&Config,plan:&InstallPlan,cancel:&AtomicBool,progress:&WorkProgress)->Result<Value,String>{
    confirm_guarded(base,c,plan,cancel,progress,&||Ok(()))
}
pub fn confirm_guarded(base:&Path,c:&Config,plan:&InstallPlan,cancel:&AtomicBool,progress:&WorkProgress,guard:&dyn Fn()->Result<(),String>)->Result<Value,String>{
    let app=app_id(&plan.app)?;progress.begin(app,Phase::Checking,Some(plan.asset.size),"Validating the selected install plan");
    let result=confirm_inner(base,c,plan,cancel,progress,guard);if let Err(error)=&result{progress.phase(Phase::Failed,error);}result
}
fn confirm_inner(base:&Path,c:&Config,plan:&InstallPlan,cancel:&AtomicBool,progress:&WorkProgress,guard:&dyn Fn()->Result<(),String>)->Result<Value,String>{
    guard()?;
    let mut plans=read_plans(base)?;let now=now()?;
    let stored=plans.iter_mut().find(|p|p.plan.binding==plan.binding&&!p.used&&now.saturating_sub(p.created)<900).ok_or("Unknown, consumed or expired owner-issued install plan; request a fresh plan")?;
    if serde_json::to_vec(&stored.plan).map_err(|e|e.to_string())?!=serde_json::to_vec(plan).map_err(|e|e.to_string())?||root(base,c)?!=PathBuf::from(&plan.root){return Err("Install plan fields/root changed".into());}
    trusted_asset(&plan.app,&plan.tag,&plan.asset)?;
    // Consume durably before download. Process death never makes this plan replayable.
    stored.used=true;save_plans(base,&plans)?;
    let app=app_id(&plan.app)?;
    let install_root=Path::new(&plan.root);
    let checked_guard=||{guard()?;let (latest,_)=load(base)?;if apps_folder(base,&latest)?!=install_root{return Err("Apps root changed before publication".into());}Ok(())};
    let i=installer::install_guarded(install_root,app,&plan.tag,&plan.asset.asset(),cancel,progress,&checked_guard)?;
    let selection_warning=prefer_installed(base,c,install_root,app,&plan.tag,&i.folder).err();
    Ok(json!({"folder":i.folder.to_string_lossy(),"stage":i.stage.to_string_lossy(),"retained_archive_bytes":i.retained_archive_bytes,"cleanup_warning":i.cleanup_warning,"selection_warning":selection_warning,"preferred":selection_warning.is_none(),"progress":progress.snapshot()}))
}
/// Merge only this successful install's preference into the latest settings.
/// A committed publication remains successful when preference persistence conflicts.
pub fn prefer_installed(base:&Path,expected:&Config,installed_root:&Path,app:usize,tag:&str,folder:&Path)->Result<(),String>{
    let _lock=CONFIG_WRITE_LOCK.lock().unwrap_or_else(|p|p.into_inner());
    let (mut latest,mut observed)=load(base)?;
    let root=core::checked_path(installed_root)?;
    if apps_folder(base,expected)?!=root||apps_folder(base,&latest)?!=root{return Err("Apps root changed; installed version was not made preferred".into());}
    let id=APPS.get(app).ok_or("Unknown app")?.0;
    if latest.selections.get(id)!=expected.selections.get(id){return Err("Preferred version changed during installation; your selection was preserved".into());}
    let folder=core::checked_path(folder)?;
    let executable=core::validate_executable(&root,&folder.join(format!("{id}.exe")),app,false)?;
    let version=format!("{}.0",tag.trim_start_matches(['v','V']).split('+').next().unwrap_or(""));
    if native::file_version(&executable).as_deref()!=Some(version.as_str()){return Err("Installed version could not be reverified; preference preserved".into());}
    latest.selections.insert(id.into(),core::portable_path(&root,&folder));
    persist_locked(base,&latest,&mut observed)
}
pub struct Receipts {items:VecDeque<(Instant,String,Vec<u8>,Response)>}
impl Default for Receipts{fn default()->Self{Self{items:VecDeque::new()}}}
impl Receipts {
    fn expire(&mut self){while self.items.front().is_some_and(|r|r.0.elapsed()>Duration::from_secs(900)){self.items.pop_front();}}
    pub fn lookup(&mut self,r:&Request)->Option<Response>{self.expire();let encoded=serde_json::to_vec(&r.action).ok()?;self.items.iter().find(|v|v.1==r.id).map(|v|if v.2==encoded{v.3.clone()}else{Response::error(&r.id,"request_id_conflict","Request ID already used for another action")})}
    pub fn record(&mut self,r:&Request,response:Response){self.expire();if self.items.len()>=128{self.items.pop_front();}self.items.push_back((Instant::now(),r.id.clone(),serde_json::to_vec(&r.action).unwrap_or_default(),response));}
    pub fn status(&mut self,id:&str)->Value{self.expire();self.items.iter().find(|v|v.1==id).map(|v|json!({"status":"completed","response":v.3})).unwrap_or_else(||json!({"status":"unknown","message":"Receipt absent, expired or owner restarted. Reconcile manually; do not automatically replay."}))}
}
pub fn response(id:&str,result:Result<Value,String>)->Response{match result{Ok(v)=>Response::success(id,v),Err(e)=>{let code=if e.starts_with("ExternalConflict"){"external_conflict"}else if e.starts_with("FieldConflict"){"field_conflict"}else{"action_failed"};Response::error(id,code,e)}}}
/// Synchronous executor used under the physical owner lock. The GUI routes network
/// actions to a worker first; its UI thread alone invokes Patch/Reload.
pub fn execute(base:&Path,c:&mut Config,observed:&mut String,machine:Option<&str>,installs:&[Install],action:&Action,receipts:&mut Receipts,cancel:&AtomicBool,progress:&WorkProgress,view:&View)->Result<Value,String>{
    if needs_installations(action)&&!view.discovery.ready(){return Err("DiscoveryUnavailable: current installation IDs are unavailable; repair the root or wait for discovery".into());}
    match action{
        Action::UpdateAll=>crate::bulk::run(base,c,cancel,progress,&std::sync::Mutex::new(crate::bulk::Progress::default())),
        Action::OpenAppsFolder=>{let path=apps_folder(base,c)?;native::open_apps_folder(&path)?;Ok(json!({"opened":path.to_string_lossy()}))},
        Action::State=>Ok(state(base,c,machine,observed,view)),
        Action::Profiles=>Ok(json!({"profiles":state(base,c,machine,observed,view)["profiles"]})),
        Action::Installations{offset,limit}=>installations(installs,*offset,*limit),
        Action::Patch{fields}=>{let next=apply_fields(base,c,machine,installs,fields)?;persist(base,&next,observed)?;*c=next;Ok(state(base,c,machine,observed,view))},
        Action::Reload=>{let(next,hash)=load(base)?;*c=next;*observed=hash;Ok(state(base,c,machine,observed,view))},
        Action::Launch{app,install_id,profile_id,cli}=>launch(base,c,machine,installs,app,install_id,profile_id,*cli),
        Action::ReleasesCheck{app}=>checked_release_value(base,c,installs,&view.discovery,app,&official_release(app_id(app)?)),
        Action::InstallPlan{app,tag,asset}=>serde_json::to_value(make_plan(base,c,app,tag,asset)?).map_err(|e|e.to_string()),
        Action::InstallConfirm{plan}=>confirm(base,c,plan,cancel,progress),
        Action::OperationStatus{request_id}=>Ok(receipts.status(request_id)),
        Action::Cancel{..}=>Err("No GUI-owned asynchronous operation with that ID. Headless calls are synchronous; interruption requires manual reconciliation".into()),
    }
}
pub fn needs_worker(action:&Action)->bool{matches!(action,Action::UpdateAll|Action::ReleasesCheck{..}|Action::InstallPlan{..}|Action::InstallConfirm{..})}
/// Existing GUI handlers build a candidate immediately on their owner thread.
/// Convert only its changed supported fields to the same validator used by agents.
pub fn gui_delta(base:&Path,current:&Config,candidate:&Config,machine:Option<&str>,installs:&[Install])->Result<Config,String>{
    let mut fields=Vec::new();let mut add=|field|fields.push(FieldPatch{field,expected:None});
    if current.theme!=candidate.theme{add(Field::Theme{value:candidate.theme.name().into()});}
    if current.root!=candidate.root{let p=core::resolve_path(base,candidate.root.as_ref().ok_or("Clearing the apps root is unsupported")?)?;add(Field::AppsRoot{value:p.to_str().ok_or("GUI root is not UTF-8")?.into()});}
    for name in candidate.device_names(){if !current.device_names().contains(&name){add(Field::CreateProfile{profile_id:name.clone()});}
        for(app,_)in APPS{if current.device_arguments(&name,app)!=candidate.device_arguments(&name,app){add(Field::Arguments{profile_id:name.clone(),app:app.into(),value:candidate.device_arguments(&name,app).into()});}}
        if let Some(machine)=machine{let before=current.device_compatibility(&name,Some(machine),"filmcraft");let after=candidate.device_compatibility(&name,Some(machine),"filmcraft");
            if before.enabled!=after.enabled||before.obsidian!=after.obsidian{add(Field::Compatibility{profile_id:name,enabled:after.enabled,obsidian:after.obsidian.map(|p|core::resolve_path(base,&p).and_then(|p|p.to_str().map(str::to_owned).ok_or_else(||"Compatibility path is not UTF-8".into()))).transpose()?});}
        }
    }
    if current.selected_device(machine)!=candidate.selected_device(machine){add(Field::SelectProfile{profile_id:candidate.selected_device(machine).into()});}
    for(app,_)in APPS{if current.selections.get(app)!=candidate.selections.get(app){let a=app_id(app)?;let r=root(base,candidate)?;let i=core::selected_install(installs,a,&r,candidate.selections.get(app)).ok_or("Selected install disappeared")?;add(Field::SelectedVersion{app:app.into(),install_id:install_id(&installs[i])?});}}
    if fields.is_empty(){return Ok(current.clone());}apply_fields(base,current,machine,installs,&fields)
}
#[cfg(not(test))] pub(crate) fn official_release(app:usize)->Result<updates::Release,String>{updates::check(app)}
#[cfg(test)] thread_local!{static OFFICIAL:std::cell::RefCell<Option<updates::Release>>=const{std::cell::RefCell::new(None)};}
#[cfg(test)] pub(crate) fn official_release(app:usize)->Result<updates::Release,String>{OFFICIAL.with(|v|v.borrow().clone().filter(|r|r.assets.is_empty()||r.assets[0].name.starts_with(&format!("{}-",APPS[app].0))).ok_or_else(||"No offline official metadata fixture".into()))}
#[cfg(test)] mod tests{
    use super::*;
    const MACHINE:&str="00000000-0000-0000-0000-000000000001";
    fn base()->PathBuf{let p=Path::new(env!("CARGO_MANIFEST_DIR")).join(".build/temp").join(format!("craft-actions-{}",native::random_id().unwrap()));fs::create_dir_all(&p).unwrap();core::checked_path(&p).unwrap()}
    fn patch(field:Field)->FieldPatch{FieldPatch{field,expected:None}}
    #[test]fn unrelated_and_same_field_latest_writes(){
        let b=base();let mut c=Config::default();c.create_device("Desk").unwrap();
        let a=apply_fields(&b,&c,Some(MACHINE),&[],&[patch(Field::Arguments{profile_id:"Desk".into(),app:"filmcraft".into(),value:"--one".into()})]).unwrap();
        let next=apply_fields(&b,&a,Some(MACHINE),&[],&[patch(Field::Theme{value:"dark".into()})]).unwrap();
        assert_eq!(next.device_arguments("Desk","filmcraft"),"--one");
        let next=apply_fields(&b,&next,Some(MACHINE),&[],&[patch(Field::Arguments{profile_id:"Desk".into(),app:"filmcraft".into(),value:"--two".into()})]).unwrap();
        assert_eq!(next.device_arguments("Desk","filmcraft"),"--two");assert_eq!(next.theme,ThemeMode::Dark);
        let stale=FieldPatch{field:Field::Arguments{profile_id:"Desk".into(),app:"filmcraft".into(),value:"--three".into()},expected:Some(json!("--one"))};
        assert!(apply_fields(&b,&next,Some(MACHINE),&[],&[stale]).unwrap_err().contains("FieldConflict"));
    }
    #[test]fn explicit_profile_survives_selected_profile_change(){
        let b=base();let mut c=Config::default();c.create_device("Desk").unwrap();c.create_device("Travel").unwrap();
        let c=apply_fields(&b,&c,Some(MACHINE),&[],&[patch(Field::SelectProfile{profile_id:"Travel".into()}),patch(Field::Arguments{profile_id:"Desk".into(),app:"filmcraft".into(),value:"--desk".into()})]).unwrap();
        assert_eq!(c.selected_device(Some(MACHINE)),"Travel");assert_eq!(c.device_arguments("Desk","filmcraft"),"--desk");assert_eq!(c.device_arguments("Travel","filmcraft"),"");
        assert!(apply_fields(&b,&c,Some(MACHINE),&[],&[patch(Field::Arguments{profile_id:"Missing".into(),app:"filmcraft".into(),value:"".into()})]).is_err());
    }
    #[test]fn external_conflict_reload_and_interrupted_persist_preserve_bytes(){
        let b=base();let c=Config::default();c.save(&b).unwrap();let(mut current,mut observed)=load(&b).unwrap();
        let mut other=c.clone();other.theme=ThemeMode::Light;other.save(&b).unwrap();let external=fs::read(b.join("craft-launcher.conf")).unwrap();
        current.theme=ThemeMode::Dark;assert!(persist(&b,&current,&mut observed).unwrap_err().contains("ExternalConflict"));assert_eq!(fs::read(b.join("craft-launcher.conf")).unwrap(),external);
        let(next,hash)=load(&b).unwrap();current=next;observed=hash;assert_eq!(fs::read(b.join("craft-launcher.conf")).unwrap(),external);
        current.theme=ThemeMode::TrueDark;core::FAIL_BEFORE_REPLACE.with(|v|v.set(true));assert!(persist(&b,&current,&mut observed).unwrap_err().contains("Injected"));assert_eq!(fs::read(b.join("craft-launcher.conf")).unwrap(),external);
        persist(&b,&current,&mut observed).unwrap();assert_eq!(load(&b).unwrap().0.theme,ThemeMode::TrueDark);
    }
    #[test]fn bounds_unknown_fields_and_receipts(){
        assert!(decode(&vec![b' ';REQUEST_LIMIT+1]).is_err());
        assert!(decode(br#"{"protocol":1,"id":"a","action":{"action":"state","shell":"bad"}}"#).is_err());
        for action in ["state","profiles","reload","update_all","open_apps_folder"]{assert!(decode(&serde_json::to_vec(&json!({"protocol":1,"id":"extra","action":{"action":action,"unexpected":null}})).unwrap()).is_err());}
        let r=decode(br#"{"protocol":1,"id":"a","action":{"action":"state"}}"#).unwrap();let mut receipts=Receipts::default();assert_eq!(receipts.status("a")["status"],"unknown");
        receipts.record(&r,Response::success("a",json!({"done":true})));assert!(receipts.lookup(&r).unwrap().ok);
        let changed=Request{action:Action::Profiles,..r};assert_eq!(receipts.lookup(&changed).unwrap().code,"request_id_conflict");
        assert_eq!(Receipts::default().status("a")["status"],"unknown");
    }
    #[test]fn trusted_plan_rejects_forged_digest_size_and_unissued_confirmation(){
        let b=base();let mut c=Config::default();c.root=Some(b.clone());
        let name="filmcraft-0.6.0-windows-x64-portable.zip";let asset=AssetInput{name:name.into(),url:format!("https://github.com/storytold/filmcraft/releases/download/v0.6.0/{name}"),size:123,digest:format!("sha256:{}","a".repeat(64))};
        let release=updates::parse_release_for(4,&serde_json::to_vec(&json!({"draft":false,"prerelease":false,"tag_name":"v0.6.0","assets":[{"name":asset.name,"browser_download_url":asset.url,"size":asset.size,"digest":asset.digest}]})).unwrap()).unwrap();
        OFFICIAL.with(|v|*v.borrow_mut()=Some(release));
        let mut fake=asset.clone();fake.digest=format!("sha256:{}","b".repeat(64));assert!(make_plan(&b,&c,"filmcraft","v0.6.0",&fake).is_err());
        fake=asset.clone();fake.size+=1;assert!(make_plan(&b,&c,"filmcraft","v0.6.0",&fake).is_err());
        let issued=make_plan(&b,&c,"filmcraft","v0.6.0",&asset).unwrap();let fabricated=InstallPlan{binding:"0".repeat(32),..issued.clone()};
        assert!(confirm(&b,&c,&fabricated,&AtomicBool::new(false),&WorkProgress::new(0)).unwrap_err().contains("Unknown"));
        let mut changed=issued;changed.asset.size+=1;assert!(confirm(&b,&c,&changed,&AtomicBool::new(false),&WorkProgress::new(0)).unwrap_err().contains("changed"));
        OFFICIAL.with(|v|*v.borrow_mut()=None);
    }
}
#[cfg(test)] mod review_regressions{
    use super::*;
    fn base()->PathBuf{let p=Path::new(env!("CARGO_MANIFEST_DIR")).join(".build/temp").join(format!("craft-review-{}",native::random_id().unwrap()));fs::create_dir_all(&p).unwrap();core::checked_path(&p).unwrap()}
    fn one(field:Field)->Action{Action::Patch{fields:vec![FieldPatch{field,expected:None}]}}
    fn run(base:&Path,c:&mut Config,hash:&mut String,action:&Action,view:&View)->Result<Value,String>{
        execute(base,c,hash,Some("00000000-0000-0000-0000-000000000001"),&[],action,&mut Receipts::default(),&AtomicBool::new(false),&WorkProgress::new(0),view)
    }
    #[test]fn missing_old_root_remains_repairable_and_unrelated_fields_do_not_scan(){
        let b=base();let mut c=Config::default();c.root=Some(b.join("disconnected"));c.save(&b).unwrap();let mut observed=disk_hash(&b).unwrap();
        for action in [
            one(Field::Theme{value:"dark".into()}),
            one(Field::CreateProfile{profile_id:"Desk".into()}),
            one(Field::Arguments{profile_id:"Desk".into(),app:"filmcraft".into(),value:"--desk".into()}),
            one(Field::AppsRoot{value:b.to_string_lossy().into_owned()}),
        ]{
            let(installs,discovery)=discover_for_action(&b,&c,&action);assert!(installs.is_empty());assert!(matches!(discovery,Discovery::NotRequested));
            let view=View{owner:"headless",discovery,work:json!({"status":"synchronous"})};
            run(&b,&mut c,&mut observed,&action,&view).unwrap();
        }
        assert_eq!(root(&b,&c).unwrap(),b);assert_eq!(c.theme,ThemeMode::Dark);assert_eq!(c.device_arguments("Desk","filmcraft"),"--desk");
        for discovery in [Discovery::Scanning,Discovery::Unavailable{message:"Previous scan failed".into()}]{
            let view=View{owner:"gui",discovery,work:json!({"status":"idle"})};
            run(&b,&mut c,&mut observed,&one(Field::Arguments{profile_id:"Desk".into(),app:"filmcraft".into(),value:"--repair".into()}),&view).unwrap();
        }
        let requiring_ids=one(Field::SelectedVersion{app:"filmcraft".into(),install_id:"unavailable".into()});assert!(needs_installations(&requiring_ids));
    }
    #[test]fn release_comparison_uses_effective_selection_and_unknown_discovery_in_both_paths(){
        let b=base();let mut c=Config::default();c.root=Some(b.clone());
        let release=updates::parse_release_for(4,br#"{"draft":false,"prerelease":false,"tag_name":"v1.2.3","assets":[]}"#).unwrap();
        let result=Ok(release.clone());OFFICIAL.with(|v|*v.borrow_mut()=Some(release));
        let mut installs=vec![
            Install{app:4,gui:None,cli:None,folder:b.join("older"),version:"1.0.0.0 [PE file version]".into()},
            Install{app:4,gui:None,cli:None,folder:b.join("current"),version:"1.2.3.0 [PE file version]".into()},
        ];
        let check=|c:&Config,installs:&[Install],d:&Discovery|checked_release_value(&b,c,installs,d,"filmcraft",&result).unwrap();
        assert_eq!(check(&c,&installs,&Discovery::Ready)["comparison"]["state"],"Unknown");
        c.selections.insert("filmcraft".into(),PathBuf::from("older"));assert_eq!(check(&c,&installs,&Discovery::Ready)["comparison"]["state"],"Update available");
        c.selections.insert("filmcraft".into(),PathBuf::from("current"));assert_eq!(check(&c,&installs,&Discovery::Ready)["comparison"]["state"],"Up to date");
        let expected=check(&c,&installs,&Discovery::Ready);
        let view=View{owner:"headless",discovery:Discovery::Ready,work:json!({"status":"synchronous"})};
        let actual=execute(&b,&mut c,&mut String::new(),None,&installs,&Action::ReleasesCheck{app:"filmcraft".into()},&mut Receipts::default(),&AtomicBool::new(false),&WorkProgress::new(0),&view).unwrap();assert_eq!(actual,expected);
        for unsupported in ["Unknown; folder hint 1.2.3","1.2.3.4 [PE file version]"]{installs[1].version=unsupported.into();assert_eq!(check(&c,&installs,&Discovery::Ready)["comparison"]["state"],"Unknown");}
        assert_eq!(check(&c,&[],&Discovery::Ready)["comparison"]["state"],"Not installed");
        assert_eq!(check(&c,&[],&Discovery::Unavailable{message:"offline".into()})["comparison"]["state"],"Unknown");
        c.root=Some(b.join("disconnected"));let value=check(&c,&[],&Discovery::Ready);assert_eq!(value["comparison"]["state"],"Unknown");assert!(value["comparison"]["installed_count"].is_null());
        OFFICIAL.with(|v|*v.borrow_mut()=None);
    }
    #[test]fn state_explicitly_reports_catalogue_owner_discovery_and_work(){
        let b=base();let view=View{owner:"gui",discovery:Discovery::Scanning,work:json!({"status":"running","request_id":"fixture"})};
        let value=state(&b,&Config::default(),None,"missing",&view);assert_eq!(value["known_apps"].as_array().unwrap().len(),12);assert_eq!(value["known_apps"][4]["app_id"],"filmcraft");assert_eq!(value["owner"],"gui");assert_eq!(value["discovery"]["status"],"scanning");assert_eq!(value["work"]["request_id"],"fixture");
    }
}
#[cfg(test)] mod null_precondition_regression{
    use super::*;
    #[test]fn explicit_null_survives_cli_roundtrip_and_rejects_root_race_without_writes(){
        let base=Path::new(env!("CARGO_MANIFEST_DIR")).join(".build/temp").join(format!("craft-null-{}",native::random_id().unwrap()));
        fs::create_dir_all(&base).unwrap();let base=core::checked_path(&base).unwrap();
        let envelope=|expected:bool|{
            let mut patch=json!({"field":{"field":"apps_root","value":base.to_string_lossy()}});
            if expected{patch["expected"]=Value::Null;}
            serde_json::to_vec(&json!({"protocol":1,"id":"null-root","action":{"action":"patch","fields":[patch]}})).unwrap()
        };
        let omitted=decode(&envelope(false)).unwrap();let explicit=decode(&envelope(true)).unwrap();
        let omitted_json=serde_json::to_value(&omitted).unwrap();assert!(omitted_json["action"]["fields"][0].get("expected").is_none());
        let explicit_json=serde_json::to_value(&explicit).unwrap();assert_eq!(explicit_json["action"]["fields"][0].get("expected"),Some(&Value::Null));
        let roundtrip=decode(&serde_json::to_vec(&explicit).unwrap()).unwrap();
        let Action::Patch{fields}= &roundtrip.action else{panic!("Expected a patch")};
        assert_eq!(fields[0].expected,Some(Value::Null));
        let unset=Config::default();assert!(apply_fields(&base,&unset,None,&[],fields).is_ok());
        // Another owner action sets the root before this stale request arrives.
        let mut current=unset;current.root=Some(base.clone());current.save(&base).unwrap();
        let before=fs::read(base.join("craft-launcher.conf")).unwrap();let mut observed=disk_hash(&base).unwrap();
        let view=View{owner:"headless",discovery:Discovery::NotRequested,work:json!({"status":"synchronous"})};
        let error=execute(&base,&mut current,&mut observed,None,&[],&roundtrip.action,&mut Receipts::default(),&AtomicBool::new(false),&WorkProgress::new(0),&view).unwrap_err();
        assert!(error.contains("FieldConflict"));assert_eq!(fs::read(base.join("craft-launcher.conf")).unwrap(),before);
    }
}
/// The only folder-opening action accepts the saved root, never a caller path.
pub fn apps_folder(base:&Path,config:&Config)->Result<PathBuf,String>{let path=root(base,config)?;if !path.is_dir(){return Err("Apps root must be an existing directory".into());}Ok(path)}
#[cfg(test)] pub(crate) fn fixture_official(release:Option<updates::Release>){OFFICIAL.with(|v|*v.borrow_mut()=release);}
#[cfg(test)] mod folder_action_tests {
    use super::*;
    #[test]fn folder_action_passes_only_checked_saved_literal_path(){
        let base=Path::new(env!("CARGO_MANIFEST_DIR")).join(".build/temp").join(format!("folder-action-{}",native::random_id().unwrap()));fs::create_dir_all(&base).unwrap();
        let literal=base.join("apps & literal (folder)");fs::create_dir(&literal).unwrap();let mut config=Config::default();config.root=Some(literal.clone());let view=View{owner:"headless",discovery:Discovery::NotRequested,work:json!({})};
        native::OPENED_FOLDER.with(|v|*v.borrow_mut()=None);
        let result=execute(&base,&mut config,&mut String::new(),None,&[],&Action::OpenAppsFolder,&mut Receipts::default(),&AtomicBool::new(false),&WorkProgress::new(0),&view).unwrap();
        let checked=core::checked_path(&literal).unwrap();assert_eq!(result["opened"],checked.to_string_lossy().as_ref());assert_eq!(native::OPENED_FOLDER.with(|v|v.borrow().clone()),Some(checked));
        native::OPENED_FOLDER.with(|v|*v.borrow_mut()=None);config.root=Some(base.join("disconnected"));assert!(execute(&base,&mut config,&mut String::new(),None,&[],&Action::OpenAppsFolder,&mut Receipts::default(),&AtomicBool::new(false),&WorkProgress::new(0),&view).is_err());assert!(native::OPENED_FOLDER.with(|v|v.borrow().is_none()));
    }
}
