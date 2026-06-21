# Waylate — Local Translation Engine Spec (v1)

Status: agreed architecture, ready for implementation.
Audience: AI coding agent (Codex or similar) working in the `GolovIaroslav/waylate` repo.
Repo stack: Tauri 2 + Svelte frontend, Rust backend in `src-tauri/src`.

---

## 0. Goal

A non-technical Arch/KDE Wayland user opens Waylate, picks a model from a short
curated list, clicks Download, and translation works. No terminal, no Python,
no manual server start, no "install dependency X yourself" message anywhere
in the default flow.

Two execution engines cover every supported model. No third engine. No
"any Hugging Face repo" generic loader.

---

## 1. Root cause of current state (for context, do not re-fix what's already fixed)

Public repo as of 2026-06-20:

- `translation.rs::translate_ctranslate2` shells into an external Python
  helper (`scripts/waylate-ct2-translate`) that requires globally installed
  `ctranslate2`, `transformers`, `sentencepiece`. Nothing in the app installs
  these automatically.
- `lib.rs::download_catalog_model` shells into `huggingface-cli`, which is
  not installed by default and not bundled.
- `models.rs` profile `hy-mt-gguf` has `downloadable: false` and assumes the
  user manually runs their own OpenAI-compatible local server (e.g.
  llama.cpp) at `127.0.0.1:8080`.
- `Cargo.toml` has no ML/inference crate at all (no `ort`, no `tokenizers`,
  no llama.cpp bindings).

Conclusion: there is currently no in-process local inference path. Both
"local" paths are thin wrappers around external, manually-configured
processes. This spec replaces both with two managed, zero-config engines.

---

## 2. Architecture decision

Exactly two execution engines. Every catalog model maps to one of them.

```rust
enum EngineKind {
    OnnxEncoderDecoder, // classic MT models: NLLB, OPUS-MT/Marian
    ManagedLlamaCpp,    // LLM-style translation models: Hy-MT2, TranslateGemma, MiLMMT-46
    OpenAiCompatible,   // advanced: user's own external server, any model
    NetworkApi,         // DeepL / Google / Yandex, unchanged from current code
}
```

No Python anywhere in the default or high-quality paths. `OnnxEncoderDecoder`
uses ONNX Runtime in-process via the `ort` crate. `ManagedLlamaCpp` uses a
compiled `llama-server` binary that Waylate downloads/bundles and manages as
a child process — also not Python.

### 2.1 OnnxEncoderDecoder engine

Crates: `ort` (ONNX Runtime bindings, `download-binaries` or
`load-dynamic` feature so the native library is fetched/linked at build
time, nothing for the end user to install) + `tokenizers` (HF tokenizers
crate, reads `tokenizer.json`) or `sentencepiece` crate if a model only
ships a raw `.model` file.

New module `src-tauri/src/engines/onnx_mt.rs`:

- Lazily loads encoder/decoder/(lm_head if split) ONNX files once per
  session, held behind `OnceCell`/`Mutex` in `AppState`.
- `translate(profile_id, text, src_lang, tgt_lang) -> Result<String, String>`:
  tokenize → run encoder once → autoregressive decode loop with KV cache
  (feed previous token + cache into decoder, get logits via lm_head, argmax
  or small beam search, repeat until EOS) → detokenize.
- Reference for the decode loop algorithm: niedev/RTranslator issues #84 and
  #90 describe exactly which ONNX files are needed and the call order for an
  NLLB ONNX export (encoder, decoder_with_past, embed_and_lm_head,
  cache_initializer). Port the logic, not the Kotlin code.
- Language codes for NLLB must use FLORES-200 codes (`eng_Latn`,
  `rus_Cyrl`, `slk_Latn`, etc.), not ISO `en`/`ru`. Maintain a single
  `LanguageCodeSet` table shared across engines (see §5) so the UI doesn't
  leak per-engine code formats.

### 2.2 ManagedLlamaCpp engine

New module `src-tauri/src/engines/managed_llama.rs`.

Packaging: ship a pinned, known-good `llama-server` CPU build in the release
archive (preferred — zero setup, you control the exact build) or download it
on first install of any GGUF-based model from a pinned Waylate-controlled
manifest (fallback if you don't want to ship the binary in every release).
Do not scrape arbitrary GitHub release asset names at runtime.

```rust
struct ManagedProcess {
    child: std::process::Child,
    endpoint: String,   // http://127.0.0.1:<picked_port>
    profile_id: String,
    started_at: std::time::Instant,
}

struct RuntimeManager {
    llama_processes: std::sync::Mutex<std::collections::HashMap<String, ManagedProcess>>,
}
```

Lifecycle on translate request:

1. If a healthy managed process exists for the requested profile, reuse it.
2. Otherwise: pick a free localhost port, spawn
   `llama-server --model <path> --host 127.0.0.1 --port <port> -c <ctx_size>`,
   poll a health endpoint until ready (with timeout), store in
   `RuntimeManager`.
3. Send the request using the profile's `prompt_style` (see §3):
   - `chat`: POST `/v1/chat/completions` with a single user message built
     from `prompt_template`.
   - `completion`: POST `/completion` with the raw templated text, no chat
     wrapping.
4. On app exit, terminate all child processes. On crash/health-check
   failure, restart once before surfacing an error to the user.

Do not hardcode port 8080 for managed instances (8080 stays reserved for the
advanced/custom OpenAI-compatible profile where the user runs their own
server). Pick any free ephemeral port and track it in process state.

---

## 3. Model catalog (v1)

Replace the flat `ModelProfile` in `models.rs` with a richer struct (see
§4) and populate it with these five entries. License notes below are
verified directly against source files, not assumed.

| id | Display name | Engine | License | Approx size | Languages | Notes |
|---|---|---|---|---|---|---|
| `nllb-200-distilled-600m-onnx` | NLLB-200 (Meta) | OnnxEncoderDecoder | CC-BY-NC-4.0 (non-commercial) | ~300–600 MB (int8) | ~200, incl. Slovak | Default broad-coverage fallback. Non-commercial only — fine for a free OSS app with no monetization; re-check if Waylate ever adds paid tiers/donations-with-perks. |
| `opus-mt-marian-onnx` | OPUS-MT / Marian | OnnxEncoderDecoder | Mostly CC-BY-4.0, varies per language pair | 50–300 MB per pair | Specific pairs only (e.g. en-ru, ru-uk, de-en) | Lightest/fastest option for popular fixed pairs. **License must be re-checked per language pair before adding to the catalog** — Helsinki-NLP publishes pair-by-pair, don't assume one license covers all. |
| `tencent-hy-mt2-1.8b-gguf` | Tencent Hy-MT2 (compact) | ManagedLlamaCpp | **Apache-2.0**, confirmed directly from `raw.githubusercontent.com/Tencent-Hunyuan/Hy-MT2/main/LICENSE.txt` (released 2026-05-21) | ~440 MB (1.25-bit AngelSlim quant) up to ~1.9 GB (Q8) | 33 languages | `prompt_style: chat`. No territorial restriction — this is a different, newer license than the older `Hunyuan-MT-7B` (Sept 2025) and `HunyuanVideo`, which still carry the restrictive "Tencent Hunyuan Community License" excluding EU/UK/South Korea. Do not reuse that older license text for Hy-MT2; do not assume other Tencent-Hunyuan model families share this clean license. |
| `translategemma-4b-gguf` | TranslateGemma (Google) | ManagedLlamaCpp | Gemma license (Google ToU, requires acceptance + notice on redistribution, no territorial block) | ~2.5 GB (4B, Q4_K_M) up to ~11.7 GB (27B, Q8) | 55 languages | `prompt_style: chat`. Largest models in the catalog by far — frame as a "if you have the RAM/disk" / high-end option, not a beginner default. Exact recommended prompt template must be pulled from the model card before implementing (it uses a specific chat template, possibly multimodal-aware for image input — text-only path only for v1). |
| `milmmt-46-1b-gguf` | MiLMMT-46 (Xiaomi) | ManagedLlamaCpp | Gemma license | 1B variant likely several hundred MB once GGUF-quantized | 46 languages, incl. Slovak | `prompt_style: completion` (raw text continuation: `Translate this from {src}: \n{src}: {text}\n{tgt}:`, not chat messages — confirm against model card before shipping). **No public GGUF quant found as of this writing** — needs an offline conversion pass using llama.cpp's conversion tooling against `xiaomi-research/MiLMMT-46-1B-v0.1` (Gemma3 architecture, well supported) before this entry can be downloadable. Treat as a prep task, not a runtime dependency. |

Explicitly out of scope for v1: MADLAD-400 (too large, no longer a priority
per product decision), generic "any Hugging Face repo" loading, voice
translation/speech recognition (different product surface, not Waylate's
niche right now).

---

## 4. Data model changes

```rust
struct ModelCatalogEntry {
    id: String,
    name: String,
    engine: EngineKind,
    audience: Audience,              // Beginner | HighQuality | Advanced
    license: String,
    license_url: String,
    homepage: String,
    languages: Vec<LanguageCode>,
    files: Vec<ModelFile>,
    prompt_style: Option<PromptStyle>,  // Chat | Completion — only for ManagedLlamaCpp
    prompt_template: Option<String>,
    estimated_download_bytes: u64,
    estimated_disk_bytes: u64,
    min_ram_bytes: Option<u64>,
    downloadable: bool,
}

enum PromptStyle { Chat, Completion }

struct ModelFile {
    repo: String,           // e.g. "tencent/Hy-MT2-1.8B-1.25bit-GGUF"
    path: String,           // file name within the repo
    sha256: Option<String>,
    size_bytes: Option<u64>,
    destination: String,    // relative path under the model's install dir
}

enum Audience { Beginner, HighQuality, Advanced }

struct LanguageCode {
    ui_code: String,        // shown in UI, e.g. "sk"
    label: String,          // "Slovak"
    nllb_code: Option<String>,   // "slk_Latn"
    onnx_marian_pair: Option<String>, // e.g. "en-sk" if a direct OPUS pair exists
    llm_language_name: Option<String>, // "Slovak" — used in prompt templates
}
```

A single `LanguageCode` table is the source of truth for all five models.
No engine invents its own language code mapping independently.

```rust
enum InstallState {
    NotInstalled,
    Downloading { progress: f32, bytes_done: u64, bytes_total: Option<u64> },
    Verifying,
    Ready,
    Failed { message: String },
    Cancelled,
}
```

Persist per-model install state in config (path, version, sha256, install
timestamp). Support progress events, cancel, retry, resume-on-partial-file,
hash verification, disk-space pre-check, and a clear "delete/reinstall"
action in Settings.

UI/backend command surface (replace direct field access from Svelte):

```rust
list_model_profiles() -> Vec<ModelCatalogEntry>
install_model(profile_id: String)
cancel_model_install(profile_id: String)
get_model_status(profile_id: String) -> InstallState
translate_text(request: TranslationRequest) -> TranslationResponse
```

---

## 5. Download path (replaces `huggingface-cli` and `Command::new("huggingface-cli")`)

Use direct `reqwest` GET requests against
`https://huggingface.co/{repo}/resolve/main/{path}` for each `ModelFile` in
a catalog entry. Emit progress via Tauri `Emitter`. No external CLI tool
required at any point. This already matches the direction noted in the
repo's own implementation notes (Rust HF HTTP downloads with progress/cancel
were already planned/partially done locally — finish this, don't reinvent
it).

---

## 6. UI rules

Beginner-facing screen shows only: model name, one-line description,
audience tag (Recommended / High quality / Pro), approximate download size,
a Download button, and post-install a "Ready" state. It never shows: engine
internals, ports, endpoints, prompt templates, helper commands, device
selection, or license legal text inline (show a short license name with a
link instead).

Advanced section (collapsed by default) exposes: custom OpenAI-compatible
endpoint + model name + bearer token + prompt template + healthcheck URL,
for users who run their own external server (this is the existing
`OpenAiCompatible`/`Custom` provider, unchanged).

If the user hits Translate on a model that isn't installed, show "This
model is not installed — Download" instead of a raw backend error string.

---

## 7. Build/rollout order

PR 1 — Catalog and state machine plumbing. Add `EngineKind`,
`ModelCatalogEntry`, `InstallState`, the five-entry catalog (mark
`milmmt-46` non-downloadable until its GGUF prep task is done), new Tauri
commands. No behavior change yet for existing NLLB/OpenAI-compatible
profiles. Acceptance: app builds, old profiles still work, UI shows the new
catalog list without wiring real downloads yet.

PR 2 — Rust-native HF downloader (§5) wired into `install_model`, with
progress/cancel/resume/verify. Acceptance: clicking Download on any catalog
entry produces real files on disk with a visible progress bar, no
`huggingface-cli` involved.

PR 3 — OnnxEncoderDecoder engine. Implement `engines/onnx_mt.rs`, wire
`nllb-200-distilled-600m-onnx` end to end (download → load → translate).
Acceptance: fresh install, click Download on NLLB, translate text with zero
terminal commands, zero Python.

PR 4 — ManagedLlamaCpp engine. Implement `engines/managed_llama.rs` +
`RuntimeManager`, wire `tencent-hy-mt2-1.8b-gguf` end to end. Acceptance:
fresh install, click Download on Hy-MT2, translate text — Waylate starts and
stops `llama-server` itself, user never sees a port or endpoint.

PR 5 — Remaining catalog entries: `opus-mt-marian-onnx` (verify per-pair
license before adding each pair), `translategemma-4b-gguf` (pull exact
prompt template from model card first), `milmmt-46-1b-gguf` (do the GGUF
conversion prep task first, then add).

PR 6 — Hardening: cold-start status messaging, timeouts, zombie-process
cleanup, delete/reinstall model, disk-space and RAM warnings before
download, basic local diagnostics/log view.

---

## 8. Explicit non-goals / do-not-do

- Do not add a "download any Hugging Face repo" generic feature.
- Do not require `huggingface-cli`, system Python, or a user-managed
  `llama-server` for any Beginner or HighQuality audience model.
- Do not reuse the old `Hunyuan-MT-7B` / `HunyuanVideo`-style license text
  for Hy-MT2 — they are different licenses from different release dates.
  If Tencent ships a future Hy-MT3 or similar, re-verify its license file
  directly, don't assume it inherits Hy-MT2's Apache-2.0.
- Do not promise NLLB is commercial-safe — it's CC-BY-NC-4.0, fine for a
  free non-commercial OSS app only.
- Do not build voice/speech translation in this pass.
- Do not let the installer archive bundle the model weights themselves —
  weights are always a post-install download, keeping the base installer
  small regardless of how large individual models get.

---

## 9. Implementation TODO

- [x] Catalog/data model plumbing: `EngineKind`, `ModelCatalogEntry`,
  `InstallState`, and the five agreed model entries.
- [x] Tauri command surface: `list_model_profiles`, `install_model`,
  `cancel_model_install`, `get_model_status`, `translate_text`.
- [x] Direct Hugging Face HTTP downloader for spec catalog files.
- [x] NLLB ONNX catalog files point at current `Xenova/nllb-200-distilled-600M`
  ONNX assets with exact file sizes.
- [x] `OnnxEncoderDecoder` runtime translates NLLB through Rust `ort` +
  `tokenizers` without Python.
- [x] Fresh beginner default uses `nllb-200-distilled-600m-onnx`, not legacy CT2.
- [x] Beginner ONNX readiness does not depend on `ct2ModelPath` /
  `ct2TokenizerPath`.
- [x] Spec downloader supports retry/resume from `.part` files and verifies
  completed file sizes when `size_bytes` is known.
- [ ] Persist per-model install metadata in config: path, version, hash and
  install timestamp.
- [ ] Pin and verify `sha256` hashes for downloadable catalog files.
- [ ] Add disk-space pre-check before large model downloads.
- [ ] Add clear delete/reinstall action in Settings.
- [ ] Split or explicitly document the `ManagedLlamaCpp` implementation location
  if it remains in `runtime.rs` rather than `engines/managed_llama.rs`.
- [ ] Verify Hy-MT2 end-to-end with Waylate-managed `llama-server`.
- [ ] Verify TranslateGemma prompt template and license acceptance flow before
  treating it as shippable.
- [ ] Finish PR6 hardening: cold-start status, diagnostics/log view, RAM warning
  and zombie-process cleanup polish.
