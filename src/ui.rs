use crate::progress::{WorkProgress,Phase};
use crate::{actions,owner,updates,installer,core::{self,Config,Install,Profile,ThemeMode,APPS},native::{self,*}};
use std::{cell::{Cell,RefCell},path::PathBuf,ptr,sync::{mpsc::{self,Receiver,TryRecvError},Arc,atomic::{AtomicBool,Ordering}},thread,time::{Duration,Instant}};
const ROW_PROGRESS:i32=1100;const BATCH_PROGRESS:i32=1120;
const TAB_LAUNCHER:i32=850;const TAB_APPEARANCE:i32=851;const TAB_UPDATES:i32=852;const ROW_STATUS:i32=980;const ROW_LATEST:i32=1000;const UPDATE_BADGE:i32=111;const RELEASE_TITLE:i32=867;
const DEVICE:i32=840;const DEVICE_NAME:i32=841;const CREATE_DEVICE:i32=842;
const THEME_TRUE_DARK:i32=833;const THEME_AETHER:i32=834;
const THEME_SYSTEM:i32=830;const THEME_LIGHT:i32=831;const THEME_DARK:i32=832;
const TILE:i32=300;const SETTINGS:i32=100;const STATUS:i32=110;const TIMER:usize=1;
const ROOT:i32=801;const BROWSE:i32=802;const SAVE_ROOT:i32=803;const DETAILS:i32=805;
const VERSIONS:i32=810;const ARGS:i32=811;const SAVE_APP:i32=812;
const COMPAT:i32=814;const EXCLUDED:i32=815;const PICK_EXCLUDED:i32=816;const CLOSE:i32=820;
const UPDATE_ALL:i32=868;const OPEN_APPS_FOLDER:i32=869;const CHECK_UPDATES:i32=860;const UPDATE_STATUS:i32=861;const RELEASE_ASSETS:i32=862;const DOWNLOAD:i32=863;const OPEN_RELEASE:i32=864;const CANCEL_WORK:i32=865;const RELEASE_DETAILS:i32=866;const APP_ROW:i32=900;const PREFERRED:i32=920;const WORK_TIMER:usize=3;
enum WorkResult {CheckedOne(usize,Result<updates::Release,String>),ChecksComplete(usize),Installed(PathBuf,Result<(installer::Installed,Option<String>),String>)}
const MENU_GUI:usize=500;const MENU_OPTIONS:usize=502;const MENU_VERSION:usize=2000;
struct ScanRequest {generation:u64,root:PathBuf,save_root:bool}
struct ScanResult {request:ScanRequest,result:Result<(PathBuf,Vec<Install>,Vec<String>),String>,worker_elapsed:Duration,discovery_elapsed:Duration,stats:core::ScanStats,finished_at:Instant}
const AGENT_WAKE:u32=0x804d;const AGENT_WORK_TIMER:usize=4;
struct AgentFinished{response:actions::Response,release:Option<(usize,Result<updates::Release,String>)>,installed_root:Option<PathBuf>}
struct AgentWork {request:actions::Request,receiver:Receiver<AgentFinished>,cancel:Arc<AtomicBool>,progress:Arc<WorkProgress>}
struct State {
    bulk_progress:Option<Arc<std::sync::Mutex<crate::bulk::Progress>>>,closing:bool,observed:String,agent:Option<owner::Server>,receipts:actions::Receipts,agent_work:Option<AgentWork>,options_conflict:bool,root_conflict:bool,settings_profile:String,
    #[cfg(test)] hidden_fixture:bool,
    hwnd:Hwnd,base:PathBuf,config:Config,machine:Option<String>,device:String,config_error:Option<String>,installs:Vec<Install>,canonical_root:Option<PathBuf>,
    icons:[Handle;APPS.len()],font:Handle,name_font:Handle,detail_font:Handle,
    discovery_error:Option<String>,scanning:bool,scan_generation:u64,scan_receiver:Option<Receiver<ScanResult>>,pending_scan:Option<ScanRequest>,
    settings:Hwnd,settings_tab:usize,root_draft:Option<String>,profile_draft:String,settings_app:Option<usize>,settings_versions:Vec<usize>,details:String,
    launch_check_started:bool,release_app:Option<usize>,preferred_rows:[Vec<usize>;APPS.len()],releases:std::collections::BTreeMap<usize,Result<updates::Release,String>>,work:Option<Receiver<WorkResult>>,cancel:Arc<AtomicBool>,progress:Arc<WorkProgress>,work_status:String,
}
thread_local! {static STATE:RefCell<Option<State>>=const {RefCell::new(None)};static THEME:Cell<ThemeMode>=const {Cell::new(ThemeMode::System)};static THEME_UPDATING:Cell<bool>=const {Cell::new(false)};static MAIN:Cell<Hwnd>=const {Cell::new(0)};static OPTIONS:Cell<Hwnd>=const {Cell::new(0)};}
// A modal Win32 loop can deliver a wake while the outer handler owns STATE.
// Record it and drain synchronously when that handler's final borrow is released.
thread_local!{static AGENT_PENDING:Cell<bool>=const{Cell::new(false)};}
struct AgentWakeGuard;
impl Drop for AgentWakeGuard{fn drop(&mut self){drain_agent_wake();}}
fn drain_agent_wake(){
    loop{
        if !AGENT_PENDING.with(Cell::get){return;}
        let drained=STATE.with(|slot|{
            if let Ok(mut state)=slot.try_borrow_mut(){
                AGENT_PENDING.with(|p|p.set(false));
                if let Some(s)=state.as_mut(){poll_agent(s);}
                true
            }else{false}
        });
        if !drained{return;} // An outer owner scope still holds STATE.
    }
}
fn theme_settings(hwnd:Hwnd,mode:ThemeMode){
    if hwnd==0{return;}for(id,choice,label)in[(THEME_SYSTEM,ThemeMode::System,"System - automatic"),(THEME_LIGHT,ThemeMode::Light,"Light"),(THEME_DARK,ThemeMode::Dark,"Dark"),(THEME_TRUE_DARK,ThemeMode::TrueDark,"True Dark"),(THEME_AETHER,ThemeMode::Aetherglass,"Aetherglass")]{let button=control(hwnd,id);if button!=0{native::selected_button(button,choice==mode);native::set_text(button,&format!("{}{}",label,if choice==mode{" *"}else{""}));}}native::theme_window(hwnd);
}
fn apply_theme(mode:ThemeMode) {
    if THEME_UPDATING.with(|v|v.replace(true)){return;}
    THEME.with(|v|v.set(mode));native::set_theme_palette(mode,native::system_apps_dark());
    MAIN.with(|v|native::theme_window(v.get()));OPTIONS.with(|v|theme_settings(v.get(),mode));THEME_UPDATING.with(|v|v.set(false));
}
fn theme_changed() {apply_theme(THEME.with(|v|v.get()));}
fn control(hwnd:Hwnd,id:i32)->Hwnd {unsafe {GetDlgItem(hwnd,id)}}
const DRAFT_CONFLICT:&str="An agent changed a field with an unsaved draft. Review it before Save, or close and reopen to discard it.";
fn status(s:&State,text:&str) {let message=if (s.options_conflict||s.root_conflict)&&!text.starts_with(DRAFT_CONFLICT){format!("{DRAFT_CONFLICT} {text}")}else{text.to_owned()};native::set_text(control(s.hwnd,STATUS),&message);unsafe {let hwnd=control(s.hwnd,STATUS);if hwnd!=0&&(GetWindowLongPtrW(hwnd,-16)&0x10000000!=0)!=!message.is_empty(){ShowWindow(hwnd,if message.is_empty(){0}else{5});}}}
fn discard_conflicts(s:&mut State){let conflicted=s.root_conflict||s.options_conflict;if s.root_conflict{s.root_draft=None;}s.root_conflict=false;s.options_conflict=false;if conflicted{status(s,"");}}
fn details(s:&mut State,text:String) {s.details=text;if s.settings!=0 && s.settings_app.is_none() && s.release_app.is_none(){native::set_text(control(s.settings,DETAILS),&s.details);}}
fn report(s:&mut State,text:&str) {let _=core::log(&s.base,text);status(s,text);details(s,text.into());native::message(if s.settings!=0{s.settings}else{s.hwnd},text);}
fn root(s:&State)->Result<PathBuf,String> {core::resolve_path(&s.base,s.config.root.as_ref().ok_or("Choose the app root in Settings.")?)}
fn chosen(s:&State,app:usize)->Option<usize> {core::selected_install(&s.installs,app,s.canonical_root.as_ref()?,s.config.selections.get(APPS[app].0))}
fn persist(s:&mut State,candidate:Config)->bool {
    if let Some(error)=s.config_error.clone(){report(s,&error);return false;}
    let candidate=match actions::gui_delta(&s.base,&s.config,&candidate,s.machine.as_deref(),&s.installs){Ok(c)=>c,Err(e)=>{report(s,&e);return false;}};
    match actions::persist(&s.base,&candidate,&mut s.observed){Ok(())=>{s.config=candidate;native::launcher_context(s.hwnd,&root(s).map(|p|p.display().to_string()).unwrap_or_else(|_|"Choose app root in Settings".into()),&format!("{} profile",s.config.selected_device(s.machine.as_deref())));true},Err(error)=>{report(s,&error);false}}
}
fn short_version(version:&str)->String {
    if let Some(value)=version.strip_suffix(" [PE file version]"){value.into()}
    else if version.starts_with("Unknown;"){version.replace("Unknown; folder hint ","Unknown  -  folder hint ")}
    else {"Unknown version".into()}
}
fn update_tiles(s:&mut State) {let _=update_tiles_timed(s);}
fn update_tiles_timed(s:&mut State)->Duration {
    native::launcher_context(s.hwnd,&format!("{}",root(s).map(|p|p.display().to_string()).unwrap_or_else(|_|"Choose app root in Settings".into())),&format!("{} profile",s.config.selected_device(s.machine.as_deref())));
    let mut icon_time=Duration::ZERO;
    for app in 0..APPS.len() {
        let button=control(s.hwnd,TILE+app as i32);
        native::set_button_icon(button,0);
        if s.icons[app]!=0{unsafe {DestroyIcon(s.icons[app]);}s.icons[app]=0;}
        let count=s.installs.iter().filter(|install|install.app==app).count();
        let selected=chosen(s,app);
        // An unselected installation may supply an icon only; it is never launched.
        let preview=selected.or_else(||s.installs.iter().position(|install|install.app==app));
        if let Some(index)=preview {if let Some(exe)=s.installs[index].gui.as_ref().or(s.installs[index].cli.as_ref()){let started=Instant::now();s.icons[app]=native::executable_icon(exe);icon_time+=started.elapsed();}}
        native::set_button_icon(button,s.icons[app]);
        let secondary=if s.scanning{"Scanning...".into()}else if let Some(index)=selected {short_version(&s.installs[index].version)}else if count>1{format!("Choose version ({count})")}else{"Not installed".into()};
        native::set_text(button,&format!("{}\n{}",APPS[app].1,secondary));
    }
    refresh_update_rows(s);icon_time
}
fn scan_failed(s:&mut State,message:&str) {s.discovery_error=Some(message.into());s.scanning=false;update_tiles(s);status(s,"Could not scan the app folder. Open Settings for details or choose another root.");details(s,message.into());}
fn start_scan(s:&mut State,request:ScanRequest) {
    unsafe {if SetTimer(s.hwnd,TIMER,100,None)==0{scan_failed(s,"Windows could not start the scan timer. Use Settings → Refresh to retry.");return;}}
    let (sender,receiver)=mpsc::sync_channel(1);
    match thread::Builder::new().name("craft-discovery".into()).spawn(move||{
        let started=Instant::now();let mut stats=core::ScanStats::default();let mut discovery_elapsed=Duration::ZERO;
        let result=(||{let canonical=core::checked_path(&request.root)?;if !canonical.is_dir(){return Err("The selected root is not a folder.".into());}let discovery_started=Instant::now();let discovered=core::discover_timed(&request.root,&mut stats);discovery_elapsed=discovery_started.elapsed();let (installs,notes)=discovered?;Ok((canonical,installs,notes))})();
        let _=sender.send(ScanResult{request,result,worker_elapsed:started.elapsed(),discovery_elapsed,stats,finished_at:Instant::now()});
    }){Ok(_)=>s.scan_receiver=Some(receiver),Err(error)=>{unsafe {KillTimer(s.hwnd,TIMER);}scan_failed(s,&format!("Could not start discovery: {error}"));}}
}
fn request_scan(s:&mut State,path:PathBuf,save_root:bool) {
    if s.work.is_some(){s.cancel.store(true,Ordering::Relaxed);}if let Some(w)=&s.agent_work{if matches!(w.request.action,actions::Action::UpdateAll){w.cancel.store(true,Ordering::Relaxed);}}
    s.scan_generation=s.scan_generation.wrapping_add(1);s.scanning=true;s.installs.clear();s.canonical_root=None;update_tiles(s);
    status(s,"Scanning installed apps... Settings and window controls remain available.");details(s,format!("Scanning {}\r\nYou can choose a different root while this scan runs.",path.display()));
    let request=ScanRequest{generation:s.scan_generation,root:path,save_root};
    if s.scan_receiver.is_some(){s.pending_scan=Some(request);}else{start_scan(s,request);}
}
fn refresh(s:&mut State) {
    if s.scanning{return;}
    match root(s){Ok(path)=>request_scan(s,path,false),Err(error)=>{status(s,"Choose your app root in Settings to find installed apps.");details(s,error);update_tiles(s);}}
}
fn apply_scan(s:&mut State,finished:ScanResult) {
    if finished.request.generation!=s.scan_generation{return;}
    let apply_started=Instant::now();let delivery=finished.finished_at.elapsed();
    s.scanning=false;
    match finished.result {
        Ok((canonical,installs,notes))=>{
            if finished.request.save_root {
                let mut candidate=s.config.clone();candidate.root=Some(core::portable_path(&s.base,&finished.request.root));
                if !persist(s,candidate){update_tiles(s);return;}
            }
            s.installs=installs;s.canonical_root=Some(canonical);let tile_started=Instant::now();let icon_time=update_tiles_timed(s);let tile_time=tile_started.elapsed();
            let errors=notes.iter().filter(|note|!core::scan_note_is_informational(note)).count();s.discovery_error=if errors==0{None}else{Some(format!("Discovery has {errors} warnings; installation completeness is unknown"))};
            let informational=notes.len()-errors;
            let mut summary=format!("Root: {}\r\n{} installed versions found.\r\n{} scan warnings; {} informational scan notes.\r\n\r\n",finished.request.root.display(),s.installs.len(),errors,informational);
            if informational>0{summary.push_str("Informational notes describe depth limits and managed staging excluded from discovery. Place portable app folders directly under the selected root if an app is missing.\r\n\r\n");}
            for note in notes.iter().take(200){summary.push_str(if core::scan_note_is_informational(note){"Info: "}else{"Warning: "});summary.push_str(note);summary.push_str("\r\n");}
            if notes.len()>200{summary.push_str(&format!("{} further notes omitted. Choose a narrower root to reduce scan scope.\r\n",notes.len()-200));}
            details(s,summary);
            if s.installs.is_empty(){status(s,"No installed apps found. Open Settings and choose the folder containing your portable apps.");}
            else if errors>0{status(s,"Apps are ready. Some folders could not be scanned; open Settings for actionable details.");}
            else{status(s,"");}
            if s.settings!=0{if s.settings_app.is_some(){fill_app_versions(s);}else{fill_preferred(s);if s.release_app.is_some(){show_release_details(s);}}}
            let _=core::log(&s.base,&format!("scan timing: generation={} worker_ms={} discovery_ms={} version_ms={} delivery_ms={} ui_apply_ms={} tile_apply_ms={} icon_ms={} entries={} folders={} version_queries={} installs={}",finished.request.generation,finished.worker_elapsed.as_millis(),finished.discovery_elapsed.as_millis(),finished.stats.version_time.as_millis(),delivery.as_millis(),apply_started.elapsed().as_millis(),tile_time.as_millis(),icon_time.as_millis(),finished.stats.entries,finished.stats.folders,finished.stats.version_queries,s.installs.len()));

        }
        Err(error)=>{let _=core::log(&s.base,&format!("scan timing: failed generation={} worker_ms={} discovery_ms={} version_ms={} entries={} folders={} version_queries={}",finished.request.generation,finished.worker_elapsed.as_millis(),finished.discovery_elapsed.as_millis(),finished.stats.version_time.as_millis(),finished.stats.entries,finished.stats.folders,finished.stats.version_queries));scan_failed(s,&format!("{}\r\nChoose an existing app root in Settings and retry. The previous saved root was preserved.",error));},
    }
}
fn poll_scan(s:&mut State) {
    let result=match s.scan_receiver.as_ref().map(|receiver|receiver.try_recv()){
        Some(Ok(result))=>Some(result),Some(Err(TryRecvError::Empty))=>return,
        Some(Err(TryRecvError::Disconnected))=>{scan_failed(s,"The scan worker stopped unexpectedly. Open Settings and Refresh to retry.");None},None=>return,
    };
    s.scan_receiver=None;if let Some(result)=result{apply_scan(s,result);}
    if let Some(request)=s.pending_scan.take(){start_scan(s,request);}else{unsafe {KillTimer(s.hwnd,TIMER);}maybe_start_launch_check(s);}
}
fn save_root_text(s:&mut State) {
    if s.root_conflict {
        if unsafe{MessageBoxW(s.settings,wide("The apps root changed while this draft was open. Apply your displayed root draft now?").as_ptr(),wide("Rebase changed root").as_ptr(),0x4|0x20|0x100)}!=6{return;}
        s.root_conflict=false;
    }
    if let Some(error)=s.config_error.clone(){report(s,&error);return;}
    match core::normalize(&PathBuf::from(native::text(control(s.settings,ROOT)).trim())){Ok(path)=>request_scan(s,path,true),Err(error)=>report(s,&error)}
}
fn choose_root(s:&mut State) {
    if let Some(path)=native::folder_picker(if s.settings!=0{s.settings}else{s.hwnd},"Choose the folder containing your portable Craft apps"){
        s.root_draft=Some(path.to_string_lossy().into_owned());
        if s.settings!=0 && s.settings_app.is_none() && s.release_app.is_none(){native::set_text(control(s.settings,ROOT),&path.to_string_lossy());}
        if let Some(error)=s.config_error.clone(){report(s,&error);}else{request_scan(s,path,true);}
    }
}
fn choose_version(s:&mut State,app:usize,index:usize)->bool {
    let Some(install)=s.installs.get(index).filter(|i|i.app==app)else{return false;};
    let Some(canonical)=s.canonical_root.as_ref()else{return false;};
    let mut candidate=s.config.clone();candidate.selections.insert(APPS[app].0.into(),core::portable_path(canonical,&install.folder));
    if persist(s,candidate){update_tiles(s);fill_preferred(s);if s.release_app==Some(app){show_release_details(s);}true}else{false}
}
fn launch_app(s:&mut State,app:usize,cli:bool) {
    if s.scanning{status(s,"Discovery is still running. Settings and window controls remain available.");return;}
    let Some(index)=chosen(s,app)else{open_settings(s,Some(app));return;};
    native::click_feedback(control(s.hwnd,TILE+app as i32));
    let result=(||{let root=root(s)?;let args=core::parse_args(s.config.device_arguments(&s.device,APPS[app].0))?;let mut profile=s.config.device_compatibility(&s.device,s.machine.as_deref(),APPS[app].0);if profile.enabled{if let Some(path)=profile.obsidian.as_ref(){profile.obsidian=Some(core::resolve_path(&s.base,path)?);}}core::launch(&s.base,&root,&s.installs[index],cli,&args,&profile)})();
    match result{Ok(pid)=>status(s,&format!("Started {} (process {pid}).",APPS[app].1)),Err(error)=>report(s,&error)}
}
unsafe fn child(parent:Hwnd,class:&str,title:&str,style:u32,x:i32,y:i32,w:i32,h:i32,id:i32,font:Handle)->Hwnd {
    let style=if class=="BUTTON" {if style&0xF==0{style|0xB|0x4000}else{style|0x4000}}else{style};
    let handle=CreateWindowExW(if class=="EDIT"||class=="LISTBOX"{0x200}else{0},wide(class).as_ptr(),wide(title).as_ptr(),0x40000000|0x10000000|style,x,y,w,h,parent,id as isize,GetModuleHandleW(ptr::null()),ptr::null_mut());SendMessageW(handle,0x30,font as usize,1);native::plain_control(handle);handle
}
fn tile_bounds(w:i32,h:i32)->Vec<Rect> {native::circles::bounds(w,h)}
fn layout(hwnd:Hwnd) {
    unsafe {
        let mut rect:Rect=std::mem::zeroed();GetClientRect(hwnd,&mut rect);let w=rect.right;let h=rect.bottom;let pad=24;
        let mut moves=vec![(control(hwnd,SETTINGS),Rect{left:w-pad-96,top:24,right:w-pad,bottom:60}),
            (control(hwnd,UPDATE_BADGE),Rect{left:w-pad-140,top:29,right:w-pad-104,bottom:57})];
        moves.extend(tile_bounds(w,h).into_iter().enumerate().map(|(app,r)|(control(hwnd,TILE+app as i32),r)));
        moves.push((control(hwnd,STATUS),Rect{left:pad,top:h-25,right:w-pad,bottom:h-3}));
        native::layout_controls(hwnd,&moves);
        for app in 0..APPS.len(){native::circle_region(control(hwnd,TILE+app as i32));}
    }
}
fn fill_app_versions(s:&mut State) {
    let Some(app)=s.settings_app else{return;};let combo=control(s.settings,VERSIONS);unsafe {SendMessageW(combo,0x14B,0,0);}
    s.settings_versions=s.installs.iter().enumerate().filter(|(_,i)|i.app==app).map(|(i,_)|i).collect();
    for(row,index)in s.settings_versions.iter().enumerate(){let install=&s.installs[*index];let label=format!("{} | {}",install.version,install.folder.display());unsafe {SendMessageW(combo,0x143,0,wide(label).as_ptr()as isize);if chosen(s,app)==Some(*index){SendMessageW(combo,0x14E,row,0);}}}
}
fn overview(s:&State,app:usize)->updates::UpdateStatus{updates::update_status(app,s.installs.iter().filter(|i|i.app==app).count(),chosen(s,app).map(|i|s.installs[i].version.as_str()),s.releases.get(&app))}
fn update_count(s:&State)->usize{(0..APPS.len()).filter(|app|overview(s,*app).state==updates::UpdateState::UpdateAvailable).count()}
fn refresh_update_rows(s:&State){
    let count=update_count(s);if s.hwnd!=0{let badge=control(s.hwnd,UPDATE_BADGE);native::set_text(badge,&if count==0{String::new()}else{count.to_string()});}
    if s.settings==0||s.settings_app.is_some(){return;}native::set_text(control(s.settings,TAB_UPDATES),&if count==0{"Updates".into()}else{format!("Updates ({count})")});
    if s.settings_tab!=2{return;}for app in 0..APPS.len(){let status=overview(s,app);native::set_text(control(s.settings,ROW_STATUS+app as i32),status.label());native::set_text(control(s.settings,APP_ROW+app as i32),status.action());let latest=match s.releases.get(&app){Some(Ok(release))=>release.tag.as_str(),Some(Err(_))=>"Error",None=>"Not checked"};native::set_text(control(s.settings,ROW_LATEST+app as i32),latest);}
}
fn fill_preferred(s:&mut State){
    if s.settings==0||s.settings_app.is_some()||s.settings_tab!=2{return;}
    for app in 0..APPS.len() {let combo=control(s.settings,PREFERRED+app as i32);unsafe {SendMessageW(combo,0x14B,0,0);}
        s.preferred_rows[app]=s.installs.iter().enumerate().filter(|(_,i)|i.app==app).map(|(i,_)|i).collect();
        let versions:Vec<_>=s.preferred_rows[app].iter().map(|i|short_version(&s.installs[*i].version)).collect();
        if versions.is_empty(){unsafe{SendMessageW(combo,0x143,0,wide("Not installed").as_ptr()as isize);SendMessageW(combo,0x14E,0,0);}}
        for(row,index)in s.preferred_rows[app].iter().enumerate(){let label=format!("{} | {}",short_version(&s.installs[*index].version),s.installs[*index].folder.file_name().unwrap_or_default().to_string_lossy());unsafe {SendMessageW(combo,0x143,0,wide(label).as_ptr()as isize);if chosen(s,app)==Some(*index){SendMessageW(combo,0x14E,row,0);}}}
        unsafe {EnableWindow(combo,i32::from(!s.scanning&&!versions.is_empty()));}

    }refresh_update_rows(s);
}
fn batch_snapshot(s:&State,p:&std::sync::Mutex<crate::bulk::Progress>)->crate::bulk::Progress{if let Some(w)=&s.agent_work{crate::bulk::snapshot_live(p,&w.progress)}else{crate::bulk::snapshot(p)}}
fn current_transfer(s:&State)->crate::progress::Snapshot{s.agent_work.as_ref().map(|w|w.progress.snapshot()).unwrap_or_else(||s.progress.snapshot())}
fn work_summary(s:&State)->String {
    if let Some(p)=&s.bulk_progress{let p=batch_snapshot(s,p);if let Some(w)=&s.agent_work{let active=p.rows.iter().find(|r|matches!(r.status.as_str(),"checking"|"downloading"|"installing")).map(|r|format!("{}: {}",r.app,r.status)).unwrap_or_else(||p.status.clone());return format!("{}/{} {}{}",p.completed,p.total,active,if w.cancel.load(Ordering::Relaxed){" (stopping)".into()}else{format!("; {} B",w.progress.load(Ordering::Relaxed))});}return format!("Update All {}: {}/{}; details below",p.status,p.completed,p.total);}
    let transfer=current_transfer(s);if transfer.app.is_some(){return transfer.label();}
    if s.agent_work.is_some(){return "Checking official release...".into();}
    if s.work.is_some(){if s.cancel.load(Ordering::Relaxed){"Cancelling...".into()}else if s.work_status.starts_with("Downloading"){format!("Downloading: {} bytes received",s.progress.load(Ordering::Relaxed))}else{format!("Checking releases: {} / {}",s.progress.load(Ordering::Relaxed),APPS.len())}}
    else {let line=s.work_status.lines().next().unwrap_or("");let mut text:String=line.chars().take(70).collect();if line.chars().count()>70{text.push_str("...");}text}
}
fn work_status(s:&State){
    let batch=s.bulk_progress.as_ref().map(|p|batch_snapshot(s,p));let transfer=current_transfer(s);
    if let Some(p)=&batch{status(s,&p.summary());}
    if s.settings!=0{native::set_text(control(s.settings,UPDATE_STATUS),&work_summary(s));unsafe {
        let idle=s.work.is_none()&&s.agent_work.is_none();
        EnableWindow(control(s.settings,UPDATE_ALL),i32::from(idle&&!s.scanning&&s.config_error.is_none()));
        let selected_asset=s.release_app.and_then(|app|s.releases.get(&app)).and_then(|r|r.as_ref().ok()).is_some_and(|release|{let row=SendMessageW(control(s.settings,RELEASE_ASSETS),0x147,0,0);row>=0&&(row as usize)<release.assets.len()});
        EnableWindow(control(s.settings,CHECK_UPDATES),i32::from(idle));EnableWindow(control(s.settings,DOWNLOAD),i32::from(idle&&selected_asset));EnableWindow(control(s.settings,OPEN_RELEASE),i32::from(s.release_app.is_some()));EnableWindow(control(s.settings,CANCEL_WORK),i32::from(!idle));
    }
    if s.settings_tab==2&&s.settings_app.is_none(){
        let (position,indeterminate)=if let Some(p)=&batch{(Some(if p.total==0{0}else{(p.completed*1000/p.total)as u16}),false)}else{transfer.bar()};native::progress_bar(control(s.settings,BATCH_PROGRESS),position,indeterminate,false);
        for app in 0..APPS.len(){
            let row=batch.as_ref().and_then(|p|p.rows.iter().find(|r|r.app==APPS[app].0));
            let active=row.and_then(|r|r.progress.as_ref()).or_else(||if batch.is_none()&&transfer.app.as_deref()==Some(APPS[app].0){Some(&transfer)}else{None});
            let label=if let Some(row)=row{if matches!(row.status.as_str(),"downloading"|"installing"){active.map(|v|v.label()).unwrap_or_else(||if row.status=="installing"{"Installing".into()}else{"Downloading".into()})}else{match row.status.as_str(){"queued"=>"Queued","checking"=>"Checking","installed"=>"Installed","failed"=>"Failed","skipped"=>"Skipped","not_attempted"=>"Not attempted",_=>&row.status}.into()}}else{active.map(|v|v.label()).unwrap_or_else(||overview(s,app).label().into())};
            native::set_text(control(s.settings,ROW_STATUS+app as i32),&label);
            let (position,indeterminate)=if row.is_some_and(|r|!matches!(r.status.as_str(),"checking"|"downloading"|"installing")){(Some(if row.unwrap().status=="installed"{1000}else{0}),false)}else{active.map(|v|v.bar()).unwrap_or((Some(0),false))};
            native::progress_bar(control(s.settings,ROW_PROGRESS+app as i32),position,indeterminate,row.is_some_and(|r|r.status=="failed")||active.is_some_and(|v|v.phase==Phase::Failed));
        }
        if let Some(p)=&batch{native::set_text(control(s.settings,RELEASE_DETAILS),&p.details());}else if transfer.phase==Phase::Failed{native::set_text(control(s.settings,RELEASE_DETAILS),&transfer.message);}
    }}
}

fn claim_launch_check(started:&mut bool,scanning:bool,busy:bool)->bool{if *started||scanning||busy{return false;}*started=true;true}
fn maybe_start_launch_check(s:&mut State){#[cfg(test)] if s.hidden_fixture{s.launch_check_started=true;return;}if claim_launch_check(&mut s.launch_check_started,s.scanning,s.work.is_some()){start_checks(s,None);}}
fn start_checks(s:&mut State,only:Option<usize>){
    #[cfg(test)] if s.hidden_fixture{return;}
    if s.work.is_some()||s.agent_work.is_some(){return;}s.bulk_progress=None;if only.is_none(){s.launch_check_started=true;}
    if unsafe {SetTimer(s.hwnd,WORK_TIMER,200,None)}==0{s.work_status="Could not start the release-check timer. Use Check updates to retry.".into();work_status(s);return;}
    let cancel=Arc::new(AtomicBool::new(false));let progress=Arc::new(WorkProgress::new(0));s.cancel=cancel.clone();s.progress=progress.clone();let(sender,receiver)=mpsc::sync_channel(13);
    match thread::Builder::new().name("craft-release-check".into()).spawn(move||{let mut count=0;for app in only.map(|a|vec![a]).unwrap_or_else(||(0..APPS.len()).collect()){if cancel.load(Ordering::Relaxed){break;}let result=updates::check(app);if sender.send(WorkResult::CheckedOne(app,result)).is_err(){return;}count+=1;progress.store(count as u64,Ordering::Relaxed);}let _=sender.send(WorkResult::ChecksComplete(count));}){Ok(_)=>{s.work=Some(receiver);s.work_status="Checking official releases... No files are downloaded or installed by this check.".into();work_status(s);},Err(e)=>{unsafe {KillTimer(s.hwnd,WORK_TIMER);}s.work_status=format!("Could not start update check: {e}");work_status(s);}}
}
fn start_install(s:&mut State){
    if s.work.is_some()||s.agent_work.is_some(){return;}s.bulk_progress=None;let Some(app)=s.release_app else{return;};let Some(Ok(release))=s.releases.get(&app)else{return;};let row=unsafe {SendMessageW(control(s.settings,RELEASE_ASSETS),0x147,0,0)};if row<0{report(s,"Select a verified Windows x64 portable asset first.");return;}let Some(asset)=release.assets.get(row as usize).cloned()else{return;};let tag=release.tag.clone();let asset_size=asset.size;let root=match root(s){Ok(root)=>root,Err(e)=>{report(s,&e);return;}};
    let text=format!("Download and install {}?\r\n{}\r\nSize: {} bytes. SHA-256: {}\r\nDestination: {}\r\nExisting versions are retained. After verified publication, the owned archive is deleted; failures retain staging; extraction is limited to 2 GiB. The verified new version becomes preferred unless you change that preference during installation. It will not launch automatically.",APPS[app].1,asset.name,asset.size,asset.digest,root.display());if !native::confirm(s.settings,&text){return;}if unsafe {SetTimer(s.hwnd,WORK_TIMER,200,None)}==0{report(s,"Windows could not start the install timer. No download started.");return;}
    let cancel=Arc::new(AtomicBool::new(false));let progress=Arc::new(WorkProgress::new(0));s.cancel=cancel.clone();s.progress=progress.clone();let(sender,receiver)=mpsc::sync_channel(1);
    let base=s.base.clone();let config=s.config.clone();
    match thread::Builder::new().name("craft-portable-install".into()).spawn(move||{let guard=||crate::bulk::guard(&base,&config,&root,&cancel);let result=installer::install_guarded(&root,app,&tag,&asset,&cancel,&progress,&guard).map(|installed|{let warning=actions::prefer_installed(&base,&config,&root,app,&tag,&installed.folder).err();(installed,warning)});let _=sender.send(WorkResult::Installed(root,result));}){Ok(_)=>{s.work=Some(receiver);s.work_status=format!("Downloading selected asset ({} bytes); Cancel stops before publication when possible.",asset_size);work_status(s);},Err(e)=>{unsafe {KillTimer(s.hwnd,WORK_TIMER);}report(s,&format!("Could not start install: {e}"))}}
}

fn poll_work(s:&mut State){
    let result=match s.work.as_ref().map(|r|r.try_recv()){Some(Ok(v))=>v,Some(Err(TryRecvError::Empty))=>{work_status(s);return;},Some(Err(TryRecvError::Disconnected))=>{s.work=None;s.work_status="Worker stopped. Existing versions are preserved; inspect retained staging if present.".into();work_status(s);unsafe {KillTimer(s.hwnd,WORK_TIMER);}return;},None=>return};
    if let WorkResult::CheckedOne(app,result)=result{s.releases.insert(app,result);#[cfg(not(test))] {let latest=match s.releases.get(&app){Some(Ok(release))=>release.tag.clone(),Some(Err(error))=>error.clone(),None=>String::new()};let _=core::log(&s.base,&format!("release check: app={} latest={} state={} install_available={}",APPS[app].0,latest,overview(s,app).label(),overview(s,app).install_available));}refresh_update_rows(s);if s.release_app==Some(app){show_release_details(s);}work_status(s);return;}
    s.work=None;unsafe {KillTimer(s.hwnd,WORK_TIMER);}match result{
        WorkResult::ChecksComplete(count)=>{s.work_status=format!("Checked {count} official releases. Select an app row for status, assets, or the official release page.");refresh_update_rows(s);if s.release_app.is_some(){show_release_details(s);}},
        WorkResult::CheckedOne(_,_)=>unreachable!(),
        WorkResult::Installed(at,result)=>match result{Ok((installed,selection_warning))=>{reload_after_install(s);s.work_status=format!("Installed {}. Previous versions are retained for rollback.",installed.folder.display());if let Some(warning)=selection_warning{s.work_status.push_str(&format!(" Selection warning: {warning}"));}else{s.work_status.push_str(" New version is preferred.");}if let Some(warning)=&installed.cleanup_warning{s.work_status.push_str(&format!(" Cleanup warning: {warning}"));}native::message(s.settings,&s.work_status);if root(s).ok().as_ref()==Some(&at){refresh(s);}else{status(s,&format!("Installation completed in prior apps folder {}. Current root unchanged.",at.display()));}},Err(error)=>{s.work_status=error.clone();native::message(s.settings,&error);}},
    }work_status(s);
}
fn save_app_options(s:&mut State)->bool {
    if s.options_conflict {
        if unsafe{MessageBoxW(s.settings,wide("These draft options overlap settings changed by an agent. Apply the displayed drafts against the latest settings? Choose No to keep reviewing, or close and reopen to discard drafts.").as_ptr(),wide("Rebase changed options").as_ptr(),0x4|0x20|0x100)}!=6{return false;}
        s.options_conflict=false;
    }
    let Some(app)=s.settings_app else{return false;};
    let args=native::text(control(s.settings,ARGS));if let Err(error)=core::parse_args(&args){report(s,&error);return false;}
    let row=unsafe {SendMessageW(control(s.settings,VERSIONS),0x147,0,0)};
    let mut candidate=s.config.clone();candidate.set_device_arguments(&s.settings_profile,APPS[app].0,args);
    if !s.scanning&&row>=0 {if let Some(install)=s.settings_versions.get(row as usize).and_then(|index|s.installs.get(*index)).filter(|install|install.app==app){if let Some(root)=&s.canonical_root{candidate.selections.insert(APPS[app].0.into(),core::portable_path(root,&install.folder));}}}
    if app==4 {
        let enabled=unsafe {SendMessageW(control(s.settings,COMPAT),0xF0,0,0)==1};
        let path=native::text(control(s.settings,EXCLUDED));
        let obsidian=if path.trim().is_empty(){None}else{match core::normalize(&PathBuf::from(path.trim())).and_then(|p|core::checked_path(&p).map(|_|p)){Ok(p)if p.is_dir()=>Some(p),Ok(_)=>{report(s,"Choose an Obsidian installation folder.");return false;},Err(error)=>{report(s,&error);return false;}}};
        if enabled && obsidian.is_none(){report(s,"Choose the exact Obsidian folder before enabling FilmCraft compatibility.");return false;}
        if let Some(identity)=s.machine.as_deref(){let obsidian=obsidian.map(|path|core::portable_path(&s.base,&path));candidate.set_device_compatibility(&s.settings_profile,identity,APPS[app].0,Profile{enabled,obsidian});}else if enabled{report(s,"Machine identity is unavailable; compatibility stays disabled.");return false;}
    }
    if persist(s,candidate){update_tiles(s);status(s,&format!("{} options saved.",APPS[app].1));true}else{false}
}
fn capture_launcher_drafts(s:&mut State){if s.settings!=0&&s.settings_app.is_none()&&control(s.settings,ROOT)!=0{s.root_draft=Some(native::text(control(s.settings,ROOT)));s.profile_draft=native::text(control(s.settings,DEVICE_NAME));}}
fn close_settings(s:&mut State) {capture_launcher_drafts(s);discard_conflicts(s);if s.settings!=0 {let window=s.settings;let foreground=native::is_foreground_window(window);s.settings=0;OPTIONS.with(|v|v.set(0));s.settings_app=None;s.release_app=None;unsafe {DestroyWindow(window);}if foreground{native::activate_window(s.hwnd);}}}
unsafe fn design_child(parent:Hwnd,class:&str,title:&str,style:u32,x:i32,y:i32,w:i32,h:i32,id:i32,font:Handle,surface:bool)->Hwnd {
    // New child windows start at the bottom: mark only decoration cards for explicit lowering after all children exist.
    let handle=child(parent,class,title,style|0x04000000,x,y,w,h,id,font);native::design_bounds(handle,x,y,w,h,surface);
    if class=="STATIC"&&style&0x1F==0xD&&title.is_empty()&&id==0{native::design_background(handle);}handle
}
fn settings_window(s:&mut State,title:&str)->Hwnd {
    close_settings(s);
    unsafe {let style=0x00CF0000|0x02000000;let mut rect=Rect{left:0,top:0,right:1000,bottom:680};AdjustWindowRectEx(&mut rect,style,0,0);let mut area=Rect{left:0,top:0,right:1000,bottom:680};SystemParametersInfoW(0x30,0,(&mut area as *mut Rect).cast(),0);
        let width=(rect.right-rect.left).min(area.right-area.left);let height=(rect.bottom-rect.top).min(area.bottom-area.top);
        let hwnd=CreateWindowExW(0,wide("CraftLauncherSettings").as_ptr(),wide(title).as_ptr(),style,area.left+(area.right-area.left-width)/2,area.top+(area.bottom-area.top-height)/2,width,height,s.hwnd,0,GetModuleHandleW(ptr::null()),ptr::null_mut());
        if hwnd==0{report(s,"Windows could not open Settings.");return 0;}s.settings=hwnd;OPTIONS.with(|v|v.set(hwnd));hwnd
    }
}
fn finish_settings(s:&State,focus:i32){refresh_update_rows(s);theme_settings(s.settings,s.config.theme);native::layout_settings(s.settings);work_status(s);native::resume_settings_redraw(s.settings);#[cfg(test)] if s.hidden_fixture{return;}unsafe {ShowWindow(s.settings,5);SetFocus(control(s.settings,focus));}}
fn open_settings(s:&mut State,app:Option<usize>) {s.options_conflict=false;if app.is_some(){s.settings_profile=s.device.clone();}
    capture_launcher_drafts(s);
    let title=app.map(|i|format!("{} settings - {} profile",APPS[i].1,s.device)).unwrap_or_else(||"Craft App Launcher settings".into());
    let hwnd=if s.settings!=0&&s.settings_app==app{s.settings}else{settings_window(s,&title)};if hwnd==0{return;}s.settings_app=app;
    unsafe {SendMessageW(hwnd,0xB,0,0);native::clear_settings_page(hwnd);native::set_text(hwnd,&title);let f=s.font;let heading=s.name_font;
        design_child(hwnd,"STATIC",&title,0,24,18,850,32,0,heading,false);design_child(hwnd,"BUTTON","Done",0x10000,886,16,90,34,CLOSE,f,false);
        if let Some(app)=app {
            design_child(hwnd,"STATIC","Installed version (shared by all profiles)",0,32,80,930,26,0,heading,false);
            design_child(hwnd,"COMBOBOX","",0x10000|3|0x200000,32,114,936,220,VERSIONS,f,false);
            design_child(hwnd,"STATIC","Launch arguments for this profile - one literal argument per line",0,32,172,930,26,0,heading,false);
            design_child(hwnd,"EDIT","",0x10000|4|0x40|0x1000|0x200000,32,210,936,152,ARGS,f,false);
            design_child(hwnd,"STATIC","Compatibility for this profile and computer",0,32,392,930,26,0,heading,false);
            if app==4 {
                design_child(hwnd,"BUTTON","Enable DX12 + exact Obsidian PATH exclusion",0x10000|3,32,434,936,28,COMPAT,f,false);
                design_child(hwnd,"EDIT","",0x10000|0x80,32,478,730,34,EXCLUDED,f,false);design_child(hwnd,"BUTTON","Choose folder",0x10000,778,478,190,34,PICK_EXCLUDED,f,false);
                design_child(hwnd,"STATIC","Default off. Only the launched child's environment changes.",0,32,530,936,28,0,f,false);
                let profile=s.config.device_compatibility(&s.device,s.machine.as_deref(),"filmcraft");SendMessageW(control(hwnd,COMPAT),0xF1,usize::from(profile.enabled),0);if let Some(path)=profile.obsidian{if let Ok(path)=core::resolve_path(&s.base,&path){native::set_text(control(hwnd,EXCLUDED),&path.to_string_lossy());}}if s.machine.is_none(){for id in [COMPAT,EXCLUDED,PICK_EXCLUDED]{EnableWindow(control(hwnd,id),0);}}
            }else{design_child(hwnd,"STATIC","No compatibility override for this app. Arguments above are profile-specific.",0,32,434,936,60,0,f,false);}
            design_child(hwnd,"BUTTON","Cancel",0x10000,742,616,106,36,CLOSE,f,false);design_child(hwnd,"BUTTON","Save",0x10000,864,616,104,36,SAVE_APP,f,false);
            fill_app_versions(s);native::set_text(control(hwnd,ARGS),s.config.device_arguments(&s.device,APPS[app].0));finish_settings(s,VERSIONS);return;
        }
        for (page,(id,label)) in [(TAB_LAUNCHER,"Launcher Settings"),(TAB_APPEARANCE,"Appearance"),(TAB_UPDATES,"Updates")].into_iter().enumerate(){let tab=design_child(hwnd,"BUTTON",label,0x10000,24+page as i32*320,66,312,38,id,f,false);native::selected_button(tab,page==s.settings_tab);}
        match s.settings_tab {
            0=>{
                s.release_app=None;
                design_child(hwnd,"STATIC","Device profile",0,32,138,920,28,0,heading,false);
                let combo=design_child(hwnd,"COMBOBOX","",0x10000|3|0x200000,32,180,430,220,DEVICE,f,false);let names=s.config.device_names();for name in &names{SendMessageW(combo,0x143,0,wide(name).as_ptr()as isize);}SendMessageW(combo,0x14E,names.iter().position(|n|n==&s.device).unwrap_or(0),0);
                design_child(hwnd,"STATIC",if s.machine.is_some(){"Selection remembered on this computer."}else{"Session selection only; compatibility disabled."},0,482,184,486,28,0,f,false);
                design_child(hwnd,"STATIC","New profile name",0,32,234,200,24,0,f,false);design_child(hwnd,"EDIT","",0x10000|0x80,32,266,730,34,DEVICE_NAME,f,false);SendMessageW(control(hwnd,DEVICE_NAME),0xC5,40,0);native::set_text(control(hwnd,DEVICE_NAME),&s.profile_draft);design_child(hwnd,"BUTTON","Create",0x10000,778,266,190,34,CREATE_DEVICE,f,false);
                design_child(hwnd,"STATIC","Apps folder (shared by every profile)",0,32,340,670,28,0,heading,false);design_child(hwnd,"BUTTON","Open Apps Folder",0x10000,738,338,230,32,OPEN_APPS_FOLDER,f,false);
                design_child(hwnd,"EDIT","",0x10000|0x80,32,382,650,36,ROOT,f,false);design_child(hwnd,"BUTTON","Browse",0x10000,698,382,126,36,BROWSE,f,false);design_child(hwnd,"BUTTON","Scan",0x10000,840,382,128,36,SAVE_ROOT,f,false);
                design_child(hwnd,"EDIT",&s.details,4|0x800|0x40|0x200000,32,442,936,144,DETAILS,f,false);if let Some(draft)=s.root_draft.as_ref(){native::set_text(control(hwnd,ROOT),draft);}else if let Ok(path)=root(s){native::set_text(control(hwnd,ROOT),&path.to_string_lossy());}
                design_child(hwnd,"STATIC","Right-click an app tile for that profile's launch arguments and compatibility.",0,32,614,936,40,0,f,false);finish_settings(s,DEVICE);
            },
            1=>{
                s.release_app=None;design_child(hwnd,"STATIC","Appearance",0,32,142,930,30,0,heading,false);design_child(hwnd,"STATIC","System follows Windows app appearance. Four explicit themes are available.",0,32,186,930,30,0,f,false);
                for (row,(id,label)) in [(THEME_SYSTEM,"System - automatic"),(THEME_LIGHT,"Light"),(THEME_DARK,"Dark"),(THEME_TRUE_DARK,"True Dark"),(THEME_AETHER,"Aetherglass")].into_iter().enumerate(){design_child(hwnd,"BUTTON",label,0x10000,32,242+row as i32*64,936,46,id,f,false);}finish_settings(s,THEME_SYSTEM);
            },
            _=>{
                design_child(hwnd,"BUTTON","Check updates",0x10000,32,110,164,28,CHECK_UPDATES,f,false);design_child(hwnd,"BUTTON","Update All",0x10000,208,110,142,28,UPDATE_ALL,f,false);design_child(hwnd,"STATIC","",0,362,110,464,22,UPDATE_STATUS,f,false);design_child(hwnd,"BUTTON","Cancel",0x10000,840,110,128,28,CANCEL_WORK,f,false);
                native::init_progress_controls();design_child(hwnd,"msctls_progress32","",8,362,135,464,4,BATCH_PROGRESS,f,false);
                for(text,x,w)in[("App",32,142),("Installed / preferred",186,226),("Latest",426,90),("Progress / state",530,280),("Action",824,144)]{design_child(hwnd,"STATIC",text,0,x,142,w,24,0,f,false);}
                for app in 0..APPS.len(){let y=168+app as i32*30;design_child(hwnd,"STATIC",APPS[app].1,0,32,y,142,26,0,f,false);design_child(hwnd,"STATIC","Not checked",0,426,y,90,26,ROW_LATEST+app as i32,f,false);design_child(hwnd,"STATIC","Not checked",0,530,y,280,20,ROW_STATUS+app as i32,f,false);design_child(hwnd,"COMBOBOX","",0x10000|3|0x200000,186,y,226,220,PREFERRED+app as i32,f,false);design_child(hwnd,"BUTTON","Details...",0x10000,824,y,144,24,APP_ROW+app as i32,f,false);design_child(hwnd,"msctls_progress32","",8,530,y+22,280,5,ROW_PROGRESS+app as i32,f,false);}
                design_child(hwnd,"STATIC","Select an app's Details or Install action",0,32,566,680,28,RELEASE_TITLE,heading,false);
                design_child(hwnd,"EDIT","",4|0x800|0x40|0x200000,32,598,936,32,RELEASE_DETAILS,f,false);
                design_child(hwnd,"COMBOBOX","",0x10000|3|0x200000,32,636,600,180,RELEASE_ASSETS,f,false);design_child(hwnd,"BUTTON","Install selected...",0x10000,648,636,320,30,DOWNLOAD,f,false);
                design_child(hwnd,"BUTTON","Official release page",0x10000,738,566,230,28,OPEN_RELEASE,f,false);fill_preferred(s);show_release_details(s);finish_settings(s,CHECK_UPDATES);
            }
        }
    }
}
fn show_release_details(s:&State){
    if s.settings==0||s.settings_app.is_some()||s.settings_tab!=2{return;}
    let Some(app)=s.release_app else{work_status(s);return;};let mut information=String::new();let mut assets=Vec::new();
    match s.releases.get(&app){Some(Ok(release))=>{information.push_str(&format!("Official stable {}. {}\r\n",release.tag,release.asset_note));if let Some(index)=chosen(s,app){information.push_str(&updates::compare_installed(&s.installs[index].version,release));}else if s.installs.iter().any(|i|i.app==app){information.push_str("Choose a preferred installed version for a conservative comparison.");}else{information.push_str("Not installed.");}assets=release.assets.iter().map(|asset|format!("{} - {} bytes",asset.name,asset.size)).collect();},Some(Err(error))=>information=format!("Check failed: {error}"),None=>information="Not checked. Check updates to request official release metadata.".into()}
    information.push_str(&format!("\r\n{}",s.work_status));
    native::set_text(control(s.settings,RELEASE_TITLE),&format!("{} - release details",APPS[app].1));native::set_text(control(s.settings,RELEASE_DETAILS),&information);
    unsafe{let combo=control(s.settings,RELEASE_ASSETS);SendMessageW(combo,0x14B,0,0);for asset in assets{SendMessageW(combo,0x143,0,wide(asset).as_ptr()as isize);}}work_status(s);
}
fn open_release(s:&mut State,app:usize){
    s.release_app=Some(app);if s.settings==0||s.settings_app.is_some()||s.settings_tab!=2{s.settings_tab=2;open_settings(s,None);s.release_app=Some(app);}show_release_details(s);unsafe{SetFocus(control(s.settings,RELEASE_ASSETS));}
}
fn app_menu(s:&mut State,app:usize,x:i32,y:i32) {
    unsafe {
        let menu=CreatePopupMenu();if menu==0{return;}
        let selected=chosen(s,app);let enabled=selected.filter(|_|!s.scanning);
        let gui=enabled.and_then(|index|s.installs[index].gui.as_ref()).is_some();
        AppendMenuW(menu,if gui{0}else{1},MENU_GUI,wide(format!("Launch {}",APPS[app].1)).as_ptr());

        let choices:Vec<_>=s.installs.iter().enumerate().filter(|(_,i)|i.app==app).collect();
        if !choices.is_empty(){let versions=CreatePopupMenu();for (index,install) in choices.iter().take(30){let label=format!("{}  -  {}",install.version,install.folder.file_name().unwrap_or_default().to_string_lossy()).replace('&',"&&");AppendMenuW(versions,if selected==Some(*index){8}else{0},MENU_VERSION+*index,wide(label).as_ptr());}AppendMenuW(menu,0x10,versions as usize,wide("Installed version").as_ptr());}
        AppendMenuW(menu,0x800,0,ptr::null());AppendMenuW(menu,0,MENU_OPTIONS,wide("App settings / arguments...").as_ptr());
        let mut point=Point{x,y};if x==-1 && y==-1{let mut rect:Rect=std::mem::zeroed();GetWindowRect(control(s.hwnd,TILE+app as i32),&mut rect);point=Point{x:rect.left+16,y:rect.top+16};}
        let command=TrackPopupMenu(menu,0x100|2,point.x,point.y,0,s.hwnd,ptr::null()) as usize;DestroyMenu(menu);
        match command{MENU_GUI=>launch_app(s,app,false),MENU_OPTIONS=>open_settings(s,Some(app)),id if id>=MENU_VERSION=>{choose_version(s,app,id-MENU_VERSION);},_=>()}
    }
}
unsafe extern "system" fn settings_proc(hwnd:Hwnd,msg:u32,w:usize,l:isize)->Lresult {
    let _deferred_wake=AgentWakeGuard;
    match msg {
        0x5=>{native::layout_settings(hwnd);0},
        0x24=>{if l!=0{let limits=&mut*(l as *mut MinMaxInfo);let mut rect=Rect{left:0,top:0,right:1000,bottom:680};AdjustWindowRectEx(&mut rect,0x00CF0000|0x02000000,0,0);limits.min_track=Point{x:rect.right-rect.left,y:rect.bottom-rect.top};}0},
        0x2B=>{if l!=0{let item=&*(l as *const DrawItem);if item.control_type==4{native::draw_plain_button(item);return 1;}if item.control_type==5{native::draw_settings_static(item);return 1;}}DefWindowProcW(hwnd,msg,w,l)},
        0x14=>native::erase_background(hwnd,w as Handle),
        0x133..=0x138=>native::control_color(w as Handle,msg,l as Hwnd),
        0x15|0x1A|0x31A|0x31E=>{theme_changed();DefWindowProcW(hwnd,msg,w,l)},
        0x111=>{let id=(w&65535)as i32;let notification=w>>16;if notification==0x100||notification==3||notification==6{return 0;}
            STATE.with(|slot|if let Ok(mut state)=slot.try_borrow_mut(){if let Some(s)=state.as_mut(){
                if id==RELEASE_ASSETS&&notification==1{work_status(s);return;}
                if id==DEVICE&&notification==1 {
                    let row=SendMessageW(control(hwnd,DEVICE),0x147,0,0);let names=s.config.device_names();
                    if let Some(name)=usize::try_from(row).ok().and_then(|i|names.get(i)) {
                        let mut candidate=s.config.clone();if let Some(identity)=s.machine.as_ref(){candidate.machine_devices.insert(identity.clone(),name.clone());}
                        if s.machine.is_none()||persist(s,candidate){s.device=name.clone();status(s,&format!("Active profile: {}",s.device));}
                        SendMessageW(control(hwnd,DEVICE),0x14E,names.iter().position(|n|n==&s.device).unwrap_or(0),0);
                    }return;
                }
                if (PREFERRED..PREFERRED+APPS.len() as i32).contains(&id)&&notification==1 {let app=(id-PREFERRED)as usize;let row=SendMessageW(control(hwnd,id),0x147,0,0);if row>=0{if let Some(index)=s.preferred_rows[app].get(row as usize).copied(){choose_version(s,app,index);}}return;}
                if notification!=0{return;}if (APP_ROW..APP_ROW+APPS.len() as i32).contains(&id){open_release(s,(id-APP_ROW)as usize);return;}
                match id{
                    TAB_LAUNCHER|TAB_APPEARANCE|TAB_UPDATES=>{s.settings_tab=(id-TAB_LAUNCHER)as usize;open_settings(s,None);},
                    CREATE_DEVICE=>{
                        let mut candidate=s.config.clone();match candidate.create_device(&native::text(control(hwnd,DEVICE_NAME))){
                            Ok(name)=>{if let Some(identity)=s.machine.as_ref(){candidate.machine_devices.insert(identity.clone(),name.clone());}if persist(s,candidate){s.device=name;s.profile_draft.clear();native::set_text(control(hwnd,DEVICE_NAME),"");open_settings(s,None);}},
                            Err(error)=>report(s,&error)
                        }
                    },
                    THEME_SYSTEM|THEME_LIGHT|THEME_DARK|THEME_TRUE_DARK|THEME_AETHER=>{let mut candidate=s.config.clone();candidate.theme=match id{THEME_LIGHT=>ThemeMode::Light,THEME_DARK=>ThemeMode::Dark,THEME_TRUE_DARK=>ThemeMode::TrueDark,THEME_AETHER=>ThemeMode::Aetherglass,_=>ThemeMode::System};if persist(s,candidate){apply_theme(s.config.theme);}},
                    BROWSE=>choose_root(s),SAVE_ROOT=>save_root_text(s),SAVE_APP=>{if save_app_options(s){close_settings(s);}},
                    PICK_EXCLUDED=>if let Some(path)=native::folder_picker(hwnd,"Choose the exact Obsidian installation folder"){native::set_text(control(hwnd,EXCLUDED),&path.to_string_lossy());},
                    UPDATE_ALL=>{let request=actions::Request{protocol:1,id:format!("gui-bulk-{}",native::random_id().unwrap_or_else(|_|s.scan_generation.to_string())),action:actions::Action::UpdateAll};let response=begin_agent_work(s,&request);if !response.ok{report(s,response.result["message"].as_str().unwrap_or("Could not start Update All"));}work_status(s);},
                    OPEN_APPS_FOLDER=>{let view=agent_view(s);let result=actions::execute(&s.base,&mut s.config,&mut s.observed,s.machine.as_deref(),&s.installs,&actions::Action::OpenAppsFolder,&mut s.receipts,&AtomicBool::new(false),&WorkProgress::new(0),&view);if let Err(e)=result{report(s,&e);}},
                    CHECK_UPDATES=>start_checks(s,None),DOWNLOAD=>start_install(s),OPEN_RELEASE=>if let Some(app)=s.release_app{if let Err(error)=native::official_release_page(hwnd,app){report(s,&error);}},
                    CANCEL_WORK=>{s.cancel.store(true,Ordering::Relaxed);if let Some(w)=&s.agent_work{w.cancel.store(true,Ordering::Relaxed);}s.work_status="Cancellation requested. An in-flight network operation can take up to its timeout. Staging is retained.".into();work_status(s);},
                    CLOSE=>close_settings(s),_=>()
                }
            }});0}
        0x10=>{STATE.with(|slot|if let Ok(mut state)=slot.try_borrow_mut(){if let Some(s)=state.as_mut(){close_settings(s);}});0}
        _=>DefWindowProcW(hwnd,msg,w,l),
    }
}
fn sync_agent_controls(s:&mut State,old:&Config){
    if old.theme!=s.config.theme{apply_theme(s.config.theme);}
    let new_device=s.config.selected_device(s.machine.as_deref()).to_string();
    let root_changed=old.root!=s.config.root;
    if s.settings!=0{
        let root_control=control(s.settings,ROOT);
        if root_control!=0&&root_changed{
            let before=old.root.as_ref().and_then(|p|core::resolve_path(&s.base,p).ok()).map(|p|p.to_string_lossy().into_owned()).unwrap_or_default();
            let after=s.config.root.as_ref().and_then(|p|core::resolve_path(&s.base,p).ok()).map(|p|p.to_string_lossy().into_owned()).unwrap_or_default();
            if native::text(root_control)==before{native::set_text(root_control,&after);s.root_draft=Some(after);}else{s.root_conflict=true;}
        }
        if let Some(app)=s.settings_app{
            let id=APPS[app].0;
            let old_args=old.device_arguments(&s.settings_profile,id);
            let new_args=s.config.device_arguments(&s.settings_profile,id);
            let args=control(s.settings,ARGS);
            if old_args!=new_args&&args!=0{if native::text(args)==old_args{native::set_text(args,new_args);}else{s.options_conflict=true;}}
            if app==4{
                let before=old.device_compatibility(&s.settings_profile,s.machine.as_deref(),id);
                let after=s.config.device_compatibility(&s.settings_profile,s.machine.as_deref(),id);
                let path_text=|p:Option<PathBuf>|p.and_then(|p|core::resolve_path(&s.base,&p).ok()).map(|p|p.to_string_lossy().into_owned()).unwrap_or_default();
                let before_path=path_text(before.obsidian);let after_path=path_text(after.obsidian);
                let excluded=control(s.settings,EXCLUDED);if before_path!=after_path&&excluded!=0{if native::text(excluded)==before_path{native::set_text(excluded,&after_path);}else{s.options_conflict=true;}}
                let check=control(s.settings,COMPAT);if before.enabled!=after.enabled&&check!=0{let displayed=unsafe{SendMessageW(check,0xF0,0,0)==1};if displayed==before.enabled{unsafe{SendMessageW(check,0xF1,usize::from(after.enabled),0);}}else{s.options_conflict=true;}}
            }
            if old.selections.get(id)!=s.config.selections.get(id)||root_changed{
                let row=unsafe{SendMessageW(control(s.settings,VERSIONS),0x147,0,0)};
                let displayed=usize::try_from(row).ok().and_then(|r|s.settings_versions.get(r)).copied();
                let previous=s.canonical_root.as_ref().and_then(|r|core::selected_install(&s.installs,app,r,old.selections.get(id)));
                if displayed==previous&&!root_changed{fill_app_versions(s);}else{s.options_conflict=true;}
            }
            s.device=new_device.clone(); // Draft fields retain their separate explicit settings_profile.
        }else{
            s.device=new_device.clone();
            let device=control(s.settings,DEVICE);
            if device!=0{unsafe{SendMessageW(device,0x14B,0,0);let names=s.config.device_names();for name in &names{SendMessageW(device,0x143,0,wide(name).as_ptr()as isize);}SendMessageW(device,0x14E,names.iter().position(|n|n==&new_device).unwrap_or(0),0);}}
            if root_changed||old.selections!=s.config.selections{fill_preferred(s);}
        }
    }else{s.device=new_device;if root_changed{let before=old.root.as_ref().and_then(|p|core::resolve_path(&s.base,p).ok()).map(|p|p.to_string_lossy().into_owned());if s.root_draft.is_some()&&s.root_draft!=before{s.root_conflict=true;}else{s.root_draft=None;}}}
    if s.options_conflict||s.root_conflict{status(s,DRAFT_CONFLICT);}
    if !root_changed&&old.selections!=s.config.selections{update_tiles(s);}
    if root_changed{refresh(s);}
}
fn agent_view(s:&State)->actions::View{
    let discovery=if s.scanning{actions::Discovery::Scanning}else if let Some(message)=&s.discovery_error{actions::Discovery::Unavailable{message:message.clone()}}else if s.canonical_root.is_none(){actions::Discovery::NotRequested}else{
        match root(s).and_then(|r|core::checked_path(&r)){
            Ok(r)if s.canonical_root.as_ref()==Some(&r)=>actions::Discovery::Ready,
            Ok(_)=>actions::Discovery::Unavailable{message:"Installed list belongs to another root".into()},
            Err(message)=>actions::Discovery::Unavailable{message},
        }
    };
    let work=if let Some(w)=&s.agent_work{serde_json::json!({"status":"running","request_id":w.request.id,"progress":w.progress.load(Ordering::Relaxed),"transfer":w.progress.snapshot(),"bulk":s.bulk_progress.as_ref().map(|p|batch_snapshot(s,p))})}else if s.work.is_some(){serde_json::json!({"status":"running","source":"gui","progress":s.progress.load(Ordering::Relaxed),"transfer":s.progress.snapshot()})}else{serde_json::json!({"status":"idle"})};
    actions::View{owner:"gui",discovery,work}
}
fn begin_agent_work(s:&mut State,request:&actions::Request)->actions::Response{
    if s.config_error.is_some(){return actions::Response::error(&request.id,"settings_unavailable","Repair and reload settings first");}
    if matches!(request.action,actions::Action::UpdateAll)&&s.scanning{return actions::Response::error(&request.id,"owner_busy","Wait for discovery to finish");}
            if s.agent_work.is_some()||s.work.is_some(){actions::Response::error(&request.id,"owner_busy","An update/install worker is already active")}else{
                if unsafe{SetTimer(s.hwnd,AGENT_WORK_TIMER,200,None)}==0{return actions::Response::error(&request.id,"not_executed","Could not start active-worker completion fallback");}
                let (tx,rx)=mpsc::sync_channel(1);let wake=s.hwnd;let base=s.base.clone();let config=s.config.clone();let r=request.clone();let bulk=Arc::new(std::sync::Mutex::new(crate::bulk::Progress::default()));s.bulk_progress=if matches!(request.action,actions::Action::UpdateAll){Some(bulk.clone())}else{None};
                let cancel=Arc::new(AtomicBool::new(false));let progress=Arc::new(WorkProgress::new(0));s.progress=progress.clone();let worker_cancel=cancel.clone();let worker_progress=progress.clone();
                match thread::Builder::new().name("craft-agent-action".into()).spawn(move||{
                    let mut release_result=None;let mut installed_root=None;
                    let value=match &r.action{
                        actions::Action::UpdateAll=>{let value=crate::bulk::run(&base,&config,&worker_cancel,&worker_progress,&bulk);if value.is_ok(){installed_root=actions::apps_folder(&base,&config).ok();}value},
                        actions::Action::ReleasesCheck{app}=>match actions::app_id(app){
                            Ok(index)=>{let result=updates::check(index);let value=result.as_ref().map(|v|actions::release_value(app,v)).map_err(Clone::clone);release_result=Some((index,result));value},Err(e)=>Err(e)
                        },
                        actions::Action::InstallPlan{app,tag,asset}=>actions::make_plan(&base,&config,app,tag,asset).and_then(|v|serde_json::to_value(v).map_err(|e|e.to_string())),
                        actions::Action::InstallConfirm{plan}=>{let value=actions::confirm(&base,&config,plan,&worker_cancel,&worker_progress);if value.is_ok(){installed_root=Some(PathBuf::from(&plan.root));}value},
                        _=>Err("Action is not a worker operation".into()),
                    };
                    let _=tx.send(AgentFinished{response:actions::response(&r.id,value),release:release_result,installed_root});if unsafe{PostMessageW(wake,AGENT_WAKE,0,0)}==0{let _=core::log(&base,"Agent completion wake failed; active-worker timer will collect the retained result.");}
                }){
                    Ok(_)=>{s.agent_work=Some(AgentWork{request:request.clone(),receiver:rx,cancel,progress});actions::Response::success(&request.id,serde_json::json!({"status":"running","request_id":request.id}))},
                    Err(e)=>{s.bulk_progress=None;unsafe{KillTimer(s.hwnd,AGENT_WORK_TIMER);}actions::Response::error(&request.id,"worker_failed",e)},
                }
            }
}
fn reload_after_install(s:&mut State){
    match actions::load(&s.base){Ok((config,observed))=>{let old=s.config.clone();s.config=config;s.observed=observed;sync_agent_controls(s,&old);},Err(error)=>{s.config_error=Some(error.clone());status(s,&format!("Installed; settings reload failed: {error}"));}}
}
fn poll_agent(s:&mut State){
    work_status(s);
    let finished=s.agent_work.as_ref().and_then(|w|match w.receiver.try_recv(){Ok(r)=>Some(r),Err(TryRecvError::Disconnected)=>Some(AgentFinished{response:actions::Response::error(&w.request.id,"unknown","Worker ended without a result; reconcile manually"),release:None,installed_root:None}),Err(TryRecvError::Empty)=>None});
    if let Some(mut finished)=finished{unsafe{KillTimer(s.hwnd,AGENT_WORK_TIMER);}if let Some(work)=s.agent_work.take(){
        if let Some((app,result))=finished.release{
            finished.response=actions::response(&work.request.id,actions::checked_release_value(&s.base,&s.config,&s.installs,&agent_view(s).discovery,APPS[app].0,&result));
            s.releases.insert(app,result);refresh_update_rows(s);if s.release_app==Some(app){show_release_details(s);}s.work_status=format!("Agent checked {} official release.",APPS[app].1);work_status(s);
        }
        if let Some(installed_root)=finished.installed_root{reload_after_install(s);if root(s).and_then(|p|core::checked_path(&p)).ok().as_ref()==Some(&installed_root){refresh(s);}else{status(s,"Agent installation completed in the prior apps root; current preferences were retained.");}}
        if matches!(work.request.action,actions::Action::UpdateAll){s.work_status=finished.response.result["summary"].as_str().or_else(||finished.response.result["message"].as_str()).unwrap_or("Update All outcome unknown; reconcile manually").into();if !finished.response.ok{s.bulk_progress=None;}status(s,&s.work_status);if s.settings!=0{native::completion_report(s.settings,&s.work_status,crate::bulk::completion_rows(&finished.response.result));}work_status(s);}
        s.receipts.record(&work.request,finished.response);
    }}
    for _ in 0..8{
        let command=match s.agent.as_ref().and_then(|a|a.receiver.try_recv().ok()){Some(v)=>v,None=>break};
        if s.closing{let _=command.reply.try_send(command.dispatch.stopped_response(&command.request.id,"Owner is closing"));continue;}
        if !command.dispatch.try_start(){let _=command.reply.try_send(command.dispatch.stopped_response(&command.request.id,"Dispatch deadline expired or request was canceled"));continue;}
        let request=command.request;
        let response=if let Some(response)=s.receipts.lookup(&request){response}else if let Some(work)=s.agent_work.as_ref().filter(|w|w.request.id==request.id){
            if serde_json::to_vec(&work.request.action).ok()==serde_json::to_vec(&request.action).ok(){actions::Response::success(&request.id,serde_json::json!({"status":"running","request_id":request.id}))}else{actions::Response::error(&request.id,"request_id_conflict","ID belongs to another running action")}
        }else if let actions::Action::OperationStatus{request_id}=&request.action{
            let value=if let Some(w)=s.agent_work.as_ref().filter(|w|w.request.id==*request_id){serde_json::json!({"status":"running","progress":w.progress.load(Ordering::Relaxed),"transfer":w.progress.snapshot(),"bulk":s.bulk_progress.as_ref().map(|p|batch_snapshot(s,p))})}else{s.receipts.status(request_id)};
            actions::Response::success(&request.id,value)
        }else if let actions::Action::Cancel{request_id}=&request.action{
            if let Some(w)=s.agent_work.as_ref().filter(|w|w.request.id==*request_id){w.cancel.store(true,Ordering::Relaxed);actions::Response::success(&request.id,serde_json::json!({"status":"cancellation_requested"}))}else{actions::Response::error(&request.id,"unknown","No active GUI operation; reconcile manually")}
        }else if actions::needs_worker(&request.action){
            begin_agent_work(s,&request)
        }else{
            let old=s.config.clone();let view=agent_view(s);
            let mut value=if s.config_error.is_some()&&!matches!(&request.action,actions::Action::Reload){Err("Settings could not be loaded; repair and reload before actions".into())}else if s.scanning&&actions::needs_installations(&request.action){Err("Discovery is running; retry after it settles".into())}else{actions::execute(&s.base,&mut s.config,&mut s.observed,s.machine.as_deref(),&s.installs,&request.action,&mut s.receipts,&AtomicBool::new(false),&WorkProgress::new(0),&view)};
            if value.is_ok(){if matches!(&request.action,actions::Action::Reload){s.config_error=None;}if old.serialize()!=s.config.serialize(){sync_agent_controls(s,&old);}if matches!(&request.action,actions::Action::State|actions::Action::Patch{..}|actions::Action::Reload){value=Ok(actions::state(&s.base,&s.config,s.machine.as_deref(),&s.observed,&agent_view(s)));}}
            let response=actions::response(&request.id,value);s.receipts.record(&request,response.clone());response
        };
        let _=command.reply.try_send(response);
    }
}
unsafe extern "system" fn window_proc(hwnd:Hwnd,msg:u32,w:usize,l:isize)->Lresult {
    let _deferred_wake=AgentWakeGuard;
    match msg {
        0x14=>native::erase_launcher_background(hwnd,w as Handle),
        0xF=>{native::paint_launcher(hwnd);0}
        0x133..=0x138=>native::control_color(w as Handle,msg,l as Hwnd),
        0x15|0x1A|0x31A|0x31E=>{theme_changed();DefWindowProcW(hwnd,msg,w,l)},
        0x5=>{if w!=1{layout(hwnd);}0}
        0x24=>{if l!=0{let limits=&mut*(l as *mut MinMaxInfo);limits.min_track=Point{x:800,y:640};}0}
        0x2B=>{if l!=0{let item=&*(l as *const DrawItem);if item.control_type==4 && (TILE as u32..TILE as u32+APPS.len() as u32).contains(&item.control_id){native::draw_app_tile(item);return 1;}if item.control_type==4{native::draw_plain_button(item);return 1;}}DefWindowProcW(hwnd,msg,w,l)}
        AGENT_WAKE=>{AGENT_PENDING.with(|p|p.set(true));0}
        0x113 if w==AGENT_WORK_TIMER=>{AGENT_PENDING.with(|p|p.set(true));0}
        0x113 if w==WORK_TIMER=>{STATE.with(|slot|if let Ok(mut state)=slot.try_borrow_mut(){if let Some(s)=state.as_mut(){poll_work(s);}});0}
        0x113 if w==TIMER=>{STATE.with(|slot|if let Ok(mut state)=slot.try_borrow_mut(){if let Some(s)=state.as_mut(){poll_scan(s);}});0}
        0x7B=>{let id=GetDlgCtrlID(w as Hwnd);if (TILE..TILE+APPS.len() as i32).contains(&id){let x=(l as u32&65535)as i16 as i32;let y=((l as u32>>16)&65535)as i16 as i32;STATE.with(|slot|if let Ok(mut state)=slot.try_borrow_mut(){if let Some(s)=state.as_mut(){app_menu(s,(id-TILE)as usize,x,y);}});return 0;}DefWindowProcW(hwnd,msg,w,l)}
        0x111=>{let id=(w&65535)as i32;if w>>16!=0{return 0;}STATE.with(|slot|if let Ok(mut state)=slot.try_borrow_mut(){if let Some(s)=state.as_mut(){if id==SETTINGS{open_settings(s,None);}else if (TILE..TILE+APPS.len() as i32).contains(&id){let app=(id-TILE)as usize;if s.scanning{status(s,"Scanning installed apps... You can still open Settings.");}else if let Some(index)=chosen(s,app){if s.installs[index].gui.is_some(){launch_app(s,app,false);}else{app_menu(s,app,-1,-1);}}else if s.installs.iter().any(|i|i.app==app){app_menu(s,app,-1,-1);}else{open_settings(s,None);}}}});0}
        0x2=>{STATE.with(|slot|if let Ok(mut state)=slot.try_borrow_mut(){if let Some(s)=state.as_mut(){s.closing=true;s.cancel.store(true,Ordering::Relaxed);if let Some(work)=&s.agent_work{work.cancel.store(true,Ordering::Relaxed);}}});KillTimer(hwnd,AGENT_WORK_TIMER);KillTimer(hwnd,WORK_TIMER);MAIN.with(|v|v.set(0));KillTimer(hwnd,TIMER);for app in 0..APPS.len(){native::set_button_icon(control(hwnd,TILE+app as i32),0);native::set_detail_font(control(hwnd,TILE+app as i32),0);}PostQuitMessage(0);0}
        _=>DefWindowProcW(hwnd,msg,w,l),
    }
}
pub fn run() {
    let base=match std::env::current_exe().ok().and_then(|p|p.parent().map(|p|p.to_path_buf())){Some(path)=>path,None=>{native::message(0,"Cannot locate the launcher folder.");return;}};
    let _owner=match owner::OwnerLock::acquire(&base){Ok(v)=>v,Err(e)=>{native::message(0,&e);return;}};
    let (config,observed,config_error)=match actions::load(&base){Ok((value,hash))=>(value,hash,None),Err(error)=>(Config::default(),String::new(),Some(error))};
    unsafe {
        apply_theme(config.theme);
        let com=CoInitializeEx(ptr::null_mut(),2);let instance=GetModuleHandleW(ptr::null());
        for (name,procedure) in [("CraftAppLauncherWindow",window_proc as unsafe extern "system" fn(Hwnd,u32,usize,isize)->Lresult),("CraftLauncherSettings",settings_proc as unsafe extern "system" fn(Hwnd,u32,usize,isize)->Lresult)]{
            let name=wide(name);let class=WndClass{size:std::mem::size_of::<WndClass>()as u32,style:0,procedure:Some(procedure),cls_extra:0,wnd_extra:0,instance,icon:launcher_icon(instance,false),cursor:LoadCursorW(0,32512usize as *const u16),background:0,menu:ptr::null(),class:name.as_ptr(),small_icon:launcher_icon(instance,true)};
            if RegisterClassExW(&class)==0{native::message(0,"Could not register the native launcher window.");if com>=0{CoUninitialize();}return;}
        }
        let style=0x00CF0000; // Native chrome; launcher paint explicitly excludes circular child regions.
        let mut rect=Rect{left:0,top:0,right:1280,bottom:800};AdjustWindowRectEx(&mut rect,style,0,0);
        let mut work=Rect{left:0,top:0,right:rect.right-rect.left,bottom:rect.bottom-rect.top};SystemParametersInfoW(0x30,0,(&mut work as *mut Rect).cast(),0);
        let width=(rect.right-rect.left).min(work.right-work.left);let height=(rect.bottom-rect.top).min(work.bottom-work.top);
        let hwnd=CreateWindowExW(0,wide("CraftAppLauncherWindow").as_ptr(),wide("Craft App Launcher").as_ptr(),style,work.left+(work.right-work.left-width)/2,work.top+(work.bottom-work.top-height)/2,width,height,0,0,instance,ptr::null_mut());
        if hwnd==0{native::message(0,"Could not create the launcher window.");if com>=0{CoUninitialize();}return;}
        let(agent,agent_error)=match owner::Server::start(&base,hwnd){Ok(v)=>(Some(v),None),Err(e)=>(None,Some(format!("Agent control unavailable: {e}")))};
        MAIN.with(|v|v.set(hwnd));native::theme_window(hwnd);
        let font=native::segoe_font(16);let name_font=native::segoe_font(20);let detail_font=native::segoe_font(14);
        child(hwnd,"BUTTON","Settings",0x10000,1136,24,96,36,SETTINGS,font);child(hwnd,"STATIC","",1,1092,29,36,28,UPDATE_BADGE,font);
        for app in 0..APPS.len(){let button=child(hwnd,"BUTTON",&format!("{}\nLoading...",APPS[app].1),0x10000|0xB,0,0,272,180,TILE+app as i32,name_font);native::set_detail_font(button,detail_font);native::install_tile_feedback(button);}
        child(hwnd,"STATIC","Loading installed apps...",0x4000,48,746,1184,22,STATUS,detail_font);layout(hwnd);
        let machine=native::machine_identity();let device=config.selected_device(machine.as_deref()).to_owned();
        let first_run=config.root.is_none();let mut state=State{bulk_progress:None,closing:false,observed,agent,receipts:actions::Receipts::default(),agent_work:None,options_conflict:false,root_conflict:false,settings_profile:"Default".into(),#[cfg(test)] hidden_fixture:false,hwnd,base,config,machine,device,config_error,installs:Vec::new(),canonical_root:None,icons:[0;APPS.len()],font,name_font,detail_font,discovery_error:None,scanning:false,scan_generation:0,scan_receiver:None,pending_scan:None,settings:0,settings_tab:0,root_draft:None,profile_draft:String::new(),settings_app:None,settings_versions:Vec::new(),details:String::new(),launch_check_started:false,release_app:None,preferred_rows:std::array::from_fn(|_|Vec::new()),releases:std::collections::BTreeMap::new(),work:None,cancel:Arc::new(AtomicBool::new(false)),progress:Arc::new(WorkProgress::new(0)),work_status:"Official releases - updates not checked. Select an app row for release details.".into()};
        if let Some(error)=agent_error{let _=core::log(&state.base,&error);details(&mut state,error.clone());status(&state,&error);}
        if let Some(error)=state.config_error.clone(){details(&mut state,error);status(&state,"Settings could not be read. Open Settings for details; the file will not be overwritten.");}
        STATE.with(|slot|*slot.borrow_mut()=Some(state));ShowWindow(hwnd,5);UpdateWindow(hwnd);
        {let _deferred_wake=AgentWakeGuard;STATE.with(|slot|if let Some(s)=slot.borrow_mut().as_mut(){if s.config_error.is_none(){refresh(s);if first_run{open_settings(s,None);choose_root(s);}}maybe_start_launch_check(s);});}
        let mut msg:Msg=std::mem::zeroed();loop{let result=GetMessageW(&mut msg,0,0,0);if result<=0{break;}let settings=STATE.with(|slot|slot.borrow().as_ref().map(|s|s.settings).unwrap_or(0));if (settings==0||IsDialogMessageW(settings,&msg)==0)&&IsDialogMessageW(hwnd,&msg)==0{TranslateMessage(&msg);DispatchMessageW(&msg);}}
        STATE.with(|slot|if let Some(s)=slot.borrow_mut().take(){for icon in s.icons{if icon!=0{DestroyIcon(icon);}}for font in [s.font,s.name_font,s.detail_font]{if font!=0{DeleteObject(font);}}});if com>=0{CoUninitialize();}
    }
}

#[cfg(test)] mod settings_tests {
    use super::*;
    #[link(name="user32")] extern "system" {fn GetWindow(hwnd:Hwnd,command:u32)->Hwnd;fn GetPropW(hwnd:Hwnd,name:*const u16)->Handle;}
    #[test] fn startup_check_waits_for_icons_and_deduplicates(){let mut started=false;assert!(!claim_launch_check(&mut started,true,false));assert!(!claim_launch_check(&mut started,false,true));assert!(claim_launch_check(&mut started,false,false));assert!(!claim_launch_check(&mut started,false,false));let mut manual_started=true;assert!(!claim_launch_check(&mut manual_started,false,false));}
    #[test] fn hidden_three_pages_keep_bounds_drafts_and_update_states() {settings_fixture(false);}
    #[test] #[ignore="requires unique CraftLauncherQA private desktop"]
    fn private_completion_dialog_reselects_success_and_retains_failed_rollback(){
        #[link(name="user32")] extern "system"{fn GetThreadDesktop(thread:u32)->Handle;fn GetUserObjectInformationW(object:Handle,index:i32,value:*mut std::ffi::c_void,length:u32,needed:*mut u32)->i32;}
        #[link(name="kernel32")] extern "system"{fn GetCurrentThreadId()->u32;}
        let mut name=[0u16;256];let mut needed=0;unsafe{assert_ne!(GetUserObjectInformationW(GetThreadDesktop(GetCurrentThreadId()),2,name.as_mut_ptr().cast(),512,&mut needed),0);}
        let actual=String::from_utf16_lossy(&name[..name.iter().position(|v|*v==0).unwrap_or(name.len())]);let expected=std::env::var("CRAFT_TEST_DESKTOP").expect("Private runner must bind its exact desktop name");assert!(expected.starts_with("CraftLauncherQA"));assert_eq!(actual,expected,"Never run this modal fixture on the user's desktop");
        settings_fixture(true);
    }
    fn settings_fixture(private_completion:bool) {
        struct Fixture {host:Hwnd,hwnd:Hwnd,font:Handle,heading:Handle}impl Drop for Fixture{fn drop(&mut self){unsafe{DestroyWindow(self.host);DeleteObject(self.font);DeleteObject(self.heading);}}}
        unsafe {
            // A permanently hidden ancestor lets the real WM_SETREDRAW(TRUE) path run
            // on the page without making any top-level surface visible.
            let host=CreateWindowExW(0,wide("STATIC").as_ptr(),wide("owned hidden page host").as_ptr(),0x80000000|0x02000000,0,0,1000,680,0,0,GetModuleHandleW(ptr::null()),ptr::null_mut());assert_ne!(host,0);
            let fixture=Fixture{host,hwnd:CreateWindowExW(0,wide("STATIC").as_ptr(),wide("owned hidden three-page test").as_ptr(),0x40000000|0x10000000|0x02000000,0,0,1000,680,host,0,GetModuleHandleW(ptr::null()),ptr::null_mut()),font:native::segoe_font(16),heading:native::segoe_font(20)};assert_ne!(fixture.hwnd,0);let mut client:Rect=std::mem::zeroed();assert_ne!(GetClientRect(fixture.hwnd,&mut client),0);assert_eq!((client.right,client.bottom),(1000,680));
            let root=PathBuf::from(r"E:\owned-model-only");
            let mut s=State{bulk_progress:None,closing:false,observed:actions::disk_hash(&root).unwrap(),agent:None,receipts:actions::Receipts::default(),agent_work:None,options_conflict:false,root_conflict:false,settings_profile:"Default".into(),hidden_fixture:true,hwnd:0,base:root.clone(),config:Config::default(),machine:None,device:"Default".into(),config_error:None,installs:Vec::new(),canonical_root:Some(root.clone()),icons:[0;APPS.len()],font:fixture.font,name_font:fixture.heading,detail_font:fixture.font,discovery_error:None,scanning:false,scan_generation:0,scan_receiver:None,pending_scan:None,settings:fixture.hwnd,settings_tab:0,root_draft:None,profile_draft:String::new(),settings_app:None,settings_versions:Vec::new(),details:"owned fixture scan details".into(),launch_check_started:false,release_app:None,preferred_rows:std::array::from_fn(|_|Vec::new()),releases:std::collections::BTreeMap::new(),work:None,cancel:Arc::new(AtomicBool::new(false)),progress:Arc::new(WorkProgress::new(0)),work_status:"Not checked".into()};
            let bounds=|hwnd|{let mut r:Rect=std::mem::zeroed();assert_ne!(GetWindowRect(hwnd,&mut r),0);r};
            open_settings(&mut s,None);assert_eq!(IsWindowVisible(fixture.hwnd),0);assert_eq!(GetPropW(fixture.hwnd,wide("SysSetRedraw").as_ptr()),0);native::set_text(control(s.settings,ROOT),r"F:\draft root");native::set_text(control(s.settings,DEVICE_NAME),"Draft Surface");
            let root_before=bounds(control(s.settings,ROOT));
            s.settings_tab=1;open_settings(&mut s,None);assert_eq!(control(s.settings,ROOT),0);for id in [THEME_SYSTEM,THEME_LIGHT,THEME_DARK,THEME_TRUE_DARK,THEME_AETHER]{assert_ne!(control(s.settings,id),0);}
            s.settings_tab=0;open_settings(&mut s,None);assert_eq!(native::text(control(s.settings,ROOT)),r"F:\draft root");assert_eq!(native::text(control(s.settings,DEVICE_NAME)),"Draft Surface");let root_after=bounds(control(s.settings,ROOT));assert_eq!((root_before.left,root_before.top,root_before.right,root_before.bottom),(root_after.left,root_after.top,root_after.right,root_after.bottom));
            s.settings_tab=2;open_settings(&mut s,None);let detail_before=bounds(control(s.settings,RELEASE_DETAILS));
            // Populate all thirteen results through the real official-metadata parser; no HTTP or executable fixture is involved.
            for (app,(id,_)) in APPS.iter().enumerate(){let name=format!("{id}-1.2.3-windows-x64-portable.zip");let asset=format!(r#"{{"name":"{name}","browser_download_url":"https://github.com/storytold/{id}/releases/download/v1.2.3/{name}","size":123,"digest":"sha256:{}"}}"#,"a".repeat(64));let json=format!(r#"{{"tag_name":"v1.2.3","draft":false,"prerelease":false,"assets":[{}]}}"#,if app==5{""}else{&asset});s.releases.insert(app,updates::parse_release_for(app,json.as_bytes()));if app!=4{s.installs.push(Install{app,gui:None,cli:None,folder:root.join(id),version:if app==0{"1.2.3.0 [PE file version]"}else if app==2{"Unknown; folder hint 1.0.0"}else{"1.0.0.0 [PE file version]"}.into()});}}
            s.releases.insert(3,Err("fixture metadata error".into()));s.work_status="Checked all 13 official releases.\r\nA long worker detail remains bounded.".into();fill_preferred(&mut s);work_status(&s);
            for (app,status,action) in [(0,"Up to date","Details..."),(1,"Update available","Install..."),(2,"Unknown","Review..."),(3,"Error","Details..."),(4,"Not installed","Install..."),(5,"Update available","Details...")] {assert_eq!(native::text(control(s.settings,ROW_STATUS+app)),status);assert_eq!(native::text(control(s.settings,APP_ROW+app)),action);}
            for app in 0..APPS.len() as i32-1{let row=bounds(control(s.settings,PREFERRED+app));let next=bounds(control(s.settings,PREFERRED+app+1));assert!(row.bottom<=next.top,"preferred controls overlap at row {app}: closed top={} bottom={} next_top={} selection_height={} list_height={}",row.top,row.bottom,next.top,SendMessageW(control(s.settings,PREFERRED+app),0x154,usize::MAX,0),SendMessageW(control(s.settings,PREFERRED+app),0x154,0,0));}
            // Creation height includes the drop-down extent; the closed HWND must fit one row.
            let closed=bounds(control(s.settings,PREFERRED));let mut dropped:Rect=std::mem::zeroed();assert_ne!(SendMessageW(control(s.settings,PREFERRED),0x152,0,(&mut dropped as *mut Rect)as isize),0);assert!(dropped.bottom>closed.bottom);
            assert_eq!(update_count(&s),APPS.len()-4);assert_eq!(native::text(control(s.settings,TAB_UPDATES)),format!("Updates ({})",APPS.len()-4));assert_eq!(native::text(control(s.settings,ROW_LATEST)),"v1.2.3");
            let last=bounds(control(s.settings,PREFERRED+APPS.len() as i32-1));let header=bounds(control(s.settings,RELEASE_TITLE));assert!(last.bottom+8<=header.top);
            s.release_app=Some(1);show_release_details(&s);assert_eq!(SendMessageW(control(s.settings,RELEASE_ASSETS),0x147,0,0),-1);assert_eq!(IsWindowEnabled(control(s.settings,DOWNLOAD)),0);
            SendMessageW(control(s.settings,RELEASE_ASSETS),0x14E,0,0);work_status(&s);assert_ne!(IsWindowEnabled(control(s.settings,DOWNLOAD)),0);assert!(s.work.is_none());
            let detail_after=bounds(control(s.settings,RELEASE_DETAILS));assert_eq!((detail_before.left,detail_before.top,detail_before.right,detail_before.bottom),(detail_after.left,detail_after.top,detail_after.right,detail_after.bottom));assert!(header.bottom+4<=detail_after.top);assert!(detail_after.bottom+6<=bounds(control(s.settings,RELEASE_ASSETS)).top);for id in [RELEASE_ASSETS,DOWNLOAD,OPEN_RELEASE]{let r=bounds(control(s.settings,id));assert!(r.bottom<=680&&r.right<=1000,"footer control {id} exceeds client bounds");}
            s.installs.push(Install{app:1,gui:None,cli:None,folder:root.join("newer"),version:"1.2.3.0 [PE file version]".into()});s.config.selections.insert(APPS[1].0.into(),PathBuf::from("newer"));fill_preferred(&mut s);assert_eq!(native::text(control(s.settings,ROW_STATUS+1)),"Up to date");assert_eq!(native::text(control(s.settings,APP_ROW+1)),"Details...");assert_eq!(update_count(&s),APPS.len()-5);assert_eq!(native::text(control(s.settings,TAB_UPDATES)),format!("Updates ({})",APPS.len()-5));
            for id in [TAB_LAUNCHER,TAB_APPEARANCE,TAB_UPDATES]{assert_ne!(control(s.settings,id),0);}assert_eq!(control(s.settings,ROOT),0);assert_eq!(control(s.settings,THEME_DARK),0);assert_eq!(GetPropW(fixture.hwnd,wide("SysSetRedraw").as_ptr()),0);assert_eq!(IsWindowVisible(fixture.hwnd),0);assert_eq!(IsWindowVisible(fixture.host),0);
            // Simulate completion's cleared discovery followed by a real apply_scan.
            // The saved successful preference must be reselected with both versions listed.
            let installs=s.installs.clone();s.installs.clear();s.canonical_root=None;s.scanning=true;fill_preferred(&mut s);
            assert_eq!(SendMessageW(control(s.settings,PREFERRED+1),0x146,0,0),1);
            apply_scan(&mut s,ScanResult{request:ScanRequest{generation:0,root:root.clone(),save_root:false},result:Ok((root.clone(),installs,Vec::new())),worker_elapsed:Duration::ZERO,discovery_elapsed:Duration::ZERO,stats:core::ScanStats::default(),finished_at:Instant::now()});
            let combo=control(s.settings,PREFERRED+1);assert_eq!(SendMessageW(combo,0x146,0,0),2);
            let row=SendMessageW(combo,0x147,0,0);assert!(row>=0);assert_eq!(s.installs[s.preferred_rows[1][row as usize]].folder,root.join("newer"));assert_ne!(IsWindowEnabled(combo),0);
            // These checks never start a worker or invoke Explorer.
            assert_ne!(control(s.settings,UPDATE_ALL),0);
            let request=actions::Request{protocol:1,id:"bulk-busy".into(),action:actions::Action::UpdateAll};
            let (_tx,rx)=mpsc::channel();s.work=Some(rx);assert_eq!(begin_agent_work(&mut s,&request).code,"owner_busy");s.work=None;
            let (_tx,rx)=mpsc::channel();s.agent_work=Some(AgentWork{request:request.clone(),receiver:rx,cancel:Arc::new(AtomicBool::new(false)),progress:Arc::new(WorkProgress::new(0))});
            assert_eq!(begin_agent_work(&mut s,&request).code,"owner_busy");work_status(&s);assert_eq!(IsWindowEnabled(control(s.settings,UPDATE_ALL)),0);assert_eq!(IsWindowEnabled(control(s.settings,DOWNLOAD)),0);
            s.bulk_progress=Some(Arc::new(std::sync::Mutex::new(crate::bulk::Progress{status:"running".into(),completed:1,total:2,rows:vec![crate::bulk::Row{app:"cadcraft".into(),status:"installed".into(),message:"fixture success".into(),progress:None},crate::bulk::Row{app:"deckcraft".into(),status:"installing".into(),message:"fixture download".into(),progress:None}],stop_reason:None})));
            work_status(&s);assert_eq!(native::text(control(s.settings,ROW_STATUS)),"Installed");assert_eq!(native::text(control(s.settings,ROW_STATUS+1)),"Installing");assert!(native::text(control(s.settings,UPDATE_STATUS)).contains("1/2 deckcraft: installing"));assert!(native::text(control(s.settings,RELEASE_DETAILS)).contains("fixture success"));s.bulk_progress=None;s.agent_work=None;
            s.scanning=true;assert_eq!(begin_agent_work(&mut s,&request).code,"owner_busy");s.scanning=false;
            // Real native bar + labels use only truthful download percentages.
            let bar=control(s.settings,ROW_PROGRESS+4);assert_ne!(bar,0);s.progress.begin(4,Phase::Downloading,Some(1024),"");s.progress.store(512,Ordering::Relaxed);work_status(&s);assert!(native::text(control(s.settings,ROW_STATUS+4)).contains("50%"));assert_eq!(SendMessageW(bar,0x408,0,0),500);assert!(!native::progress_bar(bar,Some(500),false,false),"Unchanged progress must not redraw");
            s.progress.phase(Phase::Installing,"Verifying");work_status(&s);assert!(native::text(control(s.settings,ROW_STATUS+4)).starts_with("Installing"));assert!(!native::text(control(s.settings,ROW_STATUS+4)).contains('%'));assert_eq!(GetPropW(bar,wide("CraftLauncher.ProgressState").as_ptr()),2002);
            s.progress.begin(4,Phase::Downloading,None,"");s.progress.store(768,Ordering::Relaxed);work_status(&s);assert!(native::text(control(s.settings,ROW_STATUS+4)).contains("768 B"));assert!(!native::text(control(s.settings,ROW_STATUS+4)).contains('%'));
            s.progress.phase(Phase::Failed,"Fixture connection interrupted");work_status(&s);assert_eq!(native::text(control(s.settings,ROW_STATUS+4)),"Failed");assert!(native::text(control(s.settings,RELEASE_DETAILS)).contains("connection interrupted"));assert!(!native::progress_bar(bar,Some(0),false,true));s.progress=Arc::new(WorkProgress::new(0));
            let bytes_before=s.progress.load(Ordering::Relaxed);let (_tx,rx)=mpsc::channel();s.work=Some(rx);start_install(&mut s);assert!(s.work.is_some());assert!(s.agent_work.is_none());assert_eq!(s.progress.load(Ordering::Relaxed),bytes_before);s.work=None;
            if private_completion{
                #[link(name="kernel32")] extern "system"{fn GetCurrentThreadId()->u32;}
                #[link(name="user32")] extern "system"{fn EnumThreadWindows(thread:u32,callback:unsafe extern "system" fn(Hwnd,isize)->i32,value:isize)->i32;fn GetWindowThreadProcessId(hwnd:Hwnd,pid:*mut u32)->u32;fn GetClassNameW(hwnd:Hwnd,text:*mut u16,length:i32)->i32;fn GetDlgCtrlID(hwnd:Hwnd)->i32;fn EnumChildWindows(hwnd:Hwnd,callback:Option<unsafe extern "system" fn(Hwnd,isize)->i32>,value:isize)->i32;}
                struct Dismiss{expected:String,clicked:bool,closing:bool,button_id:i32}
                struct Children{expected:String,ok:Hwnd,message:bool}
                unsafe extern "system" fn find_children(hwnd:Hwnd,value:isize)->i32{
                    let children=&mut *(value as *mut Children);let mut class=[0u16;32];let length=GetClassNameW(hwnd,class.as_mut_ptr(),32);let class=String::from_utf16_lossy(&class[..length.max(0)as usize]);let text=native::text(hwnd);
                    if class.eq_ignore_ascii_case("Button")&&text.replace('&',"")=="OK"{children.ok=hwnd;}
                    if class.eq_ignore_ascii_case("Static")&&text==children.expected{children.message=true;}1
                }
                unsafe extern "system" fn dismiss(hwnd:Hwnd,value:isize)->i32{
                    let state=&mut *(value as *mut Dismiss);let mut pid=0;GetWindowThreadProcessId(hwnd,&mut pid);let mut class=[0u16;32];let length=GetClassNameW(hwnd,class.as_mut_ptr(),32);
                    if pid!=std::process::id()||String::from_utf16_lossy(&class[..length.max(0)as usize])!="CraftLauncherCompletion"||native::text(hwnd)!="Update All results"{return 1;}
                    let mut children=Children{expected:state.expected.clone(),ok:0,message:false};EnumChildWindows(hwnd,Some(find_children),(&mut children as *mut Children)as isize);if !children.message{return 1;}if state.closing{PostMessageW(hwnd,0x10,0,0);return 0;}if children.ok==0{return 1;}let id=GetDlgCtrlID(children.ok);if id<=0{return 1;}
                    let detail=GetDlgItem(hwnd,21);
                    SendMessageW(hwnd,0x111,100,0);assert_eq!(GetWindowLongPtrW(detail,-16)&0x200000,0,"Short failure must not show a scrollbar");
                    SendMessageW(hwnd,0x111,103,0);assert_ne!(GetWindowLongPtrW(detail,-16)&0x200000,0,"Overflow warning needs a scrollbar");
                    SendMessageW(detail,0xB6,0,10);assert!(SendMessageW(detail,0xCE,0,0)>0,"Long detail must remain scrollable");
                    SendMessageW(hwnd,0x111,103,0);assert_eq!(SendMessageW(detail,0xCE,0,0),0,"Reselecting the same detail resets its scroll");
                    SendMessageW(detail,0xB6,0,10);SendMessageW(hwnd,0x111,100,0);assert_eq!(GetWindowLongPtrW(detail,-16)&0x200000,0);assert_eq!(SendMessageW(detail,0xCE,0,0),0);
                    // This hook only exists in the guarded private-desktop test.
                    if let Ok(directory)=std::env::var("CRAFT_REPORT_CAPTURE_DIR"){
                        let directory=PathBuf::from(directory);let allowed=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".build/circle-ui-20261016");
                        if core::checked_path(&directory).ok()==core::checked_path(&allowed).ok()&&directory.is_dir(){
                            let mut rect:Rect=std::mem::zeroed();GetWindowRect(hwnd,&mut rect);let mut client:Rect=std::mem::zeroed();GetClientRect(hwnd,&mut client);
                            let mut labels=Vec::new();for row in 0..APPS.len(){let control=GetDlgItem(hwnd,100+row as i32);assert_ne!(control,0);let mut bounds:Rect=std::mem::zeroed();GetWindowRect(control,&mut bounds);assert!(bounds.top>=rect.top&&bounds.bottom<rect.bottom);labels.push(native::text(control));}
                            std::fs::write(directory.join("report-window.json"),serde_json::to_vec_pretty(&serde_json::json!({"hwnd":hwnd,"pid":std::process::id(),"width":rect.right-rect.left,"height":rect.bottom-rect.top,"rows":labels})).unwrap()).unwrap();
                            thread::sleep(Duration::from_secs(4));
                        }
                    }
                    state.button_id=id;state.clicked=PostMessageW(hwnd,0x111,id as usize,children.ok)!=0;0
                }
                let base=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".build/temp").join(format!("private-update-completion-{}",native::random_id().unwrap()));std::fs::create_dir_all(&base).unwrap();let base=core::checked_path(&base).unwrap();
                struct Cleanup(PathBuf);impl Drop for Cleanup{fn drop(&mut self){let allowed=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".build/temp");if let Ok(allowed)=core::checked_path(&allowed){if self.0.parent()==Some(allowed.as_path())&&self.0.file_name().is_some_and(|n|n.to_string_lossy().starts_with("private-update-completion-"))&&core::checked_path(&self.0).ok().as_ref()==Some(&self.0){let _=std::fs::remove_dir_all(&self.0);}}}}
                let _cleanup=Cleanup(base.clone());let(expected,mut result)=installer::mixed_update_fixture(&base);s.base=base.clone();s.config=expected;s.observed=actions::disk_hash(&base).unwrap();s.canonical_root=Some(base.clone());s.installs=core::discover_timed(&base,&mut core::ScanStats::default()).unwrap().0;s.release_app=None;s.bulk_progress=None;s.hwnd=fixture.host;
                assert!(!s.config.selections.contains_key("filmcraft"));fill_preferred(&mut s);let film=control(s.settings,PREFERRED+4);assert_eq!(SendMessageW(film,0x146,0,0),2);assert_eq!(SendMessageW(film,0x147,0,0),-1,"Missing preference reproduces the reported blank dropdown");
                // Extend the real failed/successful install pair with presentation-only outcomes.
                for (index,(id,_)) in APPS.iter().enumerate().filter(|(_, (id,_))|*id!="cadcraft"&&*id!="filmcraft") {
                    let (status,message)=match index%3 {0=>("not_attempted","Cancellation requested; remaining app not attempted".to_owned()),1=>("skipped","Latest version already installed; preference preserved".to_owned()),_=>("installed",serde_json::json!({"version":"v0.6.0","folder":format!("E:/fixture/{id}/{}","long-folder/".repeat(35)),"preferred":false,"selection_warning":"Preference changed during installation; previous selection preserved","cleanup_warning":format!("Retained staging for inspection\r\n{}","Fixture retained file requires review.\r\n".repeat(40))}).to_string())};
                    result["bulk"]["rows"].as_array_mut().unwrap().push(serde_json::json!({"app":id,"status":status,"message":message,"progress":null}));
                }
                result["summary"]=serde_json::json!("Update All stopped: 13 app results. Select an app for details.\r\nFixture includes installed, failed, skipped, warnings and not attempted.");
                let text=result["summary"].as_str().unwrap().to_owned();let ui_thread=GetCurrentThreadId();
                let closer=thread::spawn(move||{let mut state=Dismiss{expected:text,clicked:false,closing:false,button_id:0};let until=Instant::now()+Duration::from_secs(15);while Instant::now()<until&&!state.clicked{EnumThreadWindows(ui_thread,dismiss,(&mut state as *mut Dismiss)as isize);thread::sleep(Duration::from_millis(20));}if !state.clicked{state.closing=true;EnumThreadWindows(ui_thread,dismiss,(&mut state as *mut Dismiss)as isize);}(state.clicked,state.button_id)});
                let(tx,rx)=mpsc::sync_channel(1);let request=actions::Request{protocol:1,id:"private-mixed-completion".into(),action:actions::Action::UpdateAll};tx.send(AgentFinished{response:actions::Response::success(&request.id,result),release:None,installed_root:Some(base.clone())}).unwrap();s.agent_work=Some(AgentWork{request,receiver:rx,cancel:Arc::new(AtomicBool::new(false)),progress:Arc::new(WorkProgress::new(0))});
                poll_agent(&mut s);let(clicked,button_id)=closer.join().unwrap();assert!(clicked,"Actual completion dialog OK was not observed");
                let deadline=Instant::now()+Duration::from_secs(15);while s.scanning&&Instant::now()<deadline{poll_scan(&mut s);thread::sleep(Duration::from_millis(20));}assert!(!s.scanning,"Completion rescan did not finish");KillTimer(s.hwnd,TIMER);
                let row=SendMessageW(film,0x147,0,0);assert!(row>=0);assert_eq!(SendMessageW(film,0x146,0,0),2);assert_eq!(s.installs[s.preferred_rows[4][row as usize]].folder,base.join("filmcraft-0.6.0-windows-x64-portable"));assert_eq!(s.config.theme,ThemeMode::Dark);assert_eq!(s.config.selections["cadcraft"],PathBuf::from("cadcraft-0.5.0-windows-x64-portable"));
                for id in ["cadcraft","filmcraft"]{assert_eq!(std::fs::read(base.join(format!("{id}-0.5.0-windows-x64-portable/rollback.txt"))).unwrap(),b"keep rollback");}assert!(!std::fs::read_dir(&base).unwrap().any(|p|p.unwrap().file_name().to_string_lossy().starts_with(".craft-stage-")));
                let proof=serde_json::json!({"pid":std::process::id(),"fixture_root":base.to_string_lossy(),"actual_dialog_ok":true,"short_detail_scrollbar_hidden":true,"overflow_detail_scrollbar_visible":true,"long_detail_scrollable":true,"selection_scroll_reset":true,"observed_ok_button_id":button_id,"blank_before":true,"selected_new_after_rescan":true,"rollback_choices":2,"failed_app_preference_preserved":true,"unrelated_theme_preserved":true,"stage_cleanup":true});std::fs::write(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".build/circle-ui-20261016/private-update-completion-proof.json"),serde_json::to_vec_pretty(&proof).unwrap()).unwrap();
                let allowed=core::checked_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".build/temp")).unwrap();assert_eq!(base.parent(),Some(allowed.as_path()));assert!(base.file_name().unwrap().to_string_lossy().starts_with("private-update-completion-"));assert_eq!(core::checked_path(&base).unwrap(),base);std::fs::remove_dir_all(&base).unwrap();return;
            }
            // Reentrant owner wake: the nested modal-style callback cannot borrow
            // STATE; the outer scope must dispatch on release without another event.
            let queue_base=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".build/temp").join(format!("craft-queued-{}",native::random_id().unwrap()));
            std::fs::create_dir_all(&queue_base).unwrap();s.base=core::checked_path(&queue_base).unwrap();s.observed=actions::disk_hash(&s.base).unwrap();s.scanning=true;
            let(queue,receiver)=mpsc::sync_channel(8);s.agent=Some(owner::Server::fixture(receiver));
            let submit=|id:&str,theme:Option<&str>,deadline:Instant|{
                let(reply,response)=mpsc::sync_channel(1);
                let action=theme.map(|value|actions::Action::Patch{fields:vec![actions::FieldPatch{field:actions::Field::Theme{value:value.into()},expected:None}]}).unwrap_or(actions::Action::State);
                queue.try_send(owner::Command{request:actions::Request{protocol:1,id:id.into(),action},reply,dispatch:Arc::new(owner::Dispatch::new(deadline))}).unwrap_or_else(|_|panic!("Fixture queue is full"));response
            };
            let timely=submit("timely",Some("dark"),Instant::now()+Duration::from_secs(15));
            let expired=submit("expired",Some("light"),Instant::now()-Duration::from_secs(1));
            let read=submit("read",None,Instant::now()+Duration::from_secs(15));
            STATE.with(|slot|*slot.borrow_mut()=Some(s));
            STATE.with(|slot|{
                let _outer=AgentWakeGuard;let held=slot.borrow_mut();
                AGENT_PENDING.with(|pending|pending.set(true));
                {let _nested=AgentWakeGuard;}
                assert!(AGENT_PENDING.with(Cell::get));assert!(timely.try_recv().is_err());
                drop(held); // outer guard now drains without a timer or new input
            });
            assert!(!AGENT_PENDING.with(Cell::get));assert!(timely.try_recv().unwrap().ok);
            assert_eq!(expired.try_recv().unwrap().code,"not_executed");
            let state_response=read.try_recv().unwrap();assert_eq!(state_response.result["discovery"]["status"],"scanning");assert_eq!(state_response.result["theme"],"dark");
            let owned=STATE.with(|slot|slot.borrow_mut().take().unwrap());assert_eq!(owned.config.theme,ThemeMode::Dark);
            assert_eq!(Config::load(&queue_base).unwrap().theme,ThemeMode::Dark);drop(owned);
            assert_eq!(IsWindowVisible(fixture.host),0);
        }
    }
    #[test] fn hidden_settings_cards_stay_below_later_controls_after_layout() {
        struct Owned(Hwnd);impl Drop for Owned{fn drop(&mut self){unsafe{DestroyWindow(self.0);}}}
        unsafe {
            let parent=Owned(CreateWindowExW(0,wide("STATIC").as_ptr(),wide("owned hidden Settings ordering test").as_ptr(),0x02000000,0,0,1000,680,0,0,GetModuleHandleW(ptr::null()),ptr::null_mut()));assert_ne!(parent.0,0);
            let card=design_child(parent.0,"STATIC","",0xD,48,100,1184,400,0,0,true);
            let label=design_child(parent.0,"STATIC","Visible heading",0,72,120,500,28,0,0,true);
            let button=design_child(parent.0,"BUTTON","Action",0,72,160,200,30,812,0,true);
            let icon=design_child(parent.0,"STATIC","",0xD,300,160,20,20,940,0,true);
            for hwnd in [card,label,button,icon]{assert_ne!(hwnd,0);}
            // This is the failing original order: first-created card is above later siblings.
            assert_eq!(GetWindow(parent.0,5),card);assert_eq!(GetWindow(icon,2),0);
            for (width,height) in [(1000,680),(1100,740),(1000,680)] {
                MoveWindow(parent.0,0,0,width,height,0);native::layout_settings(parent.0);
                // Exact sibling order proves labels/buttons/icon remain above the card;
                // unlike desktop hit-testing this is valid with an entirely hidden parent.
                assert_eq!(GetWindow(parent.0,5),label);assert_eq!(GetWindow(label,2),button);
                assert_eq!(GetWindow(button,2),icon);assert_eq!(GetWindow(icon,2),card);assert_eq!(GetWindow(card,2),0);
                assert_eq!(native::text(label),"Visible heading");assert_eq!(native::text(button),"Action");
            }
        }
    }
}
