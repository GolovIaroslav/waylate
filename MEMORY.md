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
## Model Repair → see src-tauri/src/lib.rs and src/routes/+page.svelte | Catalog installs now use explicit backend model states plus a completion manifest so partial Hugging Face downloads remain retryable instead of looking installed.
## Translate Layout → see src/routes/+page.svelte and src-tauri/src/lib.rs | Translate tab stays a fixed two-pane layout with inline feedback, and the main window now opens larger to avoid hidden results and forced vertical resizing.
## Secrets UX → see src/routes/+page.svelte | API keys autosave after typing, show separate "saved in system" hints, and Yandex Folder ID should behave like a normal immediate setting with its own clear action.
## CT2 Fallback → see src-tauri/src/runtime.rs | If warm CT2 localhost translation still fails after a restart, Waylate should fall back to the direct helper script instead of surfacing the raw port error as the only path.
## Settings Feedback → see src/routes/+page.svelte | Probe results now render in a dedicated card near the action buttons, model-status pills use plain ready/download wording, and empty secret fields must not imply a key is already saved.
## Rail Layout → see src/routes/+page.svelte | The side navigation should look like a compact framed control cluster rather than a full-height stretched column.
## Svelte Reactivity Trap → see src/routes/+page.svelte | Reactive assignments must pass `snapshot` and `config` explicitly into helper functions; otherwise Translate can get stuck showing zero models even while backend state is correct.
## Engine Spec v1 → see src-tauri/src/models.rs and docs/plans/2026-06-21-engine-spec-pr1.md | Target architecture is exactly `OnnxEncoderDecoder` plus `ManagedLlamaCpp`; CT2/Python is legacy compatibility only, not the beginner path.
## Spec Catalog Commands → see src-tauri/src/lib.rs and src/routes/+page.svelte | Settings model manager uses `list_model_profiles`, `install_model`, `cancel_model_install`, and `get_model_status`; next work is ONNX engine wiring, not extending CT2.
## ONNX Runtime Path → see src-tauri/src/engines/onnx_mt.rs | NLLB now routes through Rust-native `ort` + `tokenizers` using Xenova ONNX assets; current implementation is a merged-decoder greedy loop without KV-cache reuse.
## Frontend Model Normalization → see src/routes/+page.svelte | Translate/Settings must normalize language lists and provider hints across legacy `ModelProfile` and spec `ModelCatalogEntry` instead of assuming identical fields.
## ONNX Cache Flow → see src-tauri/src/engines/onnx_mt.rs | Xenova merged decoder requires cache-aware `present.*` → `past_key_values.*` reuse; zero-length dummy cache tensors fail in `ort`.
## Beginner ONNX Default → see src-tauri/src/config.rs and src/routes/+page.svelte | Fresh installs should select `nllb-200-distilled-600m-onnx`; spec ONNX readiness must not depend on legacy CT2 config paths.
## Spec Verification → see docs/plans/2026-06-21-engine-spec-verification.md | NLLB ONNX is smoke-tested, but install metadata, hashes, disk checks, delete/reinstall, Hy-MT2 verification and PR6 hardening remain open.
## Launch Regression → see src-tauri/tauri.conf.json and src-tauri/src/lib.rs | Installed release may show `Could not connect to localhost: Connection refused`; next session should diagnose Tauri frontend loading before more engine work.
