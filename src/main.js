import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import "./style.css";

const $=selector=>document.querySelector(selector);
const app=$("#app");
const safe=(text="")=>String(text).replace(/[&<>"']/g,s=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[s]));
const initial=()=>{try {return JSON.parse(localStorage.getItem("atelier-kelian-chat")||"[]")}catch{return []}};
const state={
  view:"chat",messages:initial(),draft:"",imagePrompt:"",negative:"blurry, low quality, distorted, text, watermark",
  imageMode:"generate",checkpoint:"",steps:18,imageInput:"",videoInput:"",cutSilence:true,
  imageResult:null,videoResult:"",notice:"",working:false,workLabel:"",
  online:{ollamaOnline:false,modelReady:false,visionReady:false,comfyOnline:false,videoReady:false,
          checkpoints:[],gpuName:null,vramMb:null,usedMb:null,busy:false,model:"qwen3:8b"},
  requestId:null, attachments:[], dragActive:false
};
const save=()=>{try{localStorage.setItem("atelier-kelian-chat",JSON.stringify(state.messages.slice(-120).map(m=>({...m,attachments:(m.attachments||[]).map(({path,name,kind,size,supported})=>({path,name,kind,size,supported}))}))))}catch{}};
const tabs=[["chat","Discussion","✧"],["images","Studio image","▧"],["video","Studio vidéo","▹"],["settings","Paramètres","⚙"]];
const ready=()=>state.online.ollamaOnline&&state.online.modelReady;
function notify(text){state.notice=text;render();}

function layout(content){
  const tab=tabs.find(x=>x[0]===state.view);
  return '<div class="shell"><aside class="sidebar">'+
    '<div class="brand"><div class="brand-icon">✳</div><div class="brand-text"><strong>Atelier IA</strong><small>ÉDITION KÉLIAN</small></div></div>'+
    '<div class="nav-label">ESPACE DE TRAVAIL</div><nav>'+tabs.map(([key,title,mark])=>
    '<button class="nav-link '+(state.view===key?"selected":"")+'" data-action="tab" data-tab="'+key+'"><span class="nav-mark">'+mark+'</span>'+title+(key==="chat"?'<span class="nav-mini">⌘</span>':"")+'</button>').join("")+
    '</nav><div class="sidebar-bottom"><div class="machine"><span class="machine-icon">◈</span><div><strong>HP Victus</strong><small>RTX 4060 · 8 Go VRAM</small></div></div>'+
    '<div class="local-tag">◉ Traitement local et privé</div></div></aside>'+
    '<main class="main"><header class="topbar"><div class="breadcrumbs">ATELIER IA <span>/</span> '+safe(tab?.[1])+'</div>'+
    '<div class="top-actions"><span class="status '+(ready()?"online":"offline")+'"><i></i>'+(ready()?"IA locale prête":"À configurer")+'</span>'+
    '<button class="icon-button" data-action="refresh" title="Actualiser">↻</button></div></header>'+
    '<div class="content">'+content+'</div>'+(state.dragActive&&state.view==='chat'?'<div class="drop-overlay"><div class="drop-inner"><span>＋</span><strong>Dépose tes fichiers ici</strong><small>Images, PDF, Office, code et autres fichiers locaux</small></div></div>':'')+
    (state.notice?'<div class="toast"><span>'+safe(state.notice)+'</span><button data-action="dismiss">✕</button></div>':"")+
    '</main></div>';
}
function fileIcon(kind){return ({image:"▧",pdf:"▤",document:"▦",table:"▥",slides:"▣",text:"≡",other:"◇"})[kind]||"◇";}
function humanSize(bytes){return bytes>=1e9?(bytes/1e9).toFixed(1)+" Go":bytes>=1e6?(bytes/1e6).toFixed(1)+" Mo":bytes>=1e3?(bytes/1e3).toFixed(0)+" Ko":bytes+" o";}
async function addAttachments(paths){
  if(state.working||!Array.isArray(paths)||!paths.length)return;
  const current=state.attachments.map(a=>a.path);
  const unique=paths.filter(path=>!current.includes(path));
  if(!unique.length){state.dragActive=false;render();return;}
  const allowed=unique.slice(0,Math.max(0,12-current.length));
  if(!allowed.length){state.dragActive=false;notify("12 pièces jointes maximum par message.");return;}
  state.dragActive=false;
  try{
    const infos=await invoke("inspect_attachments",{paths:allowed});
    state.attachments.push(...infos);
    if(unique.length>allowed.length)state.notice="Seuls les 12 premiers fichiers ont été joints.";
    else if(infos.some(f=>!f.supported))state.notice="Certaines pièces jointes sont acceptées, mais leur contenu n'est pas lisible automatiquement.";
    render();$("#chat-input")?.focus();
  }catch(e){notify("Impossible de joindre le fichier : "+String(e));}
}
function chatView(){
  const msgs=state.messages.map(m=>'<div class="bubble-row '+m.role+'"><div class="avatar">'+(m.role==="user"?"K":"✳")+'</div><div class="bubble"><div class="by">'+(m.role==="user"?"TOI":"ATELIER IA")+'</div>'+
    (m.attachments?.length?'<div class="history-attachments">'+m.attachments.map(f=>'<span class="history-file">'+fileIcon(f.kind)+' '+safe(f.name)+'</span>').join('')+'</div>':'')+
    '<div class="body">'+(m.content?safe(m.content):'<span class="dots">● ● ●</span>')+'</div></div></div>').join("");
  const welcome=!state.messages.length?'<div class="welcome"><div class="welcome-spark">✳</div><div class="eyebrow">TON ESPACE DE CRÉATION PERSONNEL</div><h1>Bonjour Kélian<span class="dot">.</span></h1><p>Écris, réfléchis, crée. Une intelligence artificielle privée, directement sur ton ordinateur.</p>'+
   '<div class="idea-grid"><button data-action="idea" data-prompt="Aide-moi à expliquer simplement une idée reçue sur les douleurs lombaires pour Instagram.">✍️ <b>Écrire un post</b><small>Du contenu pertinent et accessible</small></button>'+
   '<button data-action="idea" data-prompt="Propose-moi des idées originales de carrousels Instagram pour sensibiliser aux idées reçues sur la santé.">✳ <b>Trouver des idées</b><small>Pour ton compte professionnel</small></button>'+
   '<button data-action="idea" data-prompt="Explique-moi avec des sources scientifiques les principes de la douleur chronique.">◈ <b>Comprendre un sujet</b><small>Une réponse claire et nuancée</small></button></div></div>':
   '<div class="message-list" id="message-list">'+msgs+'</div>';
  const pending=state.attachments.length?'<div class="pending-files">'+state.attachments.map((f,i)=>'<div class="pending-file">'+
    (f.preview?'<img src="'+f.preview+'" alt="Aperçu de la pièce jointe">':'<div class="file-symbol">'+fileIcon(f.kind)+'</div>')+
    '<div class="file-info"><strong title="'+safe(f.name)+'">'+safe(f.name)+'</strong><small>'+humanSize(f.size)+(f.supported?' · Lisible':' · Non analysable')+'</small></div>'+
    '<button type="button" data-action="remove-attachment" data-index="'+i+'" title="Retirer">×</button></div>').join('')+'</div>':'';
  const disabled=!ready()||state.working;
  return '<div class="chat-page"><div class="chat-toolbar"><span>DISCUSSION PRIVÉE</span><button class="soft-btn" data-action="new-chat">＋ Nouvelle discussion</button></div>'+
    '<div class="thread" id="thread">'+welcome+'</div><div class="composer-wrap">'+(!ready()?'<div class="setup-alert">Le moteur conversationnel doit être configuré. <button data-action="tab" data-tab="settings">Configurer →</button></div>':'')+
    '<form id="chat-form" class="composer">'+pending+'<div class="composer-entry"><button type="button" class="attach-btn" data-action="attach" '+(state.working?'disabled':'')+' title="Joindre plusieurs fichiers">＋</button>'+
    '<textarea id="chat-input" placeholder="'+(ready()?"Écris un message ou joins des fichiers…":"Configure d’abord Qwen3 dans Paramètres…")+'" '+(disabled?"disabled":"")+' rows="2">'+safe(state.draft)+'</textarea></div>'+
    '<div class="composer-bottom"><div class="model-chip"><i></i> '+(state.attachments.some(a=>a.kind==="image")?"Vision 3B":"Qwen3 8B")+' <span>· LOCAL</span></div>'+
    '<div><span class="key-tip">Entrée pour envoyer</span> <button class="send-btn" type="submit" '+(disabled||(!state.draft.trim()&&!state.attachments.length)?"disabled":"")+'>↑</button></div></div></form>'+
    '<div class="disclaimer">＋ Pièces jointes · Glisser-déposer · Analyse sur ton ordinateur</div></div></div>';
}
function imageView(){
  const modes=[["generate","Créer"],["edit","Retoucher"],["upscale","Améliorer ×4"]];
  return '<div class="studio"><div class="eyebrow">STUDIO CRÉATIF · 100 % LOCAL</div><h1>Studio image<span class="dot">.</span></h1><p class="lead">De l’idée à l’image, sans envoyer tes créations dans le cloud.</p>'+
  '<div class="workspace"><section class="panel"><div class="padded">'+
  '<div class="tabrow">'+modes.map(([id,n])=>'<button data-action="image-mode" data-mode="'+id+'" class="'+(state.imageMode===id?"active":"")+'">'+n+'</button>').join("")+'</div>'+
  (state.imageMode!=="upscale"?'<label class="field-label" for="prompt">Description de l’image</label><textarea class="area" id="image-prompt" rows="5" placeholder="Décris précisément ce que tu veux obtenir…">'+safe(state.imagePrompt)+'</textarea>':'<p class="hint">Agrandissement ×4 par Real-ESRGAN, exécuté sur ton ordinateur.</p>')+
  (state.imageMode!=="generate"?'<label class="field-label">Image de départ</label><button class="upload" data-action="pick-image">▧ '+(state.imageInput?safe(state.imageInput.split(/[\\/]/).pop()):"Choisir une photo sur le PC")+' <span>Parcourir →</span></button>':"")+
  (state.imageMode!=="upscale"?'<details class="advanced"><summary>Réglages avancés</summary><label class="field-label">À éviter</label><textarea class="area small" id="negative" rows="2">'+safe(state.negative)+'</textarea>'+
  '<label class="field-label">Modèle SDXL</label><select id="checkpoint"><option value="">Choisir un modèle…</option>'+state.online.checkpoints.map(n=>'<option value="'+safe(n)+'" '+(state.checkpoint===n?"selected":"")+'>'+safe(n)+'</option>').join("")+'</select>'+
  '<label class="field-label">Étapes de génération (1–35)</label><input class="number" type="number" min="1" max="35" id="steps" value="'+state.steps+'"></details>':"")+
  '<button class="primary" data-action="run-image" '+(!state.online.comfyOnline||state.working?"disabled":"")+'>✳ '+(state.imageMode==="generate"?"Générer l’image":state.imageMode==="edit"?"Retoucher la photo":"Améliorer la photo")+'</button>'+
  (!state.online.comfyOnline?'<p class="hint">ComfyUI non détecté. <button data-action="tab" data-tab="settings">Installer ou démarrer →</button></p>':"")+
  '</div></section><section class="preview"><div class="preview-label">APERÇU & RÉSULTAT</div>'+
  (state.imageResult?'<img class="result-image" src="'+state.imageResult.dataUrl+'" alt="Image générée"/><div class="result-location">✓ Sauvegardé : '+safe(state.imageResult.path)+'</div>':
   '<div class="empty-preview"><div class="empty-icon">✳</div><strong>À toi de créer</strong><p>Les images apparaissent ici et sont enregistrées automatiquement dans Documents / Atelier IA.</p></div>')+
  '</section></div></div>';
}
function videoView(){
  return '<div class="studio"><div class="eyebrow">STUDIO VIDÉO · MONTAGE AUTOMATISÉ</div><h1>Studio vidéo<span class="dot">.</span></h1><p class="lead">Transforme une vidéo parlée en Reel vertical, avec sous-titres incrustés.</p>'+
  '<div class="workspace"><section class="panel"><div class="padded"><div class="section-heading">Ton fichier vidéo</div>'+
  '<button class="upload large" data-action="pick-video">▹ '+(state.videoInput?safe(state.videoInput.split(/[\\/]/).pop()):"Choisir une vidéo MP4, MOV, MKV ou WebM")+' <span>Parcourir →</span></button>'+
  '<label class="check-row"><input type="checkbox" id="cut" '+(state.cutSilence?"checked":"")+'><span><strong>Supprimer les longs silences</strong><small>Conserver principalement les passages parlés.</small></span></label>'+
  '<div class="step-note"><b>Inclus :</b> transcription Whisper, coupes, format vertical 9:16, sous-titres incrustés, encodage NVIDIA NVENC avec secours CPU.</div>'+
  '<button class="primary" data-action="run-video" '+(!state.videoInput||!state.online.videoReady||state.working?"disabled":"")+'>▹ Créer mon Reel MP4</button>'+
  (!state.online.videoReady?'<p class="hint">Module vidéo à installer. <button data-action="tab" data-tab="settings">Configurer →</button></p>':"")+
  '</div></section><section class="preview video-preview"><div class="preview-label">EXPORT</div><div class="empty-preview"><div class="empty-icon">▹</div>'+
  (state.videoResult?'<strong>✓ Vidéo exportée</strong><p class="path">'+safe(state.videoResult)+'</p>':'<strong>Ton prochain Reel</strong><p>Le fichier MP4 et les sous-titres SRT seront enregistrés dans Documents / Atelier IA.</p>')+
  '</div></section></div></div>';
}
function settingsView(){
  const s=state.online;
  const metric=(label,value)=>'<div class="metric"><span>'+label+'</span><strong>'+safe(value)+'</strong></div>';
  const stateChip=bool=>'<span class="chip '+(bool?"okay":"warn")+'">'+(bool?"● Prêt":"○ À installer")+'</span>';
  return '<div class="studio settings"><div class="eyebrow">TON ENVIRONNEMENT LOCAL</div><h1>Paramètres<span class="dot">.</span></h1><p class="lead">Un moteur à la fois sur la RTX 4060 pour préserver la VRAM et la stabilité.</p>'+
  '<div class="onboard-panel"><div><strong>Configuration complète en un clic</strong><p>Installe Ollama, Qwen3, SDXL, ComfyUI, Whisper et FFmpeg. Connexion Internet et environ 25 Go libres requis.</p></div><button data-action="install-all" '+(state.working?'disabled':'')+'>✳ Tout configurer</button></div>'+ 
  '<div class="settings-grid"><section class="panel padded"><div class="panel-title"><div>✧ <strong>Assistant conversationnel</strong></div>'+stateChip(ready())+'</div>'+
  '<p>Ollama + Qwen3 8B, avec une fenêtre de contexte adaptée à 8 Go de VRAM.</p>'+
  metric("Service Ollama",s.ollamaOnline?"En ligne":"Hors ligne")+metric("Modèle",s.modelReady?"Qwen3 8B installé":"Non téléchargé")+
  '<div class="panel-actions"><button class="secondary" data-action="install" data-component="chat" '+(state.working?"disabled":"")+'>Installer Ollama</button>'+
  '<button class="secondary" data-action="start" data-component="chat" '+(state.working?"disabled":"")+'>Démarrer</button>'+
  '<button class="primary small-primary" data-action="pull" '+(!s.ollamaOnline||s.modelReady||state.working?"disabled":"")+'>Télécharger Qwen3</button></div>'+
  '</section><section class="panel padded"><div class="panel-title"><div>▧ <strong>Studio image</strong></div>'+stateChip(s.comfyOnline)+'</div>'+
  '<p>ComfyUI, SDXL et Real-ESRGAN. Installation volumineuse, à lancer une seule fois.</p>'+
  metric("ComfyUI",s.comfyOnline?"Connecté":"Non démarré")+metric("Modèles détectés",String(s.checkpoints.length))+
  '<div class="panel-actions"><button class="secondary" data-action="install" data-component="images" '+(state.working?"disabled":"")+'>Installer modèles et moteur</button>'+
  '<button class="primary small-primary" data-action="start" data-component="images" '+(state.working?"disabled":"")+'>Démarrer ComfyUI</button></div></section>'+
  '<section class="panel padded"><div class="panel-title"><div>▹ <strong>Studio vidéo</strong></div>'+stateChip(s.videoReady)+'</div>'+
  '<p>Whisper + FFmpeg, accélération GPU si compatible, secours CPU.</p>'+metric("Transcription",s.videoReady?"Prête":"Non installée")+
  '<div class="panel-actions"><button class="secondary" data-action="install" data-component="video" '+(state.working?"disabled":"")+'>Installer le studio vidéo</button></div></section>'+
  '<section class="panel padded"><div class="panel-title"><div>◈ <strong>Ressources GPU</strong></div></div>'+
  '<p>Contrôle mémoire pour éviter d’exécuter simultanément plusieurs modèles lourds.</p>'+
  metric("GPU détecté",s.gpuName||"Indisponible")+metric("VRAM utilisée",s.vramMb?((s.usedMb||0)/1024).toFixed(1)+" / "+(s.vramMb/1024).toFixed(1)+" Go":"Indisponible")+
  '<div class="panel-actions"><button class="secondary" data-action="free" '+(state.working?"disabled":"")+'>Libérer la VRAM</button><button class="secondary" data-action="refresh">↻ Diagnostic</button></div></section></div>'+
  '<div class="footnote">Version 1.0.0 · Modules et modèles locaux · Premiers téléchargements : Internet requis · Aucun coût API.</div></div>';
}
function render(){
  const content={chat:chatView,images:imageView,video:videoView,settings:settingsView}[state.view]();
  app.innerHTML=layout(content);
  if(state.view==="chat"){
    const thread=$("#thread");if(thread)thread.scrollTop=thread.scrollHeight;
  }
}
async function refresh(){
  try{
    state.online=await invoke("status");
    if(!state.checkpoint&&state.online.checkpoints?.length)
      state.checkpoint=state.online.checkpoints.find(x=>x.includes("sd_xl_base"))||state.online.checkpoints[0];
    render();
  } catch(e) {notify("Diagnostic indisponible : "+String(e));}
}
async function task(label,fn){
  if(state.working)return;
  state.working=true;state.workLabel=label;state.notice=label+"…";render();
  try{
    const result=await fn();
    state.notice=label+" : terminé.";
    return result;
  }catch(e){state.notice="Erreur : "+String(e).slice(0,850);}
  finally{state.working=false;await refresh();render();}
}
async function send(){
  const text=state.draft.trim();
  if((!text&&!state.attachments.length)||!ready()||state.working)return;
  // Deterministic local routing for explicit creative commands; no cloud classifier.
  const imageIntent=/^\s*(?:\/image\b|(?:génère|générer|crée|créer|dessine|dessiner|fais|faire)\b.{0,60}\b(?:image|photo|illustration|visuel)\b)/i.test(text);
  const photoIntent=/^\s*(?:\/retouche\b|(?:retouche|retoucher|modifie|modifier)\b.{0,50}\b(?:image|photo|visuel)\b)/i.test(text);
  const videoIntent=/^\s*(?:\/video\b|(?:monte|monter|transforme|transformer|sous-titre|sous-titrer)\b.{0,60}\b(?:vidéo|video|reel|clip)\b)/i.test(text);
  if(!state.attachments.length && (imageIntent||photoIntent)){
    state.imageMode=photoIntent?"edit":"generate";
    state.imagePrompt=text.replace(/^\/(image|retouche)\s*/i,"").trim();
    state.draft="";state.view="images";render();
    if(photoIntent&&!state.imageInput){
      const p=await invoke("pick",{kind:"image"});if(p)state.imageInput=p;
    }
    if(state.online.comfyOnline && (state.imageMode==="generate"||state.imageInput)){
      await action({dataset:{action:"run-image"}});
    }else{notify("Le studio image doit être configuré ou une photo doit être choisie.");}
    return;
  }
  if(!state.attachments.length && videoIntent){
    state.view="video";state.draft="";render();
    if(!state.videoInput){
      const p=await invoke("pick",{kind:"video"});if(p)state.videoInput=p;
    }
    render();
    notify("Vidéo reconnue : utilise « Créer mon Reel MP4 » pour lancer le montage.");
    return;
  }
  if(state.attachments.some(a=>a.kind==="image")&&!state.online.visionReady){
    notify("Installe d’abord le modèle Vision 3B dans Paramètres pour analyser les images.");
    return;
  }
  const incoming=state.attachments.map(x=>({...x}));
  state.working=true;state.draft="";state.attachments=[];
  state.messages.push({role:"user",content:text||"Analyse les fichiers joints.",attachments:incoming},{role:"assistant",content:""});
  save();render();
  const rid=String(Date.now())+"-"+Math.random().toString(36).slice(2);
  state.requestId=rid;
  const messages=state.messages.slice(0,-1).map(x=>({role:x.role,content:x.content,
     attachments:(x.attachments||[]).map(a=>({path:a.path}))}));
  try{
    await invoke("chat",{id:rid,messages});
    if(!state.messages.at(-1)?.content)state.messages.at(-1).content="Aucune réponse reçue.";
  }catch(e){state.messages.at(-1).content="Erreur : "+String(e);}
  finally{state.working=false;state.requestId=null;save();render();}
}
async function action(el){
  const a=el.dataset.action;
  if(a==="tab"){state.view=el.dataset.tab;state.notice="";render();}
  if(a==="dismiss"){state.notice="";render();}
  if(a==="refresh"){await refresh();}
  if(a==="new-chat"){state.messages=[];save();render();}
  if(a==="idea"){state.draft=el.dataset.prompt;render();$("#chat-input")?.focus();}
  if(a==="attach"){const paths=await invoke("pick_attachments");await addAttachments(paths);}
  if(a==="remove-attachment"){state.attachments.splice(Number(el.dataset.index),1);render();}
  if(a==="image-mode"){state.imageMode=el.dataset.mode;render();}
  if(a==="pick-image"){const p=await invoke("pick",{kind:"image"});if(p){state.imageInput=p;render();}}
  if(a==="pick-video"){const p=await invoke("pick",{kind:"video"});if(p){state.videoInput=p;render();}}
  if(a==="install-all"){
    await task("Configuration complète",async()=>{
      state.notice="Installation Ollama…";render();
      await invoke("install_component",{component:"chat"});
      try{await invoke("start_engines",{component:"chat"});}catch{}
      let connected=false;
      for(let i=0;i<15;i++){
        await new Promise(resolve=>setTimeout(resolve,1000));
        const current=await invoke("status");
        if(current.ollamaOnline){connected=true;break;}
      }
      if(!connected)throw Error("Ollama ne répond pas après installation. Redémarre Windows et reprends.");
      state.notice="Téléchargement du modèle Qwen3…";render();
      await invoke("model_pull",{model:"qwen3:8b"});
      state.notice="Téléchargement du modèle Vision pour les images jointes…";render();
      await invoke("model_pull",{model:"qwen2.5vl:3b"});
      state.notice="Installation du studio image et des modèles CUDA…";render();
      await invoke("install_component",{component:"images"});
      await invoke("start_engines",{component:"images"});
      state.notice="Installation du studio vidéo…";render();
      await invoke("install_component",{component:"video"});
    });
  }
  if(a==="install"){await task("Installation "+el.dataset.component,()=>invoke("install_component",{component:el.dataset.component}));}
  if(a==="start"){await task("Démarrage "+el.dataset.component,()=>invoke("start_engines",{component:el.dataset.component}));}
  if(a==="pull"){await task("Téléchargement Qwen3",()=>invoke("model_pull",{model:"qwen3:8b"}));}
  if(a==="pull-vision"){await task("Téléchargement modèle Vision",()=>invoke("model_pull",{model:"qwen2.5vl:3b"}));}
  if(a==="free"){await task("Libération de VRAM",()=>invoke("free_gpu"));}
  if(a==="run-image"){
    if(state.imageMode!=="upscale"&&!state.imagePrompt.trim()){notify("Décris d’abord l’image.");return;}
    if(state.imageMode!=="generate"&&!state.imageInput){notify("Choisis une photo.");return;}
    const mode=state.imageMode;
    await task("Traitement de l’image",async()=>{
      state.imageResult=await invoke("image_job",{mode,prompt:state.imagePrompt,negative:state.negative,
        checkpoint:state.checkpoint,input:mode==="generate"?null:state.imageInput,steps:state.steps});
    });
  }
  if(a==="run-video"){
    await task("Montage et transcription",async()=>{
      state.videoResult=await invoke("video_job",{file:state.videoInput,cutSilence:state.cutSilence});
    });
  }
}
app.addEventListener("click",e=>{
  const btn=e.target.closest("[data-action]");if(btn)void action(btn);
});
app.addEventListener("input",e=>{
  const id=e.target.id;
  if(id==="chat-input"){state.draft=e.target.value;const b=$(".send-btn");if(b)b.disabled=!state.draft.trim()&&!state.attachments.length;}
  if(id==="image-prompt")state.imagePrompt=e.target.value;
  if(id==="negative")state.negative=e.target.value;
  if(id==="checkpoint")state.checkpoint=e.target.value;
  if(id==="steps")state.steps=Math.min(35,Math.max(1,Number(e.target.value)||18));
  if(id==="cut")state.cutSilence=e.target.checked;
});
app.addEventListener("keydown",e=>{
  if(e.target.id==="chat-input"&&e.key==="Enter"&&!e.shiftKey&&!e.isComposing){
    e.preventDefault();void send();
  }
});
app.addEventListener("submit",e=>{if(e.target.id==="chat-form"){e.preventDefault();void send();}});
await listen("chat-token",event=>{
  if(event.payload?.id!==state.requestId)return;
  const last=state.messages.at(-1);
  if(last?.role==="assistant"){last.content+=event.payload.text||"";save();
    const bubble=$(".message-list .bubble-row:last-child .body");
    if(bubble){bubble.textContent=last.content;$("#thread").scrollTop=$("#thread").scrollHeight;}
  }
});
await listen("model-progress",event=>{
  const v=event.payload||{};
  const percent=v.total?Math.floor(100*(v.completed||0)/v.total):null;
  state.notice="Téléchargement Qwen3 : "+(v.status||"en cours")+(percent!==null?" · "+percent+" %":"");
  const toast=$(".toast span");if(toast)toast.textContent=state.notice;
});
render();
void refresh();
setInterval(()=>{if(!state.working)void refresh()},30000);

try{
  await getCurrentWebview().onDragDropEvent(event=>{
    const payload=event.payload||{};
    if(state.view!=="chat")return;
    if(payload.type==="enter"||payload.type==="over"){
      if(!state.dragActive){state.dragActive=true;render();}
    }else if(payload.type==="leave"){
      if(state.dragActive){state.dragActive=false;render();}
    }else if(payload.type==="drop"){
      state.dragActive=false;
      void addAttachments(payload.paths||[]);
    }
  });
}catch(err){console.warn("Glisser-déposer natif non disponible :",err);}
