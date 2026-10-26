# Waylate

Waylate is a popup translator for Arch Linux and KDE Plasma. Select text
anywhere, press your KDE shortcut, and a translation popup opens.

Use it right away with your own DeepL, Google Cloud Translate or Yandex Cloud
Translate API key — no downloads, no GPU, nothing to configure beyond pasting
a key. Prefer to keep translation fully offline? Waylate can also run a local
model (NLLB-200, or a GGUF model through llama.cpp) instead — no API key,
nothing leaves your machine.

## Install on Arch

Download the AppImage from [GitHub Releases](https://github.com/GolovIaroslav/waylate/releases), then run:

```bash
chmod +x Waylate-*-x86_64.AppImage
APPIMAGE_EXTRACT_AND_RUN=1 ./Waylate-*-x86_64.AppImage
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

Waylate does not pretend global shortcuts work everywhere on Wayland.
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

## API Providers

The fastest way to get started: DeepL, Google Cloud Translate and Yandex Cloud
Translate are built in. Open Settings, turn on network providers, paste your
own key, and pick that provider as the default model — no download, no GPU.
Waylate stores API keys through the system Secret Service keyring and does not
write them into `config.json`.

Waylate does not ship shared API keys. Each user must add their own provider key.
Yandex also needs a Cloud Folder ID.

## Word Dictionary and Vocabulary

The optional **Word dictionary** feature is **off by default**. Enable it in
Settings > Privacy and APIs > **Enable word dictionary (online lookups)**.
When you translate a **single word**, Waylate preserves the normal translation
and additionally shows definitions by sense, example sentences, synonyms,
available translations and pronunciation text. A counter tells you whether you
have looked up that word previously; click **Add to vocabulary** to save it.
The **My vocabulary** icon in the sidebar appears only while this feature is
enabled. You can review and remove saved words or export them to JSON, CSV or
TSV in your Downloads directory. The UI is localized in English, Russian and
Slovak. Vocabulary and lookup counts are stored locally in SQLite independently
of the normal translation history, which can remain disabled.

The dictionary uses [FreeDictionaryAPI.com](https://freedictionaryapi.com/),
with content from [Wiktionary](https://www.wiktionary.org/) (CC BY-SA). No API
key, account or paid subscription is required. The dictionary lookup sends
**only the requested single word** to that service; a separate network
translation provider, if selected, has its own data handling and API key.
Dictionary availability and coverage depend on Wiktionary and the public
service's rate limits. Unsupported words and network errors leave the normal
translation usable. Offline translation remains possible with the dictionary
turned off; offline dictionary definitions are not currently supported.

## Local Models

Prefer nothing to leave your machine, or want to translate with no network at
all? Waylate can also run a local model instead of an API key.

- `Meta NLLB-200 CTranslate2`: use Settings > Local model > Download and
  configure NLLB. Waylate downloads the catalog model from Hugging Face and
  saves the CTranslate2 model/tokenizer paths automatically.
- `Tencent Hy-MT GGUF`: use a local OpenAI-compatible server, for example
  `llama.cpp` server on `http://127.0.0.1:8080/v1/chat/completions`.
- `Custom local model`: use your own endpoint or local setup.

For CTranslate2/NLLB, Waylate calls:

```bash
waylate-ct2-translate --model /path/to/ct2-model --tokenizer facebook/nllb-200-distilled-600M --source eng_Latn --target rus_Cyrl "Text"
```

The selected model controls the visible language list. NLLB profiles show NLLB
language codes; API and GGUF profiles show a smaller practical list.

## Local Files

Waylate follows XDG directories:

- Config: `~/.config/waylate/config.json`
- Data and models: `~/.local/share/waylate`
- History: `~/.local/share/waylate/history.sqlite3`
- Vocabulary and previous word lookups: `~/.local/share/waylate/vocabulary.sqlite3`

History and dictionary are both off by default; they can be enabled separately.

## Development

```bash
npm run check
cd src-tauri && cargo check
npm run tauri build
npm run install:local-release
```

Use `npm run install:local-release` when you want to refresh the runnable
`waylate` command in `~/.local/bin`. It rebuilds the bundled frontend through
the Tauri CLI, installs the release binary, and relaunches it. Plain
`cargo build --release` is not the reliable workflow for updating the user-run
desktop binary.

Project shape:

- Tauri 2 handles tray, CLI commands, Wayland clipboard calls, config and model backends.
- Svelte renders the popup/settings/history UI.
- `wl-paste --primary` reads the Wayland primary selection, with clipboard fallback.
- Global shortcut portals are not the v1 default because KDE command shortcuts are more predictable today.

Good GitHub topics for the repository: `wayland`, `kde`, `arch-linux`,
`tauri`, `svelte`, `translation`, `llm`, `nllb`, `ctranslate2`.

## Community

Issues and pull requests are open on GitHub. Use bug reports for broken Wayland,
clipboard, tray, or model behavior; use feature requests for new model providers
and UI improvements.
