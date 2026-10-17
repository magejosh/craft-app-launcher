//! Aetherglass client pixels. Native DWM owns blur; this module never samples the desktop.
use super::*;
#[derive(Clone,Copy,Debug,PartialEq,Eq,Default)]
#[repr(C)] struct Pixel {b:u8,g:u8,r:u8,a:u8}
fn mul(value:u8,alpha:u8)->u8 {((value as u32*alpha as u32+127)/255)as u8}
fn pixel(color:u32,alpha:u8)->Pixel {Pixel{r:mul(color as u8,alpha),g:mul((color>>8)as u8,alpha),b:mul((color>>16)as u8,alpha),a:alpha}}
fn over(src:Pixel,dst:Pixel)->Pixel {
    let channel=|s:u8,d:u8|s.saturating_add(mul(d,255-src.a));
    Pixel{r:channel(src.r,dst.r),g:channel(src.g,dst.g),b:channel(src.b,dst.b),a:channel(src.a,dst.a)}
}
#[repr(C)] struct TextOptions {size:u32,flags:u32,text:u32,border:u32,shadow:u32,shadow_type:i32,offset:Point,border_size:i32,font_prop:i32,color_prop:i32,state:i32,overlay:i32,glow:i32,callback:*const c_void,param:isize}
#[link(name="uxtheme")] extern "system" {
    fn GetBufferedPaintBits(buffer:Handle,bits:*mut *mut Pixel,stride:*mut i32)->i32;
    fn OpenThemeData(hwnd:Hwnd,class:*const u16)->Handle;
    fn CloseThemeData(theme:Handle)->i32;
    fn DrawThemeTextEx(theme:Handle,dc:Handle,part:i32,state:i32,text:*const u16,count:i32,flags:u32,rect:*mut Rect,options:*const TextOptions)->i32;
}
#[link(name="gdi32")] extern "system" {fn GdiFlush()->i32;}
#[link(name="user32")] extern "system" {
    fn GetClassNameW(hwnd:Hwnd,name:*mut u16,count:i32)->i32;
    fn GetWindowLongPtrW(hwnd:Hwnd,index:i32)->isize;
    fn GetClassLongPtrW(hwnd:Hwnd,index:i32)->usize;
    fn SetWindowLongPtrW(hwnd:Hwnd,index:i32,value:isize)->isize;
    fn SetLayeredWindowAttributes(hwnd:Hwnd,key:u32,alpha:u8,flags:u32)->i32;
    fn RedrawWindow(hwnd:Hwnd,rect:*const Rect,region:Handle,flags:u32)->i32;
}
#[link(name="kernel32")] extern "system" {fn SetLastError(error:u32);#[cfg(test)] fn GetLastError()->u32;}
struct Buffer {handle:Handle,dc:Handle,bits:*mut Pixel,stride:usize,width:usize,height:usize}
impl Buffer {
    unsafe fn new(dc:Handle,rect:&Rect)->Option<Self> {
        let width=rect.right.checked_sub(rect.left)?;let height=rect.bottom.checked_sub(rect.top)?;
        if width<=0||height<=0{return None;}
        let mut memory_dc=0;let handle=BeginBufferedPaint(dc,rect,2,ptr::null(),&mut memory_dc);
        if handle==0{return None;}
        let mut buffer=Self{handle,dc:memory_dc,bits:ptr::null_mut(),stride:0,width:width as usize,height:height as usize};
        let mut stride=0;
        if memory_dc==0||GetBufferedPaintBits(handle,&mut buffer.bits,&mut stride)<0||buffer.bits.is_null()||stride<width||BufferedPaintClear(handle,ptr::null())<0{return None;}
        buffer.stride=stride as usize;
        buffer.stride.checked_mul(buffer.height)?.checked_mul(std::mem::size_of::<Pixel>())?;
        Some(buffer)
    }
    unsafe fn at(&self,x:usize,y:usize)->*mut Pixel {self.bits.add(y*self.stride+x)}
    unsafe fn fill(&self,value:Pixel) {for y in 0..self.height{for x in 0..self.width{*self.at(x,y)=value;}}}
    unsafe fn composite(&self,layer:&Self) {for y in 0..self.height{for x in 0..self.width{let dst=self.at(x,y);*dst=over(*layer.at(x,y),*dst);}}}
    unsafe fn commit(mut self)->bool {let handle=self.handle;self.handle=0;EndBufferedPaint(handle,1)>=0}
}
impl Drop for Buffer {fn drop(&mut self){if self.handle!=0{unsafe {EndBufferedPaint(self.handle,0);}}}}
struct SavedDC {dc:Handle,saved:i32}
impl SavedDC {unsafe fn new(dc:Handle)->Option<Self>{let saved=SaveDC(dc);if saved==0{None}else{Some(Self{dc,saved})}}}
impl Drop for SavedDC {fn drop(&mut self){unsafe {if RestoreDC(self.dc,self.saved)==0{SelectObject(self.dc,GetStockObject(17));}}}}
struct Theme(Handle);
impl Drop for Theme {fn drop(&mut self){unsafe {CloseThemeData(self.0);}}}
// Bounded tint: every possible backdrop channel lies between black and white.
// The tests below cover those bounds, secondary text, and all pressed/normal states.
const GLASS_MUTED:u32=0xE8E0D8;
fn tint(pressed:bool)->Pixel {let p=plain_palette(crate::core::ThemeMode::Aetherglass,true);pixel(if pressed{p.pressed}else{p.surface},if pressed{224}else{208})}
fn rounded(x:i32,y:i32,w:i32,h:i32,inset:i32,radius:i32)->bool {
    if x<inset||y<inset||x>=w-inset||y>=h-inset{return false;}
    let radius=radius.min((w-2*inset)/2).min((h-2*inset)/2).max(0);
    let dx=(inset+radius-x-1).max(x-(w-inset-radius)).max(0);
    let dy=(inset+radius-y-1).max(y-(h-inset-radius)).max(0);
    dx*dx+dy*dy<=radius*radius
}
unsafe fn card(buffer:&Buffer,pressed:bool,emphasis:bool,focus:bool,radius:i32) {
    let p=palette();let w=buffer.width as i32;let h=buffer.height as i32;let edge=if emphasis{3}else{1};
    let circular=w==h&&radius==w/2;let border_inset=if circular{1}else{0};
    for y in 0..h {for x in 0..w {
        if !rounded(x,y,w,h,0,radius){continue;}
        let border=rounded(x,y,w,h,border_inset,(radius-border_inset).max(0)) && !rounded(x,y,w,h,border_inset+edge,(radius-border_inset-edge).max(0));
        let focus_dot=focus && (x+y)%2==0 && rounded(x,y,w,h,4,(radius-4).max(0)) && !rounded(x,y,w,h,5,(radius-5).max(0));
        *buffer.at(x as usize,y as usize)=if focus_dot||border&&emphasis{pixel(p.focus,255)}else if border{pixel(p.border,255)}else{tint(pressed)};
    }}
}
unsafe fn text_layer(buffer:&Buffer,item:&DrawItem,theme:&Theme,font:Handle,text:&str,rect:Rect,color:u32,flags:u32)->bool {
    let Some(layer)=Buffer::new(item.dc,&item.rect) else{return false;};
    let Some(_state)=SavedDC::new(layer.dc) else{return false;};
    if SelectObject(layer.dc,if font!=0{font}else{GetStockObject(17)})==0{return false;}
    let options=TextOptions{size:std::mem::size_of::<TextOptions>()as u32,flags:0x2000|1,text:color,border:0,shadow:0,shadow_type:0,offset:Point{x:0,y:0},border_size:0,font_prop:0,color_prop:0,state:0,overlay:0,glow:0,callback:ptr::null(),param:0};
    let mut rect=rect;
    // DTT_COMPOSITED | DTT_TEXTCOLOR, on a separate transparent top-down DIB.
    if DrawThemeTextEx(theme.0,layer.dc,0,0,wide(text).as_ptr(),-1,flags,&mut rect,&options)<0||GdiFlush()==0{return false;}
    buffer.composite(&layer);true
}
fn icon_pixel(black:Pixel,white:Pixel)->Option<Pixel> {
    // Source-over coverage has equal nonnegative white-minus-black deltas.
    // Legacy XOR/inversion cannot be represented without knowing the backdrop;
    // reject it so the existing opaque Win32 icon path can draw it faithfully.
    let r=white.r.checked_sub(black.r)?;let g=white.g.checked_sub(black.g)?;let b=white.b.checked_sub(black.b)?;
    let transparent=r.min(g).min(b);
    if r.max(g).max(b)-transparent>1{return None;}
    let alpha=255-transparent;
    Some(Pixel{r:black.r.min(alpha),g:black.g.min(alpha),b:black.b.min(alpha),a:alpha})
}
unsafe fn icon_layer(buffer:&Buffer,item:&DrawItem,icon:Handle,x:i32,y:i32,size:i32)->bool {
    if icon==0{return true;}
    let Some(black)=Buffer::new(item.dc,&item.rect) else{return false;};
    let Some(white)=Buffer::new(item.dc,&item.rect) else{return false;};
    black.fill(pixel(0,255));white.fill(pixel(0xFFFFFF,255));
    if DrawIconEx(black.dc,x,y,icon,size,size,0,0,3)==0||DrawIconEx(white.dc,x,y,icon,size,size,0,0,3)==0||GdiFlush()==0{return false;}
    for row in 0..buffer.height{for col in 0..buffer.width{let dst=buffer.at(col,row);let Some(source)=icon_pixel(*black.at(col,row),*white.at(col,row))else{return false;};*dst=over(source,*dst);}}
    true
}
// kind: app tile, regular button, or ownerdraw static card/icon.
pub(super) unsafe fn draw(item:&DrawItem,kind:u8)->bool {
    let root=GetAncestor(item.hwnd,2);
    if !button_flag(root,"CraftLauncher.Backdrop"){return false;}
    let success=(|| {
        let buffer=Buffer::new(item.dc,&item.rect)?;
        let theme=Theme(OpenThemeData(root,wide("WINDOW").as_ptr()));if theme.0==0{return None;}
        let p=palette();let disabled=item.state&4!=0;
        let pressed=item.state&1!=0 || kind==0&&button_flag(item.hwnd,"CraftLauncher.Click");
        let focused=item.state&0x10!=0;let cue=focused&&item.state&0x200==0;
        let emphasis=focused||button_flag(item.hwnd,"CraftLauncher.Selected")||kind==0&&!disabled&&(button_flag(item.hwnd,"CraftLauncher.Hover")||pressed);
        card(&buffer,pressed,emphasis,cue,if kind==0{(item.rect.right-item.rect.left)/2}else{3});
        let font=SendMessageW(item.hwnd,0x31,0,0);let icon=GetPropW(item.hwnd,wide("CraftLauncher.ButtonIcon").as_ptr());
        let width=item.rect.right-item.rect.left;let height=item.rect.bottom-item.rect.top;
        if kind==0 {
            let(size,top,name_rect,version_rect,name_size,detail_size)=circle_content(item.rect,pressed);
            if !icon_layer(&buffer,item,icon,item.rect.left+(width-size)/2,top,size){return None;}
            let label=text(item.hwnd);let mut parts=label.splitn(2,'\n');let name=parts.next().unwrap_or("");let version=parts.next().unwrap_or("");
            let name_font=segoe_font(name_size);let detail_font=segoe_font(detail_size);
            let name_ok=text_layer(&buffer,item,&theme,name_font,name,name_rect,if disabled{GLASS_MUTED}else{p.text},0x1|0x4|0x20|0x8000);
            let detail_ok=text_layer(&buffer,item,&theme,detail_font,version,version_rect,GLASS_MUTED,0x1|0x4|0x20|0x8000|0x800);
            DeleteObject(name_font);DeleteObject(detail_font);if !name_ok||!detail_ok{return None;}        }else if kind==1 {
            let rect=Rect{left:item.rect.left+4,top:item.rect.top,right:item.rect.right-4,bottom:item.rect.bottom};
            if !text_layer(&buffer,item,&theme,font,&text(item.hwnd),rect,if disabled{GLASS_MUTED}else{p.text},0x1|0x4|0x20){return None;}
        }else if !icon_layer(&buffer,item,icon,item.rect.left,item.rect.top,width.min(height)){return None;}
        if buffer.commit(){Some(())}else{None}
    })().is_some();
    if !success {reset_backdrop(root);InvalidateRect(root,ptr::null(),1);EnumChildWindows(root,Some(repaint_child),0);}
    success
}
const LAYERED:isize=0x00080000;
// Native style notifications can change LastError after a successful zero-old-style return.
// Verify the requested state, including rollback, instead of misclassifying that return.
unsafe fn set_style(hwnd:Hwnd,value:isize)->bool {SetLastError(0);SetWindowLongPtrW(hwnd,-20,value);GetWindowLongPtrW(hwnd,-20)==value}
unsafe fn native_control(hwnd:Hwnd)->bool {
    let mut name=[0u16;32];let count=GetClassNameW(hwnd,name.as_mut_ptr(),name.len()as i32);
    let name=String::from_utf16_lossy(&name[..count.max(0)as usize]);
    matches!(name.to_ascii_uppercase().as_str(),"EDIT"|"LISTBOX"|"COMBOBOX"|"COMBOLBOX") || name.eq_ignore_ascii_case("STATIC") && GetWindowLongPtrW(hwnd,-16)&0x1F!=0xD || name.eq_ignore_ascii_case("BUTTON") && GetWindowLongPtrW(hwnd,-16)&0xF!=0xB
}
unsafe fn opaque_control(hwnd:Hwnd,enabled:bool)->bool {
    let key=wide("CraftLauncher.OpaqueIsland");let ours=GetPropW(hwnd,key.as_ptr())!=0;
    if !enabled {
        if ours {let style=GetWindowLongPtrW(hwnd,-20);if !set_style(hwnd,style&!LAYERED){return false;}RemovePropW(hwnd,key.as_ptr());RedrawWindow(hwnd,ptr::null(),0,0x485);}
        return true;
    }
    if !native_control(hwnd){return true;}
    if GetClassLongPtrW(hwnd,-26)&(0x20|0x40)!=0{return false;} // CS_OWNDC / CS_CLASSDC forbid WS_EX_LAYERED
    let style=GetWindowLongPtrW(hwnd,-20);
    // Constant alpha=255 uses OS redirection of the complete native child paint,
    // including text selection and caret, without interpreting GDI pixel alpha.
    if !ours && style&LAYERED!=0{return false;}
    if !ours && SetPropW(hwnd,key.as_ptr(),1)==0{return false;}
    let styled=set_style(hwnd,style|LAYERED);let redirected=styled&&SetLayeredWindowAttributes(hwnd,0,255,2)!=0;
    if !redirected {
        #[cfg(test)] eprintln!("opaque island failed: hwnd={hwnd} class_style={:#X} exstyle={style:#X} current={:#X} styled={styled} error={}",GetClassLongPtrW(hwnd,-26),GetWindowLongPtrW(hwnd,-20),GetLastError());
        let restored=set_style(hwnd,style);if restored && !ours{RemovePropW(hwnd,key.as_ptr());}return false;
    }
    RedrawWindow(hwnd,ptr::null(),0,0x485);true
}
pub(super) unsafe fn opaque_controls(hwnd:Hwnd,enabled:bool)->bool {
    struct Request {enabled:bool,ok:bool}
    unsafe extern "system" fn visit(child:Hwnd,data:isize)->i32 {let request=&mut*(data as *mut Request);request.ok=opaque_control(child,request.enabled)&&request.ok;1}
    let mut request=Request{enabled,ok:true};EnumChildWindows(hwnd,Some(visit),(&mut request as *mut Request)as isize);request.ok
}
pub(super) unsafe fn new_control(hwnd:Hwnd) {
    let root=GetAncestor(hwnd,2);
    if button_flag(root,"CraftLauncher.Backdrop") && (!opaque_control(hwnd,true)||!opaque_controls(hwnd,true)){reset_backdrop(root);InvalidateRect(root,ptr::null(),1);}
}
#[cfg(test)] mod tests {
    use super::*;
    fn luminance(p:Pixel)->f64 {let c=|v:u8|{let v=v as f64/255.0;if v<=0.04045{v/12.92}else{((v+0.055)/1.055).powf(2.4)}};0.2126*c(p.r)+0.7152*c(p.g)+0.0722*c(p.b)}
    fn contrast(a:Pixel,b:Pixel)->f64 {let a=luminance(a);let b=luminance(b);(a.max(b)+0.05)/(a.min(b)+0.05)}
    #[test] fn source_over_preserves_premultiplied_bounds_and_endpoints() {
        let dst=pixel(0x805020,173);assert_eq!(over(Pixel::default(),dst),dst);assert_eq!(over(pixel(0x123456,255),dst),pixel(0x123456,255));
        for alpha in 0..=255 {let out=over(pixel(0xFFFFFF,alpha),dst);assert!(out.r<=out.a&&out.g<=out.a&&out.b<=out.a);}
    }
    #[test] fn icon_reconstruction_handles_transparent_opaque_and_soft_edges() {
        for alpha in [0,1,64,128,192,254,255] {let src=pixel(0x9157D3,alpha);let recovered=icon_pixel(over(src,pixel(0,255)),over(src,pixel(0xFFFFFF,255)));assert_eq!(recovered,Some(src));}
    }
    #[test] fn unsupported_xor_icon_pixels_fail_closed() {
        assert!(icon_pixel(pixel(0xFFFFFF,255),pixel(0,255)).is_none());
        assert!(icon_pixel(pixel(0,255),pixel(0x00FFFF,255)).is_none());
    }
    #[test] fn glass_tint_bounds_text_and_focus_contrast() {
        let p=plain_palette(crate::core::ThemeMode::Aetherglass,true);
        // Each channel's composite is monotone: white is the brightest possible
        // backdrop, black the darkest. Include saturated/busy pixel extremes too.
        for pressed in [false,true] {for backdrop in [0,0xFFFFFF,0x0000FF,0x00FF00,0xFF0000,0xFF00FF,0x00FFFF,0xFFFF00] {
            let surface=over(tint(pressed),pixel(backdrop,255));
            for color in [p.text,GLASS_MUTED]{assert!(contrast(pixel(color,255),surface)>=4.5);}
            assert!(contrast(pixel(p.focus,255),surface)>=3.0);
        }}
    }
    #[link(name="user32")] extern "system" {
        fn GetDC(hwnd:Hwnd)->Handle;
        fn ReleaseDC(hwnd:Hwnd,dc:Handle)->i32;
        fn GetLayeredWindowAttributes(hwnd:Hwnd,key:*mut u32,alpha:*mut u8,flags:*mut u32)->i32;
    }
    #[link(name="kernel32")] extern "system" {
        fn FindResourceW(module:Handle,name:*const u16,kind:*const u16)->Handle;
        fn LoadResource(module:Handle,resource:Handle)->Handle;
        fn LockResource(resource:Handle)->*const u8;
        fn SizeofResource(module:Handle,resource:Handle)->u32;
    }
    struct HiddenWindow(Hwnd);
    impl HiddenWindow {unsafe fn new(class:&str,parent:Hwnd)->Self {Self::with_style(class,parent,0)}
    unsafe fn with_style(class:&str,parent:Hwnd,extra:u32)->Self {
        let hwnd=CreateWindowExW(0,wide(class).as_ptr(),wide("Craft glass automated hidden fixture").as_ptr(),(if parent==0{0x00CF0000|0x02000000}else{0x40000000})|extra,0,0,240,120,parent,0,GetModuleHandleW(ptr::null()),ptr::null_mut());
        assert_ne!(hwnd,0,"hidden owned fixture creation");assert_eq!(IsWindowVisible(hwnd),0);Self(hwnd)
    }}
    impl Drop for HiddenWindow {fn drop(&mut self){unsafe {DestroyWindow(self.0);}}}
    struct WindowDC(Hwnd,Handle);
    impl Drop for WindowDC {fn drop(&mut self){unsafe {ReleaseDC(self.0,self.1);}}}
    #[test] fn actual_test_pe_embeds_layered_child_compatibility_manifest() {unsafe {
        let module=GetModuleHandleW(ptr::null());let resource=FindResourceW(module,1usize as *const u16,24usize as *const u16);assert_ne!(resource,0,"RT_MANIFEST ID1");
        let size=SizeofResource(module,resource)as usize;assert!(size>0&&size<65536);let loaded=LoadResource(module,resource);assert_ne!(loaded,0);let bytes=LockResource(loaded);assert!(!bytes.is_null());
        let xml=String::from_utf8_lossy(std::slice::from_raw_parts(bytes,size));assert!(xml.contains("8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a"));assert!(xml.contains("compatibility"));
    }}
    #[test] fn hidden_buffer_glyphs_and_system_icon_have_valid_alpha() {unsafe {
        assert!(buffered_paint_ready());let hwnd=HiddenWindow::new("STATIC",0);let dc=WindowDC(hwnd.0,GetDC(hwnd.0));assert_ne!(dc.1,0);
        let rect=Rect{left:0,top:0,right:200,bottom:80};let buffer=Buffer::new(dc.1,&rect).expect("top-down DIB pixels");
        let item=DrawItem{control_type:4,control_id:0,item_id:0,action:1,state:0,hwnd:hwnd.0,dc:dc.1,rect,data:0};
        let theme=Theme(OpenThemeData(hwnd.0,wide("WINDOW").as_ptr()));assert_ne!(theme.0,0,"themed text availability in automated session");
        assert!(text_layer(&buffer,&item,&theme,GetStockObject(17),"Glass glyphs",Rect{left:2,top:2,right:198,bottom:30},0xFFFFFF,0x20));
        let mut covered=0;let mut clear=0;
        for y in 0..buffer.height{for x in 0..buffer.width{let p=*buffer.at(x,y);assert!(p.r<=p.a&&p.g<=p.a&&p.b<=p.a,"glyph must be premultiplied");if p.a>0{covered+=1;}else{clear+=1;}}}
        assert!(covered>0&&clear>0,"glyph layer preserves transparent surroundings");
        let icon=LoadIconW(0,32512usize as *const u16);assert_ne!(icon,0);assert!(icon_layer(&buffer,&item,icon,4,36,32));
        for y in 0..buffer.height{for x in 0..buffer.width{let p=*buffer.at(x,y);assert!(p.r<=p.a&&p.g<=p.a&&p.b<=p.a,"icon composite must be premultiplied");}}
        eprintln!("HIDDEN GLASS: real top-down buffer, composited glyph alpha and system-icon alpha checked; no surface committed or shown");
    }}
    #[test] fn hidden_native_islands_restore_styles_and_backdrop_marker() {unsafe {
        // Layered children are supported since Windows 8, independently of the Win11 Acrylic gate.
        let root=HiddenWindow::new("STATIC",0);
        for class in ["EDIT","LISTBOX","COMBOBOX","STATIC","BUTTON"] {
            let child=HiddenWindow::with_style(class,root.0,if class=="BUTTON"{3}else{0});let original=GetWindowLongPtrW(child.0,-20);
            if matches!(class,"EDIT"|"STATIC"|"BUTTON"){set_text(child.0,"native control state");}if class=="BUTTON"{SendMessageW(child.0,0xF1,1,0);}EnableWindow(child.0,0);
            eprintln!("HIDDEN CLASS {class}: style={:#X} class={:#X}",GetWindowLongPtrW(child.0,-16),GetClassLongPtrW(child.0,-26));
            assert!(opaque_control(child.0,true),"{class} supports opaque OS redirection");assert_ne!(GetWindowLongPtrW(child.0,-20)&LAYERED,0);
            let mut key=0;let mut alpha=0;let mut flags=0;assert_ne!(GetLayeredWindowAttributes(child.0,&mut key,&mut alpha,&mut flags),0);assert_eq!(alpha,255);assert_eq!(flags,2);
            SetPropW(root.0,wide("CraftLauncher.Backdrop").as_ptr(),1);reset_backdrop(root.0);
            assert_eq!(GetWindowLongPtrW(child.0,-20),original,"{class} exact extended-style restoration");assert!(!button_flag(child.0,"CraftLauncher.OpaqueIsland"));assert!(!button_flag(root.0,"CraftLauncher.Backdrop"));
            if matches!(class,"EDIT"|"STATIC"|"BUTTON"){assert_eq!(text(child.0),"native control state");}assert_eq!(IsWindowEnabled(child.0),0);if class=="BUTTON"{assert_eq!(SendMessageW(child.0,0xF0,0,0),1);}
            assert_eq!(IsWindowVisible(root.0),0);assert_eq!(IsWindowVisible(child.0),0);
        }
        eprintln!("HIDDEN GLASS: EDIT/LISTBOX/COMBOBOX/STATIC/native checkbox constant-alpha255, native text/disabled/checked state and exact reset checked; no input or visible windows");
    }}
}
// Parent foreground only. Transparent pixels stay transparent; GDI line/text RGB
// gets opaque alpha before compositing over the DWM backdrop.
pub(super) unsafe fn foreground(dc:Handle,rect:&Rect,draw:impl FnOnce(Handle))->bool{
    let Some(buffer)=Buffer::new(dc,rect)else{return false;};draw(buffer.dc);GdiFlush();
    for y in 0..buffer.height{for x in 0..buffer.width{let p=&mut*buffer.at(x,y);if p.r!=0||p.g!=0||p.b!=0{p.a=255;}}}
    buffer.commit()
}
