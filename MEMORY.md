## Project → see README.md | Waylate is a Tauri 2 + Svelte popup translator for Arch/KDE Wayland.
## Launch → see README.md | Installed users run `waylate`; KDE shortcut should call `waylate translate-selection`.
## Clipboard → see src-tauri/src/clipboard.rs | Read only text MIME types from Wayland selections; binary clipboard data must be rejected.
## UI → see src/routes/+page.svelte | Main translator popup should stay compact, with minimal top chrome and no page-level scrolling.
## Release → see .github/workflows/release.yml | Tags `v*` build a precompiled Linux archive for GitHub Releases.
## Open Source → see .github/ and CONTRIBUTING.md | CI, release, issue templates, PR template, Dependabot, contributing and security files are part of repo hygiene.
## Models Roadmap → see implementation-notes.md | Real one-click local model install needs a curated compatible model list plus backend-specific download/run logic.
## Local Model Setup → see src-tauri/src/lib.rs and src/routes/+page.svelte | NLLB download uses HF HTTP, prepares a local Python runtime for CT2/transformers/sentencepiece, fills model/tokenizer paths, and defaults to CPU for reliability.
## API Providers → see src-tauri/src/translation.rs | DeepL, Google and Yandex are network profiles; Yandex requires a Secret Service API key plus config `yandexFolderId`.
## Implementation Notes → see implementation-notes.md | For every spec/feature pass, keep running notes with decisions, tradeoffs, deviations from spec, and follow-up risks.
## User Workflow → see AGENTS.md | After building app changes, install/restart `waylate` for the user when feasible so they test the fresh binary, then commit and push when requested.
## Product Direction → see src/routes/+page.svelte | Beginner path must not require a local server; keep OpenAI-compatible/GGUF settings advanced and make local NLLB download the default onboarding path.
## Model Downloads → see src-tauri/src/lib.rs and src-tauri/src/models.rs | Built-in HF downloads use Rust HTTP progress/cancel; normal choices are NLLB CT2 variants, while Tencent GGUF stays advanced until a runner is bundled.
## Settings UI → see src/routes/+page.svelte | UI language updates labels immediately, theme is saved in config, and `?` help opens on hover/click with a visible outline.
## Commit Cadence → see AGENTS.md | User wants small logical commits after each finished feature/fix phase, with push after verified batches rather than large infrequent commits.
## Runtime Direction → see implementation-notes.md | Current local path still shells into a Python/CT2 helper per translation; next architecture step is managed in-app runtimes with explicit warm/unload policies.
## Installed Model State → see src-tauri/src/lib.rs and src/routes/+page.svelte | Translate view should trust backend-reported `installed_model_ids`, not raw config fields, so downloaded models stay marked ready and unavailable models stay hidden.
## Catalog Scope → see src-tauri/src/models.rs | Settings should show five curated local model families; only entries with a working downloader/runtime get an active Download button, while unsupported families stay visible as coming soon.
## Settings UX → see src/routes/+page.svelte | Settings apply automatically; there should be no explicit save button, model cards should stay near the top of Settings, and internal runtime identifiers belong only in collapsed diagnostics.
## Warm Runtime Recovery → see src-tauri/src/runtime.rs | If the warm CT2 process stops responding on its localhost port, Waylate should drop it and restart once automatically before surfacing an error.
