#[cfg(windows)] mod mcp;
// Console endpoint. Stdout contains one JSON response; never a Windows subsystem binary.
#[cfg(windows)]
fn main(){std::process::exit(if std::env::args().skip(1).eq(["--mcp"]) {mcp::run()} else {run()});}
#[cfg(not(windows))]
fn main(){eprintln!("Craft App Launcher requires Windows.");std::process::exit(2);}
#[cfg(windows)]
fn run()->i32{
    use craft_app_launcher::{actions::{self,Action,Request,Response}};
    use std::io::Read;
    let args:Vec<String>=std::env::args().skip(1).collect();
    let result=(||->Result<Response,String>{
        let request=match args.as_slice(){
            [command]if command=="state"=>Request{protocol:1,id:format!("read-{}",std::process::id()),action:Action::State},
            [command]if command=="profiles"=>Request{protocol:1,id:format!("read-{}",std::process::id()),action:Action::Profiles},
            [command]if command=="installations"=>Request{protocol:1,id:format!("read-{}",std::process::id()),action:Action::Installations{offset:0,limit:32}},
            [command,id,json]if command=="patch"=>Request{protocol:1,id:id.clone(),action:Action::Patch{fields:serde_json::from_str(json).map_err(|e|e.to_string())?}},
            [command,id]if command=="update-all"=>Request{protocol:1,id:id.clone(),action:Action::UpdateAll},
            [command,id]if command=="open-apps-folder"=>Request{protocol:1,id:id.clone(),action:Action::OpenAppsFolder},
            [command,id]if command=="reload"=>Request{protocol:1,id:id.clone(),action:Action::Reload},
            [command,id,app,install,profile,mode]if command=="launch"&&matches!(mode.as_str(),"gui"|"cli")=>Request{protocol:1,id:id.clone(),action:Action::Launch{app:app.clone(),install_id:install.clone(),profile_id:profile.clone(),cli:mode=="cli"}},
            [command,id,app]if command=="releases-check"=>Request{protocol:1,id:id.clone(),action:Action::ReleasesCheck{app:app.clone()}},
            [command,id,app,tag,json]if command=="install-plan"=>Request{protocol:1,id:id.clone(),action:Action::InstallPlan{app:app.clone(),tag:tag.clone(),asset:serde_json::from_str(json).map_err(|e|e.to_string())?}},
            [command,id,json]if command=="install-confirm"=>Request{protocol:1,id:id.clone(),action:Action::InstallConfirm{plan:serde_json::from_str(json).map_err(|e|e.to_string())?}},
            [command,id]if command=="operation-status"=>Request{protocol:1,id:format!("status-{}",std::process::id()),action:Action::OperationStatus{request_id:id.clone()}},
            [command,id]if command=="cancel"=>Request{protocol:1,id:format!("cancel-{}",std::process::id()),action:Action::Cancel{request_id:id.clone()}},
            [command]if command=="request"=>{let mut bytes=Vec::new();std::io::stdin().take(actions::REQUEST_LIMIT as u64+1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;actions::decode(&bytes)?},
            [command,json]if command=="request"=>actions::decode(json.as_bytes())?,
            _=>return Err("Usage: craft-launcher-agent state | profiles | installations | update-all ID | open-apps-folder ID | request [JSON]. request without JSON reads at most 64 KiB from stdin. Each mutation requires a unique request ID. Use --mcp for on-demand MCP over stdio.".into()),
        };
        craft_app_launcher::agent::dispatch(request)
    })();
    let response=match result{Ok(v)=>v,Err(e)=>{eprintln!("{e}");Response::error("unavailable",if e.starts_with("OwnerBusy"){"owner_busy"}else{"unavailable"},e)}};
    match actions::encode(&response){Ok(bytes)=>{use std::io::Write;if std::io::stdout().write_all(&bytes).and_then(|_|std::io::stdout().write_all(b"\n")).is_err(){return 4;}},Err(e)=>{eprintln!("{e}");return 4;}}
    if response.ok{0}else if response.code=="owner_busy"{3}else{2}
}