param([Parameter(Mandatory=$true)][ValidateSet('chat','images','video')][string]$Component)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$root = Join-Path $env:LOCALAPPDATA 'AtelierIA'
New-Item -ItemType Directory -Force -Path $root | Out-Null
function Run([string]$command,[string[]]$arguments) {
    & $command @arguments
    if ($LASTEXITCODE -ne 0) { throw "$command s'est terminé avec le code $LASTEXITCODE" }
}
function Ensure-WinGet {
    if (-not (Get-Command winget.exe -ErrorAction SilentlyContinue)) {
        throw 'WinGet manquant. Installe App Installer depuis Microsoft Store puis relance cette étape.'
    }
}
function Ensure-Python {
    $paths = @(
        (Join-Path $env:LOCALAPPDATA 'Programs\Python\Python311\python.exe'),
        (Join-Path $env:ProgramFiles 'Python311\python.exe')
    )
    foreach ($p in $paths) { if (Test-Path $p) { return $p } }
    $known = Get-Command py.exe -ErrorAction SilentlyContinue
    if ($known) {
        $candidate = (& py.exe -3.11 -c 'import sys;print(sys.executable)' 2>$null)
        if ($LASTEXITCODE -eq 0 -and $candidate -and (Test-Path $candidate)) { return $candidate.Trim() }
    }
    Ensure-WinGet
    Write-Output 'Installation de Python 3.11…'
    Run 'winget.exe' @('install','--id','Python.Python.3.11','--exact','--silent',
        '--accept-package-agreements','--accept-source-agreements','--disable-interactivity')
    foreach ($p in $paths) { if (Test-Path $p) { return $p } }
    throw 'Python 3.11 installé mais non détecté. Redémarre Windows puis relance.'
}
function Ensure-Venv([string]$name) {
    $python = Ensure-Python
    $folder = Join-Path $root $name
    $venv = Join-Path $folder 'Scripts\python.exe'
    if (-not (Test-Path $venv)) { Run $python @('-m','venv',$folder) }
    Run $venv @('-m','pip','install','--disable-pip-version-check','--upgrade','pip')
    return $venv
}
function Download-File([string]$url,[string]$target,[long]$minimum=0) {
    if ((Test-Path $target) -and ((Get-Item $target).Length -ge $minimum)) {
        Write-Output "Déjà présent : $target"
        return
    }
    $partial = "$target.partial"
    if (Test-Path $partial) { Remove-Item -Force $partial }
    Write-Output "Téléchargement : $url"
    try {
        Invoke-WebRequest -Uri $url -OutFile $partial -MaximumRedirection 10 -UseBasicParsing
        if ((Get-Item $partial).Length -lt $minimum) { throw 'Téléchargement incomplet.' }
        Move-Item -Force $partial $target
    } catch {
        if (Test-Path $partial) { Remove-Item -Force $partial }
        throw
    }
}
if ($Component -eq 'chat') {
    Ensure-WinGet
    $ollama = Join-Path $env:LOCALAPPDATA 'Programs\Ollama\ollama.exe'
    if (-not (Test-Path $ollama) -and -not (Get-Command ollama.exe -ErrorAction SilentlyContinue)) {
        Write-Output 'Installation officielle Ollama…'
        Run 'winget.exe' @('install','--id','Ollama.Ollama','--exact','--silent',
            '--accept-package-agreements','--accept-source-agreements','--disable-interactivity')
    }
    if (Test-Path $ollama) {
        Start-Process -FilePath $ollama -ArgumentList 'serve' -WindowStyle Hidden -ErrorAction SilentlyContinue
    }
    Write-Output 'Ollama installé. Dans Paramètres, télécharge Qwen3 8B pour activer le chat.'
    exit 0
}
if ($Component -eq 'images') {
    Write-Output 'Préparation du moteur image CUDA pour RTX 4060…'
    $comfy = Join-Path $root 'ComfyUI'
    if (-not (Test-Path (Join-Path $comfy 'main.py'))) {
        $archive = Join-Path $root 'ComfyUI-source.zip'
        Download-File 'https://github.com/Comfy-Org/ComfyUI/archive/refs/heads/master.zip' $archive 100000
        Expand-Archive -Force -Path $archive -DestinationPath $root
        $extracted = Join-Path $root 'ComfyUI-master'
        if (-not (Test-Path (Join-Path $extracted 'main.py'))) { throw 'Archive ComfyUI incorrecte.' }
        Move-Item -Force $extracted $comfy
        Remove-Item $archive -Force
    }
    $py = Ensure-Venv 'image-venv'
    Write-Output 'Installation PyTorch CUDA 12.8…'
    Run $py @('-m','pip','install','--disable-pip-version-check','torch','torchvision','--index-url',
        'https://download.pytorch.org/whl/cu128')
    Write-Output 'Installation ComfyUI…'
    Run $py @('-m','pip','install','--disable-pip-version-check','-r',(Join-Path $comfy 'requirements.txt'))
    $checkpoints = Join-Path $comfy 'models\checkpoints'
    $upscale = Join-Path $comfy 'models\upscale_models'
    New-Item -Force -ItemType Directory -Path $checkpoints,$upscale | Out-Null
    $ckpt = Join-Path $checkpoints 'sd_xl_base_1.0.safetensors'
    Download-File 'https://huggingface.co/stabilityai/stable-diffusion-xl-base-1.0/resolve/main/sd_xl_base_1.0.safetensors?download=true' $ckpt 6900000000
    $expected = '31e35c80fc4829d14f90153f4c74cd59c90b779f6afe05a74cd6120b893f7e5b'
    if ((Get-FileHash $ckpt -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) {
        Remove-Item $ckpt -Force
        throw 'Empreinte SHA256 SDXL non conforme. Le modèle a été supprimé.'
    }
    Download-File 'https://github.com/xinntao/Real-ESRGAN/releases/download/v0.1.0/RealESRGAN_x4plus.pth' (Join-Path $upscale 'RealESRGAN_x4plus.pth') 50000000
    Write-Output 'Studio image installé. Clique sur Démarrer ComfyUI dans les paramètres.'
    exit 0
}
if ($Component -eq 'video') {
    Ensure-WinGet
    Write-Output 'Installation FFmpeg…'
    Run 'winget.exe' @('install','--id','Gyan.FFmpeg','--exact','--silent',
        '--accept-package-agreements','--accept-source-agreements','--disable-interactivity')
    $py = Ensure-Venv 'video-venv'
    Write-Output 'Installation faster-whisper…'
    Run $py @('-m','pip','install','--disable-pip-version-check','faster-whisper')
    Write-Output 'Studio vidéo installé. Le modèle Whisper small sera téléchargé au premier montage.'
    exit 0
}
