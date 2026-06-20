# Waylate

Waylate is a small Wayland-first popup translator for Arch Linux and KDE Plasma.
The main flow is simple: select text anywhere, press your KDE shortcut, and a
translation popup opens. Local translation models are the default idea; network
API providers exist, but they stay disabled until you explicitly enable them.

## Install on Arch

Download the prebuilt archive from GitHub Releases, unpack it, then run:

```bash
./install.sh
waylate
```

From a checkout:

```bash
makepkg -si
```

For development:

```bash
npm install
npm run tauri dev
```

Useful runtime packages:

```bash
sudo pacman -S wl-clipboard webkit2gtk-4.1 libayatana-appindicator
```

Optional model tooling:

```bash
pipx install "huggingface_hub[cli]"
pipx inject huggingface-hub ctranslate2 transformers sentencepiece
```

## KDE Plasma Wayland Shortcut

Waylate does not pretend that global shortcuts on Wayland are solved everywhere.
For v1, the reliable KDE path is a command shortcut:

1. Open System Settings.
2. Go to Keyboard > Shortcuts > Custom Shortcuts.
3. Add a command shortcut.
4. Use this command:

```bash
waylate translate-selection
```

When the selected text is not readable from the Wayland primary selection,
Waylate falls back to the clipboard and still lets you paste text manually in
the popup.

Other useful commands:

```bash
waylate
waylate translate-clipboard
waylate settings
```

## Models

Waylate ships with a small model catalog instead of pretending that every
Hugging Face repository can run everywhere.

- `Tencent Hy-MT GGUF`: use a local OpenAI-compatible server, for example
  `llama.cpp` server on `http://127.0.0.1:8080/v1/chat/completions`.
- `Meta NLLB-200 CTranslate2`: use Settings > Local model > Download and
  configure NLLB. Waylate downloads the catalog model from Hugging Face and
  saves the CTranslate2 model/tokenizer paths automatically.
- `Custom local model`: use your own endpoint or local setup.

For CTranslate2/NLLB, Waylate calls:

```bash
waylate-ct2-translate --model /path/to/ct2-model --tokenizer facebook/nllb-200-distilled-600M --source eng_Latn --target rus_Cyrl "Text"
```

The selected model controls the visible language list. NLLB profiles show NLLB
language codes; API and GGUF profiles show a smaller practical list.

## API Providers

DeepL, Google Cloud Translate and Yandex Cloud Translate profiles are included
for testing and fallback workflows. They are disabled by default. Enable network
providers in Settings before selecting them. API keys are stored through the
system Secret Service keyring; Waylate does not write them into `config.json`.

Waylate does not ship shared API keys. Each user must add their own provider key.
Yandex also needs a Cloud Folder ID.

## Local Files

Waylate follows XDG directories:

- Config: `~/.config/Waylate/config.json`
- Data and models: `~/.local/share/Waylate`
- History: `~/.local/share/Waylate/history.sqlite3`

History is off by default. When enabled, it is stored locally in SQLite.

## Development

```bash
npm run check
cd src-tauri && cargo check
npm run tauri build
```

Project shape:

- Tauri 2 handles tray, CLI commands, Wayland clipboard calls, config and model backends.
- Svelte renders the popup/settings/history UI.
- `wl-paste --primary` is used for Wayland primary selection, with clipboard fallback.
- Global shortcut portals are intentionally not the v1 default because KDE command shortcuts are more predictable today.

Good GitHub topics for the repository: `wayland`, `kde`, `arch-linux`,
`tauri`, `svelte`, `translation`, `llm`, `nllb`, `ctranslate2`.

## Community

Issues and pull requests are open on GitHub. Use bug reports for broken Wayland,
clipboard, tray, or model behavior; use feature requests for new model providers
and UI improvements.
