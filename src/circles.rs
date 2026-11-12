//! Creative workflow groups, expressed in named nodes so APPS ordering remains stable.
use super::Rect;
pub const NODES:[(&str,f64,f64);17]=[("gridcraft",100.,190.),("deckcraft",225.,190.),("wordcraft",160.,300.),("designcraft",85.,415.),("pdfcraft",235.,415.),("figcraft",360.,190.),("cadcraft",490.,190.),("vectorcraft",425.,300.),("photocraft",365.,420.),("lightcraft",365.,550.),("effectcraft",655.,300.),("filmcraft",625.,420.),("soundcraft",735.,420.),("craft-anything",300.,300.),("materialcraft",495.,420.),("anicraft",655.,190.),("texturecraft",495.,550.)];
pub const EDGES:[(usize,usize);20]=[(0,2),(1,2),(2,3),(2,4),(4,7),(5,7),(6,7),(7,8),(8,9),(10,11),(11,12),(12,10),(7,10),(8,10),(13,5),(14,16),(14,8),(16,9),(15,7),(15,10)];
pub fn point(w:i32,h:i32,x:f64,y:f64)->(i32,i32){((x*w as f64/800.).round()as i32,(y*h as f64/640.).round()as i32)}
pub fn bounds(w:i32,h:i32)->Vec<Rect>{
    let t=((w as f64-800.)/480.).clamp(0.,1.);
    // Interpolate approved diameters, then fit the complete graph to the actual client.
    let d=((96.+30.*t)*(w as f64/(800.+480.*t)).min(h as f64/(640.+160.*t)).min(1.)).floor().max(1.)as i32;
    crate::core::APPS.iter().map(|(id,_)|{let (_,x,y)=NODES.iter().find(|(name,_,_)|name==id).expect("approved node");let(cx,cy)=point(w,h,*x,*y);Rect{left:cx-d/2,top:cy-d/2,right:cx-d/2+d,bottom:cy-d/2+d}}).collect()
}
pub fn contains(w:i32,h:i32,x:i32,y:i32)->bool{let dx=2*x+1-w;let dy=2*y+1-h;x>=0&&y>=0&&x<w&&y<h&&(dx as i64*dx as i64+dy as i64*dy as i64)<=(w.min(h)as i64).pow(2)}
pub fn edge(w:i32,h:i32,a:usize,b:usize)->((i32,i32),(i32,i32)){
    let rects=bounds(w,h);let lookup=|n:usize|rects[crate::core::APPS.iter().position(|(id,_)|*id==NODES[n].0).unwrap()];let ra=lookup(a);let rb=lookup(b);
    let (ax,ay)=((ra.left+ra.right)as f64/2.,(ra.top+ra.bottom)as f64/2.);let(bx,by)=((rb.left+rb.right)as f64/2.,(rb.top+rb.bottom)as f64/2.);let length=(bx-ax).hypot(by-ay);let radius=(ra.right-ra.left)as f64/2.+1.;let(dx,dy)=((bx-ax)/length*radius,(by-ay)/length*radius);
    (((ax+dx).round()as i32,(ay+dy).round()as i32),((bx-dx).round()as i32,(by-dy).round()as i32))
}
#[cfg(test)] mod tests{
use super::*;
#[test]fn approved_anchors_and_app_identity(){for(w,h,d)in[(800,640,96),(1280,800,126)]{let r=bounds(w,h);for(i,(id,_))in crate::core::APPS.iter().enumerate(){let(_,x,y)=NODES.iter().find(|(n,_,_)|n==id).unwrap();let(cx,cy)=point(w,h,*x,*y);assert_eq!((r[i].left+d/2,r[i].top+d/2),(cx,cy));assert_eq!(r[i].right-r[i].left,d);}}assert_eq!(EDGES.len(),20);}
#[test]fn graph_fits_resize_sweep_without_overlap(){for w in (784..=1920).step_by(17){for h in (601..=1080).step_by(29){let r=bounds(w,h);for(i,a)in r.iter().enumerate(){assert!(a.left>=0&&a.right<=w&&a.top>=100&&a.bottom<h-20);for b in &r[..i]{let dx=(a.left+a.right-b.left-b.right)as f64/2.;let dy=(a.top+a.bottom-b.top-b.bottom)as f64/2.;assert!(dx.hypot(dy)>(a.right-a.left)as f64);}}}}}
#[test]fn creative_groups_have_explicit_workflow_links(){
    let linked=|a:&str,b:&str|EDGES.iter().any(|&(x,y)|(NODES[x].0==a&&NODES[y].0==b)||(NODES[x].0==b&&NODES[y].0==a));
    for(a,b)in[("anicraft","vectorcraft"),("anicraft","effectcraft"),("craft-anything","figcraft"),("materialcraft","texturecraft")]{assert!(linked(a,b),"missing workflow link {a} / {b}");}
    assert!(!linked("craft-anything","materialcraft"));assert!(!linked("craft-anything","texturecraft"));
    let node=|id:&str|NODES.iter().find(|n|n.0==id).unwrap();
    assert!(node("anicraft").1>=600.);
    for id in ["materialcraft","texturecraft","craft-anything"]{assert!((300.0..600.0).contains(&node(id).1));}
}
#[test]fn circular_hits_exclude_corners(){for d in [90,96,126]{assert!(contains(d,d,d/2,d/2));for(x,y)in[(0,0),(d-1,0),(0,d-1),(d-1,d-1),(-1,d/2),(d,d/2)]{assert!(!contains(d,d,x,y));}}}
#[test]fn links_do_not_cross_unrelated_nodes(){for(w,h)in[(784,601),(800,640),(1000,680),(1280,800),(1920,601)]{let rects=bounds(w,h);for(a,b)in EDGES{let(p,q)=edge(w,h,a,b);let(dx,dy)=((q.0-p.0)as f64,(q.1-p.1)as f64);for(i,r)in rects.iter().enumerate(){let id=crate::core::APPS[i].0;if id==NODES[a].0||id==NODES[b].0{continue;}let(cx,cy)=((r.left+r.right)as f64/2.,(r.top+r.bottom)as f64/2.);let t=(((cx-p.0 as f64)*dx+(cy-p.1 as f64)*dy)/(dx*dx+dy*dy)).clamp(0.,1.);assert!((cx-p.0 as f64-t*dx).hypot(cy-p.1 as f64-t*dy)>(r.right-r.left)as f64/2.);}}}}}
