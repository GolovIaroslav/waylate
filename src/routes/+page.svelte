<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import {
    CheckCircle2,
    ChevronDown,
    Clipboard,
    Copy,
    CircleHelp,
    Download,
    FolderOpen,
    History,
    Languages,
    RefreshCw,
    Repeat2,
    Save,
    Settings,
    Trash2,
    ZoomIn,
    ZoomOut,
  } from "@lucide/svelte";

  type ProviderKind =
    | "open-ai-compatible"
    | "c-translate2"
    | "deep-l"
    | "google"
    | "yandex"
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
    yandexFolderId: string;
    uiLanguage: string;
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
    hasYandexKey: boolean;
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
  let yandexKey = "";
  let localKey = "";
  let uiScale = 1;
  let sourceLanguageQuery = "";
  let targetLanguageQuery = "";
  let sourceLanguageOpen = false;
  let targetLanguageOpen = false;

  $: selectedModel = snapshot?.catalog.find((model) => model.id === config?.modelId);
  $: languages = selectedModel?.languages ?? [];
  $: localModelReady = Boolean(config?.ct2ModelPath && config?.ct2TokenizerPath);

  const languageAliases: Record<string, string[]> = {
    auto: ["auto"],
    en: ["en", "eng_Latn"],
    eng_Latn: ["eng_Latn", "en"],
    ru: ["ru", "rus_Cyrl"],
    rus_Cyrl: ["rus_Cyrl", "ru"],
    sk: ["sk", "slk_Latn"],
    slk_Latn: ["slk_Latn", "sk"],
    cs: ["cs", "ces_Latn"],
    ces_Latn: ["ces_Latn", "cs"],
    de: ["de", "deu_Latn"],
    deu_Latn: ["deu_Latn", "de"],
    uk: ["uk", "ukr_Cyrl"],
    ukr_Cyrl: ["ukr_Cyrl", "uk"],
    pl: ["pl", "pol_Latn"],
    pol_Latn: ["pol_Latn", "pl"],
    fr: ["fr", "fra_Latn"],
    fra_Latn: ["fra_Latn", "fr"],
    es: ["es", "spa_Latn"],
    spa_Latn: ["spa_Latn", "es"],
    zh: ["zh", "zho_Hans"],
    zho_Hans: ["zho_Hans", "zh"],
    ja: ["ja", "jpn_Jpan"],
    jpn_Jpan: ["jpn_Jpan", "ja"],
    it: ["it", "ita_Latn"],
    ita_Latn: ["ita_Latn", "it"],
    pt: ["pt", "por_Latn"],
    por_Latn: ["por_Latn", "pt"],
    tr: ["tr", "tur_Latn"],
    tur_Latn: ["tur_Latn", "tr"],
    ko: ["ko", "kor_Hang"],
    kor_Hang: ["kor_Hang", "ko"],
  };

  const languageSearchAliases: Record<string, string[]> = {
    auto: ["auto", "detect", "авто", "автоопределение", "automaticky"],
    en: ["english", "английский", "англ", "anglictina", "angličtina"],
    eng_Latn: ["english", "английский", "англ", "anglictina", "angličtina"],
    ru: ["russian", "русский", "рус", "rustina", "ruština"],
    rus_Cyrl: ["russian", "русский", "рус", "rustina", "ruština"],
    sk: ["slovak", "словацкий", "slovencina", "slovenčina"],
    slk_Latn: ["slovak", "словацкий", "slovencina", "slovenčina"],
    cs: ["czech", "чешский", "cestina", "čeština"],
    ces_Latn: ["czech", "чешский", "cestina", "čeština"],
    de: ["german", "немецкий", "nemcina", "nemčina"],
    deu_Latn: ["german", "немецкий", "nemcina", "nemčina"],
    uk: ["ukrainian", "украинский", "ukrajincina", "ukrajinčina"],
    ukr_Cyrl: ["ukrainian", "украинский", "ukrajincina", "ukrajinčina"],
    pl: ["polish", "польский", "polstina", "poľština"],
    pol_Latn: ["polish", "польский", "polstina", "poľština"],
    fr: ["french", "французский", "francuzstina", "francúzština"],
    fra_Latn: ["french", "французский", "francuzstina", "francúzština"],
    es: ["spanish", "испанский", "spanielcina", "španielčina"],
    spa_Latn: ["spanish", "испанский", "spanielcina", "španielčina"],
    zh: ["chinese", "китайский", "cinstina", "čínština"],
    zho_Hans: ["chinese", "китайский", "cinstina", "čínština"],
    ja: ["japanese", "японский", "japoncina", "japončina"],
    jpn_Jpan: ["japanese", "японский", "japoncina", "japončina"],
    it: ["italian", "итальянский", "taliancina", "taliančina"],
    ita_Latn: ["italian", "итальянский", "taliancina", "taliančina"],
    pt: ["portuguese", "португальский", "portugalcina", "portugalčina"],
    por_Latn: ["portuguese", "португальский", "portugalcina", "portugalčina"],
    tr: ["turkish", "турецкий", "turectina", "turečtina"],
    tur_Latn: ["turkish", "турецкий", "turectina", "turečtina"],
    ko: ["korean", "корейский", "korejcina", "kórejčina"],
    kor_Hang: ["korean", "корейский", "korejcina", "kórejčina"],
  };

  const helpTexts = {
    openaiEndpoint: {
      en: "Advanced only. Used by GGUF/custom profiles when you run your own local HTTP translation server, for example llama.cpp.",
      ru: "Только для advanced-сценария. Нужно GGUF/custom профилям, если ты сам запускаешь локальный HTTP сервер перевода, например llama.cpp.",
      sk: "Iba pre pokročilé nastavenie. Používa sa pri GGUF/custom profiloch, keď spúšťaš vlastný lokálny HTTP prekladový server, napríklad llama.cpp.",
    },
    openaiModel: {
      en: "Model id sent to an OpenAI-compatible local server. Some local servers ignore it, some require the loaded model name.",
      ru: "ID модели, который отправляется OpenAI-compatible локальному серверу. Некоторые серверы игнорируют его, другим нужно имя загруженной модели.",
      sk: "ID modelu posielané lokálnemu OpenAI-compatible serveru. Niektoré servery ho ignorujú, iné vyžadujú názov načítaného modelu.",
    },
    ct2ModelPath: {
      en: "Folder with converted CTranslate2 model files. The NLLB download button fills this automatically.",
      ru: "Папка с файлами CTranslate2-модели. Кнопка скачивания NLLB заполняет это автоматически.",
      sk: "Priečinok so súbormi konvertovaného CTranslate2 modelu. Tlačidlo na stiahnutie NLLB to vyplní automaticky.",
    },
    ct2TokenizerPath: {
      en: "Usually the same downloaded NLLB folder. Advanced users can point it to a Hugging Face tokenizer id.",
      ru: "Обычно та же скачанная папка NLLB. Продвинутые пользователи могут указать Hugging Face tokenizer id.",
      sk: "Zvyčajne rovnaký stiahnutý priečinok NLLB. Pokročilí používatelia môžu zadať Hugging Face tokenizer id.",
    },
    ct2HelperCommand: {
      en: "Command Waylate runs for NLLB translation. Installed releases include waylate-ct2-translate.",
      ru: "Команда, которую Waylate запускает для NLLB-перевода. В релизах она ставится как waylate-ct2-translate.",
      sk: "Príkaz, ktorý Waylate spúšťa na NLLB preklad. Inštalované releasy obsahujú waylate-ct2-translate.",
    },
    device: {
      en: "auto tries CUDA when CTranslate2 sees a CUDA device, otherwise CPU. CUDA is faster but uses VRAM while translating.",
      ru: "auto пробует CUDA, если CTranslate2 видит CUDA-устройство, иначе CPU. CUDA быстрее, но во время перевода занимает VRAM.",
      sk: "auto skúsi CUDA, keď CTranslate2 vidí CUDA zariadenie, inak CPU. CUDA je rýchlejšia, ale počas prekladu používa VRAM.",
    },
    history: {
      en: "When enabled, translations are stored locally in SQLite. Nothing is uploaded because of history.",
      ru: "Если включено, переводы сохраняются локально в SQLite. Из-за истории ничего не отправляется в сеть.",
      sk: "Ak je zapnuté, preklady sa ukladajú lokálne do SQLite. História nič neodosiela do siete.",
    },
    autostart: {
      en: "Starts Waylate in the background so the tray and shortcut workflow are ready after login.",
      ru: "Запускает Waylate в фоне, чтобы tray и shortcut были готовы после входа в систему.",
      sk: "Spustí Waylate na pozadí, aby bol tray a workflow so skratkou pripravený po prihlásení.",
    },
    networkApis: {
      en: "Allows cloud providers. Keep this off if you only want local translation.",
      ru: "Разрешает облачные провайдеры. Оставь выключенным, если нужен только локальный перевод.",
      sk: "Povolí cloudových providerov. Nechaj vypnuté, ak chceš iba lokálny preklad.",
    },
    deeplKey: {
      en: "DeepL Free/Pro API key. Text is sent to DeepL only when the DeepL profile is selected and network providers are enabled.",
      ru: "API-ключ DeepL Free/Pro. Текст отправляется в DeepL только когда выбран профиль DeepL и включены сетевые провайдеры.",
      sk: "API kľúč DeepL Free/Pro. Text sa odošle do DeepL iba keď je vybraný DeepL profil a sieťoví provideri sú povolení.",
    },
    googleKey: {
      en: "Google Cloud Translation API key. The user owns the Cloud project and quota.",
      ru: "API-ключ Google Cloud Translation. Пользователь сам владеет Cloud-проектом и квотой.",
      sk: "API kľúč Google Cloud Translation. Používateľ vlastní Cloud projekt a kvótu.",
    },
    yandexKey: {
      en: "Yandex Cloud Translate API key. You also need a Folder ID below.",
      ru: "API-ключ Yandex Cloud Translate. Ещё нужен Folder ID ниже.",
      sk: "API kľúč Yandex Cloud Translate. Nižšie treba zadať aj Folder ID.",
    },
    yandexFolderId: {
      en: "Required by Yandex Cloud Translate v2 to choose the cloud folder that owns billing and permissions.",
      ru: "Нужен Yandex Cloud Translate v2, чтобы выбрать cloud folder с биллингом и правами доступа.",
      sk: "Vyžaduje ho Yandex Cloud Translate v2 na výber cloud priečinka s billingom a oprávneniami.",
    },
    localBearer: {
      en: "Optional password for an advanced local OpenAI-compatible server. Most users should leave it empty.",
      ru: "Необязательный пароль для advanced локального OpenAI-compatible сервера. Большинству пользователей это поле не нужно.",
      sk: "Voliteľné heslo pre pokročilý lokálny OpenAI-compatible server. Väčšina používateľov ho nepotrebuje.",
    },
    uiLanguage: {
      en: "Controls explanatory tooltips. Full interface translation can be expanded later.",
      ru: "Меняет язык поясняющих подсказок. Полный перевод интерфейса можно расширить позже.",
      sk: "Mení jazyk vysvetľujúcich tooltipov. Plný preklad rozhrania sa dá rozšíriť neskôr.",
    },
  } as const;

  onMount(() => {
    let unlisten: (() => void) | undefined;
    const savedScale = Number(localStorage.getItem("waylate-ui-scale"));
    setUiScale(Number.isFinite(savedScale) ? savedScale : 1);
    const handleKeydown = (event: KeyboardEvent) => {
      if (!event.ctrlKey) return;
      if (event.key === "+" || event.key === "=") {
        event.preventDefault();
        setUiScale(uiScale + 0.1);
      } else if (event.key === "-") {
        event.preventDefault();
        setUiScale(uiScale - 0.1);
      } else if (event.key === "0") {
        event.preventDefault();
        setUiScale(1);
      }
    };
    const handleWheel = (event: WheelEvent) => {
      if (!event.ctrlKey) return;
      event.preventDefault();
      setUiScale(uiScale + (event.deltaY < 0 ? 0.1 : -0.1));
    };
    window.addEventListener("keydown", handleKeydown);
    window.addEventListener("wheel", handleWheel, { passive: false });
    void (async () => {
      await refresh();
      if (config && !config.ct2ModelPath) {
        tab = "settings";
      }
      await consumePending();
      unlisten = await listen("waylate-pending", consumePending);
    })();
    return () => {
      unlisten?.();
      window.removeEventListener("keydown", handleKeydown);
      window.removeEventListener("wheel", handleWheel);
    };
  });

  function setUiScale(next: number) {
    uiScale = Math.min(1.8, Math.max(0.75, Math.round(next * 10) / 10));
    document.documentElement.style.setProperty("--ui-scale", String(uiScale));
    localStorage.setItem("waylate-ui-scale", String(uiScale));
  }

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

  function changeModel(modelId: string) {
    if (!config || !snapshot) return;
    const nextModel = snapshot.catalog.find((model) => model.id === modelId);
    if (!nextModel) return;
    config.modelId = modelId;
    config.sourceLang = closestLanguage(config.sourceLang, nextModel.languages, true);
    config.targetLang = closestLanguage(config.targetLang, nextModel.languages, false);
  }

  function closestLanguage(current: string, nextLanguages: Language[], allowAuto: boolean) {
    const available = new Set(nextLanguages.map((language) => language.code));
    const aliases = languageAliases[current] ?? [current];
    const match = aliases.find((code) => available.has(code));
    if (match) return match;
    if (allowAuto && available.has("auto")) return "auto";
    return nextLanguages.find((language) => language.code !== "auto")?.code ?? current;
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
      yandexKey = "";
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
      const path = await invoke<string>("download_catalog_model", { modelId: "nllb-200-ct2" });
      await refresh();
      status = `Downloaded and configured: ${path}`;
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

  function selectLanguage(kind: "source" | "target", code: string) {
    if (!config) return;
    if (kind === "source") {
      config.sourceLang = code;
      sourceLanguageQuery = "";
      sourceLanguageOpen = false;
    } else {
      config.targetLang = code;
      targetLanguageQuery = "";
      targetLanguageOpen = false;
    }
  }

  function languageLabel(code: string) {
    return languages.find((language) => language.code === code)?.name ?? code;
  }

  function languageSearchText(language: Language) {
    return [language.code, language.name, ...(languageSearchAliases[language.code] ?? [])]
      .join(" ")
      .toLocaleLowerCase();
  }

  function filteredLanguages(query: string, includeAuto: boolean) {
    const normalized = query.trim().toLocaleLowerCase();
    return languages
      .filter((language) => includeAuto || language.code !== "auto")
      .filter((language) => !normalized || languageSearchText(language).includes(normalized))
      .slice(0, 36);
  }

  function help(key: keyof typeof helpTexts) {
    const lang = config?.uiLanguage === "ru" || config?.uiLanguage === "sk" ? config.uiLanguage : "en";
    return helpTexts[key][lang];
  }

</script>

<svelte:head>
  <title>Waylate</title>
</svelte:head>

<main class="shell">
  <aside class="rail">
    <button class="mark" title="Translate" aria-label="Translate" on:click={() => (tab = "translate")}>W</button>
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
            <select value={config.modelId} on:change={(event) => changeModel(event.currentTarget.value)}>
              {#each snapshot.catalog as model}
                <option value={model.id}>{model.name}</option>
              {/each}
            </select>
          </label>
          <label class="combo-label">
            From
            <div class="combo">
              <button type="button" class="combo-button" on:click={() => (sourceLanguageOpen = !sourceLanguageOpen)}>
                <span>{languageLabel(config.sourceLang)}</span>
                <ChevronDown size={14} />
              </button>
              {#if sourceLanguageOpen}
                <div class="combo-menu">
                  <input bind:value={sourceLanguageQuery} placeholder="Search language" />
                  <div class="combo-options">
                    {#each filteredLanguages(sourceLanguageQuery, true) as language}
                      <button type="button" class:active={language.code === config.sourceLang} on:click={() => selectLanguage("source", language.code)}>
                        <span>{language.name}</span>
                        <small>{language.code}</small>
                      </button>
                    {/each}
                  </div>
                </div>
              {/if}
            </div>
          </label>
          <button class="icon" title="Swap languages" on:click={swapLanguages} disabled={config.sourceLang === "auto"}>
            <Repeat2 size={15} />
          </button>
          <label class="combo-label">
            To
            <div class="combo">
              <button type="button" class="combo-button" on:click={() => (targetLanguageOpen = !targetLanguageOpen)}>
                <span>{languageLabel(config.targetLang)}</span>
                <ChevronDown size={14} />
              </button>
              {#if targetLanguageOpen}
                <div class="combo-menu">
                  <input bind:value={targetLanguageQuery} placeholder="Search language" />
                  <div class="combo-options">
                    {#each filteredLanguages(targetLanguageQuery, false) as language}
                      <button type="button" class:active={language.code === config.targetLang} on:click={() => selectLanguage("target", language.code)}>
                        <span>{language.name}</span>
                        <small>{language.code}</small>
                      </button>
                    {/each}
                  </div>
                </div>
              {/if}
            </div>
          </label>
          <button class="primary run" on:click={translate} disabled={busy}>
            <span class:spin={busy}><RefreshCw size={15} /></span> Translate
          </button>
          <div class="zoom-controls" aria-label="Interface zoom">
            <button class="icon small" title="Zoom out" aria-label="Zoom out" on:click={() => setUiScale(uiScale - 0.1)}><ZoomOut size={13} /></button>
            <button class="zoom-value" title="Reset zoom" on:click={() => setUiScale(1)}>{Math.round(uiScale * 100)}%</button>
            <button class="icon small" title="Zoom in" aria-label="Zoom in" on:click={() => setUiScale(uiScale + 0.1)}><ZoomIn size={13} /></button>
          </div>
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
          <div class="group-head">
            <h2>Local model</h2>
            {#if localModelReady}
              <span class="pill ok"><CheckCircle2 size={13} /> Ready</span>
            {:else}
              <span class="pill">Setup needed</span>
            {/if}
          </div>
          <p class="muted">Recommended path: download NLLB, then translate locally without sending text to a cloud API.</p>
          <div class="actions">
            <button class="primary" on:click={downloadModel} disabled={busy}>
              <Download size={16} /> Download and configure NLLB
            </button>
            <button on:click={saveSettings}><Save size={16} /> Save</button>
          </div>
          <div class="setup-list">
            <span class:ok={Boolean(config.ct2ModelPath)}>Model path</span>
            <span class:ok={Boolean(config.ct2TokenizerPath)}>Tokenizer</span>
            <span class:ok={snapshot.environment.hasPython}>Python</span>
            <span class:ok={snapshot.environment.hasNvidiaSmi || snapshot.environment.hasRocmSmi || config.ct2Device === "cpu"}>Device</span>
          </div>
          <details>
            <summary>Advanced local backend</summary>
            <label>
              <span>OpenAI-compatible endpoint <span class="help" title={help("openaiEndpoint")}><CircleHelp size={13} /></span></span>
              <input bind:value={config.openaiEndpoint} placeholder="http://127.0.0.1:8080/v1/chat/completions" />
            </label>
            <label>
              <span>Model name <span class="help" title={help("openaiModel")}><CircleHelp size={13} /></span></span>
              <input bind:value={config.openaiModel} placeholder="local-translation-model" />
            </label>
            <label>
              <span>CTranslate2 model path <span class="help" title={help("ct2ModelPath")}><CircleHelp size={13} /></span></span>
              <input bind:value={config.ct2ModelPath} placeholder="/home/user/.local/share/Waylate/models/nllb-200-ct2" />
            </label>
            <label>
              <span>Tokenizer path or HF id <span class="help" title={help("ct2TokenizerPath")}><CircleHelp size={13} /></span></span>
              <input bind:value={config.ct2TokenizerPath} placeholder="same as model path" />
            </label>
            <label>
              <span>CTranslate2 helper command <span class="help" title={help("ct2HelperCommand")}><CircleHelp size={13} /></span></span>
              <input bind:value={config.ct2HelperCommand} placeholder="waylate-ct2-translate" />
            </label>
            <label>
              <span>Device <span class="help" title={help("device")}><CircleHelp size={13} /></span></span>
              <select bind:value={config.ct2Device}>
                <option value="auto">auto</option>
                <option value="cuda">cuda</option>
                <option value="cpu">cpu</option>
              </select>
            </label>
          </details>
        </div>

        <div class="group">
          <h2>Privacy and APIs</h2>
          <label>
            <span>Interface language <span class="help" title={help("uiLanguage")}><CircleHelp size={13} /></span></span>
            <select bind:value={config.uiLanguage}>
              <option value="en">English</option>
              <option value="ru">Русский</option>
              <option value="sk">Slovenčina</option>
            </select>
          </label>
          <label class="check">
            <input type="checkbox" bind:checked={config.historyEnabled} />
            <span>Save translation history locally <span class="help" title={help("history")}><CircleHelp size={13} /></span></span>
          </label>
          <label class="check">
            <input type="checkbox" bind:checked={config.autostart} />
            <span>Start Waylate in background <span class="help" title={help("autostart")}><CircleHelp size={13} /></span></span>
          </label>
          <label class="check">
            <input type="checkbox" bind:checked={config.apiProviderEnabled} />
            <span>Allow network API providers <span class="help" title={help("networkApis")}><CircleHelp size={13} /></span></span>
          </label>
          <p class="muted">DeepL, Google and Yandex need your own key. Waylate stores keys in Secret Service, not in config.json.</p>
          <label>
            <span>DeepL API key {snapshot.hasDeeplKey ? "(saved)" : ""} <span class="help" title={help("deeplKey")}><CircleHelp size={13} /></span></span>
            <div class="inline">
              <input bind:value={deeplKey} type="password" placeholder="Stored in Secret Service" />
              <button on:click={() => saveKey("deepl", deeplKey)} disabled={!deeplKey}><Save size={15} /></button>
              <button on:click={() => clearKey("deepl")}><Trash2 size={15} /></button>
            </div>
          </label>
          <label>
            <span>Google API key {snapshot.hasGoogleKey ? "(saved)" : ""} <span class="help" title={help("googleKey")}><CircleHelp size={13} /></span></span>
            <div class="inline">
              <input bind:value={googleKey} type="password" placeholder="Stored in Secret Service" />
              <button on:click={() => saveKey("google", googleKey)} disabled={!googleKey}><Save size={15} /></button>
              <button on:click={() => clearKey("google")}><Trash2 size={15} /></button>
            </div>
          </label>
          <label>
            <span>Yandex API key {snapshot.hasYandexKey ? "(saved)" : ""} <span class="help" title={help("yandexKey")}><CircleHelp size={13} /></span></span>
            <div class="inline">
              <input bind:value={yandexKey} type="password" placeholder="Stored in Secret Service" />
              <button on:click={() => saveKey("yandex", yandexKey)} disabled={!yandexKey}><Save size={15} /></button>
              <button on:click={() => clearKey("yandex")}><Trash2 size={15} /></button>
            </div>
          </label>
          <label>
            <span>Yandex Folder ID <span class="help" title={help("yandexFolderId")}><CircleHelp size={13} /></span></span>
            <input bind:value={config.yandexFolderId} placeholder="b1g..." />
          </label>
          <label>
            <span>Local bearer token {snapshot.hasLocalKey ? "(saved)" : ""} <span class="help" title={help("localBearer")}><CircleHelp size={13} /></span></span>
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

  :global(:root) {
    --ui-scale: 1;
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
    width: calc(100vw / var(--ui-scale));
    height: calc(100vh / var(--ui-scale));
    display: grid;
    grid-template-columns: 44px minmax(0, 1fr);
    overflow: hidden;
    transform: scale(var(--ui-scale));
    transform-origin: 0 0;
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

  .mark:hover,
  .mark:focus-visible {
    color: #ffffff;
    border-color: #183f3a;
    background: #1e5d55;
    box-shadow: 0 0 0 2px rgba(37, 107, 98, 0.18);
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
    grid-template-columns: minmax(150px, 1.5fr) minmax(120px, 1fr) 30px minmax(120px, 1fr) auto auto;
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

  label > span,
  .group-head {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .group-head {
    justify-content: space-between;
  }

  .help {
    display: inline-flex;
    color: #69777f;
  }

  .combo {
    position: relative;
  }

  .combo-button {
    width: 100%;
    justify-content: space-between;
    background: #ffffff;
  }

  .combo-button span {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .combo-menu {
    position: absolute;
    z-index: 20;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    min-width: 220px;
    padding: 6px;
    display: grid;
    gap: 6px;
    border: 1px solid #b8c4ca;
    border-radius: 8px;
    background: #ffffff;
    box-shadow: 0 12px 30px rgba(24, 32, 38, 0.18);
  }

  .combo-options {
    max-height: 220px;
    display: grid;
    gap: 3px;
    overflow: auto;
  }

  .combo-options button {
    width: 100%;
    min-height: 30px;
    justify-content: space-between;
    border-color: transparent;
    background: transparent;
  }

  .combo-options button:hover,
  .combo-options button.active {
    border-color: #c8d6dc;
    background: #eef5f2;
  }

  .combo-options small {
    color: #69777f;
    font-size: 11px;
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

  .zoom-controls {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .zoom-value {
    min-width: 44px;
    padding: 0 6px;
    font-size: 12px;
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

  .muted {
    margin: 0;
    color: #5f6f77;
    font-size: 13px;
    line-height: 1.4;
  }

  .pill {
    min-height: 24px;
    padding: 3px 8px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    border: 1px solid #cfd6da;
    border-radius: 999px;
    color: #6d4b34;
    background: #fff7ed;
    font-size: 12px;
    font-weight: 700;
  }

  .pill.ok {
    color: #1f6848;
    border-color: #acd7bd;
    background: #effaf3;
  }

  .setup-list {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
  }

  .setup-list span {
    min-height: 26px;
    padding: 4px 8px;
    border: 1px solid #d4b9aa;
    border-radius: 6px;
    color: #7b4a37;
    background: #fff7f2;
    font-size: 12px;
    font-weight: 700;
  }

  .setup-list span.ok {
    color: #1f6848;
    background: #effaf3;
    border-color: #acd7bd;
  }

  details {
    display: grid;
    gap: 10px;
  }

  summary {
    cursor: pointer;
    color: #34454f;
    font-size: 13px;
    font-weight: 800;
  }

  details[open] {
    gap: 12px;
  }

  details[open] label {
    margin-top: 10px;
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
