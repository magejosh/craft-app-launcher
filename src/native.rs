//! Small raw Win32 boundary; all other modules use owned Rust values for paths.
use std::{ffi::{c_void, OsStr, OsString}, os::windows::ffi::{OsStrExt,OsStringExt}, path::{Path,PathBuf}, ptr};
#[path="glass.rs"] mod glass;
pub type Hwnd = isize;
pub type Handle = isize;
pub type Lresult = isize;
pub fn wide(s: impl AsRef<OsStr>) -> Vec<u16> { s.as_ref().encode_wide().chain(Some(0)).collect() }
// LR_SHARED keeps these class icons alive for the process lifetime.
pub unsafe fn launcher_icon(instance:Handle,small:bool)->Handle {
    let (cx,cy)=if small{(49,50)}else{(11,12)}; // SM_CX/CYSMICON or SM_CX/CYICON
    LoadImageW(instance,101usize as *const u16,1,GetSystemMetrics(cx),GetSystemMetrics(cy),0x8000)
}
#[repr(C)] pub struct Point { pub x:i32, pub y:i32 }
#[repr(C)] pub struct Msg { pub hwnd:Hwnd, pub message:u32, pub wparam:usize, pub lparam:isize, pub time:u32, pub pt:Point, pub private:u32 }
#[derive(Clone, Copy)]
#[repr(C)] pub struct Rect { pub left:i32, pub top:i32, pub right:i32, pub bottom:i32 }
// DRAWITEMSTRUCT: pointer-sized fields preserve the Windows x64 ABI alignment.
#[repr(C)] pub struct DrawItem { pub control_type:u32, pub control_id:u32, pub item_id:u32, pub action:u32, pub state:u32, pub hwnd:Hwnd, pub dc:Handle, pub rect:Rect, pub data:usize }
#[repr(C)] pub struct WndClass { pub size:u32,pub style:u32,pub procedure:Option<unsafe extern "system" fn(Hwnd,u32,usize,isize)->Lresult>,pub cls_extra:i32,pub wnd_extra:i32,pub instance:Handle,pub icon:Handle,pub cursor:Handle,pub background:Handle,pub menu:*const u16,pub class:*const u16,pub small_icon:Handle }
#[repr(C)] struct ShellFileInfo { icon:Handle, index:i32, attributes:u32, display_name:[u16;260], type_name:[u16;80] }
#[repr(C)] pub struct MinMaxInfo { pub reserved:Point,pub max_size:Point,pub max_position:Point,pub min_track:Point,pub max_track:Point }
#[repr(C)] struct Guid { a:u32,b:u16,c:u16,d:[u8;8] }
#[link(name="user32")] extern "system" {
    pub fn RegisterClassExW(class:*const WndClass)->u16;
    pub fn CreateWindowExW(ex:u32,class:*const u16,title:*const u16,style:u32,x:i32,y:i32,width:i32,height:i32,parent:Hwnd,menu:Handle,instance:Handle,param:*mut c_void)->Hwnd;
    pub fn DefWindowProcW(hwnd:Hwnd,msg:u32,w:usize,l:isize)->Lresult;
    pub fn IsIconic(hwnd:Hwnd)->i32;
    pub fn GetForegroundWindow()->Hwnd;
    fn GetWindow(hwnd:Hwnd,command:u32)->Hwnd;
    fn SetForegroundWindow(hwnd:Hwnd)->i32;
    fn SetActiveWindow(hwnd:Hwnd)->Hwnd;
    pub fn IsWindowVisible(hwnd:Hwnd)->i32;
    pub fn GetWindowLongPtrW(hwnd:Hwnd,index:i32)->isize;
    fn SetWindowLongPtrW(hwnd:Hwnd,index:i32,value:isize)->isize;

    pub fn IsWindowEnabled(hwnd:Hwnd)->i32;
    pub fn ShowWindow(hwnd:Hwnd,cmd:i32)->i32;
    pub fn DestroyWindow(hwnd:Hwnd)->i32;
    pub fn GetClientRect(hwnd:Hwnd,rect:*mut Rect)->i32;
    pub fn GetWindowRect(hwnd:Hwnd,rect:*mut Rect)->i32;
    fn FillRect(dc:Handle,rect:*const Rect,brush:Handle)->i32;
    fn EnumChildWindows(hwnd:Hwnd,callback:Option<unsafe extern "system" fn(Hwnd,isize)->i32>,data:isize)->i32;
    pub fn GetDlgCtrlID(hwnd:Hwnd)->i32;
    pub fn MoveWindow(hwnd:Hwnd,x:i32,y:i32,w:i32,h:i32,repaint:i32)->i32;
    pub fn AdjustWindowRectEx(rect:*mut Rect,style:u32,menu:i32,extended:u32)->i32;
    pub fn SystemParametersInfoW(action:u32,param:u32,data:*mut c_void,flags:u32)->i32;
    pub fn CreatePopupMenu()->Handle;
    pub fn AppendMenuW(menu:Handle,flags:u32,id:usize,label:*const u16)->i32;
    pub fn TrackPopupMenu(menu:Handle,flags:u32,x:i32,y:i32,reserved:i32,owner:Hwnd,rect:*const Rect)->i32;
    pub fn DestroyMenu(menu:Handle)->i32;
    fn PrivateExtractIconsW(path:*const u16,index:i32,width:i32,height:i32,icons:*mut Handle,ids:*mut u32,count:u32,flags:u32)->u32;
    pub fn UpdateWindow(hwnd:Hwnd)->i32;
    pub fn GetMessageW(msg:*mut Msg,hwnd:Hwnd,min:u32,max:u32)->i32;
    pub fn TranslateMessage(msg:*const Msg)->i32;
    pub fn DispatchMessageW(msg:*const Msg)->Lresult;
    pub fn IsDialogMessageW(hwnd:Hwnd,msg:*const Msg)->i32;
    pub fn PostQuitMessage(code:i32);
    pub fn PostMessageW(hwnd:Hwnd,msg:u32,w:usize,l:isize)->i32;
    pub fn SetTimer(hwnd:Hwnd,id:usize,milliseconds:u32,callback:Option<unsafe extern "system" fn(Hwnd,u32,usize,u32)>)->usize;
    pub fn KillTimer(hwnd:Hwnd,id:usize)->i32;
    pub fn SendMessageW(hwnd:Hwnd,msg:u32,w:usize,l:isize)->Lresult;
    pub fn SetWindowTextW(hwnd:Hwnd,text:*const u16)->i32;
    pub fn GetWindowTextW(hwnd:Hwnd,text:*mut u16,max:i32)->i32;
    pub fn GetWindowTextLengthW(hwnd:Hwnd)->i32;
    pub fn EnableWindow(hwnd:Hwnd,enable:i32)->i32;
    pub fn SetFocus(hwnd:Hwnd)->Hwnd;
    pub fn GetDlgItem(hwnd:Hwnd,id:i32)->Hwnd;
    pub fn MessageBoxW(hwnd:Hwnd,text:*const u16,caption:*const u16,kind:u32)->i32;
    pub fn LoadCursorW(instance:Handle,name:*const u16)->Handle;
    pub fn LoadIconW(instance:Handle,name:*const u16)->Handle;
    fn LoadImageW(instance:Handle,name:*const u16,kind:u32,width:i32,height:i32,flags:u32)->Handle;
    fn GetSystemMetrics(index:i32)->i32;
    pub fn DestroyIcon(icon:Handle)->i32;
    fn SetPropW(hwnd:Hwnd,name:*const u16,value:Handle)->i32;
    fn GetPropW(hwnd:Hwnd,name:*const u16)->Handle;
    fn RemovePropW(hwnd:Hwnd,name:*const u16)->Handle;
    pub fn InvalidateRect(hwnd:Hwnd,rect:*const Rect,erase:i32)->i32;
    fn GetSysColor(index:i32)->u32;

    fn DrawIconEx(dc:Handle,x:i32,y:i32,icon:Handle,width:i32,height:i32,step:u32,brush:Handle,flags:u32)->i32;
    fn DrawTextW(dc:Handle,text:*const u16,count:i32,rect:*mut Rect,format:u32)->i32;
    fn DrawFocusRect(dc:Handle,rect:*const Rect)->i32;
}
#[link(name="kernel32")] extern "system" {
    pub fn GetModuleHandleW(name:*const u16)->Handle;

    fn MoveFileExW(from:*const u16,to:*const u16,flags:u32)->i32;
}
#[link(name="gdi32")] extern "system" {
    pub fn GetStockObject(index:i32)->Handle;
    fn SelectObject(dc:Handle,object:Handle)->Handle;
    fn SetBkColor(dc:Handle,color:u32)->u32;
    fn SetDCBrushColor(dc:Handle,color:u32)->u32;
    fn SetBkMode(dc:Handle,mode:i32)->i32;
    fn SetTextColor(dc:Handle,color:u32)->u32;
    fn SaveDC(dc:Handle)->i32;
    fn RestoreDC(dc:Handle,saved:i32)->i32;
    fn CreateFontW(height:i32,width:i32,escapement:i32,orientation:i32,weight:i32,italic:u32,underline:u32,strikeout:u32,charset:u32,out_precision:u32,clip_precision:u32,quality:u32,pitch_family:u32,face:*const u16)->Handle;
    pub fn DeleteObject(object:Handle)->i32;
    fn CreatePen(style:i32,width:i32,color:u32)->Handle;
    fn RoundRect(dc:Handle,left:i32,top:i32,right:i32,bottom:i32,width:i32,height:i32)->i32;
}
#[link(name="shell32")] extern "system" {
    fn ExtractIconExW(path:*const u16,index:i32,large:*mut Handle,small:*mut Handle,count:u32)->u32;
    fn SHGetFileInfoW(path:*const u16,attributes:u32,info:*mut ShellFileInfo,size:u32,flags:u32)->usize;
}
#[link(name="ole32")] extern "system" {
    pub fn CoInitializeEx(reserved:*mut c_void,flags:u32)->i32;
    pub fn CoUninitialize();
    fn CoTaskMemFree(memory:*mut c_void);
    fn CoCreateInstance(class:*const Guid,outer:*mut c_void,context:u32,iid:*const Guid,object:*mut *mut c_void)->i32;
}
#[link(name="version")] extern "system" {
    fn GetFileVersionInfoSizeW(path:*const u16,handle:*mut u32)->u32;
    fn GetFileVersionInfoW(path:*const u16,handle:u32,size:u32,data:*mut c_void)->i32;
    fn VerQueryValueW(data:*const c_void,key:*const u16,value:*mut *mut c_void,length:*mut u32)->i32;
}
#[link(name="advapi32")] extern "system" {
    fn RegGetValueW(key:Handle,subkey:*const u16,value:*const u16,flags:u32,kind:*mut u32,data:*mut c_void,size:*mut u32)->i32;
}
/// Read-only OS-install identity; compatibility fails closed if unavailable.
pub fn machine_identity()->Option<String> {
    unsafe {
        let mut buffer=[0u16;128];let mut bytes=std::mem::size_of_val(&buffer) as u32;
        // HKLM; REG_SZ only; explicitly use the 64-bit registry view.
        let result=RegGetValueW(-2147483646isize,wide(r"SOFTWARE\Microsoft\Cryptography").as_ptr(),wide("MachineGuid").as_ptr(),0x2|0x00010000,ptr::null_mut(),buffer.as_mut_ptr().cast(),&mut bytes);
        if result!=0 || bytes<2 || bytes as usize>std::mem::size_of_val(&buffer) || bytes%2!=0 {return None;}
        let length=buffer[..bytes as usize/2].iter().position(|v|*v==0)?;
        let value=String::from_utf16(&buffer[..length]).ok()?.trim().to_ascii_lowercase();
        if value.len()!=36 || !value.bytes().enumerate().all(|(i,v)| if matches!(i,8|13|18|23) {v==b'-'} else {v.is_ascii_hexdigit()}) {return None;}
        Some(value)
    }
}
pub fn replace_file(from:&Path,to:&Path)->Result<(),String> {
    unsafe { if MoveFileExW(wide(from).as_ptr(),wide(to).as_ptr(),0x1|0x8)!=0 { Ok(()) } else { Err(format!("Could not replace adjacent settings: {}",std::io::Error::last_os_error())) } }
}
pub fn file_version(path:&Path)->Option<String> {
    unsafe {
        let path=wide(path); let mut unused=0; let size=GetFileVersionInfoSizeW(path.as_ptr(),&mut unused);
        if size==0 || size>8*1024*1024 { return None; }
        // u32 storage provides the native fixed-info alignment.
        let mut data=vec![0u32;(size as usize+3)/4];
        if GetFileVersionInfoW(path.as_ptr(),0,size,data.as_mut_ptr().cast())==0 { return None; }
        let mut value=ptr::null_mut(); let mut length=0;
        if VerQueryValueW(data.as_ptr().cast(),wide("\\").as_ptr(),&mut value,&mut length)==0 || value.is_null() || length<52 { return None; }
        let fixed=std::slice::from_raw_parts(value.cast::<u32>(),13);
        if fixed[0]!=0xFEEF04BD { return None; }
        let ms=fixed[2]; let ls=fixed[3];
        Some(format!("{}.{}.{}.{}",ms>>16,ms&65535,ls>>16,ls&65535))
    }
}
// IFileOpenDialog inherits IFileDialog and IModalWindow. All COM methods below
// use the documented vtable indices and system ABI; every acquired interface is released.
unsafe fn com_method(object:*mut c_void,index:usize)->*const c_void { *(*(object as *const *const *const c_void)).add(index) }
unsafe fn com_release(object:*mut c_void) {
    if !object.is_null() {let release:unsafe extern "system" fn(*mut c_void)->u32=std::mem::transmute(com_method(object,2));release(object);}
}
pub fn folder_picker(owner:Hwnd,title:&str)->Option<PathBuf> {
    unsafe {
        let class=Guid{a:0xDC1C5A9C,b:0xE88A,c:0x4DDE,d:[0xA5,0xA1,0x60,0xF8,0x2A,0x20,0xAE,0xF7]};
        let iid=Guid{a:0xD57C7288,b:0xD4AD,c:0x4768,d:[0xBE,0x02,0x9D,0x96,0x95,0x32,0xD9,0x60]};
        let mut dialog=ptr::null_mut();
        let hr=CoCreateInstance(&class,ptr::null_mut(),1,&iid,&mut dialog);
        if hr<0 || dialog.is_null() {message(owner,&format!("Could not open the Windows folder picker (0x{:08X}). Enter the root in Settings instead.",hr as u32));return None;}
        let result=(|| -> Result<Option<PathBuf>,String> {
            let get_options:unsafe extern "system" fn(*mut c_void,*mut u32)->i32=std::mem::transmute(com_method(dialog,10));
            let set_options:unsafe extern "system" fn(*mut c_void,u32)->i32=std::mem::transmute(com_method(dialog,9));
            let set_title:unsafe extern "system" fn(*mut c_void,*const u16)->i32=std::mem::transmute(com_method(dialog,17));
            let show:unsafe extern "system" fn(*mut c_void,Hwnd)->i32=std::mem::transmute(com_method(dialog,3));
            let get_result:unsafe extern "system" fn(*mut c_void,*mut *mut c_void)->i32=std::mem::transmute(com_method(dialog,20));
            let mut options=0;
            if get_options(dialog,&mut options)<0 || set_options(dialog,options|0x20|0x40|0x800|0x8|0x02000000)<0 || set_title(dialog,wide(title).as_ptr())<0 {return Err("Windows could not configure the folder picker.".into());}
            let hr=show(dialog,owner);
            if hr as u32==0x800704C7 {return Ok(None);}
            if hr<0 {return Err(format!("Windows folder selection failed (0x{:08X}).",hr as u32));}
            let mut item=ptr::null_mut();if get_result(dialog,&mut item)<0 || item.is_null() {return Err("Windows returned no selected folder.".into());}
            let get_name:unsafe extern "system" fn(*mut c_void,u32,*mut *mut u16)->i32=std::mem::transmute(com_method(item,5));
            let mut name=ptr::null_mut();let hr=get_name(item,0x80058000,&mut name);com_release(item);
            if hr<0 || name.is_null() {if !name.is_null(){CoTaskMemFree(name.cast());}return Err("The selected item has no filesystem path.".into());}
            let mut length=0;while *name.add(length)!=0 {length+=1;}
            let path=PathBuf::from(OsString::from_wide(std::slice::from_raw_parts(name,length)));CoTaskMemFree(name.cast());Ok(Some(path))
        })();
        com_release(dialog);
        match result {Ok(path)=>path,Err(error)=>{message(owner,&error);None}}
    }
}
pub fn executable_icon(path:&Path)->Handle {
    unsafe {
        let path=wide(path);let mut icon=0;
        PrivateExtractIconsW(path.as_ptr(),0,96,96,&mut icon,ptr::null_mut(),1,0);
        if icon==0 {ExtractIconExW(path.as_ptr(),0,&mut icon,ptr::null_mut(),1);}
        if icon!=0 {return icon;}
        // CLI executables often omit icon resources. Ask Windows for their real
        // shell file icon (including its standard executable fallback).
        let mut info:ShellFileInfo=std::mem::zeroed();
        if SHGetFileInfoW(path.as_ptr(),0,&mut info,std::mem::size_of::<ShellFileInfo>() as u32,0x100)!=0 {return info.icon;}
        if info.icon!=0 {DestroyIcon(info.icon);}
        0
    }
}
pub fn text(hwnd:Hwnd)->String {
    unsafe { let len=GetWindowTextLengthW(hwnd); let mut buffer=vec![0u16;(len+1).max(1) as usize]; let actual=GetWindowTextW(hwnd,buffer.as_mut_ptr(),buffer.len() as i32); String::from_utf16_lossy(&buffer[..actual.max(0) as usize]) }
}
pub fn set_text(hwnd:Hwnd,value:&str) {if hwnd==0||text(hwnd)==value{return;}unsafe {SetWindowTextW(hwnd,wide(value).as_ptr());}}
pub fn message(hwnd:Hwnd,text:&str) { unsafe { MessageBoxW(hwnd,wide(text).as_ptr(),wide("Craft App Launcher").as_ptr(),0x40); } }

/// Only explicit dismissal of a foreground dialog may restore its launcher.
fn dismissal_restores_launcher(dialog:Hwnd,foreground:Hwnd)->bool {dialog!=0&&dialog==foreground}
#[test] fn dismissal_focus_policy_never_activates_from_background(){assert!(dismissal_restores_launcher(11,11));assert!(!dismissal_restores_launcher(11,22));assert!(!dismissal_restores_launcher(11,0));assert!(!dismissal_restores_launcher(0,0));}
pub fn is_foreground_window(hwnd:Hwnd)->bool {unsafe{dismissal_restores_launcher(hwnd,GetAncestor(GetForegroundWindow(),2))}}
// GA_ROOTOWNER follows GetParent, which misses owners of overlapped top-level windows.
fn owner_root(hwnd:Hwnd)->Hwnd {unsafe {
    let mut root=GetAncestor(hwnd,2);
    while root!=0 {let owner=GetWindow(root,4);if owner==0{break;}root=owner;}
    root
}}
#[test] fn completion_owner_chain_reaches_main_through_overlapped_settings(){unsafe {
    struct Hidden(Hwnd);impl Drop for Hidden{fn drop(&mut self){unsafe{DestroyWindow(self.0);}}}
    let create=|style,parent|{let hwnd=CreateWindowExW(0,wide("STATIC").as_ptr(),wide("owned hidden focus regression").as_ptr(),style,0,0,200,120,parent,0,GetModuleHandleW(ptr::null()),ptr::null_mut());assert_ne!(hwnd,0);assert_eq!(IsWindowVisible(hwnd),0);Hidden(hwnd)};
    let main=create(0x00CF0000,0);let settings=create(0x00CF0000,main.0);let report=create(0x80C80000,settings.0);
    assert_eq!(GetWindow(settings.0,4),main.0);assert_eq!(GetWindow(report.0,4),settings.0);
    assert_eq!(GetAncestor(report.0,3),settings.0,"GA_ROOTOWNER stops at owned overlapped Settings");
    assert_eq!(owner_root(settings.0),main.0);assert_eq!(owner_root(report.0),main.0);assert_eq!(owner_root(main.0),main.0);assert_eq!(owner_root(0),0);
}}
pub fn activate_window(hwnd:Hwnd){unsafe{if hwnd!=0&&IsWindowEnabled(hwnd)!=0&&IsWindowVisible(hwnd)!=0{SetForegroundWindow(hwnd);SetActiveWindow(hwnd);SetFocus(hwnd);}}}

// A bounded modal report owns its presentation data; paint/selection never borrow UI State.
struct CompletionReport {rows:Vec<(String,String)>,font:Handle,restore_focus:std::cell::Cell<bool>}
unsafe fn close_completion(hwnd:Hwnd){
    let data=GetPropW(hwnd,wide("CraftLauncher.CompletionData").as_ptr())as *const CompletionReport;
    if !data.is_null(){(*data).restore_focus.set(is_foreground_window(hwnd));}
    DestroyWindow(hwnd);
}
unsafe fn completion_detail(edit:Hwnd,value:&str) {
    #[link(name="user32")] extern "system" {fn ShowScrollBar(hwnd:Hwnd,bar:i32,show:i32)->i32;fn GetDC(hwnd:Hwnd)->Handle;fn ReleaseDC(hwnd:Hwnd,dc:Handle)->i32;}
    ShowScrollBar(edit,1,0);set_text(edit,value);
    SendMessageW(edit,0xB1,0,0);let first=SendMessageW(edit,0xCE,0,0);SendMessageW(edit,0xB6,0,-first);
    // Count the edit's wrapped lines at its full width, using its actual font height.
    let dc=GetDC(edit);if dc==0{ShowScrollBar(edit,1,1);return;}
    let previous=SelectObject(dc,SendMessageW(edit,0x31,0,0));let mut line:Rect=std::mem::zeroed();
    let height=DrawTextW(dc,wide("Ag").as_ptr(),-1,&mut line,0x400|0x20);SelectObject(dc,previous);ReleaseDC(edit,dc);
    let mut area:Rect=std::mem::zeroed();SendMessageW(edit,0xB2,0,(&mut area as *mut Rect)as isize);
    ShowScrollBar(edit,1,i32::from(height<=0||SendMessageW(edit,0xBA,0,0)*height as isize>(area.bottom-area.top)as isize));
}
unsafe extern "system" fn completion_proc(hwnd:Hwnd,msg:u32,w:usize,l:isize)->Lresult {
    match msg {
        0x14=>erase_background(hwnd,w as Handle),
        0x133..=0x138=>control_color(w as Handle,msg,l),
        0x2B if l!=0=>{draw_plain_button(&*(l as *const DrawItem));1},
        0x111=>{
            let id=(w&0xffff)as i32;
            if id==1||id==2 {close_completion(hwnd);return 0;}
            let data=GetPropW(hwnd,wide("CraftLauncher.CompletionData").as_ptr())as *const CompletionReport;
            if !data.is_null(){let report=&*data;if let Some((label,detail))=report.rows.get((id-100)as usize){
                set_text(GetDlgItem(hwnd,20),label);completion_detail(GetDlgItem(hwnd,21),detail);
                for row in 0..report.rows.len(){selected_button(GetDlgItem(hwnd,100+row as i32),row as i32==id-100);}
            }}0
        },
        0x10=>{close_completion(hwnd);0},
        0x82=>{RemovePropW(hwnd,wide("CraftLauncher.CompletionData").as_ptr());DefWindowProcW(hwnd,msg,w,l)},
        _=>DefWindowProcW(hwnd,msg,w,l),
    }
}
/// All catalogue outcomes remain visible; only unusually long selected details need scrolling.
pub fn completion_report(owner:Hwnd,summary:&str,rows:Vec<(String,String)>) {
    unsafe {
        #[link(name="user32")] extern "system" {fn IsWindow(hwnd:Hwnd)->i32;fn GetAncestor(hwnd:Hwnd,flags:u32)->Hwnd;}
        let instance=GetModuleHandleW(ptr::null());let name=wide("CraftLauncherCompletion");
        let class=WndClass{size:std::mem::size_of::<WndClass>()as u32,style:0,procedure:Some(completion_proc),cls_extra:0,wnd_extra:0,instance,icon:0,cursor:LoadCursorW(0,32512usize as *const u16),background:0,menu:ptr::null(),class:name.as_ptr(),small_icon:0};RegisterClassExW(&class);
        let mut area=Rect{left:0,top:0,right:1024,bottom:768};SystemParametersInfoW(0x30,0,(&mut area as *mut Rect).cast(),0);
        let width=920.min((area.right-area.left).max(640));let height=610.min((area.bottom-area.top).max(480));
        let owner=GetAncestor(owner,2);let main=owner_root(owner);let activate=is_foreground_window(owner)||is_foreground_window(main);let hwnd=CreateWindowExW(0x1,name.as_ptr(),wide("Update All results").as_ptr(),0x80C80000|0x02000000,area.left+(area.right-area.left-width)/2,area.top+(area.bottom-area.top-height)/2,width,height,owner,0,instance,ptr::null_mut());
        if hwnd==0{message(owner,summary);return;}
        let report=CompletionReport{rows:rows.into_iter().take(crate::core::APPS.len()).collect(),font:segoe_font(16),restore_focus:std::cell::Cell::new(false)};if SetPropW(hwnd,wide("CraftLauncher.CompletionData").as_ptr(),(&report as *const CompletionReport)as isize)==0{DestroyWindow(hwnd);DeleteObject(report.font);message(owner,summary);return;}
        let mut client:Rect=std::mem::zeroed();GetClientRect(hwnd,&mut client);let left=320.min(client.right/2);let body=client.bottom-136;let row_height=(body/crate::core::APPS.len() as i32).min(34);
        let child=|class:&str,text:&str,style:u32,x:i32,y:i32,width:i32,height:i32,id:i32|{
            let control=CreateWindowExW(0,wide(class).as_ptr(),wide(text).as_ptr(),0x50000000|style,x,y,width,height,hwnd,id as isize,instance,ptr::null_mut());SendMessageW(control,0x30,report.font as usize,0);plain_control(control);control
        };
        child("STATIC",summary,0,18,16,client.right-36,50,10);
        for (row,(label,_)) in report.rows.iter().enumerate(){child("BUTTON",label,0x10000|0xB,18,78+row as i32*row_height,left-28,row_height-3,100+row as i32);}
        child("STATIC","Details",0,left+10,78,client.right-left-28,34,20);
        child("EDIT","",0x10000|0x200000|0x4|0x40|0x800,left+10,116,client.right-left-28,body-38,21);
        child("BUTTON","OK",0x10000|0xB,client.right-118,client.bottom-48,100,32,1);
        if report.rows.is_empty(){completion_detail(GetDlgItem(hwnd,21),summary);}else{SendMessageW(hwnd,0x111,100,0);}
        let enabled=IsWindowEnabled(owner)!=0;EnableWindow(owner,0);theme_window(hwnd);ShowWindow(hwnd,if activate{5}else{4});UpdateWindow(hwnd);if activate{SetFocus(GetDlgItem(hwnd,1));}
        let mut msg:Msg=std::mem::zeroed();while IsWindow(hwnd)!=0{let result=GetMessageW(&mut msg,0,0,0);if result<=0{if result==0{PostQuitMessage(msg.wparam as i32);}break;}if IsDialogMessageW(hwnd,&msg)==0{TranslateMessage(&msg);DispatchMessageW(&msg);}}
        if IsWindow(hwnd)!=0{DestroyWindow(hwnd);}if enabled{EnableWindow(owner,1);}if report.restore_focus.get(){activate_window(main);}DeleteObject(report.font);
    }
}

/// The property borrows an icon owned by UI State. Remove it before destroying the icon.
pub fn set_button_icon(hwnd:Hwnd,icon:Handle)->bool {
    if hwnd==0 {return false;}
    unsafe {
        let key=wide("CraftLauncher.ButtonIcon");
        RemovePropW(hwnd,key.as_ptr());
        let ok=icon==0 || SetPropW(hwnd,key.as_ptr(),icon)!=0;
        InvalidateRect(hwnd,ptr::null(),0);
        ok
    }
}
pub fn segoe_font(pixels:i32)->Handle {unsafe {CreateFontW(-pixels,0,0,0,400,0,0,0,1,0,0,5,0,wide("Segoe UI").as_ptr())}}
pub fn set_detail_font(hwnd:Hwnd,font:Handle) {if hwnd==0{return;}unsafe {let key=wide("CraftLauncher.DetailFont");RemovePropW(hwnd,key.as_ptr());if font!=0{SetPropW(hwnd,key.as_ptr(),font);}}}
/// Drawing uses only control-local data, never the reentrant application RefCell.
pub unsafe fn draw_app_tile(item:&DrawItem) {
    let saved=SaveDC(item.dc);if saved==0{return;}
    let r=item.rect;let region=CreateEllipticRgn(r.left,r.top,r.right,r.bottom);
    if region!=0{ExtSelectClipRgn(item.dc,region,1);DeleteObject(region);if !glass::draw(item,0){draw_app_tile_content(item);}}
    RestoreDC(item.dc,saved);
}
unsafe fn draw_app_tile_content(item:&DrawItem) {
    let saved=SaveDC(item.dc);if saved==0{return;}
    let pressed=item.state&1!=0||button_flag(item.hwnd,"CraftLauncher.Click");let disabled=item.state&4!=0;let hot=!disabled&&button_flag(item.hwnd,"CraftLauncher.Hover");let p=palette();
    // Fill the clipped circle, then keep the centered pen safely inside its mask.
    SetDCBrushColor(item.dc,if pressed{p.pressed}else{p.surface});FillRect(item.dc,&item.rect,GetStockObject(18));
    let emphasis=hot||pressed;let inset=if emphasis{2}else{1};
    let pen=CreatePen(0,if emphasis{3}else{1},if emphasis||item.state&0x10!=0 {p.focus}else{p.border});
    SelectObject(item.dc,pen);SelectObject(item.dc,GetStockObject(5));
    Ellipse(item.dc,item.rect.left+inset,item.rect.top+inset,item.rect.right-inset,item.rect.bottom-inset);
    let width=item.rect.right-item.rect.left;
    let(icon_size,top,mut name_rect,mut version_rect,name_size,detail_size)=circle_content(item.rect,pressed);
    let icon=GetPropW(item.hwnd,wide("CraftLauncher.ButtonIcon").as_ptr());
    if icon!=0 {DrawIconEx(item.dc,item.rect.left+(width-icon_size)/2,top,icon,icon_size,icon_size,0,0,3);}
    let label=text(item.hwnd);let mut parts=label.splitn(2,'\n');let name=parts.next().unwrap_or("");let version=parts.next().unwrap_or("");
    let name_font=segoe_font(name_size);let detail_font=segoe_font(detail_size);
    SelectObject(item.dc,name_font);SetBkMode(item.dc,1);SetTextColor(item.dc,if disabled{p.muted}else{p.text});
    DrawTextW(item.dc,wide(name).as_ptr(),-1,&mut name_rect,0x1|0x4|0x20|0x8000);
    SelectObject(item.dc,detail_font);SetTextColor(item.dc,p.muted);
    DrawTextW(item.dc,wide(version).as_ptr(),-1,&mut version_rect,0x1|0x4|0x20|0x8000|0x800);
    if item.state&0x10!=0 && item.state&0x200==0{SelectObject(item.dc,GetStockObject(5));Ellipse(item.dc,4,4,width-4,width-4);}
    RestoreDC(item.dc,saved);DeleteObject(name_font);DeleteObject(detail_font);DeleteObject(pen);
}
#[repr(C)] struct Margins {left:i32,right:i32,top:i32,bottom:i32}
#[link(name="dwmapi")] extern "system" {
    fn DwmSetWindowAttribute(hwnd:Hwnd,attribute:u32,value:*const c_void,size:u32)->i32;
    fn DwmExtendFrameIntoClientArea(hwnd:Hwnd,margins:*const Margins)->i32;
    fn DwmIsCompositionEnabled(enabled:*mut i32)->i32;
}
#[link(name="kernel32")] extern "system" {fn GetSystemDirectoryW(path:*mut u16,size:u32)->u32;}
#[link(name="uxtheme")] extern "system" {
    fn BufferedPaintInit()->i32;
    fn BufferedPaintUnInit()->i32;
    fn BeginBufferedPaint(dc:Handle,rect:*const Rect,format:i32,params:*const c_void,buffer_dc:*mut Handle)->Handle;
    fn BufferedPaintClear(buffer:Handle,rect:*const Rect)->i32;

    fn EndBufferedPaint(buffer:Handle,update:i32)->i32;
}
// Per-UI-thread balanced lifetime; never initialize a runtime or create a canvas.
struct PaintRuntime(bool);
impl Drop for PaintRuntime {fn drop(&mut self){if self.0{unsafe {BufferedPaintUnInit();}}}}
thread_local! {static PAINT_RUNTIME:PaintRuntime=PaintRuntime(unsafe {BufferedPaintInit()>=0});}
fn buffered_paint_ready()->bool {PAINT_RUNTIME.with(|v|v.0)}
fn backdrop_supported()->bool {
    // File version avoids GetVersionEx's application-manifest compatibility lie.
    // A failed/unrecognized identity conservatively keeps the solid fallback.
    static SUPPORTED:std::sync::OnceLock<bool>=std::sync::OnceLock::new();
    *SUPPORTED.get_or_init(||unsafe {
        let mut path=[0u16;32768];let length=GetSystemDirectoryW(path.as_mut_ptr(),path.len()as u32)as usize;
        if length==0||length>=path.len(){return false;}
        let path=PathBuf::from(OsString::from_wide(&path[..length])).join("kernel32.dll");
        let Some(version)=file_version(&path) else{return false;};
        let parts:Vec<u32>=version.split('.').filter_map(|v|v.parse().ok()).collect();
        parts.len()==4 && parts[0]>=10 && parts[2]>=22621
    })
}
fn transparency_enabled()->bool {unsafe {
    let mut value=0u32;let mut bytes=4;
    // Read only; missing/unreadable user preference fails closed. DWM still owns
    // material policy (power saving, remote sessions and inactive-window fallback).
    RegGetValueW(-2147483647isize,wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize").as_ptr(),wide("EnableTransparency").as_ptr(),0x10,ptr::null_mut(),(&mut value as *mut u32).cast(),&mut bytes)==0 && bytes==4 && value==1
}}
fn overlapped_content_enabled()->bool {unsafe {
    let mut disabled=1i32;
    // SPI_GETDISABLEOVERLAPPEDCONTENT: accessibility requests simpler visuals.
    SystemParametersInfoW(0x1040,0,(&mut disabled as *mut i32).cast(),0)!=0 && disabled==0
}}
unsafe fn reset_backdrop(hwnd:Hwnd) {
    RemovePropW(hwnd,wide("CraftLauncher.Backdrop").as_ptr());
    glass::opaque_controls(hwnd,false);
    let none=1i32;DwmSetWindowAttribute(hwnd,38,(&none as *const i32).cast(),4);
    DwmExtendFrameIntoClientArea(hwnd,&Margins{left:0,right:0,top:0,bottom:0});
    InvalidateRect(hwnd,ptr::null(),1);EnumChildWindows(hwnd,Some(repaint_child),0);
}
unsafe fn configure_backdrop(hwnd:Hwnd)->bool {
    reset_backdrop(hwnd);
    if MODE.with(|v|v.get())!=crate::core::ThemeMode::Aetherglass || high_contrast() || !backdrop_supported() || !transparency_enabled() || !overlapped_content_enabled() || !buffered_paint_ready(){return false;}
    let mut composition=0;
    if DwmIsCompositionEnabled(&mut composition)<0 || composition==0{return false;}
    // Documented DWMWA_SYSTEMBACKDROP_TYPE / DWMSBT_TRANSIENTWINDOW:
    // Desktop Acrylic on Windows 11. A successful request is not visual proof.
    let acrylic=3i32;
    if DwmSetWindowAttribute(hwnd,38,(&acrylic as *const i32).cast(),4)<0{reset_backdrop(hwnd);return false;}
    if DwmExtendFrameIntoClientArea(hwnd,&Margins{left:-1,right:-1,top:-1,bottom:-1})<0 || SetPropW(hwnd,wide("CraftLauncher.Backdrop").as_ptr(),1)==0 {reset_backdrop(hwnd);return false;}
    if !glass::opaque_controls(hwnd,true){reset_backdrop(hwnd);return false;}
    true
}
#[link(name="uxtheme")] extern "system" {fn SetWindowTheme(hwnd:Hwnd,app:*const u16,ids:*const u16)->i32;}
#[derive(Clone,Copy)]
struct Palette {background:u32,surface:u32,pressed:u32,text:u32,muted:u32,border:u32,focus:u32}
thread_local! {
    static MODE:std::cell::Cell<crate::core::ThemeMode>=const {std::cell::Cell::new(crate::core::ThemeMode::System)};

    static DARK:std::cell::Cell<bool>=const {std::cell::Cell::new(false)};
    static HIGH_CONTRAST:std::cell::Cell<bool>=const {std::cell::Cell::new(false)};
}
fn plain_palette(mode:crate::core::ThemeMode,dark:bool)->Palette {
    use crate::core::ThemeMode;
    if mode==ThemeMode::TrueDark {return Palette{background:0x000000,surface:0x0A0A0A,pressed:0x141414,text:0xF3F3F3,muted:0xBEBEBE,border:0x242424,focus:0xFFCD60};}
    if mode==ThemeMode::Aetherglass {return Palette{background:0x242220,surface:0x302D29,pressed:0x403C36,text:0xF6F3EF,muted:0xD0CAC4,border:0x8C8278,focus:0xFFD090};}
    if dark {Palette{background:0x202020,surface:0x2B2B2B,pressed:0x3D3D3D,text:0xF3F3F3,muted:0xBEBEBE,border:0x707070,focus:0xEFC783}}
    else {Palette{background:0xF4F4F4,surface:0xFFFFFF,pressed:0xE5E5E5,text:0x202020,muted:0x606060,border:0xAAAAAA,focus:0xB45A00}}
}
fn palette()->Palette {
    if HIGH_CONTRAST.with(|v|v.get()) {unsafe {return Palette{background:GetSysColor(15),surface:GetSysColor(5),pressed:GetSysColor(15),text:GetSysColor(8),muted:GetSysColor(17),border:GetSysColor(8),focus:GetSysColor(13)};}}
    plain_palette(MODE.with(|v|v.get()),DARK.with(|v|v.get()))
}
pub fn system_apps_dark()->bool {
    unsafe {let mut value=1u32;let mut bytes=4;RegGetValueW(-2147483647isize,wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize").as_ptr(),wide("AppsUseLightTheme").as_ptr(),0x10,ptr::null_mut(),(&mut value as *mut u32).cast(),&mut bytes)==0 && bytes==4 && value==0}
}
pub fn set_theme_palette(mode:crate::core::ThemeMode,system_dark:bool) {
    MODE.with(|v|v.set(mode));let dark=mode.is_dark(system_dark);
    #[repr(C)] struct HighContrast {size:u32,flags:u32,scheme:*mut u16}
    let mut contrast=HighContrast{size:std::mem::size_of::<HighContrast>()as u32,flags:0,scheme:ptr::null_mut()};
    let high=unsafe {SystemParametersInfoW(0x42,contrast.size,(&mut contrast as *mut HighContrast).cast(),0)==0 || contrast.flags&1!=0};
    HIGH_CONTRAST.with(|v|v.set(high));DARK.with(|v|v.set(dark && !high));
}
unsafe extern "system" fn repaint_child(hwnd:Hwnd,_:isize)->i32 {InvalidateRect(hwnd,ptr::null(),1);1}
// DWMWA_COLOR_DEFAULT resets caption/text colors to Windows policy, including high contrast.
const DWM_COLOR_DEFAULT:u32=0xFFFFFFFF;
pub fn theme_window(hwnd:Hwnd) {
    if hwnd==0{return;}
    unsafe {let dark:i32=i32::from(DARK.with(|v|v.get()));
        // Documented DWMWA_USE_IMMERSIVE_DARK_MODE. Unsupported Windows versions
        // return failure and retain their native chrome; no ordinal/registry hacks.
        DwmSetWindowAttribute(hwnd,20,(&dark as *const i32).cast(),4);
        let glass=configure_backdrop(hwnd);
        let p=palette();let caption=if glass || HIGH_CONTRAST.with(|v|v.get()){DWM_COLOR_DEFAULT}else{p.background};let text=if HIGH_CONTRAST.with(|v|v.get()){DWM_COLOR_DEFAULT}else{p.text};
        DwmSetWindowAttribute(hwnd,35,(&caption as *const u32).cast(),4);
        DwmSetWindowAttribute(hwnd,36,(&text as *const u32).cast(),4);
        InvalidateRect(hwnd,ptr::null(),1);EnumChildWindows(hwnd,Some(repaint_child),0);
    }
}
pub fn plain_control(hwnd:Hwnd) {unsafe {SetWindowTheme(hwnd,wide("").as_ptr(),wide("").as_ptr());glass::new_control(hwnd);}}
/// Paint paths access only small Cells and control properties, never application STATE.
pub unsafe fn erase_background(hwnd:Hwnd,dc:Handle)->Lresult {
    let mut rect:Rect=std::mem::zeroed();GetClientRect(hwnd,&mut rect);
    if rect.right<=rect.left || rect.bottom<=rect.top{return 1;}
    if button_flag(hwnd,"CraftLauncher.Backdrop") {
        let mut buffer_dc=0;let buffer=BeginBufferedPaint(dc,&rect,2,ptr::null(),&mut buffer_dc);
        if buffer!=0 {
            let cleared=BufferedPaintClear(buffer,ptr::null())>=0;
            let committed=EndBufferedPaint(buffer,i32::from(cleared))>=0;
            if cleared && committed{return 1;}
        }
        reset_backdrop(hwnd); // Never leave an extended frame with a failed alpha surface.
        EnumChildWindows(hwnd,Some(repaint_child),0);
    }
    SetDCBrushColor(dc,palette().background);FillRect(dc,&rect,GetStockObject(18));1
}
pub unsafe fn control_color(dc:Handle,msg:u32,hwnd:Hwnd)->Lresult {
    let p=palette();let background=if matches!(msg,0x133|0x134)||button_flag(hwnd,"CraftLauncher.Surface"){p.surface}else{p.background};
    SetTextColor(dc,if IsWindowEnabled(hwnd)!=0{p.text}else{p.muted});SetBkColor(dc,background);SetDCBrushColor(dc,background);GetStockObject(18)
}
pub unsafe fn draw_plain_button(item:&DrawItem) {if !glass::draw(item,1){draw_plain_button_content(item);}}
unsafe fn draw_plain_button_content(item:&DrawItem) {
    let saved=SaveDC(item.dc);if saved==0{return;}let p=palette();let pressed=item.state&1!=0;let disabled=item.state&4!=0;let selected=button_flag(item.hwnd,"CraftLauncher.Selected");
    SetDCBrushColor(item.dc,p.background);FillRect(item.dc,&item.rect,GetStockObject(18));
    SetDCBrushColor(item.dc,if pressed{p.pressed}else{p.surface});SelectObject(item.dc,GetStockObject(18));
    let pen=CreatePen(0,if selected{3}else{1},if selected||item.state&0x10!=0{p.focus}else{p.border});SelectObject(item.dc,pen);
    RoundRect(item.dc,item.rect.left,item.rect.top,item.rect.right,item.rect.bottom,6,6);
    let font=SendMessageW(item.hwnd,0x31,0,0);SelectObject(item.dc,if font!=0{font}else{GetStockObject(17)});SetBkMode(item.dc,1);SetTextColor(item.dc,if disabled{p.muted}else{p.text});
    let mut rect=item.rect;rect.left+=4;rect.right-=4;DrawTextW(item.dc,wide(text(item.hwnd)).as_ptr(),-1,&mut rect,0x1|0x4|0x20);
    if item.state&0x10!=0 && item.state&0x200==0 {rect.left+=2;rect.top+=4;rect.right-=2;rect.bottom-=4;DrawFocusRect(item.dc,&rect);}
    RestoreDC(item.dc,saved);DeleteObject(pen);
}
pub fn high_contrast()->bool {HIGH_CONTRAST.with(|v|v.get())}
#[cfg(test)] mod theme_tests {
    use super::*;use crate::core::ThemeMode;
    fn luminance(color:u32)->f64 {let channel=|shift:u32|{let c=((color>>shift)&255u32) as f64/255.0;if c<=0.04045{c/12.92}else{((c+0.055)/1.055).powf(2.4)}};0.2126*channel(0)+0.7152*channel(8)+0.0722*channel(16)}
    fn contrast(a:u32,b:u32)->f64 {let a=luminance(a);let b=luminance(b);(a.max(b)+0.05)/(a.min(b)+0.05)}
    #[test] fn four_palettes_have_readable_text_and_focus() {
        for mode in [ThemeMode::Light,ThemeMode::Dark,ThemeMode::TrueDark,ThemeMode::Aetherglass] {let p=plain_palette(mode,mode.is_dark(false));for background in [p.background,p.surface,p.pressed] {assert!(contrast(p.text,background)>=4.5,"{mode:?} text");assert!(contrast(p.muted,background)>=4.5,"{mode:?} secondary/disabled");assert!(contrast(p.focus,background)>=3.0,"{mode:?} focus");}}
        let p=plain_palette(ThemeMode::TrueDark,true);assert_eq!((p.background,p.surface,p.pressed,p.border),(0,0x0A0A0A,0x141414,0x242424));
    }
}
#[link(name="winhttp")] extern "system" {
    fn WinHttpOpen(agent:*const u16,access:u32,proxy:*const u16,bypass:*const u16,flags:u32)->Handle;
    fn WinHttpConnect(session:Handle,server:*const u16,port:u16,reserved:u32)->Handle;
    fn WinHttpOpenRequest(connection:Handle,verb:*const u16,path:*const u16,version:*const u16,referrer:*const u16,accept:*const *const u16,flags:u32)->Handle;
    fn WinHttpSetTimeouts(handle:Handle,resolve:i32,connect:i32,send:i32,receive:i32)->i32;
    fn WinHttpSetOption(handle:Handle,option:u32,value:*const c_void,size:u32)->i32;
    fn WinHttpSendRequest(request:Handle,headers:*const u16,length:u32,optional:*const c_void,optional_length:u32,total:u32,context:usize)->i32;
    fn WinHttpReceiveResponse(request:Handle,reserved:*mut c_void)->i32;
    fn WinHttpQueryHeaders(request:Handle,level:u32,name:*const u16,buffer:*mut c_void,length:*mut u32,index:*mut u32)->i32;
    fn WinHttpReadData(request:Handle,buffer:*mut c_void,size:u32,read:*mut u32)->i32;
    fn WinHttpCloseHandle(handle:Handle)->i32;
}
struct HttpHandle(Handle);
impl Drop for HttpHandle {fn drop(&mut self){if self.0!=0 {unsafe {WinHttpCloseHandle(self.0);}}}}
fn http_error(action:&str)->String {format!("{action}: {}. No version conclusion was made.",std::io::Error::last_os_error())}
/// Only fixed official latest-release endpoints; unauthenticated direct TLS 1.2.
/// No redirects/cookies/authentication, no response writes, no retries by this code.
pub fn github_release_get(path:&str,maximum:usize)->Result<Vec<u8>,String> {
    if !crate::core::APPS.iter().any(|(id,_)|path==format!("/repos/storytold/{id}/releases/latest")){return Err("Unrecognized official release endpoint".into());}
    unsafe {
        let started=std::time::Instant::now();let deadline=std::time::Duration::from_secs(25);
        let session=HttpHandle(WinHttpOpen(wide("Craft-App-Launcher/0.1").as_ptr(),1,ptr::null(),ptr::null(),0));if session.0==0{return Err(http_error("Could not create HTTPS session"));}
        let tls=0x800u32;
        if WinHttpSetTimeouts(session.0,5000,5000,5000,5000)==0||WinHttpSetOption(session.0,84,(&tls as *const u32).cast(),4)==0{return Err(http_error("Could not configure bounded TLS request"));}
        let connection=HttpHandle(WinHttpConnect(session.0,wide("api.github.com").as_ptr(),443,0));if connection.0==0{return Err(http_error("Could not connect to api.github.com"));}
        let request=HttpHandle(WinHttpOpenRequest(connection.0,wide("GET").as_ptr(),wide(path).as_ptr(),ptr::null(),ptr::null(),ptr::null(),0x00800000));if request.0==0{return Err(http_error("Could not create release request"));}
        let redirects=0u32;let disable=1u32|4;
        if WinHttpSetOption(request.0,88,(&redirects as *const u32).cast(),4)==0||WinHttpSetOption(request.0,63,(&disable as *const u32).cast(),4)==0{return Err(http_error("Could not disable redirects/authentication"));}
        let headers=wide("Accept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28\r\n");
        if WinHttpSendRequest(request.0,headers.as_ptr(),(headers.len()-1)as u32,ptr::null(),0,0,0)==0{return Err(http_error("Release request failed (direct HTTPS; a required proxy may be unavailable)"));}
        if started.elapsed()>deadline{return Err("Release check exceeded its time budget".into());}
        if WinHttpReceiveResponse(request.0,ptr::null_mut())==0{return Err(http_error("No HTTPS release response"));}
        let mut status=0u32;let mut length=4;
        if WinHttpQueryHeaders(request.0,19|0x20000000,ptr::null(),(&mut status as *mut u32).cast(),&mut length,ptr::null_mut())==0{return Err(http_error("Could not read HTTP status"));}
        if status!=200{return Err(match status{403|429=>format!("GitHub denied/rate-limited this unauthenticated check (HTTP {status}); try later."),404=>"No accessible official stable release (HTTP 404).".into(),300..=399=>"GitHub returned a redirect; repository identity needs review. Redirect was not followed.".into(),_=>format!("GitHub returned HTTP {status}; version status is unknown.")});}
        let mut body=Vec::new();let mut buffer=[0u8;8192];loop{
            if started.elapsed()>deadline{return Err("Release check exceeded its time budget".into());}
            let mut read=0;if WinHttpReadData(request.0,buffer.as_mut_ptr().cast(),buffer.len()as u32,&mut read)==0{return Err(http_error("Could not read release metadata"));}if read==0{break;}
            if body.len()+read as usize>maximum{return Err("Release metadata exceeded the 2 MiB size limit".into());}body.extend_from_slice(&buffer[..read as usize]);
        }
        if started.elapsed()>deadline{return Err("Release check exceeded its time budget".into());}Ok(body)
    }
}
#[repr(C)] struct TrackMouse {size:u32,flags:u32,hwnd:Hwnd,hover_time:u32}
type Subclass=unsafe extern "system" fn(Hwnd,u32,usize,isize,usize,usize)->Lresult;
#[link(name="comctl32")] extern "system" {
    fn SetWindowSubclass(hwnd:Hwnd,callback:Option<Subclass>,id:usize,data:usize)->i32;
    fn RemoveWindowSubclass(hwnd:Hwnd,callback:Option<Subclass>,id:usize)->i32;
    fn DefSubclassProc(hwnd:Hwnd,msg:u32,w:usize,l:isize)->Lresult;
}
#[link(name="user32")] extern "system" {
    fn TrackMouseEvent(track:*mut TrackMouse)->i32;
    fn GetAncestor(hwnd:Hwnd,flags:u32)->Hwnd;
}
const FEEDBACK_TIMER:usize=0xCA11;
fn button_flag(hwnd:Hwnd,key:&str)->bool {unsafe {GetPropW(hwnd,wide(key).as_ptr())!=0}}
unsafe fn clear_feedback(hwnd:Hwnd) {KillTimer(hwnd,FEEDBACK_TIMER);RemovePropW(hwnd,wide("CraftLauncher.Click").as_ptr());RemovePropW(hwnd,wide("CraftLauncher.Hover").as_ptr());InvalidateRect(hwnd,ptr::null(),0);}
pub fn click_feedback(hwnd:Hwnd) {unsafe {
    if hwnd==0||IsWindowEnabled(hwnd)==0||IsWindowVisible(hwnd)==0||IsIconic(GetAncestor(hwnd,2))!=0{return;}
    SetPropW(hwnd,wide("CraftLauncher.Click").as_ptr(),1);InvalidateRect(hwnd,ptr::null(),0);UpdateWindow(hwnd);
    if SetTimer(hwnd,FEEDBACK_TIMER,160,None)==0{RemovePropW(hwnd,wide("CraftLauncher.Click").as_ptr());InvalidateRect(hwnd,ptr::null(),0);}
}}
unsafe extern "system" fn tile_subclass(hwnd:Hwnd,msg:u32,w:usize,l:isize,_id:usize,_data:usize)->Lresult {
    match msg {
        0x14=>return 1, // Ownerdraw fills only the circle; no rectangular erase.
        0x87=>{return DefSubclassProc(hwnd,msg,w,l)|1;}, // DLGC_WANTARROWS; Tab remains native dialog navigation.
        0x100 if (37..=40).contains(&w)=>{let parent=GetAncestor(hwnd,1);let mut r:Rect=std::mem::zeroed();GetClientRect(parent,&mut r);let bounds=circles::bounds(r.right,r.bottom);let index=(GetDlgCtrlID(hwnd)-300)as usize;if index<bounds.len(){let a=bounds[index];let mut best=None;for(i,b)in bounds.iter().enumerate(){if i==index{continue;}let dx=(b.left+b.right-a.left-a.right)as f64;let dy=(b.top+b.bottom-a.top-a.bottom)as f64;let (along,cross)=match w{37=>(-dx,dy),38=>(-dy,dx),39=>(dx,dy),_=>(dy,dx)};if along>0.{let score=along+cross.abs()*3.;if best.map(|(_,v)|score<v).unwrap_or(true){best=Some((i,score));}}}if let Some((i,_))=best{SetFocus(GetDlgItem(parent,300+i as i32));}}return 0;},
        0x202=>{let mut rect:Rect=std::mem::zeroed();GetClientRect(hwnd,&mut rect);let x=(l as u32&65535)as i16 as i32;let y=((l as u32>>16)&65535)as i16 as i32;if !circles::contains(rect.right,rect.bottom,x,y){SendMessageW(hwnd,0xF3,0,0);ReleaseCapture();return 0;}if SendMessageW(hwnd,0xF2,0,0)&4!=0{click_feedback(hwnd);}},
        0x200=>{if !button_flag(hwnd,"CraftLauncher.Hover"){let mut track=TrackMouse{size:std::mem::size_of::<TrackMouse>()as u32,flags:2,hwnd,hover_time:0};if TrackMouseEvent(&mut track)!=0{SetPropW(hwnd,wide("CraftLauncher.Hover").as_ptr(),1);InvalidateRect(hwnd,ptr::null(),0);}}},
        0x2A3=>{RemovePropW(hwnd,wide("CraftLauncher.Hover").as_ptr());InvalidateRect(hwnd,ptr::null(),0);},
        0x101 if w==32=>click_feedback(hwnd),0xF5=>click_feedback(hwnd),
        0x113 if w==FEEDBACK_TIMER=>{KillTimer(hwnd,FEEDBACK_TIMER);RemovePropW(hwnd,wide("CraftLauncher.Click").as_ptr());if IsWindowVisible(hwnd)!=0&&IsIconic(GetAncestor(hwnd,2))==0{InvalidateRect(hwnd,ptr::null(),0);}return 0;},
        0x1F=>clear_feedback(hwnd),
        0x82=>{clear_feedback(hwnd);RemoveWindowSubclass(hwnd,Some(tile_subclass),1);},_=>(),
    }
    DefSubclassProc(hwnd,msg,w,l)
}
pub fn install_tile_feedback(hwnd:Hwnd)->bool {unsafe {SetWindowSubclass(hwnd,Some(tile_subclass),1,0)!=0}}

pub fn official_release_page(hwnd:Hwnd,app:usize)->Result<(),String>{
    #[link(name="shell32")] extern "system" {fn ShellExecuteW(hwnd:Hwnd,operation:*const u16,file:*const u16,parameters:*const u16,directory:*const u16,show:i32)->Handle;}
    let url=format!("{}/releases/latest",crate::updates::repository(app)?);let result=unsafe {ShellExecuteW(hwnd,wide("open").as_ptr(),wide(&url).as_ptr(),ptr::null(),ptr::null(),1)};
    if result<=32{Err(format!("Could not open the official release page (Windows {result}). {url}"))}else{Ok(())}
}
pub fn download_asset(url:&str,size:u64,file:&mut std::fs::File,cancel:&std::sync::atomic::AtomicBool,progress:&crate::progress::WorkProgress)->Result<(),String>{
    use std::{io::Write,sync::atomic::Ordering};let started=std::time::Instant::now();let mut current=url.to_string();
    unsafe {let session=HttpHandle(WinHttpOpen(wide("Craft-App-Launcher/0.1").as_ptr(),1,ptr::null(),ptr::null(),0));if session.0==0{return Err(http_error("Could not start download"));}
        let tls=0x800u32;if WinHttpSetTimeouts(session.0,5000,5000,10000,10000)==0||WinHttpSetOption(session.0,84,(&tls as *const u32).cast(),4)==0{return Err(http_error("Could not bound download request"));}
        for hop in 0..4 {
            if cancel.load(Ordering::Relaxed){return Err("Download cancelled".into());}
            let (host,path)=crate::installer::download_origin(&current,hop==0)?;
            let connection=HttpHandle(WinHttpConnect(session.0,wide(host).as_ptr(),443,0));if connection.0==0{return Err(http_error("Download connection failed"));}
            let request=HttpHandle(WinHttpOpenRequest(connection.0,wide("GET").as_ptr(),wide(path).as_ptr(),ptr::null(),ptr::null(),ptr::null(),0x00800000));if request.0==0{return Err(http_error("Download request failed"));}
            let redirect=0u32;let disabled=5u32;
            if WinHttpSetOption(request.0,88,(&redirect as *const u32).cast(),4)==0||WinHttpSetOption(request.0,63,(&disabled as *const u32).cast(),4)==0{return Err(http_error("Could not secure download request"));}
            if WinHttpSendRequest(request.0,ptr::null(),0,ptr::null(),0,0,0)==0||WinHttpReceiveResponse(request.0,ptr::null_mut())==0{return Err(http_error("Download response failed"));}
            let mut status=0u32;let mut bytes=4;if WinHttpQueryHeaders(request.0,19|0x20000000,ptr::null(),(&mut status as *mut u32).cast(),&mut bytes,ptr::null_mut())==0{return Err(http_error("Could not read download status"));}
            if matches!(status,301|302|303|307|308){let mut location=[0u16;8192];let mut bytes=std::mem::size_of_val(&location)as u32;if WinHttpQueryHeaders(request.0,33,ptr::null(),location.as_mut_ptr().cast(),&mut bytes,ptr::null_mut())==0{return Err("Missing or oversized redirect location".into());}let length=location.iter().position(|c|*c==0).ok_or("Unterminated redirect")?;current=String::from_utf16(&location[..length]).map_err(|_|"Invalid redirect")?;crate::installer::download_origin(&current,false)?;continue;}
            if status!=200{return Err(format!("Official asset download returned HTTP {status}; nothing was installed."));}
            let mut total=0u64;let mut buffer=[0u8;65536];loop{if cancel.load(Ordering::Relaxed){return Err("Download cancelled".into());}if started.elapsed()>std::time::Duration::from_secs(600){return Err("Download exceeded 10-minute budget".into());}let mut read=0;if WinHttpReadData(request.0,buffer.as_mut_ptr().cast(),buffer.len()as u32,&mut read)==0{return Err(http_error("Download interrupted"));}if read==0{break;}total+=read as u64;if total>size{return Err("Download exceeded official asset size".into());}file.write_all(&buffer[..read as usize]).map_err(|e|e.to_string())?;progress.store(total,Ordering::Relaxed);}
            if total!=size{return Err("Download length does not match official asset metadata".into());}file.sync_all().map_err(|e|e.to_string())?;return Ok(());
        }Err("Too many asset redirects; nothing was installed".into())
    }
}
#[link(name="bcrypt")] extern "system" {
    fn BCryptOpenAlgorithmProvider(handle:*mut Handle,algorithm:*const u16,implementation:*const u16,flags:u32)->i32;
    fn BCryptCloseAlgorithmProvider(handle:Handle,flags:u32)->i32;
    fn BCryptGetProperty(handle:Handle,property:*const u16,output:*mut u8,size:u32,result:*mut u32,flags:u32)->i32;
    fn BCryptCreateHash(algorithm:Handle,hash:*mut Handle,object:*mut u8,object_size:u32,secret:*const u8,secret_size:u32,flags:u32)->i32;
    fn BCryptHashData(hash:Handle,input:*const u8,length:u32,flags:u32)->i32;
    fn BCryptFinishHash(hash:Handle,output:*mut u8,length:u32,flags:u32)->i32;
    fn BCryptDestroyHash(hash:Handle)->i32;
}
pub fn sha256_file(file:&mut (impl std::io::Read+std::io::Seek),cancel:&std::sync::atomic::AtomicBool)->Result<String,String>{
    use std::io::SeekFrom;file.seek(SeekFrom::Start(0)).map_err(|e|e.to_string())?;
    struct Algorithm(Handle);impl Drop for Algorithm{fn drop(&mut self){unsafe {BCryptCloseAlgorithmProvider(self.0,0);}}}
    struct Hash(Handle);impl Drop for Hash{fn drop(&mut self){unsafe {BCryptDestroyHash(self.0);}}}
    unsafe {let mut algorithm=0;if BCryptOpenAlgorithmProvider(&mut algorithm,wide("SHA256").as_ptr(),ptr::null(),0)<0{return Err("Windows SHA-256 provider unavailable".into());}let algorithm=Algorithm(algorithm);
        let mut object_size=0u32;let mut result=0;if BCryptGetProperty(algorithm.0,wide("ObjectLength").as_ptr(),(&mut object_size as *mut u32).cast(),4,&mut result,0)<0||object_size>1024*1024{return Err("Invalid Windows hash object size".into());}let mut object=vec![0u8;object_size as usize];let mut hash=0;if BCryptCreateHash(algorithm.0,&mut hash,object.as_mut_ptr(),object_size,ptr::null(),0,0)<0{return Err("Could not initialize SHA-256".into());}let hash=Hash(hash);
        let mut buffer=[0u8;65536];loop{if cancel.load(std::sync::atomic::Ordering::Relaxed){return Err("Verification cancelled".into());}let read=file.read(&mut buffer).map_err(|e|e.to_string())?;if read==0{break;}if BCryptHashData(hash.0,buffer.as_ptr(),read as u32,0)<0{return Err("SHA-256 verification failed".into());}}
        let mut digest=[0u8;32];if BCryptFinishHash(hash.0,digest.as_mut_ptr(),32,0)<0{return Err("SHA-256 completion failed".into());}Ok(digest.iter().map(|b|format!("{b:02x}")).collect())
    }
}
pub fn publish_directory(from:&Path,to:&Path)->Result<(),String>{unsafe {if MoveFileExW(wide(from).as_ptr(),wide(to).as_ptr(),0x8)!=0{Ok(())}else{Err(format!("Could not publish new version without replacing existing data: {}",std::io::Error::last_os_error()))}}}
pub fn design_bounds(hwnd:Hwnd,x:i32,y:i32,w:i32,h:i32,surface:bool){unsafe {for(key,value)in[("DesignX",x),("DesignY",y),("DesignW",w),("DesignH",h)]{SetPropW(hwnd,wide(key).as_ptr(),value as Handle+1);}if surface{SetPropW(hwnd,wide("CraftLauncher.Surface").as_ptr(),1);}}}
pub fn design_background(hwnd:Hwnd){unsafe {SetPropW(hwnd,wide("CraftLauncher.BackgroundCard").as_ptr(),1);}}
#[link(name="user32")] extern "system" {
    fn BeginDeferWindowPos(count:i32)->Handle;
    fn DeferWindowPos(batch:Handle,hwnd:Hwnd,after:Hwnd,x:i32,y:i32,w:i32,h:i32,flags:u32)->Handle;
    fn EndDeferWindowPos(batch:Handle)->i32;
    fn SetWindowPos(hwnd:Hwnd,after:Hwnd,x:i32,y:i32,w:i32,h:i32,flags:u32)->i32;
    fn RedrawWindow(hwnd:Hwnd,rect:*const Rect,region:Handle,flags:u32)->i32;
}
/// Complete a page transaction after layout. Invalidating only the clipped parent
/// leaves the new child/client-edge surfaces showing pixels from the previous page.
pub fn resume_settings_redraw(hwnd:Hwnd){
    if hwnd==0{return;}
    unsafe {SendMessageW(hwnd,0xB,1,0);RedrawWindow(hwnd,ptr::null(),0,0x1|0x4|0x80|0x400|0x100);}
    // RDW_INVALIDATE | ERASE | ALLCHILDREN | FRAME | UPDATENOW, after WM_SETREDRAW(TRUE).
}
pub fn clear_settings_page(hwnd:Hwnd){
    if hwnd==0{return;}
    unsafe extern "system" fn collect(child:Hwnd,data:isize)->i32 {(&mut*(data as *mut Vec<Hwnd>)).push(child);1}
    unsafe {let mut children=Vec::<Hwnd>::new();EnumChildWindows(hwnd,Some(collect),(&mut children as *mut Vec<Hwnd>)as isize);for child in children{DestroyWindow(child);}}
}
/// Commit final child geometry before painting. Discard copied pixels, then queue
/// one complete repaint including children excluded by WS_CLIPCHILDREN.
pub fn layout_controls(hwnd:Hwnd,moves:&[(Hwnd,Rect)]){
    if hwnd==0{return;}
    unsafe {
        const FLAGS:u32=0x4|0x10|0x8|0x100; // NOZORDER | NOACTIVATE | NOREDRAW | NOCOPYBITS
        let mut batch=BeginDeferWindowPos(moves.len()as i32);
        // WM_SIZE can arrive during parent creation, before its controls exist.
        for (child,r) in moves {if *child==0{continue;}if batch==0{break;}batch=DeferWindowPos(batch,*child,0,r.left,r.top,r.right-r.left,r.bottom-r.top,FLAGS);}
        let committed=batch!=0 && EndDeferWindowPos(batch)!=0;
        if !committed {for (child,r) in moves {if *child!=0{SetWindowPos(*child,0,r.left,r.top,r.right-r.left,r.bottom-r.top,FLAGS);}}}
        RedrawWindow(hwnd,ptr::null(),0,0x1|0x4|0x80|0x400); // INVALIDATE | ERASE | ALLCHILDREN | FRAME; queued, no UPDATENOW
    }
}
pub fn layout_settings(hwnd:Hwnd){
    if hwnd==0{return;}
    struct Layout {width:i32,height:i32,moves:Vec<(Hwnd,Rect)>}
    unsafe extern "system" fn collect(child:Hwnd,data:isize)->i32 {
        let layout=&mut*(data as *mut Layout);let x=GetPropW(child,wide("DesignX").as_ptr())-1;let y=GetPropW(child,wide("DesignY").as_ptr())-1;let w=GetPropW(child,wide("DesignW").as_ptr())-1;let h=GetPropW(child,wide("DesignH").as_ptr())-1;
        if x>=0&&y>=0&&w>0&&h>0 {let left=x as i32*layout.width/1000;let top=y as i32*layout.height/680;layout.moves.push((child,Rect{left,top,right:left+w as i32*layout.width/1000,bottom:top+h as i32*layout.height/680}));}1
    }
    unsafe {
        let mut rect:Rect=std::mem::zeroed();GetClientRect(hwnd,&mut rect);
        let mut layout=Layout{width:rect.right,height:rect.bottom,moves:Vec::new()};EnumChildWindows(hwnd,Some(collect),(&mut layout as *mut Layout)as isize);
        // CreateWindowEx puts new child windows at the bottom. Lower decoration cards
        // only after all foreground children exist; never lower app-icon ownerdraw statics.
        for (child,_) in &layout.moves {if GetPropW(*child,wide("CraftLauncher.BackgroundCard").as_ptr())!=0{SetWindowPos(*child,1,0,0,0,0,0x1|0x2|0x10|0x8);}} // HWND_BOTTOM; NOSIZE | NOMOVE | NOACTIVATE | NOREDRAW
        // Preserve the corrected z-order/focus; neither copy old pixels nor paint halfway through
        // a resize. Each fixed page fits a 1000 x 680 client; there is no shared scroll offset.
        layout_controls(hwnd,&layout.moves);
    }
}
pub unsafe fn draw_settings_static(item:&DrawItem) {if !glass::draw(item,2){draw_settings_static_content(item);}}
unsafe fn draw_settings_static_content(item:&DrawItem){
    let p=palette();let icon=GetPropW(item.hwnd,wide("CraftLauncher.ButtonIcon").as_ptr());let saved=SaveDC(item.dc);if saved==0{return;}
    SetDCBrushColor(item.dc,p.surface);FillRect(item.dc,&item.rect,GetStockObject(18));
    if icon!=0{let size=(item.rect.right-item.rect.left).min(item.rect.bottom-item.rect.top);DrawIconEx(item.dc,item.rect.left,item.rect.top,icon,size,size,0,0,3);}else{let pen=CreatePen(0,1,p.border);SelectObject(item.dc,pen);SelectObject(item.dc,GetStockObject(18));RoundRect(item.dc,item.rect.left,item.rect.top,item.rect.right,item.rect.bottom,6,6);RestoreDC(item.dc,saved);DeleteObject(pen);return;}RestoreDC(item.dc,saved);
}
pub fn confirm(hwnd:Hwnd,message:&str)->bool{unsafe {MessageBoxW(hwnd,wide(message).as_ptr(),wide("Install selected portable version").as_ptr(),0x4|0x20|0x100)==6}}
pub fn selected_button(hwnd:Hwnd,selected:bool){if hwnd==0{return;}unsafe {let key=wide("CraftLauncher.Selected");let value=if selected{1}else{0};if GetPropW(hwnd,key.as_ptr())==value{return;}SetPropW(hwnd,key.as_ptr(),value);InvalidateRect(hwnd,ptr::null(),0);}}

pub fn random_id()->Result<String,String>{
    #[link(name="bcrypt")] extern "system"{fn BCryptGenRandom(algorithm:Handle,buffer:*mut u8,size:u32,flags:u32)->i32;}
    let mut bytes=[0u8;16];if unsafe{BCryptGenRandom(0,bytes.as_mut_ptr(),16,2)}<0{return Err("Windows random source unavailable".into());}
    Ok(bytes.iter().map(|b|format!("{b:02x}")).collect())
}
/// Deletes only this already-open, identity-checked regular archive, never a path tree.
pub fn delete_owned_archive(file:&std::fs::File,identity:&[u32;3])->Result<(),String>{
    use std::os::windows::io::AsRawHandle;
    #[repr(C)] struct Info{attributes:u32,created:[u32;2],accessed:[u32;2],written:[u32;2],volume:u32,size_high:u32,size_low:u32,links:u32,index_high:u32,index_low:u32}
    #[link(name="kernel32")] extern "system"{fn GetFileInformationByHandle(file:Handle,info:*mut Info)->i32;fn SetFileInformationByHandle(file:Handle,class:u32,info:*const std::ffi::c_void,size:u32)->i32;}
    let mut info:Info=unsafe{std::mem::zeroed()};let handle=file.as_raw_handle()as Handle;
    if unsafe{GetFileInformationByHandle(handle,&mut info)}==0{return Err(std::io::Error::last_os_error().to_string());}
    if info.attributes&(0x10|0x400)!=0||info.links!=1||[info.volume,info.index_high,info.index_low]!=*identity{return Err("Owned archive identity/type changed; retained for inspection".into());}
    let delete=1u8;if unsafe{SetFileInformationByHandle(handle,4,(&delete as *const u8).cast(),1)}==0{return Err(std::io::Error::last_os_error().to_string());}Ok(())
}
pub fn archive_identity(file:&std::fs::File)->Result<[u32;3],String>{
    use std::os::windows::io::AsRawHandle;
    #[repr(C)] struct Info{attributes:u32,created:[u32;2],accessed:[u32;2],written:[u32;2],volume:u32,size_high:u32,size_low:u32,links:u32,index_high:u32,index_low:u32}
    #[link(name="kernel32")] extern "system"{fn GetFileInformationByHandle(file:Handle,info:*mut Info)->i32;}
    let mut info:Info=unsafe{std::mem::zeroed()};if unsafe{GetFileInformationByHandle(file.as_raw_handle()as Handle,&mut info)}==0{return Err(std::io::Error::last_os_error().to_string());}
    if info.attributes&(0x10|0x400)!=0||info.links!=1{return Err("Archive must be an owned regular single-link file".into());}Ok([info.volume,info.index_high,info.index_low])
}
/// Directory handles pin fresh staging against replacement; deletion never recurses.
pub fn owned_directory(path:&std::path::Path)->Result<(std::fs::File,[u32;3]),String>{
    use std::os::windows::fs::OpenOptionsExt;
    let file=std::fs::OpenOptions::new().read(true).access_mode(0x80010000).share_mode(3).custom_flags(0x02200000).open(path).map_err(|e|e.to_string())?;
    let identity=directory_identity(&file)?;Ok((file,identity))
}
fn directory_identity(file:&std::fs::File)->Result<[u32;3],String>{
    use std::os::windows::io::AsRawHandle;
    #[repr(C)] struct Info{attributes:u32,created:[u32;2],accessed:[u32;2],written:[u32;2],volume:u32,size_high:u32,size_low:u32,links:u32,index_high:u32,index_low:u32}
    #[link(name="kernel32")] extern "system"{fn GetFileInformationByHandle(file:Handle,info:*mut Info)->i32;}
    let mut info:Info=unsafe{std::mem::zeroed()};if unsafe{GetFileInformationByHandle(file.as_raw_handle()as Handle,&mut info)}==0{return Err(std::io::Error::last_os_error().to_string());}
    if info.attributes&0x410!=0x10{return Err("Staging directory type changed; retained for inspection".into());}Ok([info.volume,info.index_high,info.index_low])
}
pub fn delete_owned_empty_directory(file:&std::fs::File,identity:&[u32;3])->Result<(),String>{
    use std::os::windows::io::AsRawHandle;
    if directory_identity(file)?!=*identity{return Err("Staging directory identity changed; retained for inspection".into());}
    #[link(name="kernel32")] extern "system"{fn SetFileInformationByHandle(file:Handle,class:u32,info:*const std::ffi::c_void,size:u32)->i32;}
    let delete=1u8;if unsafe{SetFileInformationByHandle(file.as_raw_handle()as Handle,4,(&delete as *const u8).cast(),1)}==0{return Err(std::io::Error::last_os_error().to_string());}Ok(())
}
/// Scope the standard-handle inheritance flags around an agent child spawn.
/// The handles themselves remain open and usable by this process.
static STDIO_INHERITANCE:std::sync::Mutex<()>=std::sync::Mutex::new(());
pub struct IsolatedStdio {_lock:std::sync::MutexGuard<'static,()>,handles:Vec<(Handle,u32)>}
#[link(name="kernel32")] extern "system" {
    fn GetStdHandle(which:u32)->Handle;
    fn GetHandleInformation(handle:Handle,flags:*mut u32)->i32;
    fn SetHandleInformation(handle:Handle,mask:u32,flags:u32)->i32;
}
impl IsolatedStdio {
    pub fn new()->Result<Self,String>{
        let lock=STDIO_INHERITANCE.lock().map_err(|_|"Protocol handle inheritance lock is poisoned")?;
        let mut guard=Self{_lock:lock,handles:Vec::new()};
        for which in [-10i32,-11,-12]{
            let handle=unsafe{GetStdHandle(which as u32)};
            if handle==0||handle==-1||guard.handles.iter().any(|(h,_)|*h==handle){continue;}
            let mut flags=0;
            if unsafe{GetHandleInformation(handle,&mut flags)}==0{return Err(format!("Cannot inspect protocol handle inheritance: {}",std::io::Error::last_os_error()));}
            if flags&1!=0{
                if unsafe{SetHandleInformation(handle,1,0)}==0{return Err(format!("Cannot isolate protocol handle inheritance: {}",std::io::Error::last_os_error()));}
                guard.handles.push((handle,flags));
            }
        }
        Ok(guard)
    }
}
impl Drop for IsolatedStdio {fn drop(&mut self){for(handle,flags)in self.handles.drain(..){unsafe{SetHandleInformation(handle,1,flags&1);}}}}
#[cfg(test)] thread_local!{pub(crate) static OPENED_FOLDER:std::cell::RefCell<Option<std::path::PathBuf>>=const{std::cell::RefCell::new(None)};}
/// Open one checked directory as a literal ShellExecute file target, without arguments.
pub fn open_apps_folder(path:&std::path::Path)->Result<(),String>{
    #[cfg(not(test))] use std::os::windows::ffi::OsStrExt;
    let path=crate::core::checked_path(path)?;if !path.is_dir(){return Err("Apps root is not a directory".into());}
    #[cfg(test)] {OPENED_FOLDER.with(|v|*v.borrow_mut()=Some(path));return Ok(());}
    #[cfg(not(test))] {
    #[link(name="shell32")] extern "system" {fn ShellExecuteW(hwnd:Hwnd,operation:*const u16,file:*const u16,parameters:*const u16,directory:*const u16,show:i32)->Handle;}
    let path:Vec<u16>=path.as_os_str().encode_wide().chain(Some(0)).collect();
    let result=unsafe{ShellExecuteW(0,wide("explore").as_ptr(),path.as_ptr(),ptr::null(),ptr::null(),1)};
    if result<=32{Err(format!("Could not open apps folder (Windows {result})"))}else{Ok(())}
    }
}
/// Common-controls progress bars update only when their visible state changes.
pub fn init_progress_controls(){
    #[repr(C)]struct Classes{size:u32,classes:u32}
    #[link(name="comctl32")]extern "system"{fn InitCommonControlsEx(classes:*const Classes)->i32;}
    let classes=Classes{size:std::mem::size_of::<Classes>()as u32,classes:0x20};unsafe{InitCommonControlsEx(&classes);}
}
pub fn progress_bar(hwnd:Hwnd,position:Option<u16>,indeterminate:bool,failed:bool)->bool{
    if hwnd==0{return false;}let code=if indeterminate{2002}else{position.unwrap_or(0).min(1000)as isize+1}+if failed{4000}else{0};
    unsafe{let key=wide("CraftLauncher.ProgressState");let before=GetPropW(hwnd,key.as_ptr());if before==code{return false;}SetPropW(hwnd,key.as_ptr(),code);let style=GetWindowLongPtrW(hwnd,-16);if !indeterminate{SendMessageW(hwnd,0x40A,0,0);}SetWindowLongPtrW(hwnd,-16,if indeterminate{style|8}else{style&!8});if before==0{SendMessageW(hwnd,0x406,0,1000);}if indeterminate{SendMessageW(hwnd,0x40A,1,40);}SendMessageW(hwnd,0x410,if failed{2}else{1},0);if !indeterminate{SendMessageW(hwnd,0x402,position.unwrap_or(0).min(1000)as usize,0);}}true
}
#[path="circles.rs"] pub mod circles;
#[repr(C)] struct PaintStruct{dc:Handle,erase:i32,rect:Rect,restore:i32,inc_update:i32,reserved:[u8;32]}
#[link(name="user32")] extern "system"{fn BeginPaint(hwnd:Hwnd,paint:*mut PaintStruct)->Handle;fn EndPaint(hwnd:Hwnd,paint:*const PaintStruct)->i32;fn SetWindowRgn(hwnd:Hwnd,region:Handle,redraw:i32)->i32;fn ReleaseCapture()->i32;}
#[link(name="gdi32")] extern "system"{fn CreateEllipticRgn(left:i32,top:i32,right:i32,bottom:i32)->Handle;fn Ellipse(dc:Handle,left:i32,top:i32,right:i32,bottom:i32)->i32;fn MoveToEx(dc:Handle,x:i32,y:i32,old:*mut Point)->i32;fn LineTo(dc:Handle,x:i32,y:i32)->i32;}
thread_local!{static LAUNCHER_CONTEXT:std::cell::RefCell<(String,String)>=const{std::cell::RefCell::new((String::new(),String::new()))};}
// Presentation only: preserve canonical filesystem paths in application state.
fn friendly_root(root:&str)->String{if let Some(rest)=root.strip_prefix(r"\\?\UNC\"){format!(r"\\{}",rest)}else{root.strip_prefix(r"\\?\").unwrap_or(root).to_owned()}}
pub fn launcher_context(hwnd:Hwnd,root:&str,profile:&str){let display=friendly_root(root);let root=display.as_str();LAUNCHER_CONTEXT.with(|v|{let mut v=v.borrow_mut();if v.0!=root||v.1!=profile{*v=(root.into(),profile.into());unsafe{InvalidateRect(hwnd,ptr::null(),1);}}});}
pub fn circle_region(hwnd:Hwnd){if hwnd==0{return;}unsafe{let mut r:Rect=std::mem::zeroed();GetClientRect(hwnd,&mut r);let region=CreateEllipticRgn(0,0,r.right,r.bottom);if region!=0&&SetWindowRgn(hwnd,region,0)==0{DeleteObject(region);}}}
// Shared geometry for opaque GDI and premultiplied glass text/icon layers.
fn circle_content(r:Rect,pressed:bool)->(i32,i32,Rect,Rect,i32,i32){let d=r.right-r.left;let size=(d*32/96).clamp(24,42);let top=r.top+d*16/96+i32::from(pressed);let name_top=r.top+d*52/96;let name_h=d*17/96;let version_top=name_top+name_h;let inset=d*10/96;
(size,top,Rect{left:r.left+inset,top:name_top,right:r.right-inset,bottom:version_top},Rect{left:r.left+d/6,top:version_top,right:r.right-d/6,bottom:version_top+d*13/96},(d*12/96).max(11),(d*10/96).max(9))}
pub unsafe fn paint_launcher(hwnd:Hwnd){
    let mut ps:PaintStruct=std::mem::zeroed();let dc=BeginPaint(hwnd,&mut ps);if dc==0{EndPaint(hwnd,&ps);return;}
    let mut r:Rect=std::mem::zeroed();GetClientRect(hwnd,&mut r);
    let saved=SaveDC(dc);if saved==0{EndPaint(hwnd,&ps);return;}clip_launcher_children(hwnd,dc);
    if button_flag(hwnd,"CraftLauncher.Backdrop"){if !glass::foreground(dc,&r,|target|paint_launcher_content(target,r)){reset_backdrop(hwnd);InvalidateRect(hwnd,ptr::null(),1);}}else{paint_launcher_content(dc,r);}
    RestoreDC(dc,saved);EndPaint(hwnd,&ps);
}
unsafe fn paint_launcher_content(dc:Handle,r:Rect){let saved=SaveDC(dc);if saved==0{return;}let(w,h)=(r.right,r.bottom);let p=palette();
    let pen=CreatePen(0,1,p.border);SelectObject(dc,pen);
    for(a,b)in circles::EDGES{let(a,b)=circles::edge(w,h,a,b);MoveToEx(dc,a.0,a.1,ptr::null_mut());LineTo(dc,b.0,b.1);}
    MoveToEx(dc,24,65,ptr::null_mut());LineTo(dc,w-24,65);
    SetBkMode(dc,1);let title_font=segoe_font(22);let small_font=segoe_font(12);let label_font=segoe_font(10);
    let draw=|text:&str,mut rect:Rect,font:Handle,color:u32,flags:u32|{SelectObject(dc,font);SetTextColor(dc,color);DrawTextW(dc,wide(text).as_ptr(),-1,&mut rect,0x20|0x800|flags);};
    let icon=launcher_icon(GetModuleHandleW(ptr::null()),false);if icon!=0{DrawIconEx(dc,24,21,icon,32,32,0,0,3);}
    draw("Craft App Launcher",Rect{left:68,top:19,right:w-180,bottom:52},title_font,p.text,0);
    LAUNCHER_CONTEXT.with(|v|{let v=v.borrow();draw(&v.0,Rect{left:26,top:78,right:w-240,bottom:98},small_font,p.muted,0);draw(&v.1,Rect{left:w-230,top:78,right:w-26,bottom:98},small_font,p.muted,2);});
    for(text,x,y)in[("DOCUMENTS",37.,116.),("VISUAL TOOLS",355.,116.),("MOTION + AUDIO",605.,226.)]{let(x,y)=circles::point(w,h,x,y);draw(text,Rect{left:x,top:y,right:x+170,bottom:y+18},label_font,p.muted,0);}
    RestoreDC(dc,saved);for object in[pen,title_font,small_font,label_font]{DeleteObject(object);}
}

#[link(name="user32")] extern "system"{fn MapWindowPoints(from:Hwnd,to:Hwnd,points:*mut Point,count:u32)->i32;}
#[link(name="gdi32")] extern "system"{fn ExtSelectClipRgn(dc:Handle,region:Handle,mode:i32)->i32;fn ExcludeClipRect(dc:Handle,left:i32,top:i32,right:i32,bottom:i32)->i32;}
// WS_CLIPCHILDREN excludes rectangular button bounds on this ownerdraw path.
// Explicit subtraction preserves connector segments in the circle corners, while
// keeping parent erasure and glass compositing away from native child surfaces.
unsafe fn clip_launcher_children(hwnd:Hwnd,dc:Handle){
    for id in [100,110,111].into_iter().chain(300..300+crate::core::APPS.len()as i32){
        let child=GetDlgItem(hwnd,id);if child==0||IsWindowVisible(child)==0{continue;}
        let mut r:Rect=std::mem::zeroed();if GetWindowRect(child,&mut r)==0{continue;}
        MapWindowPoints(0,hwnd,(&mut r as *mut Rect).cast(),2);
        if id>=300{let region=CreateEllipticRgn(r.left,r.top,r.right,r.bottom);if region!=0{ExtSelectClipRgn(dc,region,4);DeleteObject(region);}}
        else{ExcludeClipRect(dc,r.left,r.top,r.right,r.bottom);}
    }
}
pub unsafe fn erase_launcher_background(hwnd:Hwnd,dc:Handle)->Lresult{
    let saved=SaveDC(dc);if saved==0{return 1;}clip_launcher_children(hwnd,dc);
    let result=erase_background(hwnd,dc);RestoreDC(dc,saved);result
}
