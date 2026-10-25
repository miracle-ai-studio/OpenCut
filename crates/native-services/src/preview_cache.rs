//! Trusted installed-runtime fingerprint for derived preview cache admission.
//! No media, native paths or executable selectors cross the public boundary.
use crate::classic_visual::ClassicVisualProcessConfig;
use opencut_contracts::EditorError;
use sha2::{Digest,Sha256};
use std::{fs::{self,File},io::Read,path::{Path,PathBuf},sync::{Mutex,OnceLock},time::SystemTime};
type Stamp = (PathBuf,u64,SystemTime);
static LAST: OnceLock<Mutex<Option<(Vec<Stamp>,String)>>> = OnceLock::new();
fn failure() -> EditorError {EditorError::new("PREVIEW_RUNTIME_UNAVAILABLE","Cannot identify the installed preview renderer")}
fn files(root:&Path, paths:&mut Vec<PathBuf>) -> Result<(),EditorError> {
    for entry in fs::read_dir(root).map_err(|_|failure())? {
        let entry=entry.map_err(|_|failure())?; let path=entry.path();
        if entry.file_type().map_err(|_|failure())?.is_dir() {files(&path,paths)?;}
        else if matches!(path.extension().and_then(|v|v.to_str()),Some("mjs"|"wgsl"|"json")) {paths.push(path);}
        if paths.len()>512 {return Err(failure());}
    } Ok(())
}
pub fn fingerprint(config:&ClassicVisualProcessConfig, ffmpeg:&Path, ffprobe:&Path) -> Result<String,EditorError> {
    let package=config.runner.parent().and_then(Path::parent).ok_or_else(failure)?;
    let shared=package.join("node_modules/@opencut/classic-audio-renderer").canonicalize().map_err(|_|failure())?;
    let mut paths=vec![config.node.clone(),config.chromium.clone(),std::env::current_exe().map_err(|_|failure())?,
        ffmpeg.into(),ffprobe.into(),package.join("package.json"),package.join("node_modules/mediabunny/dist/bundles/mediabunny.mjs")];
    for root in [package.join("src"),package.join("host"),package.join("vendor"),shared.join("src"),shared.join("host")] {files(&root,&mut paths)?;}
    paths.sort();paths.dedup();
    let stamps=paths.iter().map(|path| {let stat=fs::metadata(path).map_err(|_|failure())?;Ok((path.clone(),stat.len(),stat.modified().map_err(|_|failure())?))}).collect::<Result<Vec<_>,EditorError>>()?;
    let mut last=LAST.get_or_init(||Mutex::new(None)).lock().map_err(|_|failure())?;
    if let Some((previous,key))=&*last {if previous==&stamps {return Ok(key.clone());}}
    let mut hash=Sha256::new();hash.update(b"opencut-preview-runtime-1\0");
    let mut buffer=[0u8;65536];
    for path in paths {
        // File names/order participate as well as actual content. Absolute paths
        // are private and affect reuse only, never source timeline semantics.
        hash.update(path.to_string_lossy().as_bytes());hash.update([0]);
        let mut file=File::open(path).map_err(|_|failure())?;
        loop {let count=file.read(&mut buffer).map_err(|_|failure())?;if count==0{break;}hash.update(&buffer[..count]);}
        hash.update([0]);
    }
    let key=format!("{:x}",hash.finalize());*last=Some((stamps,key.clone()));Ok(key)
}
