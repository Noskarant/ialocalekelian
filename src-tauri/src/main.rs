use base64::Engine as _;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{path::{Path, PathBuf}, sync::atomic::{AtomicBool, Ordering}, time::Duration};
use tauri::{Emitter, State};
use tokio::{fs, process::Command, time::sleep};

const OLLAMA: &str = "http://127.0.0.1:11434";
const COMFY: &str = "http://127.0.0.1:8188";
const MODEL: &str = "qwen3:8b";

#[derive(Default)]
struct Engine { busy: AtomicBool }
struct Lock<'a>(&'a AtomicBool);
impl<'a> Lock<'a> {
    fn acquire(v: &'a AtomicBool) -> Result<Self,String> {
        v.compare_exchange(false,true,Ordering::AcqRel,Ordering::Relaxed)
            .map_err(|_| "Une opération est déjà en cours. Patiente avant de poursuivre.".to_string())?;
        Ok(Self(v))
    }
}
impl Drop for Lock<'_> { fn drop(&mut self) {self.0.store(false,Ordering::Release);} }

fn http() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(600)).build().expect("HTTP client")
}
fn root() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("AtelierIA")
}
fn outputs() -> PathBuf {
    dirs::document_dir().unwrap_or_else(root).join("Atelier IA")
}
fn ensure_local_file(path: &str) -> Result<PathBuf,String> {
    let p = PathBuf::from(path);
    if !p.is_file() { return Err("Le fichier sélectionné est introuvable.".into()); }
    let meta = std::fs::metadata(&p).map_err(|e| e.to_string())?;
    if meta.len() > 8 * 1024 * 1024 * 1024 {return Err("Fichier trop volumineux (8 Go maximum).".into());}
    Ok(p)
}
async fn release_ollama() {
    let _ = http().post(format!("{OLLAMA}/api/generate"))
        .json(&json!({"model":MODEL,"prompt":"","keep_alive":0,"stream":false}))
        .send().await;
}
async fn release_comfy() {
    let _=http().post(format!("{COMFY}/free")).json(&json!({"unload_models":true,"free_memory":true})).send().await;
}
async fn health(url: &str) -> bool {
    http().get(url).send().await.map(|x|x.status().is_success()).unwrap_or(false)
}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct Status {
    ollama_online:bool, model_ready:bool, comfy_online:bool, video_ready:bool,
    gpu_name:Option<String>, vram_mb:Option<u64>, used_mb:Option<u64>,
    busy:bool, model:&'static str, checkpoints:Vec<String>
}
async fn checkpoints() -> Vec<String> {
    if let Ok(r)=http().get(format!("{COMFY}/object_info/CheckpointLoaderSimple")).send().await {
        if let Ok(v)=r.json::<Value>().await {
            return v.pointer("/CheckpointLoaderSimple/input/required/ckpt_name/0")
                .and_then(Value::as_array).map(|a|a.iter().filter_map(|x|x.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
        }
    }
    Vec::new()
}
#[tauri::command]
async fn status(engine:State<'_,Engine>) -> Result<Status,String> {
    let tags=http().get(format!("{OLLAMA}/api/tags")).send().await;
    let (ollama_online,model_ready)=match tags {
        Ok(r) if r.status().is_success() => {
            let v:Value=r.json().await.unwrap_or(Value::Null);
            let installed=v.get("models").and_then(Value::as_array).map(|a|a.iter().any(|m|
                m.get("name").and_then(Value::as_str).map(|s|s==MODEL || s.starts_with("qwen3:8b-")).unwrap_or(false))).unwrap_or(false);
            (true,installed)
        }, _=>(false,false)
    };
    let smi=Command::new("nvidia-smi").args(["--query-gpu=name,memory.total,memory.used","--format=csv,noheader,nounits"])
        .output().await.ok().filter(|o|o.status.success()).map(|o|String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
    let line:Vec<_>=smi.lines().next().unwrap_or("").split(',').map(str::trim).collect();
    let comfy_online=health(&format!("{COMFY}/system_stats")).await;
    let video_ready=root().join("video-venv").join("Scripts").join("python.exe").exists();
    Ok(Status{ollama_online,model_ready,comfy_online,video_ready,
        gpu_name:line.first().filter(|s|!s.is_empty()).map(|s|s.to_string()),
        vram_mb:line.get(1).and_then(|s|s.parse().ok()),
        used_mb:line.get(2).and_then(|s|s.parse().ok()),
        busy:engine.busy.load(Ordering::Acquire),model:MODEL,
        checkpoints:if comfy_online{checkpoints().await}else{Vec::new()}
    })
}
#[derive(Deserialize)]
struct Message{role:String,content:String}
#[tauri::command]
async fn chat(app:tauri::AppHandle,engine:State<'_,Engine>,id:String,messages:Vec<Message>) -> Result<(),String> {
    let _lock=Lock::acquire(&engine.busy)?;
    if id.len()>100 || messages.is_empty() {return Err("Conversation invalide.".into());}
    let mut recent:Vec<_>=messages.into_iter().rev().take(18).collect();
    recent.reverse();
    for m in &recent {
        if !["user","assistant","system"].contains(&m.role.as_str()) || m.content.len()>12000 {
            return Err("Un message est invalide ou trop long.".into());
        }
    }
    release_comfy().await;
    let r=http().post(format!("{OLLAMA}/api/chat")).json(&json!({
        "model":MODEL,"messages":recent.iter().map(|m|json!({"role":m.role,"content":m.content})).collect::<Vec<_>>(),
        "stream":true,"think":false,"keep_alive":"10s",
        "options":{"num_ctx":4096,"num_predict":1300,"temperature":0.7}
    })).send().await.map_err(|e|format!("Ollama indisponible : {e}"))?;
    if !r.status().is_success(){return Err(format!("Le modèle a refusé la demande : HTTP {}.",r.status()));}
    let mut bytes=r.bytes_stream();let mut line=Vec::new();
    while let Some(fragment)=bytes.next().await {
        line.extend_from_slice(&fragment.map_err(|e|e.to_string())?);
        if line.len()>1_000_000{return Err("Flux Ollama trop volumineux.".into());}
        while let Some(i)=line.iter().position(|b|*b==b'\n') {
            let part:Vec<u8>=line.drain(..=i).collect();
            let Ok(v)=serde_json::from_slice::<Value>(&part) else{continue};
            if let Some(err)=v.get("error").and_then(Value::as_str){return Err(err.into());}
            if let Some(s)=v.pointer("/message/content").and_then(Value::as_str) {
                if !s.is_empty(){let _=app.emit("chat-token",json!({"id":id,"text":s}));}
            }
        }
    }
    let _=app.emit("chat-end",json!({"id":id}));
    Ok(())
}
#[tauri::command]
async fn model_pull(app:tauri::AppHandle,engine:State<'_,Engine>)->Result<(),String>{
    let _lock=Lock::acquire(&engine.busy)?;
    let c=reqwest::Client::builder().no_proxy().connect_timeout(Duration::from_secs(10)).build().map_err(|e|e.to_string())?;
    let r=c.post(format!("{OLLAMA}/api/pull")).json(&json!({"model":MODEL,"stream":true}))
        .send().await.map_err(|e|e.to_string())?;
    if !r.status().is_success(){return Err(format!("Erreur Ollama : {}",r.status()));}
    let mut stream=r.bytes_stream();let mut buf=Vec::new();
    while let Some(ch)=stream.next().await {
        buf.extend_from_slice(&ch.map_err(|e|e.to_string())?);
        while let Some(i)=buf.iter().position(|b|*b==b'\n') {
            let bytes:Vec<u8>=buf.drain(..=i).collect();
            if let Ok(v)=serde_json::from_slice::<Value>(&bytes) {
                let _=app.emit("model-progress",&v);
                if let Some(e)=v.get("error").and_then(Value::as_str){return Err(e.to_string());}
            }
        }
    }
    Ok(())
}
#[tauri::command]
async fn free_gpu(engine:State<'_,Engine>)->Result<(),String>{
    let _lock=Lock::acquire(&engine.busy)?;
    release_ollama().await;release_comfy().await;Ok(())
}
#[tauri::command]
async fn pick(kind:String)->Result<Option<String>,String>{
    let picked=tokio::task::spawn_blocking(move||{
        let d=rfd::FileDialog::new();
        let d=match kind.as_str(){
            "video"=>d.add_filter("Vidéos",&["mp4","mov","mkv","webm"]),
            _=>d.add_filter("Images",&["png","jpg","jpeg","webp"])
        };
        d.pick_file().map(|p|p.to_string_lossy().into_owned())
    }).await.map_err(|e|e.to_string())?;
    Ok(picked)
}
#[tauri::command]
async fn install_component(engine:State<'_,Engine>,component:String)->Result<String,String>{
    let _lock=Lock::acquire(&engine.busy)?;
    if !["chat","images","video"].contains(&component.as_str()){return Err("Module inconnu.".into());}
    let home=root();fs::create_dir_all(&home).await.map_err(|e|e.to_string())?;
    let script=home.join("bootstrap.ps1");
    fs::write(&script,include_str!("../../scripts/bootstrap.ps1")).await.map_err(|e|e.to_string())?;
    let out=Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-ExecutionPolicy","Bypass","-File"])
        .arg(&script).arg("-Component").arg(&component)
        .output().await.map_err(|e|format!("PowerShell indisponible : {e}"))?;
    if !out.status.success() {
        return Err(format!("Échec de l'installation : {}\n{}",String::from_utf8_lossy(&out.stderr),String::from_utf8_lossy(&out.stdout))
            .chars().take(1800).collect());
    }
    Ok(String::from_utf8_lossy(&out.stdout).chars().rev().take(800).collect::<String>().chars().rev().collect())
}
#[tauri::command]
async fn start_engines(component:String)->Result<(),String>{
    if component=="chat" {
        let _=Command::new("ollama").arg("serve").spawn().map_err(|e|format!("Ollama introuvable : {e}"))?;
        return Ok(());
    }
    if component=="images" {
        let home=root();
        let py=home.join("image-venv").join("Scripts").join("python.exe");
        let main=home.join("ComfyUI").join("main.py");
        if !py.exists()||!main.exists(){return Err("Installe d'abord le studio image.".into());}
        Command::new(py).arg(main).args(["--listen","127.0.0.1","--port","8188","--lowvram"])
            .current_dir(home.join("ComfyUI")).spawn().map_err(|e|e.to_string())?;
        return Ok(());
    }
    Err("Moteur non reconnu.".into())
}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct ImageResult{data_url:String,path:String}
#[tauri::command]
async fn image_job(engine:State<'_,Engine>, mode:String,prompt:String,negative:String,
    checkpoint:String,input:Option<String>,steps:u32) -> Result<ImageResult,String> {
    let _lock=Lock::acquire(&engine.busy)?;
    if !["generate","edit","upscale"].contains(&mode.as_str()) {return Err("Mode image invalide.".into());}
    if prompt.len()>2500 || negative.len()>1200 || steps>35 || steps<1 {return Err("Paramètres image invalides.".into());}
    if checkpoint.is_empty()&&mode!="upscale"{return Err("Aucun modèle SDXL installé.".into());}
    if mode!="generate"&&input.is_none(){return Err("Sélectionne une image.".into());}
    release_ollama().await;
    let client=http();
    let mut source_name=String::new();
    if let Some(p)=input {
        let path=ensure_local_file(&p)?;
        let bytes=fs::read(&path).await.map_err(|e|e.to_string())?;
        if bytes.len()>25_000_000{return Err("Image de plus de 25 Mo : réduis-la avant import.".into());}
        let part=reqwest::multipart::Part::bytes(bytes).file_name(
            path.file_name().and_then(|s|s.to_str()).unwrap_or("image.png").to_string()
        );
        let form=reqwest::multipart::Form::new().part("image",part).text("overwrite","false");
        let r=client.post(format!("{COMFY}/upload/image")).multipart(form).send().await.map_err(|e|e.to_string())?;
        if !r.status().is_success(){return Err(format!("Import image refusé : {}",r.status()));}
        let v:Value=r.json().await.map_err(|e|e.to_string())?;
        source_name=v.get("name").and_then(Value::as_str).unwrap_or("").to_string();
        if source_name.is_empty(){return Err("L'import n'a pas produit de nom de fichier.".into());}
    }
    let seed=uuid::Uuid::new_v4().as_u128() as u64 % 2_147_483_647;
    let mut graph=json!({
        "4":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":checkpoint}},
        "6":{"class_type":"CLIPTextEncode","inputs":{"text":prompt,"clip":["4",1]}},
        "7":{"class_type":"CLIPTextEncode","inputs":{"text":negative,"clip":["4",1]}},
        "3":{"class_type":"KSampler","inputs":{"seed":seed,"steps":steps,"cfg":6.0,"sampler_name":"euler","scheduler":"normal","denoise":1.0,
             "model":["4",0],"positive":["6",0],"negative":["7",0],"latent_image":["5",0]}},
        "8":{"class_type":"VAEDecode","inputs":{"samples":["3",0],"vae":["4",2]}},
        "9":{"class_type":"SaveImage","inputs":{"filename_prefix":"AtelierIA","images":["8",0]}}
    });
    if mode=="generate" { graph["5"]=json!({"class_type":"EmptyLatentImage","inputs":{"width":832,"height":1216,"batch_size":1}}); }
    if mode=="edit" {
        graph["10"]=json!({"class_type":"LoadImage","inputs":{"image":source_name}});
        graph["5"]=json!({"class_type":"VAEEncode","inputs":{"pixels":["10",0],"vae":["4",2]}});
        graph["3"]["inputs"]["denoise"]=json!(0.55);
    }
    if mode=="upscale" {
        graph=json!({
            "10":{"class_type":"LoadImage","inputs":{"image":source_name}},
            "11":{"class_type":"UpscaleModelLoader","inputs":{"model_name":"RealESRGAN_x4plus.pth"}},
            "12":{"class_type":"ImageUpscaleWithModel","inputs":{"upscale_model":["11",0],"image":["10",0]}},
            "9":{"class_type":"SaveImage","inputs":{"filename_prefix":"AtelierIA-Upscale","images":["12",0]}}
        });
    }
    let r=client.post(format!("{COMFY}/prompt")).json(&json!({"prompt":graph})).send().await
        .map_err(|e|format!("ComfyUI indisponible : {e}"))?;
    let status=r.status();
    let v:Value=r.json().await.map_err(|e|e.to_string())?;
    if !status.is_success() || v.get("error").is_some() {
        return Err(format!("ComfyUI a refusé le workflow : {}",v.to_string().chars().take(1000).collect::<String>()));
    }
    let id=v.get("prompt_id").and_then(Value::as_str).ok_or("Aucun identifiant ComfyUI.")?.to_string();
    let mut image_data=None;
    for _ in 0..360 {
        sleep(Duration::from_secs(2)).await;
        let res=client.get(format!("{COMFY}/history/{id}")).send().await;
        let Ok(r)=res else{continue};
        let Ok(history)=r.json::<Value>().await else{continue};
        let record=&history[&id];
        if record.is_null(){continue;}
        if record.pointer("/status/status_str").and_then(Value::as_str)==Some("error"){
            return Err(format!("Exécution ComfyUI en erreur : {}",record["status"].to_string().chars().take(700).collect::<String>()));
        }
        if let Some(o)=record.pointer("/outputs/9/images/0") {
            let filename=o["filename"].as_str().unwrap_or("");
            let subfolder=o["subfolder"].as_str().unwrap_or("");
            let kind=o["type"].as_str().unwrap_or("output");
            if !filename.is_empty(){
                let r=client.get(format!("{COMFY}/view")).query(&[("filename",filename),("subfolder",subfolder),("type",kind)])
                    .send().await.map_err(|e|e.to_string())?;
                if r.status().is_success(){image_data=Some(r.bytes().await.map_err(|e|e.to_string())?.to_vec());break;}
            }
        }
    }
    let bytes=image_data.ok_or("Délai dépassé : image introuvable dans ComfyUI.")?;
    let out=outputs();fs::create_dir_all(&out).await.map_err(|e|e.to_string())?;
    let p=out.join(format!("image-{}.png",uuid::Uuid::new_v4()));
    fs::write(&p,&bytes).await.map_err(|e|e.to_string())?;
    release_comfy().await;
    Ok(ImageResult{data_url:format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes)),
        path:p.to_string_lossy().into_owned()})
}
#[tauri::command]
async fn video_job(engine:State<'_,Engine>,file:String,cut_silence:bool)->Result<String,String>{
    let _lock=Lock::acquire(&engine.busy)?;
    let input=ensure_local_file(&file)?;
    release_ollama().await;release_comfy().await;
    let home=root();
    let py=home.join("video-venv").join("Scripts").join("python.exe");
    if !py.exists(){return Err("Le module vidéo doit être installé dans Paramètres.".into());}
    fs::create_dir_all(&home).await.map_err(|e|e.to_string())?;
    let script=home.join("video_worker.py");
    fs::write(&script,include_str!("../../scripts/video_worker.py")).await.map_err(|e|e.to_string())?;
    let out=outputs();fs::create_dir_all(&out).await.map_err(|e|e.to_string())?;
    let dst=out.join(format!("reel-{}.mp4",uuid::Uuid::new_v4()));
    let result=Command::new(&py).arg(script).arg("--input").arg(&input)
        .arg("--output").arg(&dst).arg("--cut-silence").arg(if cut_silence{"yes"}else{"no"})
        .output().await.map_err(|e|e.to_string())?;
    if !result.status.success() {
        return Err(format!("Montage vidéo : {}",String::from_utf8_lossy(&result.stderr).chars().rev().take(1800)
            .collect::<String>().chars().rev().collect::<String>()));
    }
    if !dst.exists(){return Err("FFmpeg n'a pas créé de fichier MP4.".into());}
    Ok(dst.to_string_lossy().into_owned())
}
fn main(){
    tauri::Builder::default().manage(Engine::default())
        .invoke_handler(tauri::generate_handler![status,chat,model_pull,free_gpu,pick,install_component,start_engines,image_job,video_job])
        .run(tauri::generate_context!()).expect("Atelier IA Windows");
}
