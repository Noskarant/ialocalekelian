#!/usr/bin/env python3
"""Local-only transcription + editable short-form reel export. No cloud API."""
from __future__ import annotations
import argparse
import gc
import glob
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

def locate(binary: str) -> str:
    current = shutil.which(binary)
    if current:
        return current
    base = Path(os.environ.get("LOCALAPPDATA", ""))
    patterns = [
        str(base / "Microsoft" / "WinGet" / "Packages" / "*" / "**" / (binary + ".exe")),
        str(base / "Programs" / "ffmpeg" / "bin" / (binary + ".exe")),
        str(Path("C:/ffmpeg/bin") / (binary + ".exe")),
    ]
    for pattern in patterns:
        paths = glob.glob(pattern, recursive=True)
        if paths:
            return paths[0]
    raise RuntimeError(f"{binary} introuvable. Réinstalle FFmpeg depuis les paramètres.")

def run(args: list[str], check=True):
    proc = subprocess.run(args, capture_output=True, text=True, encoding="utf-8", errors="replace")
    if check and proc.returncode != 0:
        raise RuntimeError((proc.stderr or proc.stdout)[-2500:])
    return proc

def srt_time(seconds: float) -> str:
    milliseconds = round(max(0.0, seconds) * 1000)
    h, m = divmod(milliseconds, 3600000)
    minute, second = divmod(m, 60000)
    return f"{h:02}:{minute:02}:{second//1000:02},{second%1000:03}"

def build_cuts(segments, duration: float, remove_silence: bool):
    if not remove_silence or not segments:
        return [(0.0, duration)]
    ranges = [(max(0, float(s.start)-0.32), min(duration,float(s.end)+0.38))
              for s in segments if float(s.end)>float(s.start)]
    if not ranges:
        return [(0.0, duration)]
    result = [ranges[0]]
    for start,end in ranges[1:]:
        prev_start,prev_end=result[-1]
        if start-prev_end <= 1.25:
            result[-1]=(prev_start,max(prev_end,end))
        else:
            result.append((start,end))
    if len(result)>30:
        return [(0.0,duration)]  # avoid very expensive fan-out encoding
    return result

def to_output_time(timestamp: float, cuts):
    elapsed=0.0
    for start,end in cuts:
        if timestamp<=start:
            return elapsed
        if timestamp < end:
            return elapsed+timestamp-start
        elapsed+=end-start
    return elapsed

def write_srt(segments,cuts,target:Path):
    count=0
    with target.open("w",encoding="utf-8-sig",newline="\n") as stream:
        for segment in segments:
            start=to_output_time(float(segment.start),cuts)
            end=to_output_time(float(segment.end),cuts)
            text=" ".join(segment.text.split())
            if end-start<0.08 or not text:
                continue
            count+=1
            stream.write(f"{count}\n{srt_time(start)} --> {srt_time(end)}\n{text}\n\n")
    return count

def filter_script(cuts,subtitles:Path,has_audio:bool) -> str:
    filters=[]
    # Each segment is trimmed independently to avoid shifting/losing audio sync.
    for i,(start,end) in enumerate(cuts):
        filters.append(f"[0:v:0]trim=start={start:.3f}:end={end:.3f},setpts=PTS-STARTPTS[v{i}]")
        if has_audio:
            filters.append(f"[0:a:0]atrim=start={start:.3f}:end={end:.3f},asetpts=PTS-STARTPTS[a{i}]")
    if has_audio:
        joined="".join(f"[v{i}][a{i}]" for i in range(len(cuts)))
        filters.append(f"{joined}concat=n={len(cuts)}:v=1:a=1[vcut][acut]")
    else:
        joined="".join(f"[v{i}]" for i in range(len(cuts)))
        filters.append(f"{joined}concat=n={len(cuts)}:v=1:a=0[vcut]")
    path=str(subtitles.resolve()).replace("\\","/").replace(":","\\:").replace("'","\\'")
    subtitle_filter=(f",subtitles=filename='{path}':force_style="
                     "'FontName=Arial,FontSize=16,PrimaryColour=&H00FFFFFF&,"
                     "OutlineColour=&H00000000&,BorderStyle=1,Outline=2,Shadow=1,Alignment=2,MarginV=115'")
    filters.append("[vcut]scale=1080:1920:force_original_aspect_ratio=increase,"
                   "crop=1080:1920,setsar=1" + subtitle_filter + "[vout]")
    return ";".join(filters)

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--input",required=True)
    parser.add_argument("--output",required=True)
    parser.add_argument("--cut-silence",choices=["yes","no"],default="yes")
    args=parser.parse_args()
    source=Path(args.input)
    target=Path(args.output)
    if not source.exists():
        raise FileNotFoundError(source)
    ffmpeg=locate("ffmpeg")
    ffprobe=locate("ffprobe")
    probe=run([ffprobe,"-v","error","-show_streams","-show_format","-of","json",str(source)])
    info=json.loads(probe.stdout)
    duration=float(info.get("format",{}).get("duration","0"))
    if duration<=0 or duration>3600:
        raise RuntimeError("La vidéo doit durer entre une seconde et une heure.")
    has_audio=any(s.get("codec_type")=="audio" for s in info["streams"])
    if not has_audio:
        raise RuntimeError("Cette version de montage nécessite une piste audio.")
    print("Transcription locale…",flush=True)
    from faster_whisper import WhisperModel
    try:
        model=WhisperModel("small",device="cuda",compute_type="int8_float16",
                           download_root=str(Path(os.environ.get("LOCALAPPDATA",""))/"AtelierIA"/"whisper"))
        segments,_=model.transcribe(str(source),beam_size=3,vad_filter=True)
        segments=list(segments)
    except Exception as exc:
        print(f"GPU Whisper indisponible ({exc}); bascule CPU.",file=sys.stderr,flush=True)
        model=WhisperModel("small",device="cpu",compute_type="int8",
                           download_root=str(Path(os.environ.get("LOCALAPPDATA",""))/"AtelierIA"/"whisper"))
        segments,_=model.transcribe(str(source),beam_size=3,vad_filter=True)
        segments=list(segments)
    del model
    gc.collect()
    cuts=build_cuts(segments,duration,args.cut_silence=="yes")
    srt=target.with_suffix(".srt")
    count=write_srt(segments,cuts,srt)
    target.parent.mkdir(parents=True,exist_ok=True)
    print(f"{count} sous-titres, {len(cuts)} séquences.",flush=True)
    graph=filter_script(cuts,srt,has_audio)
    common=[ffmpeg,"-hide_banner","-y","-i",str(source),"-filter_complex",graph,
            "-map","[vout]","-map","[acut]","-ar","48000","-c:a","aac","-b:a","160k",
            "-movflags","+faststart","-shortest"]
    # NVENC is faster on RTX 4060; CPU fallback when unavailable or GPU memory exhausted.
    nvenc=run(common+["-c:v","h264_nvenc","-preset","p5","-cq","23","-b:v","0",
                      "-pix_fmt","yuv420p",str(target)],check=False)
    if nvenc.returncode!=0:
        if target.exists():
            target.unlink()
        cpu=run(common+["-c:v","libx264","-preset","veryfast","-crf","23",
                         "-pix_fmt","yuv420p",str(target)],check=False)
        if cpu.returncode!=0:
            raise RuntimeError("Export FFmpeg :\n"+cpu.stderr[-3000:]+"\nGPU : "+nvenc.stderr[-800:])
    if not target.exists() or target.stat().st_size<10000:
        raise RuntimeError("L'export vidéo semble incomplet.")
    print(str(target),flush=True)

if __name__=="__main__":
    try: main()
    except Exception as error:
        print(f"ERREUR : {error}",file=sys.stderr)
        sys.exit(1)
