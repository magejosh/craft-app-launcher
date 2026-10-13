//! Filesystem and launch policy. No shell, identification subprocess, or updater.
use std::{collections::BTreeMap, ffi::{OsStr, OsString}, fs, io::{self, Write}, os::windows::{ffi::{OsStrExt, OsStringExt}, fs::MetadataExt, process::CommandExt}, path::{Component, Path, PathBuf}, process::Command, time::{SystemTime, UNIX_EPOCH}};
use crate::native;

pub const APPS: [(&str, &str); 12] = [("cadcraft","CADCraft"),("deckcraft","DeckCraft"),("designcraft","DesignCraft"),("effectcraft","EffectCraft"),("filmcraft","FilmCraft"),("gridcraft","GridCraft"),("lightcraft","LightCraft"),("pdfcraft","PDFCraft"),("photocraft","PhotoCraft"),("soundcraft","SoundCraft"),("vectorcraft","VectorCraft"),("wordcraft","WordCraft")];
const MAX_DEPTH: usize = 4;
const MAX_ENTRIES: usize = 20_000;
const REPARSE: u32 = 0x400;
#[derive(Clone, Debug)]
pub struct Install { pub app: usize, pub gui: Option<PathBuf>, pub cli: Option<PathBuf>, pub folder: PathBuf, pub version: String }
#[derive(Default, Clone, Debug)]
pub struct Profile { pub enabled: bool, pub obsidian: Option<PathBuf> }
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMode { #[default] System, Light, Dark, TrueDark, Aetherglass }
impl ThemeMode {
    pub fn name(self)->&'static str {match self {Self::System=>"system",Self::Light=>"light",Self::Dark=>"dark",Self::TrueDark=>"true-dark",Self::Aetherglass=>"aetherglass"}}
    pub fn parse(value:&str)->Result<Self,String> {match value {"system"=>Ok(Self::System),"light"=>Ok(Self::Light),"dark"=>Ok(Self::Dark),"true-dark"=>Ok(Self::TrueDark),"aetherglass"=>Ok(Self::Aetherglass),_=>Err("Unknown theme preference; existing settings were not replaced.".into())}}
    pub fn is_dark(self,system_dark:bool)->bool {match self {Self::System=>system_dark,Self::Light=>false,Self::Dark|Self::TrueDark|Self::Aetherglass=>true}}
}
#[derive(Default, Clone, Debug)]
pub struct DeviceProfile { pub arguments:BTreeMap<String,String>, pub compatibility:BTreeMap<String,Profile> }
#[derive(Default, Clone, Debug)]
pub struct Config { pub theme: ThemeMode, pub root: Option<PathBuf>, pub profiles: BTreeMap<String, Profile>, pub selections:BTreeMap<String,PathBuf>, pub arguments:BTreeMap<String,String>, pub devices:BTreeMap<String,DeviceProfile>, pub machine_devices:BTreeMap<String,String> }

fn profile_key(identity:&str,app:&str)->String {format!("{identity}|{app}")}
fn valid_machine_identity(value:&str)->bool {value.len()==36&&value.bytes().enumerate().all(|(i,v)|if matches!(i,8|13|18|23){v==b'-'}else{v.is_ascii_digit()||(b'a'..=b'f').contains(&v)})}
pub fn normalize(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() { return Err("Choose an absolute folder path.".into()); }
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => { if !out.pop() { return Err("Path escapes its volume root.".into()); } }
            Component::CurDir => (),
            other => out.push(other.as_os_str()),
        }
    }
    Ok(out)
}
/// Check every ancestor before canonicalization so junctions are never silently followed.
pub fn checked_path(path: &Path) -> Result<PathBuf, String> {
    let path = normalize(path)?;
    for ancestor in path.ancestors() {
        let meta = fs::symlink_metadata(ancestor).map_err(|e| format!("{}: {e}", ancestor.display()))?;
        if meta.file_attributes() & REPARSE != 0 { return Err(format!("Reparse points are not supported: {}", ancestor.display())); }
    }
    fs::canonicalize(&path).map_err(|e| e.to_string())
}
fn same_component(a: &OsStr, b: &OsStr) -> bool { a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy()) }
pub fn portable_path(base: &Path, target: &Path) -> PathBuf {
    let a: Vec<_> = base.components().collect(); let b: Vec<_> = target.components().collect();
    if a.first().zip(b.first()).map(|(a,b)| same_component(a.as_os_str(), b.as_os_str())) != Some(true) { return target.to_path_buf(); }
    let shared = a.iter().zip(&b).take_while(|(a,b)| same_component(a.as_os_str(), b.as_os_str())).count();
    let mut out = PathBuf::new();
    for _ in shared..a.len() { out.push(".."); }
    for part in &b[shared..] { out.push(part.as_os_str()); }
    if out.as_os_str().is_empty() { out.push("."); }
    out
}
pub fn resolve_path(base: &Path, saved: &Path) -> Result<PathBuf, String> { normalize(&if saved.is_absolute() { saved.to_path_buf() } else { base.join(saved) }) }
fn encode(value: &OsStr) -> String { value.encode_wide().map(|v| format!("{v:04x}")).collect() }
fn decode(value: &str) -> Result<OsString, String> {
    if value.len() % 4 != 0 || !value.is_ascii() { return Err("Invalid configuration encoding.".into()); }
    let units: Result<Vec<_>,_> = value.as_bytes().chunks(4).map(|v| u16::from_str_radix(std::str::from_utf8(v).unwrap(),16)).collect();
    Ok(OsString::from_wide(&units.map_err(|_| "Invalid configuration encoding.")?))
}
impl Config {
    pub fn device_names(&self)->Vec<String> {std::iter::once("Default".into()).chain(self.devices.keys().cloned()).collect()}
    pub fn create_device(&mut self,input:&str)->Result<String,String> {
        let name=input.trim();
        if name.is_empty()||name.chars().count()>40||name.chars().any(char::is_control){return Err("Use a profile name of 1 to 40 characters without control characters.".into());}
        if self.device_names().iter().any(|n|n.to_lowercase()==name.to_lowercase()){return Err("That profile name already exists.".into());}
        if self.devices.len()>=31{return Err("At most 32 profiles including Default are supported.".into());}
        self.devices.insert(name.into(),DeviceProfile::default());Ok(name.into())
    }
    pub fn selected_device(&self,identity:Option<&str>)->&str {identity.and_then(|id|self.machine_devices.get(id)).map(String::as_str).unwrap_or("Default")}
    pub fn device_arguments(&self,name:&str,app:&str)->&str {
        if name=="Default" {self.arguments.get(app)}else{self.devices.get(name).and_then(|p|p.arguments.get(app))}.map(String::as_str).unwrap_or("")
    }
    pub fn set_device_arguments(&mut self,name:&str,app:&str,args:String) {
        if name=="Default" {self.arguments.insert(app.into(),args);}else{self.devices.entry(name.into()).or_default().arguments.insert(app.into(),args);}
    }
    pub fn device_compatibility(&self,name:&str,identity:Option<&str>,app:&str)->Profile {
        let Some(identity)=identity else{return Profile::default();};let key=profile_key(identity,app);
        if name=="Default" {self.profiles.get(&key)}else{self.devices.get(name).and_then(|p|p.compatibility.get(&key))}.cloned().unwrap_or_default()
    }
    pub fn set_device_compatibility(&mut self,name:&str,identity:&str,app:&str,value:Profile) {
        let key=profile_key(identity,app);if name=="Default" {self.profiles.insert(key,value);}else{self.devices.entry(name.into()).or_default().compatibility.insert(key,value);}
    }
    pub fn parse(text: &str) -> Result<Self,String> {
        let mut lines = text.lines();
        let version=lines.next();let v4=version==Some("craft-launcher-v4");
        if !matches!(version,Some("craft-launcher-v1"|"craft-launcher-v2"|"craft-launcher-v3"|"craft-launcher-v4")) { return Err("Unknown settings format; existing settings were not replaced.".into()); }
        let mut cfg = Self::default();
        for line in lines {
            let fields: Vec<_> = line.split('\t').collect();
            match fields.as_slice() {
                ["device", name] if v4 => {
                    let name=decode(name)?.into_string().map_err(|_|"Invalid profile name.")?;
                    if name.trim()!=name{return Err("Untrimmed profile name.".into());}cfg.create_device(&name)?;
                }
                ["device-choice", identity, name] if v4 => {
                    let identity=decode(identity)?.into_string().map_err(|_|"Invalid machine identity.")?;
                    let name=decode(name)?.into_string().map_err(|_|"Invalid profile name.")?;
                    if !valid_machine_identity(&identity)||cfg.machine_devices.insert(identity,name).is_some(){return Err("Invalid or duplicate machine identity.".into());}
                }
                ["device-arguments", name, app, value] if v4 => {
                    let name=decode(name)?.into_string().map_err(|_|"Invalid profile name.")?;
                    let app=decode(app)?.into_string().map_err(|_|"Invalid app key.")?;
                    if !APPS.iter().any(|(id,_)|*id==app){return Err("Unknown argument app.".into());}
                    let value=decode(value)?.into_string().map_err(|_|"Invalid argument text.")?;parse_args(&value)?;
                    let device=cfg.devices.get_mut(&name).ok_or("Profile must be declared before its settings.")?;
                    if device.arguments.insert(app,value).is_some(){return Err("Duplicate profile arguments.".into());}
                }
                ["device-compatibility", name, key, enabled, folder] if v4 && matches!(*enabled,"0"|"1") => {
                    let name=decode(name)?.into_string().map_err(|_|"Invalid profile name.")?;
                    let key=decode(key)?.into_string().map_err(|_|"Invalid profile key.")?;
                    if !key.strip_suffix("|filmcraft").is_some_and(valid_machine_identity){return Err("Invalid FilmCraft machine key.".into());}
                    let obsidian=if folder.is_empty(){None}else{Some(PathBuf::from(decode(folder)?))};
                    let device=cfg.devices.get_mut(&name).ok_or("Profile must be declared before its settings.")?;
                    if device.compatibility.insert(key,Profile{enabled:*enabled=="1",obsidian}).is_some(){return Err("Duplicate profile compatibility.".into());}
                }
                ["theme", value] => cfg.theme = ThemeMode::parse(value)?,
                ["root", path] => cfg.root = Some(PathBuf::from(decode(path)?)),
                ["selection", app, folder] => {
                    let app=decode(app)?.into_string().map_err(|_|"Invalid app key.")?;
                    if !APPS.iter().any(|(id,_)|*id==app) {return Err("Unknown selected app.".into());}
                    cfg.selections.insert(app,PathBuf::from(decode(folder)?));
                }
                ["arguments", app, value] => {
                    let app=decode(app)?.into_string().map_err(|_|"Invalid app key.")?;
                    if !APPS.iter().any(|(id,_)|*id==app) {return Err("Unknown argument app.".into());}
                    let value=decode(value)?.into_string().map_err(|_|"Invalid argument text.")?;
                    parse_args(&value)?;cfg.arguments.insert(app,value);
                }
                ["profile", key, enabled, folder] if *enabled == "0" || *enabled == "1" => {
                    let key = decode(key)?.into_string().map_err(|_| "Invalid profile key.")?;
                    let obsidian = if folder.is_empty() { None } else { Some(PathBuf::from(decode(folder)?)) };
                    cfg.profiles.insert(key, Profile { enabled: *enabled == "1", obsidian });
                }
                _ => return Err("Malformed settings; existing settings were not replaced.".into()),
            }
        }
        if cfg.machine_devices.values().any(|name|name!="Default"&&!cfg.devices.contains_key(name)){return Err("Selected device profile does not exist.".into());}
        Ok(cfg)
    }
    pub fn serialize(&self) -> String {
        let mut text = format!("craft-launcher-v4\ntheme\t{}\n",self.theme.name());
        if let Some(root) = &self.root { text.push_str(&format!("root\t{}\n", encode(root.as_os_str()))); }
        for (key,p) in &self.profiles { text.push_str(&format!("profile\t{}\t{}\t{}\n", encode(OsStr::new(key)), u8::from(p.enabled), p.obsidian.as_ref().map(|p| encode(p.as_os_str())).unwrap_or_default())); }
        for (app,folder) in &self.selections {text.push_str(&format!("selection\t{}\t{}\n",encode(OsStr::new(app)),encode(folder.as_os_str())));}
        for (app,args) in &self.arguments {text.push_str(&format!("arguments\t{}\t{}\n",encode(OsStr::new(app)),encode(OsStr::new(args))));}
        for (name,device) in &self.devices {
            let name=encode(OsStr::new(name));text.push_str(&format!("device\t{name}\n"));
            for (app,args) in &device.arguments {text.push_str(&format!("device-arguments\t{}\t{}\t{}\n",name,encode(OsStr::new(app)),encode(OsStr::new(args))));}
            for (key,p) in &device.compatibility {text.push_str(&format!("device-compatibility\t{}\t{}\t{}\t{}\n",name,encode(OsStr::new(key)),u8::from(p.enabled),p.obsidian.as_ref().map(|p|encode(p.as_os_str())).unwrap_or_default()));}
        }
        for (identity,name) in &self.machine_devices {text.push_str(&format!("device-choice\t{}\t{}\n",encode(OsStr::new(identity)),encode(OsStr::new(name))));}
        text
    }
    pub fn load(base: &Path) -> Result<Self,String> {
        match fs::read_to_string(base.join("craft-launcher.conf")) {
            Ok(text) => Self::parse(&text),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("Cannot read portable settings: {e}")),
        }
    }
    pub fn save(&self, base: &Path) -> Result<(),String> {
        // Replace with a same-directory temporary file; retain old settings on failure.
        let destination = base.join("craft-launcher.conf");
        let temporary = base.join(format!("craft-launcher.conf.{}.tmp", std::process::id()));
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&temporary).map_err(|e| format!("Cannot create adjacent settings: {e}"))?;
        let result = (|| { file.write_all(self.serialize().as_bytes()).map_err(|e| e.to_string())?; file.sync_all().map_err(|e| e.to_string())?; drop(file); #[cfg(test)] if FAIL_BEFORE_REPLACE.with(|v|v.replace(false)){return Err("Injected interruption before atomic replacement".into());} native::replace_file(&temporary,&destination) })();
        if result.is_err() { let _ = fs::remove_file(&temporary); }
        result
    }
}
pub fn folder_hint(app: &str, folder: &str) -> Option<String> {
    let version = folder.strip_prefix(&format!("{app}-"))?.strip_suffix("-windows-x64-portable")?;
    let parts: Vec<_> = version.split('.').collect();
    if parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.bytes().all(|v| v.is_ascii_digit())) { Some(version.to_owned()) } else { None }
}
pub fn version_label(path: &Path, app: &str, folder: &Path) -> String {
    if let Some(version) = native::file_version(path) { return format!("{version} [PE file version]"); }
    if let Some(hint) = folder.file_name().and_then(OsStr::to_str).and_then(|s| folder_hint(app,s)) { return format!("Unknown; folder hint {hint}"); }
    "Unknown (no PE version)".into()
}
#[derive(Default)]
pub struct ScanStats {pub entries:usize,pub folders:usize,pub version_queries:usize,pub version_time:std::time::Duration}
#[cfg(test)]
pub fn discover(root:&Path)->Result<(Vec<Install>,Vec<String>),String> {discover_timed(root,&mut ScanStats::default())}
pub fn discover_timed(root: &Path,stats:&mut ScanStats) -> Result<(Vec<Install>, Vec<String>), String> {
    let root = checked_path(root)?;
    if !root.is_dir() { return Err("The selected root is not a folder.".into()); }
    let mut installs = Vec::new(); let mut warnings = Vec::new(); let mut seen = 0;
    let mut stack = vec![(root.clone(),0)];
    while let Some((folder,depth)) = stack.pop() {
        // Recheck the folder when consuming the queue: directory-entry metadata
        // describes enumeration time and must not authorize a later reparse hop.
        match fs::symlink_metadata(&folder) {Ok(meta) if meta.file_attributes()&REPARSE!=0=>{warnings.push(format!("Skipped reparse point: {}",folder.display()));continue;},Err(e)=>{warnings.push(format!("{}: {e}",folder.display()));continue;},_=>{}}
        stats.folders+=1;
        let entries = match fs::read_dir(&folder) { Ok(v) => v, Err(e) => { warnings.push(format!("{}: {e}", folder.display())); continue; } };
        let mut files = BTreeMap::new(); let mut dirs = Vec::new();
        for entry in entries {
            seen += 1;stats.entries=seen;
            if seen > MAX_ENTRIES { warnings.push(format!("Scan stopped after {MAX_ENTRIES} entries; narrow the root.")); return Ok((installs,warnings)); }
            let entry = match entry { Ok(v) => v, Err(e) => { warnings.push(e.to_string()); continue; } };
            let path = entry.path();
            if entry.file_name().to_string_lossy().starts_with(".craft-stage-"){warnings.push(format!("Managed staging skipped: {}",path.display()));continue;}
            // Windows DirEntry metadata is cached and does not follow symlinks.
            let meta = match entry.metadata() { Ok(v) => v, Err(e) => { warnings.push(e.to_string()); continue; } };
            if meta.file_attributes() & REPARSE != 0 { warnings.push(format!("Skipped reparse point: {}", path.display())); continue; }
            if meta.is_dir() { dirs.push(path); }
            else if meta.is_file() { if let Some(name) = path.file_name().and_then(OsStr::to_str) { files.insert(name.to_ascii_lowercase(),path); } }
        }
        let mut found = false;
        for (app,(id,_)) in APPS.iter().enumerate() {
            let gui = files.get(&format!("{id}.exe")).cloned(); let cli = files.get(&format!("{id}-cli.exe")).cloned();
            if let Some(exe) = gui.as_ref().or(cli.as_ref()) {
                // Refresh the selected executable/ancestors before reading PE data.
                if let Err(error)=checked_path(exe){warnings.push(error);continue;}
                let version_started=std::time::Instant::now();let version = version_label(exe,id,&folder);stats.version_time+=version_started.elapsed();stats.version_queries+=1;
                installs.push(Install {app,gui,cli,folder:folder.clone(),version}); found = true;
            }
        }
        // Portable app resource trees need not be scanned; versions are sibling folders.
        if !found && depth < MAX_DEPTH { dirs.sort(); for dir in dirs.into_iter().rev() { stack.push((dir,depth+1)); } }
        else if !found && !dirs.is_empty() { warnings.push(format!("Depth limit reached: {}", folder.display())); }
    }
    installs.sort_by(|a,b| a.app.cmp(&b.app).then(a.folder.cmp(&b.folder)));
    Ok((installs,warnings))
}
/// A single installation is unambiguous. Multiple installations require a valid
/// saved relative-folder choice; never silently choose the newest/first version.
pub fn selected_install(installs:&[Install],app:usize,root:&Path,saved:Option<&PathBuf>)->Option<usize> {
    let choices:Vec<_>=installs.iter().enumerate().filter(|(_,i)|i.app==app).collect();
    if choices.len()==1 {return Some(choices[0].0);}
    let path=resolve_path(root,saved?) .ok()?;
    choices.into_iter().find(|(_,i)|same_component(i.folder.as_os_str(),path.as_os_str())).map(|(index,_)|index)
}
pub fn scan_note_is_informational(note:&str)->bool { note.starts_with("Depth limit reached:") || note.starts_with("Managed staging skipped:") }
pub fn validate_executable(root: &Path, exe: &Path, app: usize, cli: bool) -> Result<PathBuf,String> {
    let root = checked_path(root)?; let exe = checked_path(exe)?;
    if !exe.starts_with(&root) || !exe.is_file() { return Err("Executable is missing or outside the selected root.".into()); }
    let expected = format!("{}{}.exe", APPS[app].0, if cli {"-cli"} else {""});
    if !exe.file_name().map(|v| same_component(v,OsStr::new(&expected))).unwrap_or(false) { return Err("Unexpected executable name.".into()); }
    Ok(exe)
}
/// Argument editor uses one literal argument per line, including spaces and quotes.
/// Empty lines are omitted. This intentionally avoids shell/command-line parsing.
pub fn parse_args(text: &str) -> Result<Vec<OsString>,String> {
    if text.contains('\0') { return Err("Arguments cannot contain NUL characters.".into()); }
    Ok(text.lines().filter(|line| !line.is_empty()).map(OsString::from).collect())
}
pub fn filtered_path(current: &OsStr, excluded: &Path) -> Result<OsString,String> {
    let excluded = normalize(excluded)?;
    let paths = std::env::split_paths(current).filter(|p| {
        // Exact normalized directory equality only. No substring matching.
        normalize(p).map(|p| !same_component(p.as_os_str(),excluded.as_os_str())).unwrap_or(true)
    });
    std::env::join_paths(paths).map_err(|e| format!("Cannot construct child PATH: {e}"))
}
/// Returns only explicit child overrides; does not mutate process/global environment.
pub fn child_environment(app: &str, profile: &Profile, inherited_path: &OsStr) -> Result<Vec<(&'static str,OsString)>,String> {
    if !profile.enabled || app != "filmcraft" { return Ok(Vec::new()); }
    let folder=profile.obsidian.as_ref().ok_or("Choose an Obsidian folder before enabling compatibility.")?;
    Ok(vec![("WGPU_BACKEND",OsString::from("dx12")),("PATH",filtered_path(inherited_path,folder)?)])
}
pub fn log(base: &Path, text: &str) -> Result<(),String> {
    let path = base.join("craft-launcher.log");
    // Keep a single bounded diagnostic log; arguments are deliberately omitted.
    if fs::metadata(&path).map(|m| m.len() > 1_048_576).unwrap_or(false) { fs::write(&path,b"").map_err(|e| e.to_string())?; }
    let mut file = fs::OpenOptions::new().create(true).append(true).open(path).map_err(|e| e.to_string())?;
    writeln!(file,"{} {}",SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),text).map_err(|e| e.to_string())
}
pub fn launch(base: &Path, root: &Path, install: &Install, cli: bool, args: &[OsString], profile: &Profile) -> Result<u32,String> {launch_with_io(base,root,install,cli,args,profile,false)}
pub fn launch_agent(base:&Path,root:&Path,install:&Install,cli:bool,args:&[OsString],profile:&Profile)->Result<u32,String>{launch_with_io(base,root,install,cli,args,profile,true)}
fn launch_with_io(base:&Path,root:&Path,install:&Install,cli:bool,args:&[OsString],profile:&Profile,isolate:bool)->Result<u32,String>{
    let selected = if cli { &install.cli } else { &install.gui };
    let exe = validate_executable(root,selected.as_ref().ok_or("This installation has no matching executable.")?,install.app,cli)?;
    let mut command = Command::new(&exe); command.args(args).current_dir(exe.parent().unwrap());
    if isolate {command.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).creation_flags(0x08000000);}else if cli { command.creation_flags(0x00000010); } // CREATE_NEW_CONSOLE keeps interactive CLI usable.
    if profile.enabled && APPS[install.app].0 == "filmcraft" {
        if native::machine_identity().is_none() { return Err("Compatibility is disabled because the machine identity is unavailable.".into()); }
        let folder = checked_path(profile.obsidian.as_ref().ok_or("Choose the exact Obsidian installation folder in Settings before enabling this profile.")?)?;
        if !folder.is_dir() { return Err("The excluded Obsidian path is not a folder.".into()); }
    }
    command.envs(child_environment(APPS[install.app].0,profile,&std::env::var_os("PATH").unwrap_or_default())?);
    let _stdio_guard=if isolate{Some(native::IsolatedStdio::new()?)}else{None};
    match command.spawn() {
        Ok(child) => { let id = child.id(); let _ = log(base,&format!("Started {} pid={id} profile={}",exe.display(),profile.enabled)); Ok(id) }
        Err(e) => { let message = format!("Could not start {}: {e}",exe.display()); let _ = log(base,&message); Err(message) }
    }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn device_profiles_preserve_legacy_and_isolate_machines() {
        // Fixed historical UTF-16 hex fixtures; deliberately independent of serialize().
        let fixtures=[
            "craft-launcher-v1\nroot\t002e002e005c0041007000700073\nprofile\t00610061006100610061006100610061002d0031003100310031002d0032003200320032002d0033003300330033002d003400340034003400340034003400340034003400340034007c00660069006c006d00630072006100660074\t1\t0045003a005c004f006200730069006400690061006e\nprofile\t00620062006200620062006200620062002d0031003100310031002d0032003200320032002d0033003300330033002d003400340034003400340034003400340034003400340034007c00660069006c006d00630072006100660074\t1\t0045003a005c004f006200730069006400690061006e\n",
            "craft-launcher-v2\nroot\t002e002e005c0041007000700073\nprofile\t00610061006100610061006100610061002d0031003100310031002d0032003200320032002d0033003300330033002d003400340034003400340034003400340034003400340034007c00660069006c006d00630072006100660074\t1\t0045003a005c004f006200730069006400690061006e\nprofile\t00620062006200620062006200620062002d0031003100310031002d0032003200320032002d0033003300330033002d003400340034003400340034003400340034003400340034007c00660069006c006d00630072006100660074\t1\t0045003a005c004f006200730069006400690061006e\narguments\t00660069006c006d00630072006100660074\t002d002d006c00650067006100630079\nselection\t00660069006c006d00630072006100660074\t00460069006c006d00430072006100660074\n",
            "craft-launcher-v3\ntheme\tdark\nroot\t002e002e005c0041007000700073\nprofile\t00610061006100610061006100610061002d0031003100310031002d0032003200320032002d0033003300330033002d003400340034003400340034003400340034003400340034007c00660069006c006d00630072006100660074\t1\t0045003a005c004f006200730069006400690061006e\nprofile\t00620062006200620062006200620062002d0031003100310031002d0032003200320032002d0033003300330033002d003400340034003400340034003400340034003400340034007c00660069006c006d00630072006100660074\t1\t0045003a005c004f006200730069006400690061006e\narguments\t00660069006c006d00630072006100660074\t002d002d006c00650067006100630079\nselection\t00660069006c006d00630072006100660074\t00460069006c006d00430072006100660074\n",
        ];
        for (version,old) in fixtures.iter().enumerate() {
            let legacy=Config::parse(old).unwrap();let mut migrated=legacy.clone();
            assert_eq!(legacy.root,Some(PathBuf::from(r"..\Apps")));assert_eq!(legacy.theme,if version==2{ThemeMode::Dark}else{ThemeMode::System});
            assert_eq!(legacy.selections.get("filmcraft").cloned(),if version==0{None}else{Some(PathBuf::from("FilmCraft"))});
            assert_eq!(migrated.device_arguments("Default","filmcraft"),if version==0{""}else{"--legacy"});assert!(migrated.device_compatibility("Default",Some("bbbbbbbb-1111-2222-3333-444444444444"),"filmcraft").enabled);
            assert!(!migrated.device_compatibility("Default",Some("cccccccc-1111-2222-3333-444444444444"),"filmcraft").enabled);assert!(!migrated.device_compatibility("Default",None,"filmcraft").enabled);
            migrated.create_device("Surface").unwrap();migrated.create_device("Desktop").unwrap();assert_eq!(migrated.device_arguments("Surface","filmcraft"),"");
            migrated.set_device_arguments("Surface","filmcraft","--surface".into());migrated.set_device_arguments("Desktop","filmcraft","--desktop".into());
            migrated.set_device_compatibility("Surface","aaaaaaaa-1111-2222-3333-444444444444","filmcraft",Profile{enabled:true,obsidian:Some(PathBuf::from(r"..\Obsidian"))});
            migrated.machine_devices.insert("aaaaaaaa-1111-2222-3333-444444444444".into(),"Surface".into());migrated.machine_devices.insert("bbbbbbbb-1111-2222-3333-444444444444".into(),"Desktop".into());
            let saved=Config::parse(&migrated.serialize()).unwrap();assert_eq!(saved.root,legacy.root);assert_eq!(saved.selections,legacy.selections);assert_eq!(saved.profiles.len(),2);
            assert_eq!(saved.selected_device(Some("aaaaaaaa-1111-2222-3333-444444444444")),"Surface");assert_eq!(saved.selected_device(Some("bbbbbbbb-1111-2222-3333-444444444444")),"Desktop");assert_eq!(saved.selected_device(None),"Default");
            assert_eq!(saved.device_arguments("Surface","filmcraft"),"--surface");assert_eq!(saved.device_arguments("Desktop","filmcraft"),"--desktop");
            assert!(saved.device_compatibility("Surface",Some("aaaaaaaa-1111-2222-3333-444444444444"),"filmcraft").enabled);assert!(!saved.device_compatibility("Surface",Some("bbbbbbbb-1111-2222-3333-444444444444"),"filmcraft").enabled);assert!(!saved.device_compatibility("Desktop",Some("aaaaaaaa-1111-2222-3333-444444444444"),"filmcraft").enabled);
            assert_eq!(resolve_path(Path::new(r"F:\Suite\Launcher"),saved.device_compatibility("Surface",Some("aaaaaaaa-1111-2222-3333-444444444444"),"filmcraft").obsidian.as_ref().unwrap()).unwrap(),PathBuf::from(r"F:\Suite\Obsidian"));
        }
    }
    #[test] fn device_profile_names_and_v4_records_are_validated() {
        let mut config=Config::default();assert_eq!(config.create_device("  Travel laptop  ").unwrap(),"Travel laptop");
        for name in ["", "default", "DEFAULT", "travel LAPTOP", "line\nbreak"] {assert!(config.create_device(name).is_err());}
        assert!(config.create_device(&"x".repeat(41)).is_err());
        let name=encode(OsStr::new("Surface"));let app=encode(OsStr::new("filmcraft"));let identity=encode(OsStr::new("aaaaaaaa-1111-2222-3333-444444444444"));
        for records in [format!("device\t{name}\ndevice\t{name}\n"),format!("device-choice\t{identity}\t{name}\n"),format!("device-arguments\t{name}\t{app}\t\n"),format!("device\t{name}\ndevice-arguments\t{name}\t{app}\t\ndevice-arguments\t{name}\t{app}\t\n")] {assert!(Config::parse(&format!("craft-launcher-v4\n{records}")).is_err());}
        assert!(Config::parse(&format!("craft-launcher-v3\ndevice\t{name}\n")).is_err());
        for invalid in ["", "PC-A", "AAAAAAAA-1111-2222-3333-444444444444", "aaaaaaaa_1111-2222-3333-444444444444", "gggggggg-1111-2222-3333-444444444444"] {
            let id=encode(OsStr::new(invalid));let key=encode(OsStr::new(&format!("{invalid}|filmcraft")));
            assert!(Config::parse(&format!("craft-launcher-v4\ndevice\t{name}\ndevice-choice\t{id}\t{name}\n")).is_err());
            assert!(Config::parse(&format!("craft-launcher-v4\ndevice\t{name}\ndevice-compatibility\t{name}\t{key}\t0\t\n")).is_err());
        }
        // Legacy records stay byte-for-byte meaningful even with pre-existing non-GUID keys.
        let legacy=Config::parse("craft-launcher-v1\nprofile\t00500043007c00660069006c006d00630072006100660074\t0\t\n").unwrap();
        assert!(Config::parse(&legacy.serialize()).unwrap().profiles.contains_key("PC|filmcraft"));
        let roundtrip=Config::parse(&config.serialize()).unwrap();assert!(roundtrip.devices.contains_key("Travel laptop"));assert!(roundtrip.machine_devices.is_empty());
    }
    #[test] fn theme_migration_and_roundtrip() {
        assert_eq!(Config::default().theme,ThemeMode::System);
        for version in ["v1","v2"] {let legacy=Config::parse(&format!("craft-launcher-{version}\nroot\t002e\n")).unwrap();assert_eq!(legacy.theme,ThemeMode::System);assert_eq!(legacy.root,Some(PathBuf::from(".")));}
        for theme in [ThemeMode::System,ThemeMode::Light,ThemeMode::Dark,ThemeMode::TrueDark,ThemeMode::Aetherglass] {let mut c=Config::default();c.theme=theme;c.arguments.insert("filmcraft".into(),"--literal".into());let d=Config::parse(&c.serialize()).unwrap();assert_eq!(d.theme,theme);assert_eq!(d.arguments,c.arguments);}
        assert!(Config::parse("craft-launcher-v3\ntheme\tglass\n").is_err());
    }
    #[test] fn system_theme_changes_do_not_override_explicit_choice() {
        assert!(!ThemeMode::System.is_dark(false));assert!(ThemeMode::System.is_dark(true));
        for system in [false,true] {assert!(!ThemeMode::Light.is_dark(system));assert!(ThemeMode::Dark.is_dark(system));assert!(ThemeMode::TrueDark.is_dark(system));assert!(ThemeMode::Aetherglass.is_dark(system));}
    }
    #[test] fn portable_sibling_and_relocation() {
        let relative = portable_path(Path::new(r"E:\Tools\Launcher"),Path::new(r"E:\Apps\Craft"));
        assert_eq!(relative,PathBuf::from(r"..\..\Apps\Craft"));
        assert_eq!(resolve_path(Path::new(r"F:\Tools\Launcher"),&relative).unwrap(),PathBuf::from(r"F:\Apps\Craft"));
        assert_eq!(portable_path(Path::new(r"E:\Tools"),Path::new(r"D:\Apps")),PathBuf::from(r"D:\Apps"));
    }
    #[test] fn config_roundtrip_and_default_off() {
        let mut c = Config::default(); c.root = Some(PathBuf::from(r"..\..\Craft apps"));
        c.profiles.insert("PC|filmcraft".into(),Profile {enabled:true,obsidian:Some(PathBuf::from(r"E:\Obsidian"))});
        let d = Config::parse(&c.serialize()).unwrap(); assert_eq!(c.root,d.root); assert!(d.profiles["PC|filmcraft"].enabled);
        assert!(!Profile::default().enabled); assert!(Config::parse("garbage").is_err());
    }
    #[test] fn version_hints_are_conservative() {
        assert_eq!(folder_hint("filmcraft","filmcraft-0.6.0-windows-x64-portable"),Some("0.6.0".into()));
        assert!(folder_hint("filmcraft","filmcraft-latest-windows-x64-portable").is_none());
        assert!(folder_hint("filmcraft","photocraft-0.6.0-windows-x64-portable").is_none());
    }
    #[test] fn arguments_are_literal() {
        assert_eq!(parse_args("--output\r\nE:\\My Videos\\out.mp4\r\n\"quoted\"\n& del *").unwrap(),vec![OsString::from("--output"),OsString::from(r"E:\My Videos\out.mp4"),OsString::from("\"quoted\""),OsString::from("& del *")]);
        assert!(parse_args("a\0b").is_err());
    }
    #[test] fn path_exclusion_is_exact() {
        assert_eq!(filtered_path(OsStr::new(r"E:\Obsidian;E:\Obsidian-backup;C:\Windows"),Path::new(r"e:\obsidian")).unwrap(),OsString::from(r"E:\Obsidian-backup;C:\Windows"));
    }
    #[test] fn discovery_and_containment() {
        let test = std::env::temp_dir().join(format!("craft-launcher-test-{}",std::process::id()));
        fs::create_dir(&test).unwrap(); let app = test.join("filmcraft-0.6.0-windows-x64-portable"); fs::create_dir(&app).unwrap();
        fs::write(app.join("filmcraft.exe"),b"not an executable").unwrap(); fs::write(app.join("filmcraft-cli.exe"),b"not an executable").unwrap(); fs::write(app.join("unrelated.exe"),b"").unwrap();
        let mut stats=ScanStats::default();let (found,_) = discover_timed(&test,&mut stats).unwrap(); assert_eq!((stats.entries,stats.folders,stats.version_queries),(4,2,1));assert_eq!(found.len(),1); assert!(found[0].cli.is_some()); assert!(found[0].version.starts_with("Unknown; folder hint"));
        assert!(validate_executable(&test,&app.join("filmcraft.exe"),4,false).is_ok());
        fs::write(test.join("filmcraft.exe"),b"").unwrap(); assert!(validate_executable(&app,&test.join("filmcraft.exe"),4,false).is_err());
        fs::remove_dir_all(test).unwrap();
    }
    #[test] fn profile_is_opt_in_app_scoped_and_child_only() {
        let inherited=OsStr::new(r"E:\Obsidian;C:\Windows");
        let profile=Profile{enabled:true,obsidian:Some(PathBuf::from(r"E:\Obsidian"))};
        assert!(child_environment("photocraft",&profile,inherited).unwrap().is_empty());
        assert!(child_environment("filmcraft",&Profile::default(),inherited).unwrap().is_empty());
        let before=std::env::var_os("WGPU_BACKEND");
        let overrides=child_environment("filmcraft",&profile,inherited).unwrap();
        let mut command=Command::new("filmcraft.exe");command.envs(overrides);
        assert_eq!(command.get_envs().find(|(key,_)| *key==OsStr::new("WGPU_BACKEND")).unwrap().1,Some(OsStr::new("dx12")));
        assert_eq!(command.get_envs().find(|(key,_)| *key==OsStr::new("PATH")).unwrap().1,Some(OsStr::new(r"C:\Windows")));
        assert_eq!(std::env::var_os("WGPU_BACKEND"),before);
    }
    #[test] fn reparse_root_ancestor_and_entry_are_rejected() {
        let test=std::env::temp_dir().join(format!("craft-launcher-reparse-{}",std::process::id()));
        fs::create_dir(&test).unwrap();let real=test.join("real");fs::create_dir(&real).unwrap();fs::create_dir(real.join("child")).unwrap();
        let link=test.join("link");
        // Windows requires Developer Mode or symlink privilege for this fixture.
        if let Err(error)=std::os::windows::fs::symlink_dir(&real,&link) {
            eprintln!("SKIPPED actual reparse fixture: {error}; run under Developer Mode to cover it.");fs::remove_dir_all(test).unwrap();return;
        }
        assert!(checked_path(&link).is_err());assert!(checked_path(&link.join("child")).is_err());
        let (_,warnings)=discover(&test).unwrap();assert!(warnings.iter().any(|w|w.contains("Skipped reparse point")));
        fs::remove_dir(&link).unwrap();fs::remove_dir_all(test).unwrap();
    }
    #[test] fn selections_arguments_and_v1_migration() {
        let old=Config::parse("craft-launcher-v1\nroot\t002e\n").unwrap();assert_eq!(old.root,Some(PathBuf::from(".")));
        let mut config=old;config.selections.insert("filmcraft".into(),PathBuf::from("filmcraft-0.6.0-windows-x64-portable"));
        config.arguments.insert("filmcraft".into(),"--output\nE:\\My Videos\\clip.mp4\n& literal".into());
        let decoded=Config::parse(&config.serialize()).unwrap();assert_eq!(decoded.selections,config.selections);assert_eq!(decoded.arguments,config.arguments);
    }
    #[test] fn multiple_versions_require_a_valid_choice() {
        let root=Path::new(r"E:\Craft");
        let item=|version:&str|Install{app:4,gui:Some(root.join(version).join("filmcraft.exe")),cli:None,folder:root.join(version),version:version.into()};
        let installs=vec![item("v1"),item("v2")];
        assert_eq!(selected_install(&installs,4,root,None),None);
        assert_eq!(selected_install(&installs,4,root,Some(&PathBuf::from("v2"))),Some(1));
        assert_eq!(selected_install(&installs,4,root,Some(&PathBuf::from("../elsewhere"))),None);
        assert_eq!(selected_install(&installs[..1],4,root,None),Some(0));
    }
    #[test] fn bounded_depth_notes_are_not_access_errors() {
        assert!(scan_note_is_informational("Depth limit reached: E:\\Craft\\diagnostics"));
        assert!(scan_note_is_informational("Managed staging skipped: E:\\Craft\\.craft-stage-1"));
        assert!(!scan_note_is_informational("E:\\Craft: access denied"));
        assert!(!scan_note_is_informational("Scan stopped after 20000 entries; narrow the root."));
    }}

#[cfg(test)] thread_local!{pub(crate) static FAIL_BEFORE_REPLACE:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}