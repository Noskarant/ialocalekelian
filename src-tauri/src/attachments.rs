use std::{fs::File, io::{Read,Cursor}, path::{Path,PathBuf}};
use serde::{Deserialize,Serialize};
use base64::Engine as _;

const MAX_FILE:u64=2_147_483_648; // path metadata can be attached up to 2 GiB
const TEXT_BYTES:u64=2_000_000;
const PDF_BYTES:u64=32_000_000;
const IMAGE_BYTES:u64=25_000_000;

#[derive(Deserialize,Serialize,Clone)]
pub struct AttachmentRef {
    pub path:String,
}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct AttachmentInfo {
    pub path:String,
    pub name:String,
    pub size:u64,
    pub kind:String,
    pub supported:bool,
    pub preview:Option<String>,
}
fn ext(path:&Path)->String{
    path.extension().and_then(|s|s.to_str()).unwrap_or("").to_lowercase()
}
fn is_image(e:&str)->bool {matches!(e,"png"|"jpg"|"jpeg"|"webp"|"bmp"|"gif"|"tif"|"tiff")}
fn is_text(e:&str)->bool {
    matches!(e,"txt"|"md"|"markdown"|"csv"|"tsv"|"json"|"jsonl"|"xml"|"html"|"htm"|"yaml"|"yml"|
    "py"|"js"|"jsx"|"ts"|"tsx"|"rs"|"java"|"c"|"cpp"|"h"|"hpp"|"css"|"sql"|"log"|"ini"|
    "toml"|"sh"|"ps1"|"bat"|"cmd"|"svg"|"srt"|"vtt"|"tex"|"eml"|"ics"|"conf"|"ipynb"|
    "go"|"rb"|"php"|"swift"|"kt"|"dart"|"vue"|"svelte"|"env"|"gitignore"|"gradle"|"properties")
}
fn kind(e:&str)->&'static str{
    if is_image(e){"image"}
    else if e=="pdf"{"pdf"}
    else if matches!(e,"docx"|"docm"){"document"}
    else if matches!(e,"xlsx"|"xls"|"xlsm"|"xlsb"|"ods"){"table"}
    else if matches!(e,"pptx"|"pptm"){"slides"}
    else if is_text(e){"text"}
    else {"other"}
}
pub fn checked(path:&str)->Result<(PathBuf,u64),String>{
    let p=PathBuf::from(path);
    if !p.is_file(){return Err(format!("Fichier introuvable : {}",p.display()));}
    let metadata=std::fs::metadata(&p).map_err(|e|e.to_string())?;
    if metadata.len()>MAX_FILE{return Err("Fichier supérieur à 2 Go : non accepté.".into());}
    Ok((p,metadata.len()))
}
pub fn inspect(paths:Vec<String>)->Result<Vec<AttachmentInfo>,String>{
    if paths.len()>12{return Err("12 pièces jointes maximum par message.".into());}
    paths.iter().map(|p|{
        let (path,size)=checked(p)?;
        let e=ext(&path);
        let group=kind(&e);
        let name=path.file_name().and_then(|s|s.to_str()).unwrap_or("Sans nom").to_string();
        let supported= group!="other" && (group=="image"||size<=PDF_BYTES);
        let preview=if group=="image" && size<=IMAGE_BYTES {
            image_preview(&path,250).ok()
        }else{None};
        Ok(AttachmentInfo{path:p.clone(),name,size,kind:group.into(),supported,preview})
    }).collect()
}
fn image_preview(path:&Path,max:u32)->Result<String,String>{
    let img=image::open(path).map_err(|e|format!("Image non décodable : {e}"))?;
    let thumbnail=img.thumbnail(max,max);
    let mut memory=Cursor::new(Vec::new());
    thumbnail.write_to(&mut memory,image::ImageFormat::Png).map_err(|e|e.to_string())?;
    Ok(format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(memory.into_inner())))
}
pub fn vision_image(path:&Path)->Result<String,String>{
    let metadata=std::fs::metadata(path).map_err(|e|e.to_string())?;
    if metadata.len()>IMAGE_BYTES {return Err("L'image dépasse 25 Mo : réduis-la.".into());}
    // Scale for low VRAM and bound the encoded image before sending locally to Ollama.
    let img=image::open(path).map_err(|e|format!("Impossible de lire l'image : {e}"))?;
    let resized=img.thumbnail(1280,1280).to_rgb8();
    let mut cursor=Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(resized).write_to(&mut cursor,image::ImageFormat::Jpeg).map_err(|e|e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(cursor.into_inner()))
}
fn read_text(p:&Path)->Result<String,String>{
    let metadata=std::fs::metadata(p).map_err(|e|e.to_string())?;
    if metadata.len()>TEXT_BYTES {
        return Err("Document texte trop volumineux (limite 2 Mo).".into());
    }
    let raw=std::fs::read(p).map_err(|e|e.to_string())?;
    let text=String::from_utf8(raw).map_err(|_|"Fichier texte non UTF-8 : convertis-le en UTF-8 pour l'analyser.".to_string())?;
    Ok(text)
}
fn read_pdf(p:&Path)->Result<String,String>{
    if std::fs::metadata(p).map_err(|e|e.to_string())?.len()>PDF_BYTES{
        return Err("PDF supérieur à 32 Mo : réduis sa taille.".into());
    }
    let doc=lopdf::Document::load(p).map_err(|e|format!("PDF illisible : {e}"))?;
    let pages=doc.get_pages();
    if pages.len()>120{return Err("PDF de plus de 120 pages : sélectionne un extrait.".into());}
    let numbers:Vec<u32>=pages.keys().copied().collect();
    let mut extracted=String::new();
    for chunk in numbers.chunks(8){
        let text=doc.extract_text(chunk)
            .map_err(|e|format!("PDF texte illisible : {e}"))?;
        extracted.push_str(&text);
        if extracted.len()>32_000{break;}
    }
    if extracted.trim().is_empty(){return Err("Ce PDF semble scanné ou sans texte sélectionnable. Exporte ses pages en images et utilise l'analyse visuelle.".into());}
    Ok(extracted)
}
fn extract_xml_text(xml:&str)->Result<String,String>{
    let doc=roxmltree::Document::parse(xml).map_err(|e|format!("XML invalide : {e}"))?;
    let mut out=String::new();
    for n in doc.descendants().filter(|n|n.is_element() && n.tag_name().name()=="t"){
        if let Some(t)=n.text(){out.push_str(t);out.push(' ');}
        if out.len()>35_000{break;}
    }
    if out.trim().is_empty(){return Err("Aucun texte sélectionnable détecté.".into());}
    Ok(out)
}
fn zip_extract(p:&Path,prefix:&str)->Result<String,String>{
    if std::fs::metadata(p).map_err(|e|e.to_string())?.len()>PDF_BYTES{
        return Err("Archive Office supérieure à 32 Mo.".into());
    }
    let file=File::open(p).map_err(|e|e.to_string())?;
    let mut zip=zip::ZipArchive::new(file).map_err(|e|format!("Document Office invalide : {e}"))?;
    let names:Vec<String> = (0..zip.len()).filter_map(|idx|{
        let f=zip.by_index(idx).ok()?;
        let name=f.name().to_owned();
        if (prefix=="word/" && name=="word/document.xml")
          || (prefix=="ppt/slides/" && name.starts_with(prefix) && name.ends_with(".xml")) {
            Some(name)
        }else{None}
    }).take(40).collect();
    let mut result=String::new();
    for name in names {
        let mut entry=zip.by_name(&name).map_err(|e|e.to_string())?;
        if entry.size()>8_000_000{return Err("XML Office trop volumineux.".into());}
        let mut xml=String::new();entry.read_to_string(&mut xml).map_err(|e|e.to_string())?;
        let extracted=extract_xml_text(&xml).unwrap_or_default();
        if !extracted.is_empty(){result.push_str(&format!("\n[{}]\n{}\n",name,extracted));}
        if result.len()>35_000{break;}
    }
    if result.is_empty(){return Err("Aucun texte détecté dans le document Office.".into());}
    Ok(result)
}
fn spreadsheet(p:&Path)->Result<String,String>{
    use calamine::Reader;
    if std::fs::metadata(p).map_err(|e|e.to_string())?.len()>PDF_BYTES {
        return Err("Tableur supérieur à 32 Mo.".into());
    }
    let mut workbook=calamine::open_workbook_auto(p).map_err(|e|format!("Tableur illisible : {e}"))?;
    let mut result=String::new();
    for name in workbook.sheet_names().iter().take(6).cloned().collect::<Vec<_>>() {
        if let Ok(r)=workbook.worksheet_range(&name){
            result.push_str(&format!("\n[Onglet : {}]\n",name));
            for row in r.rows().take(250){
                let cells=row.iter().take(30).map(|x|x.to_string()).collect::<Vec<_>>();
                result.push_str(&cells.join("\t"));
                result.push('\n');
                if result.len()>35_000{break;}
            }
        }
        if result.len()>35_000{break;}
    }
    if result.trim().is_empty(){return Err("Aucune donnée exploitable dans le tableau.".into());}
    Ok(result)
}
pub fn extract(p:&Path)->Result<String,String>{
    let k=kind(&ext(p));
    let result=match k{
        "text"=>read_text(p)?,
        "pdf"=>read_pdf(p)?,
        "document"=>zip_extract(p,"word/")?,
        "slides"=>zip_extract(p,"ppt/slides/")?,
        "table"=>spreadsheet(p)?,
        "image"=>return Err("Image : utiliser le modèle visuel.".into()),
        _=>return Err("Format conservé comme pièce jointe, mais son contenu ne peut pas être interprété automatiquement.".into())
    };
    Ok(result.chars().take(11500).collect())
}
