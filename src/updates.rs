//! Explicit, read-only stable release check. No downloads, app execution, or auto polling.
use std::{cmp::Ordering,collections::BTreeMap};
use crate::{core::APPS,native};
pub const MAX_RESPONSE:usize=2*1024*1024;
#[derive(Clone,Debug,Eq,PartialEq)]
pub struct Version {numbers:[u64;3],pre:Vec<String>}
impl Version {
    pub fn parse(value:&str)->Result<Self,String> {
        if value.len()>256 || !value.is_ascii(){return Err("Unsupported semantic version".into());}
        let value=value.strip_prefix('v').or_else(||value.strip_prefix('V')).unwrap_or(value);
        let (value,build)=value.split_once('+').map(|(a,b)|(a,Some(b))).unwrap_or((value,None));
        let valid_ident=|part:&str|!part.is_empty() && part.bytes().all(|v|v.is_ascii_alphanumeric()||v==b'-');
        if build.is_some_and(|build|!build.split('.').all(valid_ident)){return Err("Invalid version build metadata".into());}
        let (core,pre)=value.split_once('-').map(|(a,b)|(a,Some(b))).unwrap_or((value,None));
        let parts:Vec<_>=core.split('.').collect();if parts.len()!=3{return Err("Unsupported semantic version".into());}
        let mut numbers=[0;3];for (i,part) in parts.iter().enumerate(){if part.is_empty()||!part.bytes().all(|v|v.is_ascii_digit())||(part.len()>1&&part.starts_with('0')){return Err("Invalid numeric version".into());}numbers[i]=part.parse().map_err(|_|"Version number exceeds supported range")?;}
        let mut identifiers=Vec::new();if let Some(pre)=pre {for part in pre.split('.') {if !valid_ident(part)||(part.bytes().all(|v|v.is_ascii_digit())&&part.len()>1&&part.starts_with('0')){return Err("Invalid prerelease version".into());}identifiers.push(part.into());}}
        Ok(Self{numbers,pre:identifiers})
    }
    fn compare(&self,other:&Self)->Ordering {
        let core=self.numbers.cmp(&other.numbers);if core!=Ordering::Equal{return core;}
        if self.pre.is_empty()!=other.pre.is_empty(){return if self.pre.is_empty(){Ordering::Greater}else{Ordering::Less};}
        for (a,b) in self.pre.iter().zip(&other.pre){let numeric=|v:&str|v.bytes().all(|c|c.is_ascii_digit());let ordering=match(numeric(a),numeric(b)){(true,true)=>a.len().cmp(&b.len()).then(a.cmp(b)),(true,false)=>Ordering::Less,(false,true)=>Ordering::Greater,(false,false)=>a.cmp(b)};if ordering!=Ordering::Equal{return ordering;}}
        self.pre.len().cmp(&other.pre.len())
    }
}
#[derive(Clone,Debug)] pub struct Release {pub tag:String,version:Version,pub assets:Vec<Asset>,pub asset_note:String}
#[derive(Clone,Debug)] pub struct Asset {pub name:String,pub url:String,pub size:u64,pub digest:String}
pub fn repository(app:usize)->Result<String,String> {let id=APPS.get(app).ok_or("Unknown app")?.0;Ok(format!("https://github.com/storytold/{id}"))}
pub fn request_path(app:usize)->Result<String,String> {let id=APPS.get(app).ok_or("Unknown app")?.0;Ok(format!("/repos/storytold/{id}/releases/latest"))}
pub fn check(app:usize)->Result<Release,String> {parse_release_for(app,&native::github_release_get(&request_path(app)?,MAX_RESPONSE)?)}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Comparison {Older,Current,Ahead,Unknown}
pub fn compare_version(label:&str,release:&Release)->Comparison {
    let Some(value)=label.strip_suffix(" [PE file version]")else{return Comparison::Unknown;};let parts:Vec<_>=value.split('.').collect();
    if parts.len()!=4||parts[3]!="0"{return Comparison::Unknown;}
    let Ok(installed)=Version::parse(&parts[..3].join("."))else{return Comparison::Unknown;};
    match installed.compare(&release.version){Ordering::Less=>Comparison::Older,Ordering::Equal=>Comparison::Current,Ordering::Greater=>Comparison::Ahead}
}
pub fn compare_installed(label:&str,release:&Release)->String {
    match compare_version(label,release){Comparison::Older=>format!("Update available: official stable {}",release.tag),Comparison::Current=>format!("Current: matches official stable {}",release.tag),Comparison::Ahead=>format!("Installed version is ahead of official stable {}",release.tag),Comparison::Unknown=>"Unknown comparison: a supported PE version is required; folder hints are not proof.".into()}
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum UpdateState {NotChecked,NotInstalled,UpdateAvailable,UpToDate,Ahead,Unknown,Error}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct UpdateStatus {pub state:UpdateState,pub install_available:bool}
impl UpdateStatus {
    pub fn label(self)->&'static str {match self.state{UpdateState::NotChecked=>"Not checked",UpdateState::NotInstalled=>"Not installed",UpdateState::UpdateAvailable=>"Update available",UpdateState::UpToDate=>"Up to date",UpdateState::Ahead=>"Ahead of stable",UpdateState::Unknown=>"Unknown",UpdateState::Error=>"Error"}}
    pub fn action(self)->&'static str {if self.install_available{"Install..."}else if self.state==UpdateState::Unknown{"Review..."}else{"Details..."}}
}
pub fn update_status(app:usize,installed_count:usize,selected:Option<&str>,result:Option<&Result<Release,String>>)->UpdateStatus {
    let release=match result{None=>return UpdateStatus{state:UpdateState::NotChecked,install_available:false},Some(Err(_))=>return UpdateStatus{state:UpdateState::Error,install_available:false},Some(Ok(release))=>release};
    let state=if installed_count==0{UpdateState::NotInstalled}else{match selected.map(|label|compare_version(label,release)){Some(Comparison::Older)=>UpdateState::UpdateAvailable,Some(Comparison::Current)=>UpdateState::UpToDate,Some(Comparison::Ahead)=>UpdateState::Ahead,_=>UpdateState::Unknown}};
    let verified=release.assets.len()==1&&validate_asset(app,&release.tag,&release.assets[0]).is_ok();
    UpdateStatus{state,install_available:verified&&matches!(state,UpdateState::NotInstalled|UpdateState::UpdateAvailable)}
}
// Complete JSON syntax validation, bounded by input bytes, nesting, and node count.
// Reject duplicate object keys to avoid ambiguous release metadata.
#[derive(Debug)] enum Json {Object(BTreeMap<String,Json>),String(String),Bool(bool),Number(String),Array(Vec<Json>),Other}
struct Parser<'a>{input:&'a str,pos:usize,nodes:usize}
impl Parser<'_>{
    fn ws(&mut self){while self.input.as_bytes().get(self.pos).is_some_and(|c|matches!(c,b' '|b'\r'|b'\n'|b'\t')){self.pos+=1;}}
    fn byte(&mut self,b:u8)->bool{if self.input.as_bytes().get(self.pos)==Some(&b){self.pos+=1;true}else{false}}
    fn expect(&mut self,b:u8)->Result<(),String>{if self.byte(b){Ok(())}else{Err("Invalid JSON punctuation".into())}}
    fn hex(&mut self)->Result<u16,String>{let end=self.pos+4;let raw=self.input.get(self.pos..end).ok_or("Invalid JSON Unicode escape")?;if !raw.bytes().all(|c|c.is_ascii_hexdigit()){return Err("Invalid JSON Unicode escape".into());}self.pos=end;u16::from_str_radix(raw,16).map_err(|_|"Invalid Unicode".into())}
    fn string(&mut self)->Result<String,String>{
        self.expect(b'"')?;let mut value=String::new();loop{let c=self.input[self.pos..].chars().next().ok_or("Unterminated JSON string")?;self.pos+=c.len_utf8();match c{
            '"'=>return Ok(value),'\\'=>{let escape=*self.input.as_bytes().get(self.pos).ok_or("Missing JSON escape")?;self.pos+=1;match escape{
                b'"'=>value.push('"'),b'\\'=>value.push('\\'),b'/'=>value.push('/'),b'b'=>value.push('\u{8}'),b'f'=>value.push('\u{c}'),b'n'=>value.push('\n'),b'r'=>value.push('\r'),b't'=>value.push('\t'),
                b'u'=>{let first=self.hex()?;let scalar=if(0xD800..=0xDBFF).contains(&first){self.expect(b'\\')?;self.expect(b'u')?;let second=self.hex()?;if !(0xDC00..=0xDFFF).contains(&second){return Err("Invalid surrogate pair".into());}0x10000+((first as u32-0xD800)<<10)+(second as u32-0xDC00)}else{first as u32};value.push(char::from_u32(scalar).ok_or("Invalid Unicode scalar")?);},_=>return Err("Invalid JSON escape".into())}},
            c if c<'\u{20}'=>return Err("Control character in JSON".into()),_=>value.push(c),
        }}
    }
    fn value(&mut self,depth:usize)->Result<Json,String>{
        self.ws();self.nodes+=1;if depth>32||self.nodes>100_000{return Err("JSON complexity limit exceeded".into());}
        match self.input.as_bytes().get(self.pos).copied(){
            Some(b'"')=>Ok(Json::String(self.string()?)),
            Some(b'{')=>{self.pos+=1;self.ws();let mut map=BTreeMap::new();if self.byte(b'}'){return Ok(Json::Object(map));}loop{self.ws();let key=self.string()?;self.ws();self.expect(b':')?;let value=self.value(depth+1)?;if map.insert(key,value).is_some(){return Err("Duplicate JSON key".into());}self.ws();if self.byte(b'}'){break;}self.expect(b',')?;}Ok(Json::Object(map))},
            Some(b'[')=>{self.pos+=1;self.ws();let mut values=Vec::new();if self.byte(b']'){return Ok(Json::Array(values));}loop{values.push(self.value(depth+1)?);self.ws();if self.byte(b']'){break;}self.expect(b',')?;}Ok(Json::Array(values))},
            Some(b't') if self.input[self.pos..].starts_with("true")=>{self.pos+=4;Ok(Json::Bool(true))},
            Some(b'f') if self.input[self.pos..].starts_with("false")=>{self.pos+=5;Ok(Json::Bool(false))},
            Some(b'n') if self.input[self.pos..].starts_with("null")=>{self.pos+=4;Ok(Json::Other)},
            Some(b'-'|b'0'..=b'9')=>{let start_number=self.pos;self.byte(b'-');if !self.byte(b'0'){let start=self.pos;while self.input.as_bytes().get(self.pos).is_some_and(u8::is_ascii_digit){self.pos+=1;}if start==self.pos{return Err("Invalid JSON number".into());}}if self.byte(b'.'){let start=self.pos;while self.input.as_bytes().get(self.pos).is_some_and(u8::is_ascii_digit){self.pos+=1;}if start==self.pos{return Err("Invalid JSON fraction".into());}}if self.byte(b'e')||self.byte(b'E'){if !self.byte(b'+'){self.byte(b'-');}let start=self.pos;while self.input.as_bytes().get(self.pos).is_some_and(u8::is_ascii_digit){self.pos+=1;}if start==self.pos{return Err("Invalid JSON exponent".into());}}Ok(Json::Number(self.input[start_number..self.pos].into()))},
            _=>Err("Invalid JSON value".into()),
        }
    }
}
pub fn parse_release(bytes:&[u8])->Result<Release,String>{parse_release_for(usize::MAX,bytes)}
pub fn parse_release_for(app:usize,bytes:&[u8])->Result<Release,String>{
    if bytes.len()>MAX_RESPONSE{return Err("GitHub response exceeds 2 MiB".into());}let input=std::str::from_utf8(bytes).map_err(|_|"GitHub returned invalid UTF-8")?;
    let mut parser=Parser{input,pos:0,nodes:0};let object=parser.value(0)?;parser.ws();if parser.pos!=input.len(){return Err("Trailing data after JSON".into());}
    let Json::Object(mut object)=object else{return Err("GitHub did not return a release object".into());};
    if !matches!(object.remove("draft"),Some(Json::Bool(false)))||!matches!(object.remove("prerelease"),Some(Json::Bool(false))){return Err("No verified non-draft stable release".into());}
    let Some(Json::String(tag))=object.remove("tag_name")else{return Err("Missing release tag".into());};let version=Version::parse(&tag)?;if !version.pre.is_empty(){return Err("Stable release has a prerelease version tag".into());}let mut assets=Vec::new();let mut asset_note="No verified Windows x64 portable ZIP asset. Use the official release page.".to_string();
    if let Some((id,_))=APPS.get(app){let expected=format!("{id}-{}-windows-x64-portable.zip",tag.trim_start_matches(['v','V']));
        if let Some(Json::Array(items))=object.remove("assets"){let mut matches=0;for item in items{let Json::Object(mut item)=item else{continue;};let Some(Json::String(name))=item.remove("name")else{continue;};if name!=expected{continue;}matches+=1;
            let (Some(Json::String(url)),Some(Json::Number(size)),Some(Json::String(digest)))=(item.remove("browser_download_url"),item.remove("size"),item.remove("digest"))else{asset_note="Portable asset lacks verified size or SHA-256 metadata. Use the official release page.".into();continue;};
            let Ok(size)=size.parse::<u64>()else{continue;};let asset=Asset{name,url,size,digest};if validate_asset(app,&tag,&asset).is_ok(){assets.push(asset);}
        }if matches>1{assets.clear();asset_note="Ambiguous duplicate portable assets. Use the official release page.".into();}}
    }
    if assets.len()==1{asset_note="Windows x64 portable ZIP with an official SHA-256 digest. Installation requires your explicit selection.".into();}Ok(Release{tag,version,assets,asset_note})
}
pub fn validate_asset(app:usize,tag:&str,asset:&Asset)->Result<(),String>{
    let id=APPS.get(app).ok_or("Unknown app")?.0;let parsed=Version::parse(tag)?;if !parsed.pre.is_empty(){return Err("Prerelease installation is not supported".into());}
    let name=format!("{id}-{}-windows-x64-portable.zip",tag.trim_start_matches(['v','V']));
    if asset.name!=name||asset.url!=format!("https://github.com/storytold/{id}/releases/download/{tag}/{name}"){return Err("Asset name or official origin does not match this app/version/x64 platform".into());}
    let Some(hash)=asset.digest.strip_prefix("sha256:")else{return Err("Official SHA-256 digest is missing".into());};
    if hash.len()!=64||!hash.bytes().all(|v|v.is_ascii_hexdigit())||asset.size==0||asset.size>1024*1024*1024{return Err("Invalid digest or asset exceeds the 1 GiB download limit".into());}Ok(())
}
#[cfg(test)] mod tests{
    use super::*;
    #[test] fn update_rows_use_selected_version_and_verified_assets() {
        let asset=r#"{"name":"filmcraft-1.2.3-windows-x64-portable.zip","browser_download_url":"https://github.com/storytold/filmcraft/releases/download/v1.2.3/filmcraft-1.2.3-windows-x64-portable.zip","size":123,"digest":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#;
        let fixture=format!(r#"{{"tag_name":"v1.2.3","draft":false,"prerelease":false,"assets":[{asset}]}}"#);let result=Ok(parse_release_for(4,fixture.as_bytes()).unwrap());
        for (count,label,state,install) in [(0,None,UpdateState::NotInstalled,true),(1,Some("1.0.0.0 [PE file version]"),UpdateState::UpdateAvailable,true),(2,Some("1.2.3.0 [PE file version]"),UpdateState::UpToDate,false),(2,None,UpdateState::Unknown,false),(1,Some("Unknown; folder hint 1.0.0"),UpdateState::Unknown,false),(1,Some("2.0.0.0 [PE file version]"),UpdateState::Ahead,false)] {assert_eq!(update_status(4,count,label,Some(&result)),UpdateStatus{state,install_available:install});}
        assert_eq!(update_status(4,1,None,Some(&Err("offline".into()))).state,UpdateState::Error);assert_eq!(update_status(4,1,None,None).state,UpdateState::NotChecked);
        let no_asset=Ok(parse_release_for(4,br#"{"tag_name":"v1.2.3","draft":false,"prerelease":false,"assets":[]}"#).unwrap());let status=update_status(4,1,Some("1.0.0.0 [PE file version]"),Some(&no_asset));assert_eq!(status.state,UpdateState::UpdateAvailable);assert_eq!(status.action(),"Details...");
        assert!(!update_status(0,0,None,Some(&result)).install_available);
    }
    #[test] fn official_identity_is_catalog_bound(){for(app,(id,_))in APPS.iter().enumerate(){assert_eq!(request_path(app).unwrap(),format!("/repos/storytold/{id}/releases/latest"));}assert!(request_path(12).is_err());}
    #[test] fn json_handles_escaped_keys_unicode_and_nested_values(){let value=br#"{"tag\u005fname":"v1.2.3","draft":false,"prerelease":false,"body":"\uD83D\uDE00","assets":[{"n":-1.25e+2,"b":null}]}"#;assert_eq!(parse_release(value).unwrap().tag,"v1.2.3");}
    #[test] fn malformed_or_ambiguous_release_is_unknown(){for value in [r#"{"tag_name":"v1.2.3","tag_name":"v9.0.0","draft":false,"prerelease":false}"#,r#"{"tag_name":"v1.2.3","draft":false,"prerelease":true}"#,r#"{"tag_name":"v1.2.3","draft":false,"prerelease":false,"x":01}"#,r#"{"tag_name":"v1.2.3","draft":false,"prerelease":false,"x":"\uD800"}"#,r#"{"tag_name":"v1.2.3","draft":false,"prerelease":false,}"#]{assert!(parse_release(value.as_bytes()).is_err());}assert!(parse_release(&[0xFF]).is_err());assert!(parse_release("[".repeat(34).as_bytes()).is_err());}
    #[test] fn semver_and_pe_evidence_are_conservative(){let release=parse_release(br#"{"tag_name":"v1.2.3+build","draft":false,"prerelease":false}"#).unwrap();assert!(compare_installed("1.2.3.0 [PE file version]",&release).starts_with("Current"));assert!(compare_installed("1.2.2.0 [PE file version]",&release).starts_with("Update"));assert!(compare_installed("1.3.0.0 [PE file version]",&release).contains("ahead"));assert!(compare_installed("1.2.3.1 [PE file version]",&release).starts_with("Unknown"));assert!(compare_installed("Unknown; folder hint 1.2.3",&release).starts_with("Unknown"));assert!(Version::parse("1.2.3-rc.01").is_err());assert_eq!(Version::parse("1.2.3-rc.2").unwrap().compare(&Version::parse("1.2.3-rc.10").unwrap()),Ordering::Less);}
    #[test] fn portable_asset_requires_exact_identity_size_and_digest(){
        let name="filmcraft-0.6.0-windows-x64-portable.zip";let good=Asset{name:name.into(),url:format!("https://github.com/storytold/filmcraft/releases/download/v0.6.0/{name}"),size:123,digest:format!("sha256:{}","a".repeat(64))};assert!(validate_asset(4,"v0.6.0",&good).is_ok());
        let mut bad=good.clone();bad.digest.clear();assert!(validate_asset(4,"v0.6.0",&bad).is_err());bad=good.clone();bad.url=bad.url.replace("github.com","github.com.evil");assert!(validate_asset(4,"v0.6.0",&bad).is_err());bad=good.clone();bad.name=bad.name.replace("x64","arm64");assert!(validate_asset(4,"v0.6.0",&bad).is_err());bad=good.clone();bad.size=1024*1024*1024+1;assert!(validate_asset(4,"v0.6.0",&bad).is_err());assert!(validate_asset(0,"v0.6.0",&good).is_err());
        let item=format!(r#"{{"name":"{}","browser_download_url":"{}","size":123,"digest":"{}"}}"#,good.name,good.url,good.digest);
        let response=|items:&str|format!(r#"{{"tag_name":"v0.6.0","draft":false,"prerelease":false,"assets":[{items}]}}"#);
        assert_eq!(parse_release_for(4,response(&item).as_bytes()).unwrap().assets.len(),1);
        assert!(parse_release_for(4,response(&format!("{item},{item}")).as_bytes()).unwrap().assets.is_empty());
    }
}