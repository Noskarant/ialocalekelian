# Atelier IA — Kélian

Application Windows locale, pensée pour le HP Victus i7-13700F / RTX 4060 8 Go VRAM / 16 Go RAM.

## Modules
- **Discussion :** Ollama + Qwen3 8B 4-bit, chat progressif et historique local.
- **Images :** ComfyUI + SDXL, génération d'images et retouche img2img.
- **Amélioration :** Real-ESRGAN ×4.
- **Vidéo :** faster-whisper + FFmpeg, coupes guidées par la parole, sous-titres incrustés, vidéo verticale 1080 × 1920, accélération NVENC ou secours CPU.

## Installation pour Kélian

1. Télécharger le fichier Windows setup.exe dans [GitHub Releases](../../releases) **si la compilation est réussie**.
2. Installer et ouvrir Atelier IA.
3. Dans Paramètres, installer Ollama et télécharger Qwen3.
4. Pour le studio image, installer ComfyUI, PyTorch CUDA, SDXL et Real-ESRGAN via le bouton prévu. Ensuite cliquer Démarrer ComfyUI.
5. Pour la vidéo, installer FFmpeg et faster-whisper. Le modèle Whisper small se télécharge au premier montage.
6. Utiliser les onglets Discussion, Images et Vidéo.

**Espace disque :** prévoir au moins 25 Go d'espace disponible pour les différents moteurs et modèles. Leur téléchargement initial requiert Internet ; aucune inférence ne transite par un service IA externe.

## Confidentialité et limites

- Pas de compte ou API payante requise.
- Historique du chat stocké dans le WebView local ; images et MP4 dans Documents/Atelier IA.
- Téléchargements depuis GitHub, Hugging Face, Python Package Index et WinGet ; aucun modèle IA stocké sur GitHub.
- La retouche est une transformation img2img de la photo (sans pinceau de masquage).
- Le module vidéo est un montage automatisé fondé sur la parole ; ce n'est pas un éditeur multipiste.
- **Pas encore de validation GPU sur la machine cible.** Les performances doivent être mesurées sur le HP Victus réel.
- **Le .exe n'est pas signé Authenticode.** Windows SmartScreen peut afficher un avertissement.

## Architecture et optimisation

Tauri 2 (Rust + WebView2), UI Vite/JS/CSS, Ollama et ComfyUI en localhost uniquement. Exécution exclusive des opérations lourdes ; libération de mémoire entre le chat et les traitements image et vidéo. Qwen3 conserve un contexte de 4096 tokens, ComfyUI est démarré en lowvram.

## Compilation Windows

Avec Node 22, Rust et les prérequis Tauri 2 :

1. npm install
2. npx tauri icon assets/icon.svg -o src-tauri/icons
3. npm run tauri build

Le fichier NSIS est produit dans src-tauri/target/release/bundle/nsis/.

## GitHub et quotas

Le dépôt est public. Les runners standard GitHub Actions sont gratuits pour les dépôts publics selon [GitHub](https://docs.github.com/fr/actions/concepts/billing-and-usage). La compilation n'est déclenchée qu'en modifiant le workflow Windows ou à la demande. Les artifacts Actions sont conservés un seul jour ; l'installateur réussi est également publié en GitHub Release. Aucun poids IA ni fichier vidéo n'est suivi par Git.

## Dépannage

- Si Ollama ne démarre pas après WinGet : relancer l'application ou Windows.
- Si ComfyUI reste hors ligne : lancer le moteur depuis Paramètres et actualiser.
- Si une génération sature la VRAM : Libérer la VRAM, fermer les programmes GPU gourmands.
- Si Whisper CUDA ne fonctionne pas, son traitement bascule sur CPU.
- Si FFmpeg manque : relancer l'installation vidéo, puis Windows pour recharger PATH.
- Une erreur de téléchargement SDXL peut venir de Hugging Face ou de la connexion.

Les modèles IA possèdent leurs propres licences (notamment SDXL OpenRAIL++), à examiner avant toute diffusion commerciale.
