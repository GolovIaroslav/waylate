# Changelog

## 0.1.6

- Replace the 16-language DeepL/Google/Yandex shortlist with DeepL's real ~30-language catalog and a much broader Google/Yandex list covering all major world regions, not just Europe/EN/RU.
- Fix DeepL translation to English or Portuguese, which required a regional target variant (EN-GB/EN-US, PT-PT/PT-BR) that the app never sent.
- Surface the detected source language in the UI when translating with `auto` source on a network provider.
- Fix autostart on login: use the running binary's absolute path in the `.desktop` entry instead of a bare `waylate` command, which failed under a minimal login PATH.

## 0.1.5

- Reposition Waylate as an API-key translator first (DeepL, Google, Yandex), with local models as an offline/privacy option, in the README and package metadata.
- Fix the popup window not raising above other windows when triggered from a background instance (e.g. a mouse macro) on KDE/Wayland; run under XWayland and activate via `xdotool` so KWin's focus-stealing prevention no longer blocks it.
- Add Vulkan GPU acceleration for AMD/Intel via a bundled llama-server Vulkan binary.
- Support fp16 KV-cache for the ONNX NLLB engine, shorten the model-cache lock, and initialize ONNX Runtime once instead of per translation.
- Fix local translation: block auto-detect for engines that need an explicit source language, use the correct TranslateGemma chat template, respect the configured context size, and fix the local llama-server URL constant.
- Harden config saves, secrets and history against corruption and bad input; add test coverage.
- Handle clipboard copy errors instead of failing silently, and fix a model reinstall race.
- Add a default-model picker to Settings, and a Save button for the Yandex Folder ID field.
- Fix the toolbar zoom controls overlapping the side rail, and a duplicate CSS rule that broke the API key rows' layout.

## 0.1.4

- Enable TranslateGemma: force the built-in gemma chat template so llama-server no longer crashes on the model's strict embedded template.
- Make online providers (DeepL, Google, Yandex) selectable as translation sources once their API key is saved and network access is on; group the model picker into local and online sections.
- Save API keys with an explicit Save button instead of auto-saving while typing; saving a network key now also turns network providers on. Keys are stored only in the system keychain.
- Add a button to unload models from memory, and collapse the translation footer (model status and acceleration banner) behind a toggle.
- Show recently used languages at the top of the language pickers.
- Auto-detect the source language for online providers (DeepL/Google) so you do not have to set it each time.
- Fix the translate button getting stuck disabled after a download or until switching tabs.
- GPU acceleration no longer closes the window without warning: it downloads, then asks you to restart, and shows an honest banner if the GPU runtime fails to load.

## 0.1.3

- Eliminate O(N²) KV-cache copies in the NLLB ONNX decoder: self-attention tensors are now moved directly between decoder steps instead of being extracted to Vec<f32> and re-uploaded on every token, cutting data-movement from ~10 GB to a constant per translation.
- Switch releases from tar.gz to AppImage — download once, mark executable, run.

## 0.1.2

- Add GitHub CI, Dependabot, issue templates, pull request template, contributing guide and security notes.
- Make the Waylate `W` button return to the translate view.
- Keep the closest compatible language when switching model profiles.

## 0.1.1

- Ignore non-text Wayland clipboard contents instead of pasting image bytes.
- Compact the translator popup: side rail navigation, smaller icons, icon-only clipboard actions.
- Prevent page-level scrolling in the main translator window.

## 0.1.0

- Added a Wayland-first popup translator workflow for KDE command shortcuts.
- Added local model profiles for OpenAI-compatible GGUF servers and NLLB/CTranslate2.
- Added disabled-by-default DeepL and Google API profiles.
- Added local config, optional SQLite history, tray menu and Arch packaging files.
