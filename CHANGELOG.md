# Changelog

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
