<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import {
    Clipboard,
    Copy,
    Download,
    FolderOpen,
    History,
    Languages,
    RefreshCw,
    Repeat2,
    Save,
    Settings,
    Trash2,
  } from "@lucide/svelte";

  type ProviderKind =
    | "open-ai-compatible"
    | "c-translate2"
    | "deep-l"
    | "google"
    | "custom";

  type Language = {
    code: string;
    name: string;
  };

  type ModelProfile = {
    id: string;
    name: string;
    provider: ProviderKind;
    description: string;
    license: string;
    homepage: string;
    engineHint: string;
    defaultEndpoint?: string;
    hfRepo?: string;
    languages: Language[];
    downloadable: boolean;
  };

  type AppConfig = {
    modelId: string;
    sourceLang: string;
    targetLang: string;
    historyEnabled: boolean;
    autostart: boolean;
    openaiEndpoint: string;
    openaiModel: string;
    customModelPath: string;
    ct2ModelPath: string;
    ct2TokenizerPath: string;
    ct2HelperCommand: string;
    ct2Device: string;
    apiProviderEnabled: boolean;
  };

  type HistoryEntry = {
    id: number;
    createdAt: string;
    sourceLang: string;
    targetLang: string;
    modelId: string;
    sourceText: string;
    translatedText: string;
  };

  type Snapshot = {
    config: AppConfig;
    catalog: ModelProfile[];
    history: HistoryEntry[];
    environment: {
      desktop: string;
      sessionType: string;
      hasWlClipboard: boolean;
      hasHuggingfaceCli: boolean;
      hasPython: boolean;
      hasNvidiaSmi: boolean;
      hasRocmSmi: boolean;
    };
    hasDeeplKey: boolean;
    hasGoogleKey: boolean;
    hasLocalKey: boolean;
    paths: {
      configDir: string;
      dataDir: string;
      modelsDir: string;
      historyDb: string;
    };
  };

  type PendingRequest = {
    mode: "translate" | "settings";
    sourceText: string;
    notice?: string;
  };

  let snapshot: Snapshot | null = null;
  let config: AppConfig | null = null;
  let tab: "translate" | "settings" | "history" = "translate";
  let sourceText = "";
  let translatedText = "";
  let status = "";
  let error = "";
  let busy = false;
  let deeplKey = "";
  let googleKey = "";
  let localKey = "";

  $: selectedModel = snapshot?.catalog.find((model) => model.id === config?.modelId);
  $: languages = selectedModel?.languages ?? [];
  $: canDownload = Boolean(selectedModel?.downloadable);

  onMount(() => {
    let unlisten: (() => void) | undefined;
    void (async () => {
      await refresh();
      await consumePending();
      unlisten = await listen("waylate-pending", consumePending);
    })();
    return () => unlisten?.();
  });

  async function refresh() {
    snapshot = await invoke<Snapshot>("get_snapshot");
    config = structuredClone(snapshot.config);
  }

  async function consumePending() {
    const pending = await invoke<PendingRequest | null>("take_pending_request");
    if (!pending) return;
    if (pending.mode === "settings") {
      tab = "settings";
      return;
    }
    tab = "translate";
    sourceText = pending.sourceText ?? "";
    translatedText = "";
    status = pending.notice ?? "";
    if (sourceText.trim()) {
      await translate();
    }
  }

  async function translate() {
    if (!config) return;
    error = "";
    status = "";
    translatedText = "";
    if (!sourceText.trim()) {
      error = "Nothing to translate.";
      return;
    }
    busy = true;
    try {
      const response = await invoke<{ translatedText: string; providerLabel: string; warning?: string }>(
        "translate_text",
        {
          request: {
            text: sourceText,
            sourceLang: config.sourceLang,
            targetLang: config.targetLang,
            modelId: config.modelId,
          },
        },
      );
      translatedText = response.translatedText;
      status = response.warning ?? `Translated with ${response.providerLabel}.`;
      await refresh();
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  async function pasteSelection() {
    error = "";
    try {
      sourceText = await invoke<string>("read_selection_text");
    } catch (err) {
      error = String(err);
    }
  }

  async function pasteClipboard() {
    error = "";
    try {
      sourceText = await invoke<string>("read_clipboard_text");
    } catch (err) {
      error = String(err);
    }
  }

  async function copyTranslation() {
    if (!translatedText.trim()) return;
    await invoke("write_clipboard_text", { text: translatedText });
    status = "Translation copied.";
  }

  function swapLanguages() {
    if (!config || config.sourceLang === "auto") return;
    const nextSource = config.targetLang;
    config.targetLang = config.sourceLang;
    config.sourceLang = nextSource;
    const oldSource = sourceText;
    sourceText = translatedText;
    translatedText = oldSource;
  }

  async function saveSettings() {
    if (!config) return;
    error = "";
    try {
      config = await invoke<AppConfig>("save_config", { next: config });
      await refresh();
      status = "Settings saved.";
    } catch (err) {
      error = String(err);
    }
  }

  async function saveKey(provider: string, value: string) {
    error = "";
    try {
      await invoke("save_api_key", { provider, key: value });
      deeplKey = "";
      googleKey = "";
      localKey = "";
      await refresh();
      status = "API key saved in Secret Service.";
    } catch (err) {
      error = String(err);
    }
  }

  async function clearKey(provider: string) {
    error = "";
    try {
      await invoke("clear_api_key", { provider });
      await refresh();
      status = "API key removed.";
    } catch (err) {
      error = String(err);
    }
  }

  async function downloadModel() {
    if (!config) return;
    busy = true;
    error = "";
    try {
      const path = await invoke<string>("download_catalog_model", { modelId: config.modelId });
      status = `Downloaded to ${path}`;
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  async function clearLocalHistory() {
    await invoke("clear_history");
    await refresh();
  }

  async function reveal(path: string) {
    await invoke("reveal_path", { path });
  }

  async function revealConfigDir() {
    if (snapshot) await reveal(snapshot.paths.configDir);
  }

  async function revealModelsDir() {
    if (snapshot) await reveal(snapshot.paths.modelsDir);
  }
</script>

<svelte:head>
  <title>Waylate</title>
</svelte:head>

<main class="shell">
  <aside class="rail">
    <div class="mark" title="Waylate">W</div>
    <nav aria-label="Views">
      <button class:active={tab === "translate"} title="Translate" aria-label="Translate" on:click={() => (tab = "translate")}>
        <Languages size={14} />
      </button>
      <button class:active={tab === "settings"} title="Settings" aria-label="Settings" on:click={() => (tab = "settings")}>
        <Settings size={14} />
      </button>
      <button class:active={tab === "history"} title="History" aria-label="History" on:click={() => (tab = "history")}>
        <History size={14} />
      </button>
    </nav>
  </aside>

  <section class="workspace">
    {#if config && snapshot}
      {#if tab === "translate"}
        <section class="toolbar" aria-label="Translation options">
          <label>
            Model
            <select bind:value={config.modelId}>
              {#each snapshot.catalog as model}
                <option value={model.id}>{model.name}</option>
              {/each}
            </select>
          </label>
          <label>
            From
            <select bind:value={config.sourceLang}>
              {#each languages as language}
                <option value={language.code}>{language.name}</option>
              {/each}
            </select>
          </label>
          <button class="icon" title="Swap languages" on:click={swapLanguages} disabled={config.sourceLang === "auto"}>
            <Repeat2 size={15} />
          </button>
          <label>
            To
            <select bind:value={config.targetLang}>
              {#each languages.filter((language) => language.code !== "auto") as language}
                <option value={language.code}>{language.name}</option>
              {/each}
            </select>
          </label>
          <button class="primary run" on:click={translate} disabled={busy}>
            <span class:spin={busy}><RefreshCw size={15} /></span> Translate
          </button>
        </section>

        <section class="translate-grid">
          <div class="pane">
            <div class="pane-head">
              <span>Source</span>
            </div>
            <textarea bind:value={sourceText} spellcheck="false" placeholder="Paste text or run waylate translate-selection"></textarea>
            <div class="pane-actions">
              <button class="icon small" title="Read Wayland selection" aria-label="Read Wayland selection" on:click={pasteSelection}>
                <Languages size={13} />
              </button>
              <button class="icon small" title="Paste clipboard" aria-label="Paste clipboard" on:click={pasteClipboard}>
                <Clipboard size={13} />
              </button>
            </div>
          </div>
          <div class="pane">
            <div class="pane-head">
              <span>Translation</span>
            </div>
            <textarea bind:value={translatedText} spellcheck="false" readonly placeholder="Translation appears here"></textarea>
            <div class="pane-actions end">
              <button class="icon small" title="Copy translation" aria-label="Copy translation" on:click={copyTranslation} disabled={!translatedText.trim()}>
                <Copy size={13} />
              </button>
            </div>
          </div>
        </section>

        <section class="model-note">
          <strong>{selectedModel?.name}</strong>
          <span>{selectedModel?.description}</span>
        </section>
      {:else if tab === "settings"}
        <section class="settings-grid">
        <div class="group">
          <h2>Local backend</h2>
          <label>
            OpenAI-compatible endpoint
            <input bind:value={config.openaiEndpoint} placeholder="http://127.0.0.1:8080/v1/chat/completions" />
          </label>
          <label>
            Model name
            <input bind:value={config.openaiModel} placeholder="local-translation-model" />
          </label>
          <label>
            CTranslate2 model path
            <input bind:value={config.ct2ModelPath} placeholder="/home/user/.local/share/Waylate/models/nllb" />
          </label>
          <label>
            Tokenizer path or HF id
            <input bind:value={config.ct2TokenizerPath} placeholder="facebook/nllb-200-distilled-600M" />
          </label>
          <label>
            CTranslate2 helper command
            <input bind:value={config.ct2HelperCommand} placeholder="waylate-ct2-translate" />
          </label>
          <label>
            Device
            <select bind:value={config.ct2Device}>
              <option value="auto">auto</option>
              <option value="cuda">cuda</option>
              <option value="cpu">cpu</option>
            </select>
          </label>
          <div class="actions">
            <button class="primary" on:click={saveSettings}><Save size={16} /> Save</button>
            <button on:click={downloadModel} disabled={!canDownload || busy}><Download size={16} /> Download catalog model</button>
          </div>
        </div>

        <div class="group">
          <h2>Privacy and API</h2>
          <label class="check">
            <input type="checkbox" bind:checked={config.historyEnabled} />
            Save translation history locally
          </label>
          <label class="check">
            <input type="checkbox" bind:checked={config.autostart} />
            Start Waylate in background
          </label>
          <label class="check">
            <input type="checkbox" bind:checked={config.apiProviderEnabled} />
            Allow network API providers
          </label>
          <label>
            DeepL API key {snapshot.hasDeeplKey ? "(saved)" : ""}
            <div class="inline">
              <input bind:value={deeplKey} type="password" placeholder="Stored in Secret Service" />
              <button on:click={() => saveKey("deepl", deeplKey)} disabled={!deeplKey}><Save size={15} /></button>
              <button on:click={() => clearKey("deepl")}><Trash2 size={15} /></button>
            </div>
          </label>
          <label>
            Google API key {snapshot.hasGoogleKey ? "(saved)" : ""}
            <div class="inline">
              <input bind:value={googleKey} type="password" placeholder="Stored in Secret Service" />
              <button on:click={() => saveKey("google", googleKey)} disabled={!googleKey}><Save size={15} /></button>
              <button on:click={() => clearKey("google")}><Trash2 size={15} /></button>
            </div>
          </label>
          <label>
            Local bearer token {snapshot.hasLocalKey ? "(saved)" : ""}
            <div class="inline">
              <input bind:value={localKey} type="password" placeholder="Optional for local server" />
              <button on:click={() => saveKey("openai-compatible", localKey)} disabled={!localKey}><Save size={15} /></button>
              <button on:click={() => clearKey("openai-compatible")}><Trash2 size={15} /></button>
            </div>
          </label>
        </div>

        <div class="group wide">
          <h2>System</h2>
          <div class="facts">
            <span class:ok={snapshot.environment.hasWlClipboard}>wl-clipboard</span>
            <span class:ok={snapshot.environment.hasPython}>python3</span>
            <span class:ok={snapshot.environment.hasHuggingfaceCli}>huggingface-cli</span>
            <span class:ok={snapshot.environment.hasNvidiaSmi}>CUDA/NVIDIA</span>
            <span class:ok={snapshot.environment.hasRocmSmi}>ROCm</span>
          </div>
          <div class="actions">
            <button on:click={revealConfigDir}><FolderOpen size={16} /> Config</button>
            <button on:click={revealModelsDir}><FolderOpen size={16} /> Models</button>
          </div>
        </div>
        </section>
      {:else}
        <section class="history-list">
        <div class="history-head">
          <strong>Local history</strong>
          <button on:click={clearLocalHistory} disabled={!snapshot.history.length}><Trash2 size={16} /> Clear</button>
        </div>
        {#if !config.historyEnabled}
          <p>History is disabled. Enable it in Settings if you want local SQLite history.</p>
        {:else if !snapshot.history.length}
          <p>No saved translations yet.</p>
        {:else}
          {#each snapshot.history as item}
            <article>
              <small>{item.sourceLang} -> {item.targetLang} / {item.modelId}</small>
              <p>{item.sourceText}</p>
              <strong>{item.translatedText}</strong>
            </article>
          {/each}
        {/if}
        </section>
      {/if}
    {:else}
      <section class="loading">Loading Waylate...</section>
    {/if}

    {#if status}
      <p class="status">{status}</p>
    {/if}
    {#if error}
      <p class="error">{error}</p>
    {/if}
  </section>
</main>

<style>
  :global(*) {
    box-sizing: border-box;
  }

  :global(html),
  :global(body) {
    height: 100%;
    overflow: hidden;
  }

  :global(body) {
    margin: 0;
    min-width: 680px;
    color: #182026;
    background: #f5f7f4;
    font-family:
      Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  }

  button,
  input,
  select,
  textarea {
    font: inherit;
  }

  button {
    min-height: 30px;
    border: 1px solid #c4cbd0;
    border-radius: 6px;
    color: #16202a;
    background: #ffffff;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 0 9px;
    cursor: pointer;
  }

  button:hover {
    border-color: #7591a3;
    background: #f8fbfc;
  }

  button:disabled {
    cursor: default;
    opacity: 0.55;
  }

  .primary {
    color: #ffffff;
    border-color: #256b62;
    background: #256b62;
  }

  .primary:hover {
    background: #1e5d55;
  }

  .shell {
    height: 100vh;
    display: grid;
    grid-template-columns: 44px minmax(0, 1fr);
    overflow: hidden;
  }

  .rail {
    padding: 8px 6px;
    display: grid;
    grid-template-rows: 32px 1fr;
    gap: 10px;
    border-right: 1px solid #d6dce0;
    background: #ffffff;
  }

  .mark {
    width: 30px;
    height: 30px;
    border-radius: 7px;
    color: #ffffff;
    background: #256b62;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 13px;
    font-weight: 800;
  }

  nav {
    display: grid;
    align-content: start;
    gap: 7px;
  }

  nav button {
    width: 30px;
    height: 30px;
    min-height: 30px;
    padding: 0;
  }

  nav button.active {
    color: #ffffff;
    border-color: #364852;
    background: #364852;
  }

  .workspace {
    min-width: 0;
    min-height: 0;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    overflow: hidden;
  }

  .toolbar {
    min-height: 52px;
    padding: 8px 12px;
    display: grid;
    grid-template-columns: minmax(150px, 1.5fr) minmax(110px, 1fr) 30px minmax(110px, 1fr) auto;
    gap: 8px;
    align-items: end;
    border-bottom: 1px solid #d6dce0;
    background: #edf2ef;
  }

  label {
    display: grid;
    gap: 5px;
    color: #586670;
    font-size: 12px;
    font-weight: 600;
  }

  input,
  select,
  textarea {
    width: 100%;
    border: 1px solid #c4cbd0;
    border-radius: 6px;
    color: #14212a;
    background: #ffffff;
  }

  input,
  select {
    height: 30px;
    padding: 0 8px;
  }

  textarea {
    min-height: 0;
    height: 100%;
    resize: none;
    padding: 10px;
    line-height: 1.45;
  }

  .icon {
    width: 30px;
    padding: 0;
  }

  .small {
    width: 26px;
    height: 26px;
    min-height: 26px;
  }

  .run {
    min-width: 96px;
  }

  .translate-grid {
    min-height: 0;
    padding: 10px 12px 6px;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  .pane {
    min-width: 0;
    min-height: 0;
    display: grid;
    grid-template-rows: 24px minmax(0, 1fr) 28px;
    gap: 6px;
  }

  .pane-head {
    min-height: 24px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    color: #34454f;
    font-size: 13px;
    font-weight: 700;
  }

  .pane-actions,
  .actions,
  .inline {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .pane-actions {
    justify-content: flex-start;
  }

  .pane-actions.end {
    justify-content: flex-end;
  }

  .model-note {
    margin: 0 12px 8px;
    display: flex;
    gap: 8px;
    align-items: center;
    color: #4d5b62;
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
  }

  .model-note span {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .settings-grid {
    min-height: 0;
    padding: 12px;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
    overflow: auto;
  }

  .group {
    padding: 12px;
    display: grid;
    gap: 12px;
    border: 1px solid #d6dce0;
    border-radius: 8px;
    background: #ffffff;
  }

  .group.wide {
    grid-column: 1 / -1;
  }

  h2 {
    margin: 0 0 2px;
    color: #263740;
    font-size: 15px;
  }

  .check {
    grid-template-columns: 18px 1fr;
    align-items: center;
    color: #263740;
    font-size: 14px;
    font-weight: 500;
  }

  .check input {
    width: 16px;
    height: 16px;
  }

  .inline {
    display: grid;
    grid-template-columns: 1fr 36px 36px;
  }

  .facts {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .facts span {
    min-height: 28px;
    padding: 5px 9px;
    border: 1px solid #cfd6da;
    border-radius: 6px;
    color: #7b4a37;
    background: #fff7f2;
  }

  .facts span.ok {
    color: #1f6848;
    background: #effaf3;
    border-color: #acd7bd;
  }

  .history-list {
    min-height: 0;
    padding: 12px;
    display: grid;
    gap: 12px;
    align-content: start;
    overflow: auto;
  }

  .history-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  article {
    padding: 12px;
    display: grid;
    gap: 6px;
    border: 1px solid #d6dce0;
    border-radius: 8px;
    background: #ffffff;
  }

  article p,
  article strong {
    margin: 0;
    white-space: pre-wrap;
  }

  article small {
    color: #69777f;
  }

  .status,
  .error {
    margin: 0;
    padding: 8px 18px;
    font-size: 13px;
    border-top: 1px solid #d6dce0;
  }

  .status {
    color: #245342;
    background: #edf8f1;
  }

  .error {
    color: #8a2e2e;
    background: #fff0f0;
  }

  .loading {
    padding: 22px;
  }

  .spin {
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (max-width: 760px) {
    :global(body) {
      min-width: 0;
    }

    .shell,
    .toolbar,
    .translate-grid,
    .settings-grid {
      grid-template-columns: 1fr;
    }

    .shell {
      grid-template-rows: 40px minmax(0, 1fr);
    }

    .rail {
      grid-template-columns: 32px 1fr;
      grid-template-rows: 1fr;
      border-right: 0;
      border-bottom: 1px solid #d6dce0;
    }

    nav {
      display: flex;
    }

    textarea {
      min-height: 140px;
    }
  }
</style>
