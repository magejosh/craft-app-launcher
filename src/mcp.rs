//! On-demand SDK stdio server. The wrapper bounds bytes, not JSON-RPC semantics.
use craft_app_launcher::{actions,agent};
use rmcp::{ServerHandler,ServiceExt,RoleServer,ErrorData,model::*,service::RequestContext};
use std::{io,pin::Pin,sync::{Arc,atomic::{AtomicBool,Ordering}},task::{Context,Poll},time::Duration};
use tokio::io::{AsyncRead,ReadBuf};

struct BoundedInput<R>{inner:R,line:usize,failed:Arc<AtomicBool>}
impl<R:AsyncRead+Unpin> AsyncRead for BoundedInput<R>{
    fn poll_read(mut self:Pin<&mut Self>,cx:&mut Context<'_>,out:&mut ReadBuf<'_>)->Poll<io::Result<()>>{
        if out.remaining()==0{return Poll::Ready(Ok(()));}
        let mut bytes=[0u8;4096];let cap=out.remaining().min(bytes.len());let mut buf=ReadBuf::new(&mut bytes[..cap]);
        match Pin::new(&mut self.inner).poll_read(cx,&mut buf){
            Poll::Ready(Ok(()))=>{if buf.filled().is_empty()&&self.line!=0{self.failed.store(true,Ordering::Release);return Poll::Ready(Err(io::Error::new(io::ErrorKind::UnexpectedEof,"Incomplete MCP frame at EOF")));}for b in buf.filled(){if *b==b'\n'{self.line=0;}else{self.line+=1;if self.line>actions::REQUEST_LIMIT{self.failed.store(true,Ordering::Release);return Poll::Ready(Err(io::Error::new(io::ErrorKind::InvalidData,"MCP frame exceeds 64 KiB")));}}}out.put_slice(buf.filled());Poll::Ready(Ok(()))},
            Poll::Ready(Err(error))=>{self.failed.store(true,Ordering::Release);Poll::Ready(Err(error))},
            Poll::Pending=>Poll::Pending,
        }
    }
}
fn tool()->Tool{
    let mut schema=serde_json::to_value(schemars::schema_for!(actions::Request)).expect("static action schema");
    schema["properties"]["protocol"]=serde_json::json!({"type":"integer","const":1});
    schema["properties"]["id"]=serde_json::json!({"type":"string","minLength":1,"maxLength":80,"pattern":"^[A-Za-z0-9_.-]+$"});
    Tool::new("launcher_action","Run one explicitly selected launcher action. Read state/profiles/installations first. Mutations require a unique action id; launch requires explicit installation and profile IDs. Single install requires an official-metadata plan followed by explicit confirmation. Explicit update_all checks installed apps and internally issues/consumes verified plans sequentially; unknown versions/assets are skipped and preferences retained. open_apps_folder opens only the saved valid apps root. Calls can change settings, launch a known executable or download a verified official portable release. Never replay after cancellation, disconnect or unknown outcome; reconcile state manually. GUI receipts are bounded; headless work is synchronous with no durable receipt. Disconnect/cancel can leave a started headless action in progress or interrupted; after SDK handler draining and bounded runtime shutdown the process may exit before completion, leaving an unknown outcome. MCP request IDs are separate from action IDs.",schema.as_object().unwrap().clone())
        .with_annotations(ToolAnnotations::new().read_only(false).destructive(true).idempotent(false).open_world(true))
}
// The permit belongs to the worker, so aborting its async awaiter cannot admit overlap.
async fn dispatch_one<F>(gate:Arc<tokio::sync::Semaphore>,id:String,work:F)->Result<actions::Response,ErrorData>
where F:FnOnce()->actions::Response+Send+'static{
    match gate.try_acquire_owned(){
        Err(_)=>Ok(actions::Response::error(&id,"owner_busy","Another action is active; this action was not executed")),
        Ok(permit)=>tokio::task::spawn_blocking(move||{let _permit=permit;work()}).await
            .map_err(|_|ErrorData::internal_error("Action worker stopped; outcome unknown. Reconcile manually; do not replay.",None)),
    }
}
#[derive(Clone)]
struct Launcher{gate:Arc<tokio::sync::Semaphore>}
impl ServerHandler for Launcher{
    fn get_info(&self)->ServerConfig{
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_server_info(Implementation::new("craft-launcher-agent",env!("CARGO_PKG_VERSION")))
    }
    fn get_tool(&self,name:&str)->Option<Tool>{(name=="launcher_action").then(tool)}
    async fn list_tools(&self,_:Option<PaginatedRequestParams>,_:RequestContext<RoleServer>)->Result<ListToolsResult,ErrorData>{let mut result=ListToolsResult::default();result.tools=vec![tool()];Ok(result)}
    async fn call_tool(&self,request:CallToolRequestParams,context:RequestContext<RoleServer>)->Result<CallToolResponse,ErrorData>{
        if request.name!="launcher_action"||request.input_responses.is_some()||request.request_state.is_some(){return Err(ErrorData::invalid_params("Unknown tool or unsupported continuation",None));}
        let bytes=serde_json::to_vec(&request.arguments.ok_or_else(||ErrorData::invalid_params("Missing action arguments",None))?).map_err(|e|ErrorData::invalid_params(e.to_string(),None))?;
        let action=actions::decode(&bytes).map_err(|e|ErrorData::invalid_params(format!("Not executed: {e}"),None))?;
        let id=action.id.clone();
        let cancel=context.ct.clone();let worker_id=id.clone();
        let response=dispatch_one(self.gate.clone(),id,move||{
            if cancel.is_cancelled(){return actions::Response::error(&worker_id,"not_executed","Canceled before dispatch");}
            match agent::dispatch(action){Ok(response)=>response,Err(e)=>actions::Response::error(&worker_id,if e.starts_with("OwnerBusy"){"owner_busy"}else{"unavailable"},e)}
        }).await?;
        let bytes=actions::encode(&response).map_err(|e|ErrorData::internal_error(e,None))?;
        let value=serde_json::from_slice(&bytes).map_err(|e|ErrorData::internal_error(e.to_string(),None))?;
        Ok(if response.ok{CallToolResult::structured(value)}else{CallToolResult::structured_error(value)}.into())
    }
}
pub fn run()->i32{
    let runtime=match tokio::runtime::Builder::new_current_thread().enable_all().max_blocking_threads(4).build(){Ok(v)=>v,Err(e)=>{eprintln!("MCP runtime: {e}");return 2;}};
    let failed=Arc::new(AtomicBool::new(false));let flag=failed.clone();
    let result=runtime.block_on(async move{
        let(input,output)=rmcp::transport::stdio();
        let transport=rmcp::transport::async_rw::AsyncRwTransport::new_server(BoundedInput{inner:input,line:0,failed:flag},output);
        let service=Launcher{gate:Arc::new(tokio::sync::Semaphore::new(1))}.serve(transport).await.map_err(|e|e.to_string())?;
        service.waiting().await.map_err(|e|e.to_string())?;Ok::<(),String>(())
    });
    // An interrupted synchronous action may have side effects. No replay/rollback claim.
    runtime.shutdown_timeout(Duration::from_millis(500));
    match result{Err(e)=>{eprintln!("MCP session: {e}");2},Ok(())if failed.load(Ordering::Acquire)=>{eprintln!("MCP input I/O, incomplete frame, or 64 KiB limit failure; session closed");2},Ok(())=>0}
}

#[cfg(test)]
mod tests{
    use super::*;
    use tokio::io::AsyncReadExt;
    #[test]
    fn mcp_input_byte_bound_and_line_reset(){
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async{
            let flag=Arc::new(AtomicBool::new(false));
            let bytes=[vec![b'x';actions::REQUEST_LIMIT],b"\n".to_vec(),vec![b'y';actions::REQUEST_LIMIT],b"\n".to_vec()].concat();
            let mut input=BoundedInput{inner:bytes.as_slice(),line:0,failed:flag.clone()};let mut out=Vec::new();input.read_to_end(&mut out).await.unwrap();assert_eq!(out,bytes);assert!(!flag.load(Ordering::Acquire));
            let mut partial=BoundedInput{inner:&b"{}"[..],line:0,failed:flag.clone()};let mut partial_out=Vec::new();assert!(partial.read_to_end(&mut partial_out).await.is_err());flag.store(false,Ordering::Release);
            let bytes="é".repeat(actions::REQUEST_LIMIT/2+1).into_bytes();let mut input=BoundedInput{inner:bytes.as_slice(),line:0,failed:flag.clone()};let mut out=Vec::new();assert!(input.read_to_end(&mut out).await.is_err());assert!(out.len()<=actions::REQUEST_LIMIT);assert!(flag.load(Ordering::Acquire));
        });
    }
    #[test]
    fn mcp_action_schema_and_strict_decoder(){
        let schema=tool().schema_as_json_value();assert_eq!(schema["additionalProperties"],false);assert_eq!(schema["properties"]["protocol"]["const"],1);
        for name in ["state","profiles","reload","update_all","open_apps_folder"]{
            let valid=serde_json::json!({"protocol":1,"id":"schema-test","action":{"action":name}});assert!(actions::decode(&serde_json::to_vec(&valid).unwrap()).is_ok());
            let mut invalid=valid;invalid["action"]["extra"]=true.into();assert!(actions::decode(&serde_json::to_vec(&invalid).unwrap()).is_err());
        }
        for expected in [None,Some(serde_json::Value::Null)]{
            let mut patch=serde_json::json!({"field":{"field":"apps_root","value":"E:\\fixture"}});if let Some(v)=expected.clone(){patch["expected"]=v;}
            let request=serde_json::json!({"protocol":1,"id":"null-test","action":{"action":"patch","fields":[patch]}});
            let parsed=actions::decode(&serde_json::to_vec(&request).unwrap()).unwrap();let round=serde_json::to_value(parsed).unwrap();assert_eq!(round["action"]["fields"][0].get("expected"),expected.as_ref());
        }
        let defs=schema.get("$defs").unwrap();let field=&defs["FieldPatch"];assert!(!field["required"].as_array().unwrap().iter().any(|v|v=="expected"));assert!(field["properties"].get("expected").is_some());
    }
    #[test]
    fn mcp_canceled_awaiter_keeps_production_gate_until_worker_finishes(){
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async{
            let gate=Arc::new(tokio::sync::Semaphore::new(1));let worker_gate=gate.clone();
            let(started_tx,started_rx)=tokio::sync::oneshot::channel();let(release_tx,release_rx)=std::sync::mpsc::channel();
            let task=tokio::spawn(async move{dispatch_one(worker_gate,"blocked".into(),move||{
                started_tx.send(()).unwrap();release_rx.recv_timeout(Duration::from_secs(2)).unwrap();actions::Response::success("blocked",serde_json::json!({}))
            }).await});
            started_rx.await.unwrap();task.abort();assert!(task.await.unwrap_err().is_cancelled());
            let busy=dispatch_one(gate.clone(),"second".into(),||panic!("busy action must not execute")).await.unwrap();assert_eq!(busy.code,"owner_busy");
            release_tx.send(()).unwrap();let permit=tokio::time::timeout(Duration::from_secs(2),gate.clone().acquire_owned()).await.unwrap().unwrap();drop(permit);
            let after=dispatch_one(gate.clone(),"after".into(),||actions::Response::error("after","fixture","completed error")).await.unwrap();assert_eq!(after.code,"fixture");assert!(gate.try_acquire_owned().is_ok());
        });
    }
}
