//! Shared console and MCP action route; ownership is acquired for each action.
use crate::progress::WorkProgress;
use crate::{actions::{self,Request,Response},owner,native};
use std::sync::atomic::{AtomicBool};
pub fn dispatch(request:Request)->Result<Response,String>{
        let request=actions::decode(&serde_json::to_vec(&request).map_err(|e|e.to_string())?)?;
        let base=owner::base()?;owner::verify_pair(&base)?;
        if let Some(response)=owner::request(&base,&request)?{return Ok(response);}
        let _lock=owner::OwnerLock::acquire(&base)?;owner::refuse_running_gui(&base)?;
        owner::verify_pair(&base)?;
        let(mut config,mut observed)=actions::load(&base)?;
        let machine=native::machine_identity();
        let(installs,discovery)=actions::discover_for_action(&base,&config,&request.action);
        let view=actions::View{owner:"headless",discovery,work:serde_json::json!({"status":"synchronous","active_gui_work":false})};
        let value=actions::execute(&base,&mut config,&mut observed,machine.as_deref(),&installs,&request.action,&mut actions::Receipts::default(),&AtomicBool::new(false),&WorkProgress::new(0),&view);
        Ok(actions::response(&request.id,value))
}
