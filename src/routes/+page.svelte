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
    Cpu,
    Download,
    FolderOpen,
    History,
    Languages,
    Power,
    RefreshCw,
    Repeat2,
    Save,
    Settings,
    Trash2,
    Zap,
    ZoomIn,
    ZoomOut,
  } from "@lucide/svelte";

  type ProviderKind =
    | "open-ai-compatible"
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
    quantization: string;
    size: string;
    homepage: string;
    engineHint: string;
    defaultEndpoint?: string;
    hfRepo?: string;
    installCheckFiles?: string[];
    languages: Language[];
    downloadable: boolean;
  };

  type ModelInstallState = {
    modelId: string;
    status: "missing" | "partial" | "installed";
  };

  type EngineKind = "onnx-encoder-decoder" | "managed-llama-cpp" | "open-ai-compatible" | "network-api";
  type Audience = "beginner" | "high-quality" | "advanced";
  type PromptStyle = "chat" | "completion";

  type LanguageCode = {
    uiCode: string;
    label: string;
    nllbCode?: string;
    onnxMarianPair?: string;
    llmLanguageName?: string;
  };

  type TranslateModel = ModelProfile | ModelCatalogEntry;
  type UiLanguageOption = {
    code: string;
    name: string;
  };

  type ModelFile = {
    repo: string;
    path: string;
    sha256?: string;
    sizeBytes?: number;
    destination: string;
  };

  type ModelCatalogEntry = {
    id: string;
    name: string;
    engine: EngineKind;
    audience: Audience;
    license: string;
    licenseUrl: string;
    homepage: string;
    description: string;
    languages: LanguageCode[];
    actualLanguageCount?: number;
    files: ModelFile[];
    promptStyle?: PromptStyle;
    promptTemplate?: string;
    estimatedDownloadBytes: number;
    estimatedDiskBytes: number;
    minRamBytes?: number;
    downloadable: boolean;
  };

  type AppConfig = {
    modelId: string;
    sourceLang: string;
    targetLang: string;
    historyEnabled: boolean;
    autostart: boolean;
    localModelPolicy: string;
    localModelIdleTimeoutSecs: number;
    openaiEndpoint: string;
    openaiModel: string;
    customBackendMode: string;
    customModelPath: string;
    localLlamaServerPath: string;
    localPromptStyle: string;
    localPromptTemplate: string;
    localContextSize: number;
    apiProviderEnabled: boolean;
    yandexFolderId: string;
    uiLanguage: string;
    theme: string;
    gpuEnabled: boolean;
    vulkanGpuEnabled: boolean;
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
    installedModelIds: string[];
    modelStates: ModelInstallState[];
    history: HistoryEntry[];
    environment: {
      desktop: string;
      sessionType: string;
      hasWlClipboard: boolean;
      hasPython: boolean;
      hasNvidiaSmi: boolean;
      hasRocmSmi: boolean;
      hasLlamaServer: boolean;
      llamaCudaReported: boolean;
      totalMemoryBytes?: number;
    };
    runtime: {
      activeProfiles: {
        profileId: string;
        kind: string;
        device: string;
        idleSeconds: number;
      }[];
      selectedModelLoaded: boolean;
      selectedDevice?: string;
      onnxDevice?: string;
      llamaBinaryFound: boolean;
      llamaCudaReported: boolean;
      llamaVulkanReported: boolean;
      gpuVendor?: string;
      gpuName?: string;
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
      logsDir: string;
    };
  };

  type PendingRequest = {
    mode: "translate" | "settings";
    sourceText: string;
    notice?: string;
  };

  type DownloadProgress = {
    modelId: string;
    status: "starting" | "downloading" | "verifying" | "preparing" | "done" | "cancelled";
    message: string;
    progress: number;
    downloadedBytes: number;
    totalBytes?: number;
  };

  type TranslationProgress = {
    status: "streaming" | "done";
    translatedText: string;
  };

  type UiLang = "en" | "ru" | "sk";

  let snapshot: Snapshot | null = null;
  let config: AppConfig | null = null;
  let tab: "translate" | "settings" | "history" = "translate";
  let sourceText = "";
  let translatedText = "";
  let detectedSourceLang: string | null = null;
  let status = "";
  let error = "";
  let translating = false;
  let testing = false;
  let downloading = false;
  let deeplKey = "";
  let googleKey = "";
  let yandexKey = "";
  let localKey = "";
  let probeResult = "";
  let uiScale = 1;
  let sourceLanguageQuery = "";
  let targetLanguageQuery = "";
  let sourceLanguageOpen = false;
  let targetLanguageOpen = false;
  let downloadState: DownloadProgress | null = null;
  let activeHelp: keyof typeof helpTexts | null = null;
  let helpCloseTimer: number | undefined;
  let uiLang: UiLang = "en";
  let configSaveTimer: number | undefined;
  let configSignature = "";
  let configSaveBusy = false;
  let configSaveQueued = false;
  let modelProfiles: ModelCatalogEntry[] = [];
  let modelStatuses: Record<string, string> = {};
  let runtimeLogName = "llama-server.log";
  let runtimeLogText = "";
  let runtimeLogLoading = false;
  let showAccelInfo = false;
  let gpuBusy = false;
  let gpuProgress = 0;
  let gpuMessage = "";
  let gpuError = "";
  let gpuReadyRestart = false;
  let footerCollapsed = false;
  let recentLangs: string[] = [];
  let unloadingModels = false;

  type SecretProvider = "deepl" | "google" | "yandex" | "openai-compatible";

  $: selectedModel = modelProfiles.find((model) => model.id === config?.modelId)
    ?? snapshot?.catalog.find((model) => model.id === config?.modelId);
  $: languages = normalizedLanguages(selectedModel);
  $: localModelReady = Boolean(selectedModel && (specModelState(selectedModel.id) === "installed" || isModelInstalled(selectedModel.id) || hasInstalledModelFiles()));
  $: curatedModels = modelProfiles;
  $: selectableModels = availableTranslateModels(snapshot, config);
  $: networkProviderIds = new Set(["deepl-api", "google-api", "yandex-api"]);
  $: localSelectableModels = selectableModels.filter((m) => !networkProviderIds.has(m.id));
  $: networkSelectableModels = selectableModels.filter((m) => networkProviderIds.has(m.id));
  // Reactive so the button re-enables the moment translating/testing finishes or a source
  // becomes available — a plain function call in markup would not track these and could get
  // stuck disabled until an unrelated re-render (e.g. switching tabs).
  $: canTranslateNow = !translating && !testing && selectableModels.length > 0;
  $: if (config && selectableModels.length && !selectableModels.find((m) => m.id === config?.modelId)) {
    changeModel(selectableModels[0].id);
  }
  $: uiLang = config?.uiLanguage === "ru" || config?.uiLanguage === "sk" ? config.uiLanguage : "en";
  // Decide which acceleration banner to show on the translate view:
  //   "gpu"        — local translation is actually running on a GPU (success pill)
  //   "cpu-upsell" — running on CPU but NVIDIA GPU exists → offer CUDA speed-up
  //   "cpu-vulkan" — running on CPU with AMD/Intel GPU → offer Vulkan speed-up
  //   null         — nothing actionable (no model loaded yet, or no usable GPU)
  $: accelMode = computeAccelMode(snapshot, config);
  $: if (config) applyTheme(config.theme);
  $: if (config && snapshot) scheduleConfigSave();

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

  const fallbackCatalogLanguages: LanguageCode[] = [
    { uiCode: "auto", label: "Auto detect" },
    { uiCode: "en", label: "English", nllbCode: "eng_Latn", llmLanguageName: "English" },
    { uiCode: "ru", label: "Russian", nllbCode: "rus_Cyrl", llmLanguageName: "Russian" },
    { uiCode: "sk", label: "Slovak", nllbCode: "slk_Latn", llmLanguageName: "Slovak" },
    { uiCode: "de", label: "German", nllbCode: "deu_Latn", llmLanguageName: "German" },
  ];

  const fallbackCatalog: ModelCatalogEntry[] = [
    {
      id: "nllb-200-distilled-600m-onnx",
      name: "NLLB-200 (600M)",
      engine: "onnx-encoder-decoder",
      audience: "beginner",
      license: "CC-BY-NC-4.0",
      licenseUrl: "https://creativecommons.org/licenses/by-nc/4.0/",
      homepage: "https://huggingface.co/facebook/nllb-200-distilled-600M",
      description: "Recommended first local model. Fast, multilingual, and runs natively on CPU.",
      languages: fallbackCatalogLanguages,
      files: [],
      promptStyle: undefined,
      promptTemplate: undefined,
      estimatedDownloadBytes: 600 * 1024 * 1024,
      estimatedDiskBytes: 1200 * 1024 * 1024,
      minRamBytes: 2 * 1024 * 1024 * 1024,
      downloadable: false,
    },
    {
      id: "tencent-hy-mt2-1.8b-gguf",
      name: "Tencent Hy-MT2 1.8B",
      engine: "managed-llama-cpp",
      audience: "beginner",
      license: "Apache 2.0",
      licenseUrl: "https://huggingface.co/tencent/Hy-MT2-1.8B-1.25Bit-GGUF",
      homepage: "https://huggingface.co/tencent/Hy-MT2-1.8B-1.25Bit-GGUF",
      description: "Compact multilingual GGUF model with high quality for its size.",
      languages: fallbackCatalogLanguages,
      files: [],
      promptStyle: "chat",
      promptTemplate: undefined,
      estimatedDownloadBytes: 440 * 1024 * 1024,
      estimatedDiskBytes: 440 * 1024 * 1024,
      minRamBytes: undefined,
      downloadable: true,
    },
  ];

  const builtInCatalogOrder = [
    "nllb-200-distilled-600m-onnx",
    "tencent-hy-mt2-1.8b-gguf",
    "translategemma-4b-gguf",
    "milmmt-46-1b-gguf",
  ] as const;

  const helpTexts = {
    openaiEndpoint: {
      en: "For custom setups only.",
      ru: "Только для своей ручной настройки.",
      sk: "Len pre vlastné ručné nastavenie.",
    },
    openaiModel: {
      en: "Name used by a custom local setup.",
      ru: "Имя модели для ручной настройки.",
      sk: "Názov modelu pre ručné nastavenie.",
    },
    localModelPolicy: {
      en: "Balanced keeps the model warm for a while, Fast preloads it, Memory saver unloads it after each translation.",
      ru: "Стандартно держит модель в памяти ещё некоторое время. Быстрый старт загружает её заранее. Экономия памяти выгружает модель после каждого перевода.",
      sk: "Balanced nechá model chvíľu nahratý, Fast ho prednačíta, Memory saver ho po každom preklade uvoľní.",
    },
    localModelIdleTimeout: {
      en: "How long the warm local runtime stays loaded after the last translation in Balanced mode.",
      ru: "Как долго модель остаётся в памяти после последнего перевода в стандартном режиме.",
      sk: "Ako dlho zostane lokálny runtime po poslednom preklade nahratý v režime Balanced.",
    },
    customBackendMode: {
      en: "External OpenAI-compatible uses your own server. Managed GGUF starts a hidden llama-server process for a local GGUF file.",
      ru: "Внешний OpenAI-compatible использует Ваш готовый сервер. Managed GGUF сам запускает скрытый llama-server для локального GGUF-файла.",
      sk: "External OpenAI-compatible používa Váš vlastný server. Managed GGUF spustí skrytý llama-server pre lokálny GGUF súbor.",
    },
    customModelPath: {
      en: "Path to a local GGUF file when Managed GGUF mode is selected.",
      ru: "Путь к локальному GGUF файлу, если выбран режим Managed GGUF.",
      sk: "Cesta k lokálnemu GGUF súboru pri režime Managed GGUF.",
    },
    localLlamaServerPath: {
      en: "Optional path to a custom llama-server binary. Leave empty to use `llama-server` from PATH.",
      ru: "Необязательный путь к своему `llama-server`. Если пусто, используется `llama-server` из PATH.",
      sk: "Voliteľná cesta k vlastnému `llama-server`. Ak je prázdne, použije sa `llama-server` z PATH.",
    },
    localPromptStyle: {
      en: "Chat fits instruct models. Completion fits raw text continuation models.",
      ru: "Chat подходит instruct-моделям. Completion подходит моделям с продолжением текста.",
      sk: "Chat sa hodí pre instruct modely. Completion pre modely s pokračovaním textu.",
    },
    localPromptTemplate: {
      en: "Available placeholders: {source}, {target}, {text}.",
      ru: "Доступные placeholders: {source}, {target}, {text}.",
      sk: "Dostupné placeholders: {source}, {target}, {text}.",
    },
    localContextSize: {
      en: "Context window passed to llama-server for managed GGUF mode.",
      ru: "Размер контекста, который Waylate передаёт в llama-server для Managed GGUF.",
      sk: "Veľkosť kontextu, ktorú Waylate odovzdá llama-serveru v režime Managed GGUF.",
    },
    history: {
      en: "When enabled, translations are stored locally in SQLite. Nothing is uploaded because of history.",
      ru: "Если включено, переводы сохраняются локально в SQLite. Из-за истории ничего не отправляется в сеть.",
      sk: "Ak je zapnuté, preklady sa ukladajú lokálne do SQLite. História nič neodosiela do siete.",
    },
    autostart: {
      en: "Starts Waylate in the background so the tray and shortcut workflow are ready after login.",
      ru: "Запускает Waylate в фоне, чтобы значок в трее и горячие клавиши были готовы после входа в систему.",
      sk: "Spustí Waylate na pozadí, aby bol tray a workflow so skratkou pripravený po prihlásení.",
    },
    networkApis: {
      en: "Allows cloud providers. Keep this off if you only want local translation.",
      ru: "Разрешает облачные провайдеры. Оставьте выключенным, если Вам нужен только локальный перевод.",
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
      en: "ID of your Yandex Cloud folder.",
      ru: "ID Вашей папки в Yandex Cloud.",
      sk: "ID Vášho priečinka v Yandex Cloud.",
    },
    localBearer: {
      en: "Only needed for a custom local setup.",
      ru: "Нужно только для ручной локальной настройки.",
      sk: "Treba len pre ručné lokálne nastavenie.",
    },
    uiLanguage: {
      en: "Changes the visible interface language immediately and saves it automatically.",
      ru: "Меняет язык интерфейса сразу. Изменение сохраняется автоматически.",
      sk: "Okamžite zmení jazyk rozhrania. Ulož nastavenia, aby zostal aj po reštarte.",
    },
    theme: {
      en: "Switches the interface between light and dark mode and saves it automatically.",
      ru: "Переключает интерфейс между светлой и тёмной темой. Изменение сохраняется автоматически.",
      sk: "Prepína rozhranie medzi svetlým a tmavým režimom. Ulož nastavenia, aby zostal aj po reštarte.",
    },
  } as const;

  const uiTexts = {
    en: {
      translate: "Translate",
      settings: "Settings",
      history: "History",
      model: "Model",
      from: "From",
      to: "To",
      swapLanguages: "Swap languages",
      searchLanguage: "Search language",
      source: "Text",
      translation: "Translation",
      sourcePlaceholder: "Paste text to translate",
      translationPlaceholder: "Translation",
      readSelection: "Use selected text",
      pasteClipboard: "Paste from clipboard",
      copyTranslation: "Copy translation",
      defaultModel: "Default model",
      defaultModelHint: "Loads automatically when Waylate starts. Pick an API provider (DeepL, Google, Yandex) to skip loading a local neural network and save RAM.",
      localModel: "Local model",
      runtimeLoaded: "Loaded",
      runtimeCold: "Not loaded yet",
      loadingModel: "Loading model...",
      ready: "Ready",
      comingSoon: "Coming soon",
      setupNeeded: "Setup needed",
      onboardingTitle: "Local setup",
      onboardingText: "Download a local model once. After that, translation works offline.",
      download: "Download",
      downloading: "Downloading",
      cancel: "Cancel",
      retry: "Retry",
      testBackend: "Test translation",
      modelPath: "Model path",
      tokenizer: "Tokenizer",
      python: "Python",
      device: "Device",
      modelMemory: "Model memory",
      balanced: "Balanced",
      fast: "Fast",
      memorySaver: "Memory saver",
      idleTimeout: "Idle timeout",
      minutesShort: "min",
      advancedLocalBackend: "Advanced",
      customBackendMode: "Custom mode",
      externalOpenAi: "External OpenAI-compatible",
      managedGguf: "Managed GGUF",
      openaiEndpoint: "Custom endpoint",
      modelName: "Model name",
      customModelPath: "GGUF model path",
      llamaServerPath: "llama-server path",
      promptStyle: "Prompt style",
      promptTemplate: "Prompt template",
      contextSize: "Context size",
      chatStyle: "Chat",
      completionStyle: "Completion",
      privacyApis: "Privacy and APIs",
      interfaceLanguage: "Interface language",
      theme: "Theme",
      light: "Light",
      dark: "Dark",
      saveHistory: "Save translation history locally",
      autostart: "Start Waylate in background",
      networkApis: "Allow network API providers",
      apiKeysNote: "DeepL, Google and Yandex need your own key.",
      apiKeysActivationNote: "Save a key and turn on network access below to add that provider to the Model picker.",
      localModelsGroup: "Local models",
      onlineProvidersGroup: "Online (API key)",
      deeplKey: "DeepL API key",
      googleKey: "Google API key",
      yandexKey: "Yandex API key",
      yandexFolderId: "Yandex Folder ID",
      localBearer: "Local bearer token",
      storedSecret: "Enter a new key",
      optionalLocalServer: "Optional",
      system: "System",
      config: "Config",
      models: "Models",
      localHistory: "Local history",
      clear: "Clear",
      historyDisabled: "History is disabled. Enable it in Settings if you want local SQLite history.",
      noHistory: "No saved translations yet.",
      loading: "Loading Waylate...",
      nothingToTranslate: "Nothing to translate.",
      translationCopied: "Translation copied.",
      clipboardError: "Could not copy. Is wl-clipboard installed?",
      settingsSaved: "Settings saved.",
      keySaved: "API key saved.",
      keyRemoved: "API key removed.",
      backendOk: "Translation works.",
      translationReady: "Translation is ready.",
      downloaded: "Ready",
      localRuntimeUnavailable: "The local translator did not respond. Waylate restarted it. Please try again.",
      quantization: "Version",
      size: "Size",
      languages: "Languages",
      modelManager: "Model manager",
      runtime: "Runtime",
      diagnostics: "Diagnostics",
      activeRuntime: "Active runtime",
      none: "None",
      noModelsInstalled: "No translation model is ready yet. Download one in Settings.",
      localModelReadyHint: "This model is ready to translate.",
      localModelMissingHint: "This model is not installed - Download it in Settings.",
      modelInstalledCold: "The model is installed. It will load into memory on the first translation.",
      modelInstalledWarm: "The model is installed and already loaded into memory.",
      modelNeedsDownload: "The model is not installed yet. Download it below.",
      modelNeedsRetry: "The previous download did not finish. Download the model again.",
      builtInModels: "Built-in models",
      runtimeProbe: "Check local translation",
      modelDetails: "Details",
      savedInSystem: "Saved in the system",
      enterNewKey: "Enter a new key",
      clearField: "Clear field",
      partialDownload: "Download again",
      reinstall: "Reinstall",
      deleteModel: "Delete",
      openModelFolder: "Open models folder",
      openConfigFolder: "Open settings folder",
      openLogsFolder: "Open logs folder",
      sampleTranslation: "Sample translation",
      recentLog: "Recent runtime log",
      noRuntimeLog: "No log output yet.",
      modelRamWarning: "This model may be too heavy for this PC.",
      accelOnGpu: "Translating on your graphics card — nice and fast.",
      accelOnCpuTitle: "Translating on your processor",
      accelOnCpuBody: "It works fine, just a little slower.",
      accelOnCpuBodyGpu: "You have a {gpu} — translation can run faster on it.",
      accelLearnMore: "Tell me more",
      accelClose: "Got it",
      accelInfoTitle: "Making translation faster",
      accelInfoBody: "Right now translation always uses your processor. That is reliable and works on every computer — it is just a bit slower.",
      accelInfoBodyGpu: "Your computer has a graphics card ({gpu}). Graphics cards can translate much faster.",
      accelInfoSoon: "A one-click \"Speed it up\" button is on the way in a future update. It will download and switch everything on for you automatically. For now you do not need to do anything — translation keeps working.",
      accelInfoSize: "This is a one-time download of about 4 GB (the graphics runtime plus a faster model). Make sure you are on Wi-Fi. The app will restart by itself when it is ready.",
      accelVulkanInfoSize: "Downloads ~31 MB Vulkan runtime. If you don't have a GGUF model yet, it also downloads Hy-MT2 1.8B (~1.1 GB). No restart needed.",
      accelVulkanWorking: "Setting up Vulkan on your graphics card…",
      accelInfoAction: "Speed it up",
      accelCancel: "Maybe later",
      accelWorking: "Setting up your graphics card…",
      accelNoCancel: "This download can't be cancelled — please let it finish.",
      accelErrorTitle: "Could not turn it on",
      accelRetry: "Try again",
      accelDisable: "Switch back to processor",
      accelStalledTitle: "GPU is on, but translation is still using the processor",
      accelStalledBody: "The graphics runtime could not load, so Waylate fell back to the processor.",
      accelAmdNotAvailable: "GPU acceleration for AMD is not available yet.",
      runningSummaryCpu: "Currently running on: processor",
      runningSummaryGpu: "Currently running on: {gpu}",
      accelReadyRestart: "Graphics acceleration is ready. Restart Waylate to switch to the GPU.",
      accelRestartNow: "Restart now",
      keySave: "Save",
      modelsUnloaded: "Models unloaded from memory.",
      unloadModel: "Unload model from memory",
      recentLanguages: "Recently used",
      apiKeysStorageNote: "Keys are stored only in your system keychain on this computer — never in the app files or online.",
      footerToggle: "Show/hide details",
    },
    ru: {
      translate: "Перевести",
      settings: "Настройки",
      history: "История",
      model: "Модель",
      from: "С",
      to: "На",
      swapLanguages: "Поменять языки",
      searchLanguage: "Поиск языка",
      source: "Текст",
      translation: "Перевод",
      sourcePlaceholder: "Вставьте текст для перевода",
      translationPlaceholder: "Перевод",
      readSelection: "Взять выделенный текст",
      pasteClipboard: "Вставить из буфера",
      copyTranslation: "Скопировать перевод",
      defaultModel: "Модель по умолчанию",
      defaultModelHint: "Загружается автоматически при запуске Waylate. Выберите API-провайдера (DeepL, Google, Yandex), чтобы не грузить локальную нейросеть и экономить ОЗУ.",
      localModel: "Локальная модель",
      runtimeLoaded: "Загружена",
      runtimeCold: "Еще не загружена",
      loadingModel: "Загрузка модели...",
      ready: "Готово",
      comingSoon: "Скоро",
      setupNeeded: "Нужна настройка",
      onboardingTitle: "Локальная настройка",
      onboardingText: "Скачайте локальную модель один раз. После этого перевод будет работать офлайн.",
      download: "Скачать",
      downloading: "Скачивается",
      cancel: "Отмена",
      retry: "Повторить",
      testBackend: "Проверить перевод",
      modelPath: "Путь модели",
      tokenizer: "Токенизатор",
      python: "Python",
      device: "Устройство",
      modelMemory: "Память модели",
      balanced: "Стандартно",
      fast: "Быстрый старт",
      memorySaver: "Экономия памяти",
      idleTimeout: "Тайм-аут",
      minutesShort: "мин",
      advancedLocalBackend: "Дополнительно",
      customBackendMode: "Режим подключения",
      externalOpenAi: "Внешний OpenAI-compatible",
      managedGguf: "Managed GGUF",
      openaiEndpoint: "Свой endpoint",
      modelName: "Имя модели",
      customModelPath: "Путь к GGUF модели",
      llamaServerPath: "Путь к llama-server",
      promptStyle: "Стиль prompt",
      promptTemplate: "Шаблон prompt",
      contextSize: "Размер контекста",
      chatStyle: "Chat",
      completionStyle: "Completion",
      privacyApis: "Приватность и API",
      interfaceLanguage: "Язык интерфейса",
      theme: "Тема",
      light: "Светлая",
      dark: "Тёмная",
      saveHistory: "Сохранять историю переводов локально",
      autostart: "Запускать Waylate в фоне",
      networkApis: "Разрешить сетевые API-провайдеры",
      apiKeysNote: "Для DeepL, Google и Yandex нужен Ваш ключ.",
      apiKeysActivationNote: "Сохраните ключ и включите сетевой доступ ниже — провайдер появится в выборе модели.",
      localModelsGroup: "Локальные модели",
      onlineProvidersGroup: "Онлайн (по API-ключу)",
      deeplKey: "DeepL API key",
      googleKey: "Google API key",
      yandexKey: "Yandex API key",
      yandexFolderId: "Yandex Folder ID",
      localBearer: "Local bearer token",
      storedSecret: "Введите новый ключ",
      optionalLocalServer: "Необязательно",
      system: "Система",
      config: "Конфиг",
      models: "Модели",
      localHistory: "Локальная история",
      clear: "Очистить",
      historyDisabled: "История выключена. Включите её в настройках, если нужна локальная SQLite-история.",
      noHistory: "Сохранённых переводов пока нет.",
      loading: "Загрузка Waylate...",
      nothingToTranslate: "Нечего переводить.",
      translationCopied: "Перевод скопирован.",
      clipboardError: "Не удалось скопировать. Установлен ли wl-clipboard?",
      settingsSaved: "Настройки сохранены.",
      keySaved: "API-ключ сохранён.",
      keyRemoved: "API-ключ удалён.",
      backendOk: "Перевод работает.",
      translationReady: "Перевод готов.",
      downloaded: "Готово",
      localRuntimeUnavailable: "Локальный переводчик не ответил. Waylate перезапустил его. Пожалуйста, попробуйте ещё раз.",
      quantization: "Версия",
      size: "Размер",
      languages: "Языки",
      modelManager: "Менеджер моделей",
      runtime: "Runtime",
      diagnostics: "Диагностика",
      activeRuntime: "Активный runtime",
      none: "Нет",
      noModelsInstalled: "Пока нет готовых моделей для перевода. Скачайте модель в настройках.",
      localModelReadyHint: "Эта модель готова к переводу.",
      localModelMissingHint: "Эта модель не установлена. Скачайте её в настройках.",
      modelInstalledCold: "Модель установлена. Она загрузится в память при первом переводе.",
      modelInstalledWarm: "Модель установлена и уже загружена в память.",
      modelNeedsDownload: "Модель ещё не установлена. Скачайте её ниже.",
      modelNeedsRetry: "Прошлое скачивание не завершилось. Скачайте модель ещё раз.",
      builtInModels: "Встроенные модели",
      runtimeProbe: "Проверить перевод",
      modelDetails: "Подробности",
      savedInSystem: "Сохранён в системе",
      enterNewKey: "Введите новый ключ",
      clearField: "Очистить поле",
      partialDownload: "Скачать заново",
      reinstall: "Переустановить",
      deleteModel: "Удалить",
      openModelFolder: "Открыть папку моделей",
      openConfigFolder: "Открыть папку настроек",
      openLogsFolder: "Открыть папку логов",
      sampleTranslation: "Пробный перевод",
      recentLog: "Последний лог runtime",
      noRuntimeLog: "Логов пока нет.",
      modelRamWarning: "Эта модель может быть слишком тяжёлой для этого ПК.",
      accelOnGpu: "Перевод работает на видеокарте — это быстро.",
      accelOnCpuTitle: "Перевод работает на процессоре",
      accelOnCpuBody: "Всё работает, просто чуть медленнее.",
      accelOnCpuBodyGpu: "У тебя есть {gpu} — на ней перевод может стать быстрее.",
      accelLearnMore: "Подробнее",
      accelClose: "Понятно",
      accelInfoTitle: "Как ускорить перевод",
      accelInfoBody: "Сейчас перевод всегда работает на процессоре. Это надёжно и работает на любом компьютере — просто чуть медленнее.",
      accelInfoBodyGpu: "У твоего компьютера есть видеокарта ({gpu}). Видеокарты умеют переводить намного быстрее.",
      accelInfoSoon: "Кнопка «Ускорить» в одно нажатие скоро появится в обновлении. Она сама всё скачает и включит. Пока делать ничего не нужно — перевод и так работает.",
      accelInfoSize: "Это разовая загрузка примерно на 4 ГБ (графический рантайм и более быстрая модель). Лучше быть на Wi-Fi. Когда всё будет готово, приложение перезапустится само.",
      accelVulkanInfoSize: "Скачает ~31 МБ Vulkan-рантайма. Если GGUF-модели ещё нет — также скачает Hy-MT2 1.8B (~1,1 ГБ). Перезапуск не нужен.",
      accelVulkanWorking: "Настраиваю Vulkan на видеокарте…",
      accelInfoAction: "Ускорить",
      accelCancel: "Не сейчас",
      accelWorking: "Настраиваю видеокарту…",
      accelNoCancel: "Эту загрузку нельзя отменить — дождитесь завершения.",
      accelErrorTitle: "Не получилось включить",
      accelRetry: "Попробовать снова",
      accelDisable: "Вернуться на процессор",
      accelStalledTitle: "Видеокарта включена, но перевод всё ещё на процессоре",
      accelStalledBody: "Графический runtime не загрузился, поэтому Waylate вернулся на процессор.",
      accelAmdNotAvailable: "Ускорение для AMD пока недоступно.",
      runningSummaryCpu: "Сейчас работает на: процессоре",
      runningSummaryGpu: "Сейчас работает на: {gpu}",
      accelReadyRestart: "Ускорение готово. Перезапустите Waylate, чтобы включить видеокарту.",
      accelRestartNow: "Перезапустить",
      keySave: "Сохранить",
      modelsUnloaded: "Модели выгружены из памяти.",
      unloadModel: "Выгрузить модель из памяти",
      recentLanguages: "Недавние",
      apiKeysStorageNote: "Ключи хранятся только в системном хранилище паролей на этом компьютере — не в файлах приложения и не в сети.",
      footerToggle: "Показать/скрыть детали",
    },
    sk: {
      translate: "Preložiť",
      settings: "Nastavenia",
      history: "História",
      model: "Model",
      from: "Z",
      to: "Do",
      swapLanguages: "Vymeniť jazyky",
      searchLanguage: "Hľadať jazyk",
      source: "Text",
      translation: "Preklad",
      sourcePlaceholder: "Vlož text na preklad",
      translationPlaceholder: "Preklad",
      readSelection: "Použiť vybraný text",
      pasteClipboard: "Vložiť zo schránky",
      copyTranslation: "Kopírovať preklad",
      defaultModel: "Predvolený model",
      defaultModelHint: "Automaticky sa načíta pri spustení Waylate. Vyberte API poskytovateľa (DeepL, Google, Yandex), aby sa nenačítala lokálna neurónová sieť a ušetrila sa RAM.",
      localModel: "Lokálny model",
      runtimeLoaded: "Nahraté",
      runtimeCold: "Ešte nie je nahratý",
      loadingModel: "Načítava sa model...",
      ready: "Pripravené",
      comingSoon: "Už čoskoro",
      setupNeeded: "Treba nastaviť",
      onboardingTitle: "Lokálne nastavenie",
      onboardingText: "Stiahnite lokálny model raz. Potom bude preklad fungovať offline.",
      download: "Stiahnuť",
      downloading: "Sťahuje sa",
      cancel: "Zrušiť",
      retry: "Skúsiť znova",
      testBackend: "Otestovať preklad",
      modelPath: "Cesta modelu",
      tokenizer: "Tokenizer",
      python: "Python",
      device: "Zariadenie",
      modelMemory: "Pamäť modelu",
      balanced: "Balanced",
      fast: "Fast",
      memorySaver: "Memory saver",
      idleTimeout: "Timeout",
      minutesShort: "min",
      advancedLocalBackend: "Pokročilé",
      customBackendMode: "Custom režim",
      externalOpenAi: "External OpenAI-compatible",
      managedGguf: "Managed GGUF",
      openaiEndpoint: "Vlastný endpoint",
      modelName: "Názov modelu",
      customModelPath: "Cesta ku GGUF modelu",
      llamaServerPath: "Cesta k llama-server",
      promptStyle: "Štýl promptu",
      promptTemplate: "Šablóna promptu",
      contextSize: "Veľkosť kontextu",
      chatStyle: "Chat",
      completionStyle: "Completion",
      privacyApis: "Súkromie a API",
      interfaceLanguage: "Jazyk rozhrania",
      theme: "Téma",
      light: "Svetlá",
      dark: "Tmavá",
      saveHistory: "Ukladať históriu prekladov lokálne",
      autostart: "Spúšťať Waylate na pozadí",
      networkApis: "Povoliť sieťových API providerov",
      apiKeysNote: "DeepL, Google a Yandex potrebujú Váš kľúč.",
      apiKeysActivationNote: "Uložte kľúč a nižšie zapnite sieťový prístup — poskytovateľ sa pridá do výberu modelu.",
      localModelsGroup: "Lokálne modely",
      onlineProvidersGroup: "Online (API kľúč)",
      deeplKey: "DeepL API key",
      googleKey: "Google API key",
      yandexKey: "Yandex API key",
      yandexFolderId: "Yandex Folder ID",
      localBearer: "Local bearer token",
      storedSecret: "Zadajte nový kľúč",
      optionalLocalServer: "Voliteľné",
      system: "Systém",
      config: "Konfig",
      models: "Modely",
      localHistory: "Lokálna história",
      clear: "Vymazať",
      historyDisabled: "História je vypnutá. Zapni ju v nastaveniach, ak chceš lokálnu SQLite históriu.",
      noHistory: "Zatiaľ nie sú uložené žiadne preklady.",
      loading: "Načítava sa Waylate...",
      nothingToTranslate: "Nie je čo preložiť.",
      translationCopied: "Preklad skopírovaný.",
      clipboardError: "Nepodarilo sa skopírovať. Je nainštalovaný wl-clipboard?",
      settingsSaved: "Nastavenia uložené.",
      keySaved: "API kľúč uložený.",
      keyRemoved: "API kľúč odstránený.",
      backendOk: "Preklad funguje.",
      translationReady: "Preklad je pripravený.",
      downloaded: "Pripravené",
      localRuntimeUnavailable: "Lokálny prekladač neodpovedal. Waylate ho reštartoval. Skúste to prosím ešte raz.",
      quantization: "Verzia",
      size: "Veľkosť",
      languages: "Jazyky",
      modelManager: "Správca modelov",
      runtime: "Runtime",
      diagnostics: "Diagnostika",
      activeRuntime: "Aktívny runtime",
      none: "Žiadny",
      noModelsInstalled: "Zatiaľ nie je pripravený žiadny prekladový model. Stiahnite model v nastaveniach.",
      localModelReadyHint: "Tento model je pripravený na preklad.",
      localModelMissingHint: "Tento model nie je nainštalovaný. Stiahnite ho v nastaveniach.",
      modelInstalledCold: "Model je nainštalovaný. Do pamäte sa načíta pri prvom preklade.",
      modelInstalledWarm: "Model je nainštalovaný a už je načítaný v pamäti.",
      modelNeedsDownload: "Model ešte nie je nainštalovaný. Stiahnite ho nižšie.",
      modelNeedsRetry: "Predchádzajúce sťahovanie sa nedokončilo. Stiahnite model znova.",
      builtInModels: "Vstavané modely",
      runtimeProbe: "Skontrolovať lokálny preklad",
      modelDetails: "Podrobnosti",
      savedInSystem: "Uložené v systéme",
      enterNewKey: "Zadajte nový kľúč",
      clearField: "Vymazať pole",
      partialDownload: "Stiahnuť znova",
      reinstall: "Nainštalovať znova",
      deleteModel: "Odstrániť",
      openModelFolder: "Otvoriť priečinok modelov",
      openConfigFolder: "Otvoriť priečinok nastavení",
      openLogsFolder: "Otvoriť priečinok logov",
      sampleTranslation: "Skúšobný preklad",
      recentLog: "Posledný runtime log",
      noRuntimeLog: "Zatiaľ nie sú žiadne logy.",
      modelRamWarning: "Tento model môže byť pre tento počítač príliš ťažký.",
      accelOnGpu: "Preklad beží na grafickej karte — pekne rýchlo.",
      accelOnCpuTitle: "Preklad beží na procesore",
      accelOnCpuBody: "Funguje to dobre, len o čosi pomalšie.",
      accelOnCpuBodyGpu: "Máš {gpu} — preklad na nej môže byť rýchlejší.",
      accelLearnMore: "Viac info",
      accelClose: "Rozumiem",
      accelInfoTitle: "Ako zrýchliť preklad",
      accelInfoBody: "Preklad teraz vždy používa procesor. Je to spoľahlivé a funguje na každom počítači — len o čosi pomalšie.",
      accelInfoBodyGpu: "Tvoj počítač má grafickú kartu ({gpu}). Grafické karty vedia prekladať oveľa rýchlejšie.",
      accelInfoSoon: "Tlačidlo „Zrýchliť“ na jedno kliknutie čoskoro pribudne v aktualizácii. Všetko stiahne a zapne za teba. Zatiaľ nemusíš robiť nič — preklad funguje ďalej.",
      accelInfoSize: "Je to jednorazové stiahnutie približne 4 GB (grafický runtime a rýchlejší model). Najlepšie cez Wi-Fi. Keď bude všetko pripravené, aplikácia sa sama reštartuje.",
      accelVulkanInfoSize: "Stiahne ~31 MB Vulkan runtime. Ak ešte nemáš GGUF model, stiahne aj Hy-MT2 1.8B (~1,1 GB). Reštart nie je potrebný.",
      accelVulkanWorking: "Nastavujem Vulkan na grafickej karte…",
      accelInfoAction: "Zrýchliť",
      accelCancel: "Možno neskôr",
      accelWorking: "Nastavujem grafickú kartu…",
      accelNoCancel: "Toto sťahovanie sa nedá zrušiť — počkajte, kým sa dokončí.",
      accelErrorTitle: "Nepodarilo sa zapnúť",
      accelRetry: "Skúsiť znova",
      accelDisable: "Späť na procesor",
      accelStalledTitle: "Grafická karta je zapnutá, ale preklad stále beží na procesore",
      accelStalledBody: "Grafický runtime sa nepodarilo načítať, preto sa Waylate vrátil na procesor.",
      accelAmdNotAvailable: "Akcelerácia pre AMD zatiaľ nie je dostupná.",
      runningSummaryCpu: "Momentálne beží na: procesore",
      runningSummaryGpu: "Momentálne beží na: {gpu}",
      accelReadyRestart: "Zrýchlenie je pripravené. Reštartujte Waylate na prepnutie na GPU.",
      accelRestartNow: "Reštartovať",
      keySave: "Uložiť",
      modelsUnloaded: "Modely uvoľnené z pamäte.",
      unloadModel: "Uvoľniť model z pamäte",
      recentLanguages: "Nedávne",
      apiKeysStorageNote: "Kľúče sú uložené iba v systémovom úložisku hesiel v tomto počítači — nie v súboroch aplikácie ani online.",
      footerToggle: "Zobraziť/skryť detaily",
    },
  } as const;

  onMount(() => {
    let unlisten: (() => void) | undefined;
    let unlistenDownload: (() => void) | undefined;
    let unlistenTranslation: (() => void) | undefined;
    const savedScale = Number(localStorage.getItem("waylate-ui-scale"));
    setUiScale(Number.isFinite(savedScale) && savedScale >= 1 ? savedScale : 1);
    try {
      const savedRecent = JSON.parse(localStorage.getItem("waylate-recent-langs") ?? "[]");
      if (Array.isArray(savedRecent)) recentLangs = savedRecent.filter((c) => typeof c === "string").slice(0, 5);
    } catch {
      recentLangs = [];
    }
    footerCollapsed = localStorage.getItem("waylate-footer-collapsed") === "1";
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
    const handleDocumentClick = (event: MouseEvent) => {
      if ((event.target as Element | null)?.closest(".combo")) return;
      sourceLanguageOpen = false;
      targetLanguageOpen = false;
    };
    window.addEventListener("keydown", handleKeydown);
    window.addEventListener("wheel", handleWheel, { passive: false });
    document.addEventListener("click", handleDocumentClick);
    void (async () => {
      await refresh();
      await consumePending();
      unlisten = await listen("waylate-pending", consumePending);
      unlistenDownload = await listen<DownloadProgress>("model-download-progress", (event) => {
        if (event.payload.modelId === "gpu-runtime" || event.payload.modelId === "vulkan-runtime") {
          gpuProgress = event.payload.progress;
          gpuMessage = event.payload.message;
          return;
        }
        downloadState = event.payload;
      });
      unlistenTranslation = await listen<TranslationProgress>("translation-progress", (event) => {
        if (!translating) return;
        translatedText = event.payload.translatedText;
      });
    })();
    return () => {
      unlisten?.();
      unlistenDownload?.();
      unlistenTranslation?.();
      window.removeEventListener("keydown", handleKeydown);
      window.removeEventListener("wheel", handleWheel);
      document.removeEventListener("click", handleDocumentClick);
    };
  });

  function setUiScale(next: number) {
    uiScale = Math.min(1.8, Math.max(0.75, Math.round(next * 10) / 10));
    document.documentElement.style.setProperty("--ui-scale", String(uiScale));
    localStorage.setItem("waylate-ui-scale", String(uiScale));
  }

  async function refresh() {
    snapshot = await invoke<Snapshot>("get_snapshot");
    modelProfiles = await invoke<ModelCatalogEntry[]>("list_model_profiles");
    modelStatuses = Object.fromEntries(
      await Promise.all(
        modelProfiles.map(async (model) => [model.id, installStateKind(await invoke("get_model_status", { profileId: model.id }))]),
      ),
    );
    config = structuredClone(snapshot.config);
    configSignature = configStateSignature(snapshot.config);
    const available = availableTranslateModels(snapshot, config);
    if (config && available.length && !available.some((model) => model.id === config?.modelId)) {
      changeModel(available[0].id);
    }
  }

  async function loadRuntimeLog(name: string) {
    runtimeLogLoading = true;
    runtimeLogName = name;
    try {
      runtimeLogText = await invoke<string>("read_runtime_log", { name });
    } catch (err) {
      runtimeLogText = String(err);
    } finally {
      runtimeLogLoading = false;
    }
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
    probeResult = "";
    translatedText = "";
    if (!sourceText.trim()) {
      error = t("nothingToTranslate");
      return;
    }
    if (selectedModel && isLocalProfile(config.modelId) && !isModelInstalled(selectedModel.id) && specModelState(selectedModel.id) !== "installed") {
      error = t("localModelMissingHint");
      return;
    }
    if (isLocalProfile(config.modelId)) {
      status = snapshot?.runtime.selectedModelLoaded ? t("runtimeLoaded") : t("loadingModel");
    }
    translating = true;
    try {
      const response = await invoke<{
        translatedText: string;
        providerLabel: string;
        warning?: string;
        detectedSourceLang?: string | null;
      }>(
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
      status = response.warning ?? t("translationReady");
      detectedSourceLang = config.sourceLang === "auto" ? response.detectedSourceLang ?? null : null;
      await refresh();
    } catch (err) {
      error = friendlyErrorMessage(err);
    } finally {
      translating = false;
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
    try {
      await invoke("write_clipboard_text", { text: translatedText });
      status = t("translationCopied");
    } catch (err) {
      error = t("clipboardError");
    }
  }

  async function swapLanguages() {
    if (!config) return;
    // "auto" itself can't become a target language, so swap using the language DeepL/Google/
    // Yandex actually detected for the last translation instead.
    const currentSource = config.sourceLang === "auto" ? detectedSourceLang : config.sourceLang;
    if (!currentSource) return;
    const nextSource = config.targetLang;
    config.targetLang = currentSource;
    config.sourceLang = nextSource;
    detectedSourceLang = null;
    if (translatedText.trim()) {
      sourceText = translatedText;
      translatedText = "";
      await translate();
    }
  }

  function changeModel(modelId: string) {
    if (!config || !snapshot) return;
    detectedSourceLang = null;
    const nextModel = modelProfiles.find((model) => model.id === modelId)
      ?? snapshot.catalog.find((model) => model.id === modelId);
    if (!nextModel) return;
    config.modelId = modelId;
    const nextLanguages = normalizedLanguages(nextModel);
    config.sourceLang = closestLanguage(config.sourceLang, nextLanguages, true);
    config.targetLang = closestLanguage(config.targetLang, nextLanguages, false);
    // Online providers (DeepL/Google/Yandex) auto-detect the source, so default to "auto"
    // and let the user translate without picking a source language every time.
    if (networkProviderIds.has(modelId) && nextLanguages.some((l) => l.code === "auto")) {
      config.sourceLang = "auto";
    }
  }

  function closestLanguage(current: string, nextLanguages: UiLanguageOption[], allowAuto: boolean) {
    const available = new Set(nextLanguages.map((language) => language.code));
    const aliases = languageAliases[current] ?? [current];
    const match = aliases.find((code) => available.has(code));
    if (match) return match;
    if (allowAuto && available.has("auto")) return "auto";
    return nextLanguages.find((language) => language.code !== "auto")?.code ?? current;
  }

  async function saveKey(provider: SecretProvider, value: string) {
    error = "";
    const trimmed = value.trim();
    if (!trimmed) return;
    try {
      await invoke("save_api_key", { provider, key: trimmed });
      if (provider === "deepl") deeplKey = "";
      if (provider === "google") googleKey = "";
      if (provider === "yandex") yandexKey = "";
      if (provider === "openai-compatible") localKey = "";
      // Saving a network key without enabling the providers would leave it unused, so turn
      // the network providers on automatically — that is clearly what the user intended.
      if (config && provider !== "openai-compatible" && !config.apiProviderEnabled) {
        config.apiProviderEnabled = true;
        await persistConfig(false);
      }
      await refresh();
      status = t("keySaved");
    } catch (err) {
      error = String(err);
    }
  }

  async function clearKey(provider: SecretProvider) {
    error = "";
    try {
      await invoke("clear_api_key", { provider });
      if (provider === "deepl") deeplKey = "";
      if (provider === "google") googleKey = "";
      if (provider === "yandex") yandexKey = "";
      if (provider === "openai-compatible") localKey = "";
      await refresh();
      status = t("keyRemoved");
    } catch (err) {
      error = String(err);
    }
  }

  async function unloadModels() {
    if (unloadingModels) return;
    unloadingModels = true;
    error = "";
    try {
      await invoke("unload_models");
      await refresh();
      status = t("modelsUnloaded");
    } catch (err) {
      error = String(err);
    } finally {
      unloadingModels = false;
    }
  }

  function toggleFooter() {
    footerCollapsed = !footerCollapsed;
    if (typeof localStorage !== "undefined") {
      localStorage.setItem("waylate-footer-collapsed", footerCollapsed ? "1" : "0");
    }
  }

  async function downloadModel(modelId: string) {
    if (!config) return;
    downloading = true;
    error = "";
    status = "";
    probeResult = "";
    await persistConfig(false);
    downloadState = {
      modelId,
      status: "starting",
      message: "Starting download",
      progress: 0.02,
      downloadedBytes: 0,
    };
    try {
      const path = await invoke<string>("install_model", { profileId: modelId });
      await refresh();
      downloadState = {
        modelId,
        status: "done",
        message: t("downloaded"),
        progress: 1,
        downloadedBytes: downloadState?.downloadedBytes ?? 0,
        totalBytes: downloadState?.totalBytes,
      };
      status = `${t("downloaded")}: ${path}`;
    } catch (err) {
      error = friendlyErrorMessage(err);
    } finally {
      downloading = false;
    }
  }

  async function removeModel(modelId: string): Promise<boolean> {
    downloading = true;
    error = "";
    status = "";
    probeResult = "";
    try {
      await invoke("uninstall_model", { profileId: modelId });
      if (downloadState?.modelId === modelId) {
        downloadState = null;
      }
      // Optimistic update so the card switches to Download immediately
      modelStatuses = { ...modelStatuses, [modelId]: "notInstalled" };
      await refresh();
      status = t("settingsSaved");
      return true;
    } catch (err) {
      error = friendlyErrorMessage(err);
      return false;
    } finally {
      downloading = false;
    }
  }

  async function reinstallModel(modelId: string) {
    // Use removeModel's own result instead of reading the shared `error`, which another
    // event handler could change between the two awaits (TOCTOU).
    const removed = await removeModel(modelId);
    if (!removed) return;
    await downloadModel(modelId);
  }

  async function revealLogsDir() {
    if (!snapshot) return;
    await invoke("reveal_path", { path: snapshot.paths.logsDir });
  }

  async function cancelDownload() {
    if (downloadState?.modelId) {
      await invoke("cancel_model_install", { profileId: downloadState.modelId });
      return;
    }
    await invoke("cancel_model_download");
  }

  async function testBackend() {
    if (!config) return;
    error = "";
    status = "";
    probeResult = "";
    testing = true;
    try {
      await persistConfig(false);
      const response = await invoke<{ translatedText: string }>("translate_text", {
        request: {
          text: "Hello",
          sourceLang:
            config.sourceLang === "auto"
              ? modelUsesNllbCodes(selectedModel)
                ? "eng_Latn"
                : "en"
              : config.sourceLang,
          targetLang: config.targetLang,
          modelId: config.modelId,
        },
      });
      status = t("backendOk");
      probeResult = response.translatedText;
      await refresh();
    } catch (err) {
      error = friendlyErrorMessage(err);
    } finally {
      testing = false;
    }
  }

  async function clearLocalHistory() {
    await invoke("clear_history");
    await refresh();
  }

  async function deleteHistoryEntry(id: number) {
    await invoke("delete_history_entry", { id });
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

  async function selectLanguage(kind: "source" | "target", code: string) {
    if (!config) return;
    if (kind === "source") {
      config.sourceLang = code;
      detectedSourceLang = null;
      sourceLanguageQuery = "";
      sourceLanguageOpen = false;
    } else {
      config.targetLang = code;
      targetLanguageQuery = "";
      targetLanguageOpen = false;
    }
    pushRecentLang(code);
    if (sourceText.trim()) {
      await translate();
    }
  }

  // Remember the last 5 distinct languages the user picked (excluding auto-detect), most
  // recent first, persisted locally so they surface at the top of the language menus.
  function pushRecentLang(code: string) {
    if (!code || code === "auto") return;
    recentLangs = [code, ...recentLangs.filter((c) => c !== code)].slice(0, 5);
    if (typeof localStorage !== "undefined") {
      localStorage.setItem("waylate-recent-langs", JSON.stringify(recentLangs));
    }
  }

  function recentLanguageOptions(includeAuto: boolean): UiLanguageOption[] {
    return recentLangs
      .map((code) => languages.find((l) => l.code === code))
      .filter((l): l is UiLanguageOption => Boolean(l) && (includeAuto || l!.code !== "auto"));
  }

  function languageLabel(code: string) {
    return displayLanguageName(code, languages);
  }

  function languageSearchText(language: UiLanguageOption) {
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

  function normalizedLanguages(model: TranslateModel | undefined | null): UiLanguageOption[] {
    if (!model) return [];
    if ("provider" in model) {
      return model.languages.map((language) => ({ code: language.code, name: language.name }));
    }
    const useNllbCodes = modelUsesNllbCodes(model);
    return model.languages.map((language) => ({
      code: useNllbCodes ? language.nllbCode ?? language.uiCode : language.uiCode,
      name: language.label,
    }));
  }

  function modelProvider(model: TranslateModel | undefined | null): ProviderKind | null {
    if (!model) return null;
    if ("provider" in model) return model.provider;
    if (model.engine === "onnx-encoder-decoder") return "custom";
    if (model.engine === "managed-llama-cpp") return "custom";
    if (model.engine === "open-ai-compatible") return "open-ai-compatible";
    return null;
  }

  function modelUsesNllbCodes(model: TranslateModel | undefined | null) {
    if (!model) return false;
    if ("provider" in model) return false;
    return model.engine === "onnx-encoder-decoder" && model.id.startsWith("nllb-");
  }

  function isLocalProfile(modelId: string) {
    const spec = modelProfiles.find(m => m.id === modelId);
    if (spec) return true; // ONNX and GGUF are both local

    const legacy = snapshot?.catalog.find((item) => item.id === modelId);
    if (legacy?.provider === "custom") {
      return legacy.id !== "custom-local" || config?.customBackendMode === "managed-gguf";
    }
    return false;
  }

  function isCurrentModelReady() {
    return Boolean(selectedModel && (isModelInstalled(selectedModel.id) || hasInstalledModelFiles()));
  }

  function isModelInstalled(modelId: string) {
    return modelState(modelId) === "installed";
  }

  function modelState(modelId?: string) {
    if (!modelId) return "missing";
    return snapshot?.modelStates.find((item) => item.modelId === modelId)?.status ?? "missing";
  }

  function specModelState(modelId?: string) {
    if (!modelId) return "missing";
    const status = modelStatuses[modelId];
    if (status === "ready") return "installed";
    if (status === "failed") return "partial";
    return "missing";
  }

  function installStateKind(raw: unknown) {
    if (typeof raw === "string") return raw;
    if (raw && typeof raw === "object") {
      const [key] = Object.keys(raw);
      return key ?? "notInstalled";
    }
    return "notInstalled";
  }

  function hasInstalledModelFiles() {
    if (!selectedModel) return false;
    if (!("provider" in selectedModel)) {
      return specModelState(selectedModel.id) === "installed" || isModelInstalled(selectedModel.id);
    }
    if (modelProvider(selectedModel) === "custom" && selectedModel.id !== "custom-local") {
      return isModelInstalled(selectedModel.id);
    }
    if (modelProvider(selectedModel) === "custom" && config?.customBackendMode === "managed-gguf") {
      return Boolean(config.customModelPath);
    }
    // external-openai: we can't know if a server is running, don't show as installed
    return false;
  }

  function hasTokenizerReady() {
    if (!selectedModel) return false;
    if (!("provider" in selectedModel)) {
      return specModelState(selectedModel.id) === "installed" || isModelInstalled(selectedModel.id);
    }
    if (modelProvider(selectedModel) === "custom") {
      return hasInstalledModelFiles();
    }
    return false;
  }

  function needsPythonRuntime() {
    return false;
  }

  function modelReadinessSummary() {
    if (!selectedModel) return t("modelNeedsDownload");
    if (modelState(selectedModel.id) === "partial") {
      return t("modelNeedsRetry");
    }
    if (isModelInstalled(selectedModel.id)) {
      return snapshot?.runtime.selectedModelLoaded ? t("modelInstalledWarm") : t("modelInstalledCold");
    }
    return t("modelNeedsDownload");
  }

  function localCatalogModels() {
    return builtInCatalogOrder
      .map((id) => modelProfiles.find((model) => model.id === id) ?? fallbackCatalog.find((model) => model.id === id))
      .filter((model): model is ModelCatalogEntry => Boolean(model));
  }

  // Network providers become selectable translation sources only once the user has
  // saved their key and enabled network access — i.e. driven "by API key", not by install.
  function enabledNetworkProviders(currentSnapshot = snapshot, currentConfig = config) {
    if (!currentSnapshot || !currentConfig?.apiProviderEnabled) return [];
    const byId = (id: string) => currentSnapshot.catalog.find((m) => m.id === id);
    const out: ModelProfile[] = [];
    if (currentSnapshot.hasDeeplKey) { const m = byId("deepl-api"); if (m) out.push(m); }
    if (currentSnapshot.hasGoogleKey) { const m = byId("google-api"); if (m) out.push(m); }
    if (currentSnapshot.hasYandexKey && currentConfig.yandexFolderId?.trim()) {
      const m = byId("yandex-api"); if (m) out.push(m);
    }
    return out;
  }

  function availableTranslateModels(currentSnapshot = snapshot, currentConfig = config) {
    if (!currentSnapshot) return [];
    const network = enabledNetworkProviders(currentSnapshot, currentConfig);

    // Prioritize spec models from modelProfiles
    const specModels = modelProfiles.filter(m => specModelState(m.id) === "installed");
    if (specModels.length) return [...specModels, ...network];

    // Fallback to legacy installed models
    const installed = new Set(currentSnapshot.installedModelIds);
    const legacyCatalog = currentSnapshot.catalog;
    const readyLegacy = legacyCatalog.filter(m => installed.has(m.id));
    if (readyLegacy.length) return [...readyLegacy, ...network];

    // Nothing installed locally — still expose network providers and the current selection.
    const local: TranslateModel[] = [];
    if (currentConfig) {
      const currentSpec = modelProfiles.find(m => m.id === currentConfig.modelId);
      if (currentSpec) {
        local.push(currentSpec);
      } else {
        const currentLegacy = legacyCatalog.find(m => m.id === currentConfig.modelId);
        if (currentLegacy && !network.some(n => n.id === currentLegacy.id)) local.push(currentLegacy);
      }
    }
    return [...local, ...network];
  }

  function configStateSignature(next: AppConfig) {
    return JSON.stringify(next);
  }

  function scheduleConfigSave() {
    if (typeof window === "undefined") return;
    if (!config) return;
    const nextSignature = configStateSignature(config);
    if (nextSignature === configSignature) {
      window.clearTimeout(configSaveTimer);
      return;
    }
    window.clearTimeout(configSaveTimer);
    configSaveTimer = window.setTimeout(() => {
      void persistConfig(false);
    }, 250);
  }

  async function persistConfig(showSavedStatus: boolean) {
    if (!config) return;
    const nextSignature = configStateSignature(config);
    if (nextSignature === configSignature && !showSavedStatus) return;
    if (configSaveBusy) {
      configSaveQueued = true;
      await new Promise((resolve) => window.setTimeout(resolve, 50));
      await persistConfig(showSavedStatus);
      return;
    }
    configSaveBusy = true;
    error = "";
    try {
      const saved = await invoke<AppConfig>("save_config", { next: config });
      config = saved;
      configSignature = configStateSignature(saved);
      if (snapshot) {
        snapshot = {
          ...snapshot,
          config: structuredClone(saved),
        };
      }
      if (showSavedStatus) {
        status = t("settingsSaved");
      }
    } catch (err) {
      error = String(err);
    } finally {
      configSaveBusy = false;
      if (configSaveQueued) {
        configSaveQueued = false;
        await persistConfig(false);
      }
    }
  }

  function help(key: keyof typeof helpTexts) {
    return helpTexts[key][uiLang];
  }

  function t(key: keyof typeof uiTexts.en) {
    return uiTexts[uiLang][key];
  }

  function computeAccelMode(
    current = snapshot,
    currentConfig = config,
  ): "gpu" | "cpu-upsell" | "cpu-vulkan" | "gpu-stalled" | null {
    if (!current) return null;
    // Prefer the local translator's device; fall back to any active GGUF runtime.
    const device = current.runtime.onnxDevice ?? current.runtime.selectedDevice;
    if (device && device !== "cpu") return "gpu";
    const vendor = current.runtime.gpuVendor;
    // GPU was enabled but we are still on CPU — the bundle failed to load. Be honest about it
    // instead of showing the same "speed me up" upsell that just restarted into a fallback.
    if (currentConfig?.gpuEnabled && device === "cpu") return "gpu-stalled";
    if (device === "cpu" && vendor === "nvidia") return "cpu-upsell";
    if (device === "cpu" && (vendor === "amd" || vendor === "intel")) return "cpu-vulkan";
    return null;
  }

  function accelGpuName(): string {
    return snapshot?.runtime.gpuName ?? snapshot?.runtime.gpuVendor?.toUpperCase() ?? "GPU";
  }

  // Like t(), but substitutes the detected GPU name into a "{gpu}" placeholder.
  function accelText(key: keyof typeof uiTexts.en): string {
    return t(key).replace("{gpu}", accelGpuName());
  }

  // Download the GPU runtime + fp16 model. The backend used to restart the app itself, which
  // looked like a crash (window vanished). Now it just downloads, and we ask the user to
  // restart when it is ready so the change is intentional and visible.
  async function enableGpu() {
    if (gpuBusy) return;
    gpuBusy = true;
    gpuError = "";
    gpuProgress = 0;
    gpuMessage = "";
    gpuReadyRestart = false;
    try {
      await invoke("enable_gpu_acceleration");
      showAccelInfo = false;
      gpuReadyRestart = true;
    } catch (err) {
      gpuError = String(err);
    } finally {
      gpuBusy = false;
    }
  }

  async function restartApp() {
    try {
      await invoke("restart_app");
    } catch (err) {
      error = String(err);
    }
  }

  async function disableGpu() {
    try {
      await invoke("disable_gpu_acceleration");
    } catch (err) {
      gpuError = String(err);
    }
  }

  async function enableVulkan() {
    if (gpuBusy) return;
    gpuBusy = true;
    gpuError = "";
    gpuProgress = 0;
    gpuMessage = "";
    gpuReadyRestart = false;
    try {
      await invoke("enable_vulkan_acceleration");
      showAccelInfo = false;
      // No restart needed — llama-server picks the Vulkan binary on next translation.
      await refresh();
    } catch (err) {
      gpuError = String(err);
    } finally {
      gpuBusy = false;
    }
  }

  async function disableVulkan() {
    try {
      await invoke("disable_vulkan_acceleration");
      await refresh();
    } catch (err) {
      gpuError = String(err);
    }
  }

  function applyTheme(theme: string) {
    if (typeof document === "undefined") return;
    document.documentElement.dataset.theme = theme === "dark" ? "dark" : "light";
  }

  function showHelp(key: keyof typeof helpTexts) {
    window.clearTimeout(helpCloseTimer);
    activeHelp = key;
  }

  function scheduleHelpClose() {
    window.clearTimeout(helpCloseTimer);
    helpCloseTimer = window.setTimeout(() => {
      activeHelp = null;
    }, 120);
  }

  function toggleHelp(event: MouseEvent, key: keyof typeof helpTexts) {
    event.stopPropagation();
    window.clearTimeout(helpCloseTimer);
    activeHelp = activeHelp === key ? null : key;
  }

  function formatBytes(bytes?: number) {
    if (!bytes) return "";
    const units = ["B", "KB", "MB", "GB"];
    let value = bytes;
    let unit = 0;
    while (value >= 1024 && unit < units.length - 1) {
      value /= 1024;
      unit += 1;
    }
    return `${value >= 10 || unit === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
  }

  function modelDownloadSize(model: ModelCatalogEntry) {
    return formatBytes(model.estimatedDownloadBytes);
  }

  function modelAudienceLabel(model: ModelCatalogEntry) {
    if (model.audience === "beginner") {
      if (uiLang === "ru") return "Рекомендуемая";
      if (uiLang === "sk") return "Odporucane";
      return "Recommended";
    }
    if (model.audience === "high-quality") {
      if (uiLang === "ru") return "Качественная";
      if (uiLang === "sk") return "Vyssia kvalita";
      return "High quality";
    }
    if (uiLang === "ru") return "Для профи";
    if (uiLang === "sk") return "Pre pokrocilych";
    return "Pro";
  }

  function specModelDetail(model: ModelCatalogEntry) {
    const parts = [modelAudienceLabel(model), model.license];
    if (model.id === "translategemma-4b-gguf") {
      parts.push(uiLang === "ru" ? "требуется принятие лицензии Gemma" : uiLang === "sk" ? "vyžaduje prijatie licencie Gemma" : "Gemma license acceptance required");
    }
    const warning = modelRamWarning(model);
    if (warning) parts.push(warning);
    return parts.filter(Boolean).join(" · ");
  }

  function friendlyErrorMessage(err: unknown) {
    const message = String(err);
    if (
      message.includes("Warm local runtime could not translate text")
      || message.includes("Could not start warm local runtime")
      || message.includes("Warm local runtime did not become ready in time")
    ) {
      return t("localRuntimeUnavailable");
    }
    if (message.includes("Could not reach local OpenAI-compatible server")) {
      return uiLang === "ru"
        ? "Локальный сервер не отвечает. Запустите сервер (например, llama-server или Ollama) вручную и убедитесь что адрес endpoint в настройках совпадает."
        : "Local server is not responding. Start your server (e.g. llama-server or Ollama) and make sure the endpoint in Settings matches.";
    }
    return message;
  }

  function modelRamWarning(model: ModelCatalogEntry) {
    const total = snapshot?.environment.totalMemoryBytes;
    if (!total || !model.minRamBytes || model.minRamBytes <= total) return "";
    return t("modelRamWarning");
  }

  function displayLanguageName(
    code: string,
    languageList: UiLanguageOption[] = [
      ...(snapshot?.catalog.flatMap((item) => normalizedLanguages(item)) ?? []),
      ...modelProfiles.flatMap((item) => normalizedLanguages(item)),
    ],
  ) {
    return languageList.find((language) => language.code === code)?.name ?? code;
  }

  function historyLanguageLabel(code: string) {
    return displayLanguageName(code);
  }

  function keyStateHint(hasKey: boolean) {
    return hasKey ? t("savedInSystem") : "";
  }

  function modelPillLabel() {
    if (!selectedModel) return t("download");
    if (modelState(selectedModel.id) === "partial") return t("partialDownload");
    if (isModelInstalled(selectedModel.id) || hasInstalledModelFiles()) return t("ready");
    return t("download");
  }

  function modelSummary(model: ModelProfile | ModelCatalogEntry) {
    const summaries: Record<string, { en: string; ru: string; sk: string }> = {
      "nllb-200-distilled-600m-onnx": {
        en: "Recommended broad-coverage local model. Downloads once and then works offline.",
        ru: "Рекомендуемая модель для старта. Быстрая, сотни языков, работает на процессоре и видеокарте.",
        sk: "Odporúčaný lokálny model so širokým pokrytím. Stiahne sa raz a potom funguje offline.",
      },
      "nllb-200-distilled-1.3b-onnx": {
        en: "Higher-quality NLLB variant. Same 200 languages, better translation but slower and needs ~4 GB RAM.",
        ru: "Та же поддержка 200 языков, но качество выше. Медленнее, нужно ~4 ГБ ОЗУ.",
        sk: "Kvalitnejší variant NLLB. Rovnakých 200 jazykov, lepší preklad, ale pomalší a vyžaduje ~4 GB RAM.",
      },
      "opus-mt-marian-onnx": {
        en: "Lightweight models for specific popular language pairs.",
        ru: "Лёгкие модели для популярных языковых пар.",
        sk: "Odľahčené modely pre konkrétne populárne jazykové páry.",
      },
      "tencent-hy-mt2-1.8b-gguf": {
        en: "Compact high-quality local GGUF translation model. 131 languages.",
        ru: "Компактная GGUF-модель высокого качества. 131 язык.",
        sk: "Kompaktný lokálny GGUF model vysokej kvality. 131 jazykov.",
      },
      "translategemma-4b-gguf": {
        en: "High-quality model for machines with 8 GB+ RAM.",
        ru: "Качественная GGUF-модель от Google для мощных ПК (нужно ~8 ГБ ОЗУ).",
        sk: "Kvalitný model pre počítače s 8 GB+ RAM.",
      },
      "milmmt-46-1b-gguf": {
        en: "Small multilingual translation model. Needs GGUF conversion before download is enabled.",
        ru: "Небольшая многоязычная GGUF-модель, включая словацкий.",
        sk: "Malý viacjazykový prekladový model vrátane slovenčiny.",
      },
      "deepl-api": {
        en: "Network translation provider. Disabled by default; needs your own API key.",
        ru: "Облачный переводчик DeepL. Отключён по умолчанию — нужен собственный API-ключ.",
        sk: "Sieťový poskytovateľ prekladu. Predvolene vypnutý; vyžaduje vlastný API kľúč.",
      },
      "google-api": {
        en: "Network translation provider. Disabled by default; needs your own API key.",
        ru: "Google Cloud Translate. Отключён по умолчанию — нужен собственный API-ключ.",
        sk: "Sieťový poskytovateľ prekladu. Predvolene vypnutý; vyžaduje vlastný API kľúč.",
      },
      "yandex-api": {
        en: "Network translation provider. Disabled by default; needs your own API key and folder ID.",
        ru: "Yandex Cloud Translate. Отключён по умолчанию — нужен API-ключ и ID папки.",
        sk: "Sieťový poskytovateľ prekladu. Predvolene vypnutý; vyžaduje API kľúč a ID priečinka.",
      },
      "custom-local": {
        en: "Advanced profile for your own GGUF model or OpenAI-compatible local endpoint.",
        ru: "Расширенный профиль: своя GGUF-модель или OpenAI-совместимый локальный сервер.",
        sk: "Pokročilý profil pre vlastný GGUF model alebo lokálny OpenAI-kompatibilný endpoint.",
      },
    };
    return summaries[model.id]?.[uiLang] ?? model.description;
  }

  function modelDetail(model: ModelProfile | ModelCatalogEntry) {
    if (uiLang !== "ru") return ("engineHint" in model ? model.engineHint : "");
    const details: Record<string, string> = {
      "nllb-200-distilled-600m-onnx": "Запускается через встроенный ONNX Runtime. Максимальная стабильность и скорость.",
      "tencent-hy-mt2-1.8b-gguf": "Запускается через встроенный llama-server. Требует GGUF файл.",
      "milmmt-46-1b-gguf": "Запускается через встроенный llama-server. Требует GGUF файл.",
    };
    return details[model.id] ?? ("engineHint" in model ? model.engineHint : "");
  }

</script>

<svelte:head>
  <title>Waylate</title>
</svelte:head>

<svelte:window on:keydown={(event) => { if (event.key === "Escape" && showAccelInfo) showAccelInfo = false; }} />

<main class="shell">
  <aside class="rail">
    <button class="mark" title={t("translate")} aria-label={t("translate")} on:click={() => (tab = "translate")}>W</button>
    <nav aria-label="Views">
      <button class:active={tab === "translate"} title={t("translate")} aria-label={t("translate")} on:click={() => (tab = "translate")}>
        <Languages size={14} />
      </button>
      <button class:active={tab === "settings"} title={t("settings")} aria-label={t("settings")} on:click={() => (tab = "settings")}>
        <Settings size={14} />
      </button>
      <button class:active={tab === "history"} title={t("history")} aria-label={t("history")} on:click={() => (tab = "history")}>
        <History size={14} />
      </button>
    </nav>
  </aside>

  <section class="workspace">
    {#if config && snapshot}
      {#if tab === "translate"}
        <section class="translate-view">
          <section class="toolbar" aria-label="Translation options">
            <label>
              <span class="visually-hidden">{t("model")}</span>
              {#if selectableModels.length}
                <select value={config.modelId} on:change={(event) => changeModel(event.currentTarget.value)}>
                  {#if networkSelectableModels.length}
                    <optgroup label={t("localModelsGroup")}>
                      {#each localSelectableModels as model}
                        <option value={model.id}>{model.name}</option>
                      {/each}
                    </optgroup>
                    <optgroup label={t("onlineProvidersGroup")}>
                      {#each networkSelectableModels as model}
                        <option value={model.id}>{model.name}</option>
                      {/each}
                    </optgroup>
                  {:else}
                    {#each selectableModels as model}
                      <option value={model.id}>{model.name}</option>
                    {/each}
                  {/if}
                </select>
              {:else}
                <select disabled>
                  <option>{t("noModelsInstalled")}</option>
                </select>
              {/if}
            </label>
            <label class="combo-label">
              <span class="visually-hidden">{t("from")}</span>
              <div class="combo">
                <button type="button" class="combo-button" on:click={() => (sourceLanguageOpen = !sourceLanguageOpen)}>
                  <span>
                    {languageLabel(config.sourceLang)}{config.sourceLang === "auto" && detectedSourceLang ? ` · ${languageLabel(detectedSourceLang)}` : ""}
                  </span>
                  <ChevronDown size={14} />
                </button>
                {#if sourceLanguageOpen}
                  <div class="combo-menu">
                    <input bind:value={sourceLanguageQuery} placeholder={t("searchLanguage")} />
                    <div class="combo-options">
                      {#if !sourceLanguageQuery.trim() && recentLanguageOptions(true).length}
                        <span class="combo-group">{t("recentLanguages")}</span>
                        {#each recentLanguageOptions(true) as language}
                          <button type="button" class:active={language.code === config.sourceLang} on:click={() => selectLanguage("source", language.code)}>
                            <span>{language.name}</span>
                          </button>
                        {/each}
                        <span class="combo-divider"></span>
                      {/if}
                      {#each filteredLanguages(sourceLanguageQuery, true) as language}
                        <button type="button" class:active={language.code === config.sourceLang} on:click={() => selectLanguage("source", language.code)}>
                          <span>{language.name}</span>
                        </button>
                      {/each}
                    </div>
                  </div>
                {/if}
              </div>
            </label>
            <button class="icon" title={t("swapLanguages")} on:click={swapLanguages} disabled={config.sourceLang === "auto" && !detectedSourceLang}>
              <Repeat2 size={15} />
            </button>
            <label class="combo-label">
              <span class="visually-hidden">{t("to")}</span>
              <div class="combo">
                <button type="button" class="combo-button" on:click={() => (targetLanguageOpen = !targetLanguageOpen)}>
                  <span>{languageLabel(config.targetLang)}</span>
                  <ChevronDown size={14} />
                </button>
                {#if targetLanguageOpen}
                  <div class="combo-menu">
                    <input bind:value={targetLanguageQuery} placeholder={t("searchLanguage")} />
                    <div class="combo-options">
                      {#if !targetLanguageQuery.trim() && recentLanguageOptions(false).length}
                        <span class="combo-group">{t("recentLanguages")}</span>
                        {#each recentLanguageOptions(false) as language}
                          <button type="button" class:active={language.code === config.targetLang} on:click={() => selectLanguage("target", language.code)}>
                            <span>{language.name}</span>
                          </button>
                        {/each}
                        <span class="combo-divider"></span>
                      {/if}
                      {#each filteredLanguages(targetLanguageQuery, false) as language}
                        <button type="button" class:active={language.code === config.targetLang} on:click={() => selectLanguage("target", language.code)}>
                          <span>{language.name}</span>
                        </button>
                      {/each}
                    </div>
                  </div>
                {/if}
              </div>
            </label>
            <button class="primary run" on:click={translate} disabled={!canTranslateNow}>
              <span class:spin={translating}><RefreshCw size={15} /></span>
              <span class="run-label">{t("translate")}</span>
            </button>
            <button class="icon" title={t("unloadModel")} aria-label={t("unloadModel")} on:click={unloadModels} disabled={unloadingModels}>
              <Power size={15} />
            </button>
            <div class="zoom-controls" aria-label="Interface zoom">
              <button class="icon small" title="Zoom out" aria-label="Zoom out" on:click={() => setUiScale(uiScale - 0.1)}><ZoomOut size={13} /></button>
              <button class="zoom-value" title="Reset zoom" on:click={() => setUiScale(1)}>{Math.round(uiScale * 100)}%</button>
              <button class="icon small" title="Zoom in" aria-label="Zoom in" on:click={() => setUiScale(uiScale + 0.1)}><ZoomIn size={13} /></button>
            </div>
          </section>

          <section class="translate-grid">
            <div class="pane">
              <textarea aria-label={t("source")} bind:value={sourceText} spellcheck="false" placeholder={t("sourcePlaceholder")}></textarea>
              <div class="pane-actions">
                <button class="icon small" title={t("readSelection")} aria-label={t("readSelection")} on:click={pasteSelection}>
                  <Languages size={13} />
                </button>
                <button class="icon small" title={t("pasteClipboard")} aria-label={t("pasteClipboard")} on:click={pasteClipboard}>
                  <Clipboard size={13} />
                </button>
              </div>
            </div>
            <div class="pane">
              <textarea aria-label={t("translation")} bind:value={translatedText} spellcheck="false" readonly placeholder={t("translationPlaceholder")}></textarea>
              <div class="pane-actions end">
                <button class="icon small" title={t("copyTranslation")} aria-label={t("copyTranslation")} on:click={copyTranslation} disabled={!translatedText.trim()}>
                  <Copy size={13} />
                </button>
              </div>
            </div>
          </section>

          <section class="translate-footer">
            <div class="footer-bar" class:minimal={footerCollapsed && !error}>
              {#if error}
                <p class="inline-note error-note">{error}</p>
              {:else if !footerCollapsed}
                <p class="inline-note">{status || (selectableModels.length && selectedModel ? selectedModel.name : "")}</p>
              {:else}
                <span class="inline-note"></span>
              {/if}
              <button type="button" class="icon small footer-toggle" class:open={!footerCollapsed} title={t("footerToggle")} aria-label={t("footerToggle")} on:click={toggleFooter}>
                <ChevronDown size={14} />
              </button>
            </div>
            {#if gpuReadyRestart}
              <aside class="accel-banner fast">
                <Zap size={15} />
                <span class="accel-message">{t("accelReadyRestart")}</span>
                <button type="button" class="accel-cta" on:click={restartApp}>{t("accelRestartNow")}</button>
              </aside>
            {/if}
            {#if !footerCollapsed}
              <section class="model-note">
                {#if selectableModels.length && selectedModel}
                  <strong>{selectedModel.name}</strong>
                  <span>{modelReadinessSummary()}</span>
                {:else}
                  <strong>{t("onboardingTitle")}</strong>
                  <span>{t("noModelsInstalled")}</span>
                {/if}
              </section>
              {#if accelMode === "gpu"}
                <aside class="accel-banner fast">
                  <Zap size={15} />
                  <span class="accel-message">{t("accelOnGpu")}</span>
                  <button type="button" class="accel-cta" on:click={snapshot?.runtime.gpuVendor === "amd" || snapshot?.runtime.gpuVendor === "intel" ? disableVulkan : disableGpu}>{t("accelDisable")}</button>
                </aside>
              {:else if accelMode === "gpu-stalled"}
                <aside class="accel-banner">
                  <Cpu size={15} />
                  <div class="accel-message">
                    <strong>{t("accelStalledTitle")}</strong>
                    <span>{t("accelStalledBody")}</span>
                  </div>
                  <button type="button" class="accel-cta" on:click={disableGpu}>{t("accelDisable")}</button>
                </aside>
              {:else if accelMode === "cpu-upsell"}
                <aside class="accel-banner">
                  <Cpu size={15} />
                  <div class="accel-message">
                    <strong>{t("accelOnCpuTitle")}</strong>
                    <span>{accelText("accelOnCpuBodyGpu")}</span>
                  </div>
                  <button type="button" class="accel-cta" on:click={() => (showAccelInfo = true)}>{t("accelLearnMore")}</button>
                </aside>
              {:else if accelMode === "cpu-vulkan"}
                <aside class="accel-banner">
                  <Cpu size={15} />
                  <div class="accel-message">
                    <strong>{t("accelOnCpuTitle")}</strong>
                    <span>{accelText("accelOnCpuBodyGpu")}</span>
                  </div>
                  <button type="button" class="accel-cta" on:click={() => (showAccelInfo = true)}>{t("accelLearnMore")}</button>
                </aside>
              {/if}
            {/if}
          </section>
        </section>
      {:else if tab === "settings"}
        <section class="settings-grid">
          <div class="group">
            <div class="group-head">
              <h2>{t("defaultModel")}</h2>
            </div>
            <p class="muted">{t("defaultModelHint")}</p>
            {#if selectableModels.length}
              <select value={config.modelId} on:change={(event) => changeModel(event.currentTarget.value)}>
                {#if networkSelectableModels.length}
                  <optgroup label={t("localModelsGroup")}>
                    {#each localSelectableModels as model}
                      <option value={model.id}>{model.name}</option>
                    {/each}
                  </optgroup>
                  <optgroup label={t("onlineProvidersGroup")}>
                    {#each networkSelectableModels as model}
                      <option value={model.id}>{model.name}</option>
                    {/each}
                  </optgroup>
                {:else}
                  {#each selectableModels as model}
                    <option value={model.id}>{model.name}</option>
                  {/each}
                {/if}
              </select>
            {:else}
              <select disabled>
                <option>{t("noModelsInstalled")}</option>
              </select>
            {/if}
            <div class="group-head">
              <h2>{t("localModel")}</h2>
              <span class="pill" class:ok={localModelReady}><CheckCircle2 size={13} /> {modelPillLabel()}</span>
            </div>
            <p class="muted">{modelReadinessSummary()}</p>
            {#if snapshot}
              {@const device = snapshot.runtime.onnxDevice ?? snapshot.runtime.selectedDevice}
              {#if device}
                <p class="running-on-line">{device !== "cpu" ? accelText("runningSummaryGpu") : t("runningSummaryCpu")}</p>
              {/if}
            {/if}
            <h3>{t("builtInModels")}</h3>
            <div class="model-manager">
              {#if curatedModels.length}
              {#each curatedModels as model}
                <article class:active={config.modelId === model.id} class="model-card">
                  <div class="model-card-head">
                    <strong>{model.name}</strong>
                    <span>{modelDownloadSize(model)}</span>
                  </div>
                  <p>{modelSummary(model)}</p>
                  <p class="detail">{specModelDetail(model)}</p>
                  <dl>
                    <div><dt>{t("size")}</dt><dd>{modelDownloadSize(model)}</dd></div>
                    <div><dt>{t("languages")}</dt><dd>{model.actualLanguageCount ?? model.languages.length}</dd></div>
                    <div><dt>{t("modelDetails")}</dt><dd>{modelAudienceLabel(model)}</dd></div>
                  </dl>
                  {#if modelRamWarning(model)}
                    <p class="inline-note">{modelRamWarning(model)}</p>
                  {/if}
                  {#if downloadState?.modelId === model.id && downloadState.status !== "done" && downloadState.status !== "cancelled"}
                    <div class="download-progress">
                      <div class="progress-meta">
                        <span>{downloadState.status === "verifying" ? (uiLang === "ru" ? "Проверка" : "Verifying") : downloadState.status === "preparing" ? downloadState.message : t("downloading")}: {formatBytes(downloadState.totalBytes ? Math.min(downloadState.downloadedBytes, downloadState.totalBytes) : downloadState.downloadedBytes)}{downloadState.totalBytes ? ` / ${formatBytes(downloadState.totalBytes)}` : ""}</span>
                        <span>{Math.round(downloadState.progress * 100)}%</span>
                      </div>
                      <progress max="1" value={downloadState.progress}></progress>
                      <button on:click={cancelDownload}>{t("cancel")}</button>
                    </div>
                  {:else if specModelState(model.id) === "installed"}
                    <div class="model-card-actions">
                      <button class="primary" on:click={() => reinstallModel(model.id)} disabled={downloading || testing || translating}>
                        <RefreshCw size={16} /> {t("reinstall")}
                      </button>
                      <button on:click={() => removeModel(model.id)} disabled={downloading || testing || translating}>
                        <Trash2 size={16} /> {t("deleteModel")}
                      </button>
                    </div>
                  {:else if specModelState(model.id) === "partial"}
                    <div class="model-card-actions">
                      <button class="primary" on:click={() => downloadModel(model.id)} disabled={downloading || testing || translating}>
                        <Download size={16} /> {t("partialDownload")}
                      </button>
                      <button on:click={() => removeModel(model.id)} disabled={downloading || testing || translating}>
                        <Trash2 size={16} /> {t("deleteModel")}
                      </button>
                    </div>
                  {:else if !model.downloadable}
                    <button disabled>
                      {t("comingSoon")}
                    </button>
                  {:else}
                    <button class="primary" on:click={() => downloadModel(model.id)} disabled={downloading || testing || translating}>
                      <Download size={16} /> {downloadState?.modelId === model.id && error ? t("retry") : t("download")}
                    </button>
                  {/if}
                </article>
              {/each}
              {/if}
            </div>
            <label>
              <span>{t("modelMemory")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "localModelPolicy")} on:mouseenter={() => showHelp("localModelPolicy")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "localModelPolicy"}<span class="help-popover">{help("localModelPolicy")}</span>{/if}</button></span>
              <select bind:value={config.localModelPolicy}>
                <option value="balanced">{t("balanced")}</option>
                <option value="fast">{t("fast")}</option>
                <option value="memory-saver">{t("memorySaver")}</option>
              </select>
            </label>
            <label>
              <span>{t("idleTimeout")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "localModelIdleTimeout")} on:mouseenter={() => showHelp("localModelIdleTimeout")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "localModelIdleTimeout"}<span class="help-popover">{help("localModelIdleTimeout")}</span>{/if}</button></span>
              <div class="range-row">
                <input type="range" min="60" max="3600" step="60" bind:value={config.localModelIdleTimeoutSecs} />
                <span class="range-value">{Math.max(1, Math.round(config.localModelIdleTimeoutSecs / 60))} {t("minutesShort")}</span>
              </div>
            </label>
            <div class="settings-actions">
              <button on:click={testBackend} disabled={testing || downloading || translating || !hasInstalledModelFiles()}><span class:spin={testing}><RefreshCw size={16} /></span> {t("runtimeProbe")}</button>
              <button on:click={revealModelsDir}><FolderOpen size={16} /> {t("openModelFolder")}</button>
              <button on:click={revealConfigDir}><FolderOpen size={16} /> {t("openConfigFolder")}</button>
              <button on:click={revealLogsDir}><FolderOpen size={16} /> {t("openLogsFolder")}</button>
            </div>
            {#if probeResult}
              <div class="probe-result">
                <strong>{t("sampleTranslation")}</strong>
                <p>{probeResult}</p>
              </div>
            {:else if status || error}
              <p class:error-note={Boolean(error)} class="inline-note">{error || status}</p>
            {/if}
            <details class="diagnostics-card">
              <summary>{t("diagnostics")}</summary>
              <dl class="device-info">
                {#if snapshot.runtime.onnxDevice}
                  <div>
                    <dt>{uiLang === "ru" ? "ONNX устройство" : uiLang === "sk" ? "ONNX zariadenie" : "ONNX device"}</dt>
                    <dd class:device-gpu={snapshot.runtime.onnxDevice !== "cpu"}>{snapshot.runtime.onnxDevice.toUpperCase()}</dd>
                  </div>
                {/if}
                {#if snapshot.runtime.selectedDevice}
                  <div>
                    <dt>{uiLang === "ru" ? "GGUF устройство" : uiLang === "sk" ? "GGUF zariadenie" : "GGUF device"}</dt>
                    <dd class:device-gpu={snapshot.runtime.selectedDevice !== "cpu"}>{snapshot.runtime.selectedDevice.toUpperCase()}</dd>
                  </div>
                {/if}
                {#if !snapshot.runtime.onnxDevice && !snapshot.runtime.selectedDevice}
                  <div><dt>{uiLang === "ru" ? "Устройство" : "Device"}</dt><dd>{uiLang === "ru" ? "Модель ещё не загружена" : uiLang === "sk" ? "Model ešte nie je načítaný" : "Model not loaded yet"}</dd></div>
                {/if}
                {#if snapshot.runtime.gpuName}
                  <div>
                    <dt>{uiLang === "ru" ? "Видеокарта" : uiLang === "sk" ? "Grafická karta" : "Graphics card"}</dt>
                    <dd>{snapshot.runtime.gpuName}</dd>
                  </div>
                {/if}
              </dl>
              <div class="settings-actions">
                <button on:click={() => loadRuntimeLog("llama-server.log")} disabled={runtimeLogLoading}>llama-server.log</button>
              </div>
              <strong>{t("recentLog")}: {runtimeLogName}</strong>
              <pre class="runtime-log">{runtimeLogText || t("noRuntimeLog")}</pre>
            </details>
            <details>
              <summary>{t("advancedLocalBackend")}</summary>
              {#if config.customBackendMode === "external-openai"}
                <p class="muted custom-local-hint">
                  {uiLang === "ru"
                    ? "Запустите любой локальный OpenAI-совместимый сервер (Ollama, llama-server, LM Studio и др.) и укажите его адрес ниже. Пример: ollama serve → http://localhost:11434/v1/chat/completions"
                    : "Start any local OpenAI-compatible server (Ollama, llama-server, LM Studio, etc.) and enter its address below. Example: ollama serve → http://localhost:11434/v1/chat/completions"}
                </p>
              {/if}
              <label>
                <span>{t("customBackendMode")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "customBackendMode")} on:mouseenter={() => showHelp("customBackendMode")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "customBackendMode"}<span class="help-popover">{help("customBackendMode")}</span>{/if}</button></span>
                <select bind:value={config.customBackendMode}>
                  <option value="external-openai">{t("externalOpenAi")}</option>
                  <option value="managed-gguf">{t("managedGguf")}</option>
                </select>
              </label>
              <label>
                <span>{t("openaiEndpoint")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "openaiEndpoint")} on:mouseenter={() => showHelp("openaiEndpoint")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "openaiEndpoint"}<span class="help-popover">{help("openaiEndpoint")}</span>{/if}</button></span>
                <input bind:value={config.openaiEndpoint} placeholder="http://127.0.0.1:8080/v1/chat/completions" />
              </label>
              <label>
                <span>{t("modelName")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "openaiModel")} on:mouseenter={() => showHelp("openaiModel")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "openaiModel"}<span class="help-popover">{help("openaiModel")}</span>{/if}</button></span>
                <input bind:value={config.openaiModel} placeholder="local-translation-model" />
              </label>
              <label>
                <span>{t("customModelPath")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "customModelPath")} on:mouseenter={() => showHelp("customModelPath")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "customModelPath"}<span class="help-popover">{help("customModelPath")}</span>{/if}</button></span>
                <input bind:value={config.customModelPath} placeholder="/home/user/models/custom.gguf" />
              </label>
              <label>
                <span>{t("llamaServerPath")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "localLlamaServerPath")} on:mouseenter={() => showHelp("localLlamaServerPath")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "localLlamaServerPath"}<span class="help-popover">{help("localLlamaServerPath")}</span>{/if}</button></span>
                <input bind:value={config.localLlamaServerPath} placeholder="llama-server" />
              </label>
              <label>
                <span>{t("promptStyle")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "localPromptStyle")} on:mouseenter={() => showHelp("localPromptStyle")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "localPromptStyle"}<span class="help-popover">{help("localPromptStyle")}</span>{/if}</button></span>
                <select bind:value={config.localPromptStyle}>
                  <option value="chat">{t("chatStyle")}</option>
                  <option value="completion">{t("completionStyle")}</option>
                </select>
              </label>
              <label>
                <span>{t("contextSize")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "localContextSize")} on:mouseenter={() => showHelp("localContextSize")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "localContextSize"}<span class="help-popover">{help("localContextSize")}</span>{/if}</button></span>
                <input bind:value={config.localContextSize} type="number" min="512" step="256" />
              </label>
              <label>
                <span>{t("promptTemplate")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "localPromptTemplate")} on:mouseenter={() => showHelp("localPromptTemplate")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "localPromptTemplate"}<span class="help-popover">{help("localPromptTemplate")}</span>{/if}</button></span>
                <textarea class="prompt-template" bind:value={config.localPromptTemplate} rows="4"></textarea>
              </label>
            </details>
          </div>

          <div class="group">
            <h2>{t("privacyApis")}</h2>
            <label>
              <span>{t("interfaceLanguage")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "uiLanguage")} on:mouseenter={() => showHelp("uiLanguage")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "uiLanguage"}<span class="help-popover">{help("uiLanguage")}</span>{/if}</button></span>
              <select bind:value={config.uiLanguage}>
                <option value="en">English</option>
                <option value="ru">Русский</option>
                <option value="sk">Slovenčina</option>
              </select>
            </label>
            <label>
              <span>{t("theme")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "theme")} on:mouseenter={() => showHelp("theme")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "theme"}<span class="help-popover">{help("theme")}</span>{/if}</button></span>
              <select bind:value={config.theme}>
                <option value="light">{t("light")}</option>
                <option value="dark">{t("dark")}</option>
              </select>
            </label>
            <label class="check">
              <input type="checkbox" bind:checked={config.historyEnabled} />
              <span>{t("saveHistory")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "history")} on:mouseenter={() => showHelp("history")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "history"}<span class="help-popover">{help("history")}</span>{/if}</button></span>
            </label>
            <label class="check">
              <input type="checkbox" bind:checked={config.autostart} />
              <span>{t("autostart")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "autostart")} on:mouseenter={() => showHelp("autostart")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "autostart"}<span class="help-popover">{help("autostart")}</span>{/if}</button></span>
            </label>
            <label class="check">
              <input type="checkbox" bind:checked={config.apiProviderEnabled} />
              <span>{t("networkApis")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "networkApis")} on:mouseenter={() => showHelp("networkApis")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "networkApis"}<span class="help-popover">{help("networkApis")}</span>{/if}</button></span>
            </label>
            <p class="muted">{t("apiKeysNote")}</p>
            <p class="muted">{t("apiKeysActivationNote")}</p>
            <p class="muted">{t("apiKeysStorageNote")}</p>
            <label>
              <span>{t("deeplKey")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "deeplKey")} on:mouseenter={() => showHelp("deeplKey")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "deeplKey"}<span class="help-popover">{help("deeplKey")}</span>{/if}</button></span>
              <div class="inline">
                <input bind:value={deeplKey} type="password" placeholder={t("storedSecret")} />
                <button class="primary" on:click={() => saveKey("deepl", deeplKey)} disabled={!deeplKey.trim()} title={t("keySave")} aria-label={t("keySave")}><Save size={15} /></button>
                <button on:click={() => clearKey("deepl")} title={t("clearField")} aria-label={t("clearField")}><Trash2 size={15} /></button>
              </div>
              {#if keyStateHint(snapshot.hasDeeplKey)}
                <small class="field-hint">{keyStateHint(snapshot.hasDeeplKey)}</small>
              {/if}
            </label>
            <label>
              <span>{t("googleKey")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "googleKey")} on:mouseenter={() => showHelp("googleKey")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "googleKey"}<span class="help-popover">{help("googleKey")}</span>{/if}</button></span>
              <div class="inline">
                <input bind:value={googleKey} type="password" placeholder={t("storedSecret")} />
                <button class="primary" on:click={() => saveKey("google", googleKey)} disabled={!googleKey.trim()} title={t("keySave")} aria-label={t("keySave")}><Save size={15} /></button>
                <button on:click={() => clearKey("google")} title={t("clearField")} aria-label={t("clearField")}><Trash2 size={15} /></button>
              </div>
              {#if keyStateHint(snapshot.hasGoogleKey)}
                <small class="field-hint">{keyStateHint(snapshot.hasGoogleKey)}</small>
              {/if}
            </label>
            <label>
              <span>{t("yandexKey")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "yandexKey")} on:mouseenter={() => showHelp("yandexKey")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "yandexKey"}<span class="help-popover">{help("yandexKey")}</span>{/if}</button></span>
              <div class="inline">
                <input bind:value={yandexKey} type="password" placeholder={t("storedSecret")} />
                <button class="primary" on:click={() => saveKey("yandex", yandexKey)} disabled={!yandexKey.trim()} title={t("keySave")} aria-label={t("keySave")}><Save size={15} /></button>
                <button on:click={() => clearKey("yandex")} title={t("clearField")} aria-label={t("clearField")}><Trash2 size={15} /></button>
              </div>
              {#if keyStateHint(snapshot.hasYandexKey)}
                <small class="field-hint">{keyStateHint(snapshot.hasYandexKey)}</small>
              {/if}
            </label>
            <label>
              <span>{t("yandexFolderId")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "yandexFolderId")} on:mouseenter={() => showHelp("yandexFolderId")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "yandexFolderId"}<span class="help-popover">{help("yandexFolderId")}</span>{/if}</button></span>
              <div class="inline">
                <input bind:value={config.yandexFolderId} placeholder="Folder ID" />
                <button class="primary" on:click={() => persistConfig(true)} disabled={!config.yandexFolderId.trim()} title={t("keySave")} aria-label={t("keySave")}><Save size={15} /></button>
                <button on:click={() => config && (config.yandexFolderId = "")} title={t("clearField")} aria-label={t("clearField")}><Trash2 size={15} /></button>
              </div>
            </label>
            <label>
              <span>{t("localBearer")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "localBearer")} on:mouseenter={() => showHelp("localBearer")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "localBearer"}<span class="help-popover">{help("localBearer")}</span>{/if}</button></span>
              <div class="inline">
                <input bind:value={localKey} type="password" placeholder={t("optionalLocalServer")} />
                <button class="primary" on:click={() => saveKey("openai-compatible", localKey)} disabled={!localKey.trim()} title={t("keySave")} aria-label={t("keySave")}><Save size={15} /></button>
                <button on:click={() => clearKey("openai-compatible")} title={t("clearField")} aria-label={t("clearField")}><Trash2 size={15} /></button>
              </div>
              {#if keyStateHint(snapshot.hasLocalKey)}
                <small class="field-hint">{keyStateHint(snapshot.hasLocalKey)}</small>
              {/if}
            </label>
          </div>
        </section>
      {:else}
        <section class="history-list">
        <div class="history-head">
          <strong>{t("localHistory")}</strong>
          <button on:click={clearLocalHistory} disabled={!snapshot.history.length}><Trash2 size={16} /> {t("clear")}</button>
        </div>
        {#if !config.historyEnabled}
          <p>{t("historyDisabled")}</p>
        {:else if !snapshot.history.length}
          <p>{t("noHistory")}</p>
        {:else}
          {#each snapshot.history as item}
            <article>
              <div class="history-item-head">
                <small>{historyLanguageLabel(item.sourceLang)} -> {historyLanguageLabel(item.targetLang)}</small>
                <button class="icon small" title={t("clearField")} aria-label={t("clearField")} on:click={() => deleteHistoryEntry(item.id)}>
                  <Trash2 size={13} />
                </button>
              </div>
              <p>{item.sourceText}</p>
              <strong>{item.translatedText}</strong>
            </article>
          {/each}
        {/if}
        </section>
      {/if}
    {:else}
      <section class="loading">{t("loading")}</section>
    {/if}
  </section>
</main>

{#if showAccelInfo}
  <div
    class="accel-modal-backdrop"
    role="presentation"
    on:click={(event) => { if (!gpuBusy && event.target === event.currentTarget) showAccelInfo = false; }}
  >
    <div class="accel-modal" role="dialog" tabindex="-1" aria-modal="true" aria-label={t("accelInfoTitle")}>
      <div class="accel-modal-head">
        <Zap size={18} />
        <h2>{t("accelInfoTitle")}</h2>
      </div>
      {#if gpuBusy}
        <p>{gpuMessage || (snapshot?.runtime.gpuVendor === "amd" || snapshot?.runtime.gpuVendor === "intel" ? t("accelVulkanWorking") : t("accelWorking"))}</p>
        <div class="accel-progress"><div class="accel-progress-fill" style={`width: ${Math.round(gpuProgress * 100)}%`}></div></div>
        {#if snapshot?.runtime.gpuVendor !== "amd" && snapshot?.runtime.gpuVendor !== "intel"}<p class="accel-soon">{t("accelNoCancel")}</p>{/if}
      {:else if snapshot?.runtime.gpuVendor === "nvidia"}
        <p>{t("accelInfoBody")}</p>
        <p>{accelText("accelInfoBodyGpu")}</p>
        <p class="accel-soon">{t("accelInfoSize")}</p>
        {#if gpuError}
          <p class="error-note"><strong>{t("accelErrorTitle")}</strong> — {gpuError}</p>
        {/if}
        <div class="accel-modal-actions">
          <button type="button" class="ghost" on:click={() => (showAccelInfo = false)}>{t("accelCancel")}</button>
          <button type="button" class="primary" on:click={enableGpu}>{gpuError ? t("accelRetry") : t("accelInfoAction")}</button>
        </div>
      {:else if snapshot?.runtime.gpuVendor === "amd" || snapshot?.runtime.gpuVendor === "intel"}
        <p>{t("accelInfoBody")}</p>
        <p>{accelText("accelInfoBodyGpu")}</p>
        <p class="accel-soon">{t("accelVulkanInfoSize")}</p>
        {#if gpuError}
          <p class="error-note"><strong>{t("accelErrorTitle")}</strong> — {gpuError}</p>
        {/if}
        <div class="accel-modal-actions">
          <button type="button" class="ghost" on:click={() => (showAccelInfo = false)}>{t("accelCancel")}</button>
          <button type="button" class="primary" on:click={enableVulkan}>{gpuError ? t("accelRetry") : t("accelInfoAction")}</button>
        </div>
      {:else}
        <p>{t("accelInfoBody")}</p>
        <p class="accel-soon">{t("accelInfoSoon")}</p>
        <button type="button" class="primary" on:click={() => (showAccelInfo = false)}>{t("accelClose")}</button>
      {/if}
    </div>
  </div>
{/if}

<style>
  :global(*) {
    box-sizing: border-box;
  }

  :global(:root) {
    --ui-scale: 1;
    --bg: #f5f7f4;
    --surface: #ffffff;
    --surface-soft: #edf2ef;
    --text: #182026;
    --muted-text: #5f6f77;
    --border: #d6dce0;
    --control-border: #c4cbd0;
    --button-bg: #ffffff;
    --button-hover: #f8fbfc;
    --primary: #256b62;
    --primary-hover: #1e5d55;
    --rail-active: #364852;
    --shadow: rgba(24, 32, 38, 0.18);
    --warn-bg: #fff7ed;
    --warn-text: #6d4b34;
    --warn-border: #d4b9aa;
    --ok-bg: #effaf3;
    --ok-text: #1f6848;
    --ok-border: #acd7bd;
  }

  :global(:root[data-theme="dark"]) {
    --bg: #101417;
    --surface: #171d21;
    --surface-soft: #20282d;
    --text: #edf2f4;
    --muted-text: #a7b4ba;
    --border: #34424a;
    --control-border: #485860;
    --button-bg: #20282d;
    --button-hover: #273138;
    --primary: #2f8176;
    --primary-hover: #3a978a;
    --rail-active: #2f8176;
    --shadow: rgba(0, 0, 0, 0.34);
    --warn-bg: #2b211b;
    --warn-text: #e1b093;
    --warn-border: #704d3d;
    --ok-bg: #17291f;
    --ok-text: #78d6a4;
    --ok-border: #3d7655;
    color-scheme: dark;
  }

  :global(html),
  :global(body) {
    height: 100%;
    overflow: hidden;
  }

  :global(body) {
    margin: 0;
    min-width: 840px;
    color: var(--text);
    background: var(--bg);
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
    border: 1px solid var(--control-border);
    border-radius: 6px;
    color: var(--text);
    background: var(--button-bg);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 0 9px;
    cursor: pointer;
  }

  button:hover {
    border-color: #7591a3;
    background: var(--button-hover);
  }

  button:disabled {
    cursor: default;
    opacity: 0.55;
  }

  .primary {
    color: #ffffff;
    border-color: var(--primary);
    background: var(--primary);
  }

  .primary:hover {
    background: var(--primary-hover);
  }

  pre.runtime-log {
    margin: 8px 0 0;
    max-height: 240px;
    overflow: auto;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-soft);
    white-space: pre-wrap;
    word-break: break-word;
    font-size: 12px;
    line-height: 1.35;
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
    width: 44px;
    margin: 8px 0 8px 8px;
    padding: 10px 6px;
    display: grid;
    grid-template-rows: 32px 1fr;
    gap: 10px;
    align-content: start;
    align-self: start;
    justify-items: center;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
  }

  option {
    color: var(--text);
    background: var(--surface);
  }

  .mark {
    width: 30px;
    height: 30px;
    border-radius: 7px;
    color: #ffffff;
    background: var(--primary);
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
    background: var(--primary-hover);
    box-shadow: 0 0 0 2px rgba(37, 107, 98, 0.18);
  }

  nav {
    display: grid;
    align-content: start;
    gap: 7px;
    justify-items: center;
    width: 100%;
  }

  nav button {
    width: 30px;
    height: 30px;
    min-height: 30px;
    padding: 0;
  }

  nav button.active {
    color: #ffffff;
    border-color: var(--rail-active);
    background: var(--rail-active);
  }

  .workspace {
    min-width: 0;
    min-height: 0;
    display: grid;
    grid-template-rows: minmax(0, 1fr);
    overflow: hidden;
    padding: 6px 6px 6px 4px;
  }

  .translate-view {
    min-height: 0;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    overflow: hidden;
  }

  .toolbar {
    min-height: 36px;
    padding: 5px 8px 6px;
    display: flex;
    flex-wrap: nowrap;
    gap: 6px;
    align-items: end;
    border: 1px solid var(--border);
    border-radius: 8px 8px 0 0;
    background: var(--surface-soft);
  }

  .toolbar > label {
    flex: 1 1 0;
    min-width: 56px;
  }

  .toolbar > label.combo-label {
    flex: 1.6 1 0;
    min-width: 70px;
  }

  .toolbar > label select,
  .toolbar > label .combo,
  .toolbar > label .combo-button {
    min-width: 0;
  }

  .run-label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .toolbar > button,
  .toolbar > .zoom-controls {
    flex: 0 0 auto;
  }

  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }

  label {
    display: grid;
    gap: 5px;
    color: var(--muted-text);
    font-size: 12px;
    font-weight: 600;
  }

  input,
  select,
  textarea {
    width: 100%;
    border: 1px solid var(--control-border);
    border-radius: 6px;
    color: var(--text);
    background: var(--surface);
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
    position: relative;
    width: 18px;
    height: 18px;
    min-height: 18px;
    padding: 0;
    display: inline-grid;
    place-items: center;
    flex: 0 0 18px;
    gap: 0;
    color: var(--muted-text);
    border-color: transparent;
    border-radius: 999px;
    background: transparent;
    line-height: 1;
    vertical-align: middle;
  }

  .help :global(svg) {
    display: block;
    width: 13px;
    height: 13px;
  }

  .help:hover,
  .help:focus-visible {
    color: var(--text);
    border-color: transparent;
    background: transparent;
  }

  .help-popover {
    position: absolute;
    z-index: 40;
    left: 50%;
    bottom: calc(100% + 7px);
    width: min(260px, 60vw);
    padding: 8px 9px;
    color: var(--text);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 7px;
    box-shadow: 0 10px 28px var(--shadow);
    font-size: 12px;
    font-weight: 500;
    line-height: 1.35;
    text-align: left;
    transform: translateX(-50%);
    pointer-events: none;
  }

  .combo {
    position: relative;
  }

  .combo-button {
    width: 100%;
    justify-content: space-between;
    background: var(--surface);
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
    background: var(--surface);
    box-shadow: 0 12px 30px var(--shadow);
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
    justify-content: flex-start;
    border-color: transparent;
    background: transparent;
  }

  .combo-options button:hover,
  .combo-options button.active {
    border-color: var(--border);
    background: var(--surface-soft);
  }

  .combo-group {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
    padding: 2px 4px 0;
  }

  .combo-divider {
    height: 1px;
    background: var(--border);
    margin: 4px 0;
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
    padding: 8px;
    line-height: 1.45;
  }

  .prompt-template {
    min-height: 92px;
    height: auto;
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
    padding: 6px 0 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
    overflow: hidden;
  }

  .pane {
    min-width: 0;
    min-height: 0;
    display: grid;
    grid-template-rows: minmax(0, 1fr) 26px;
    gap: 4px;
    padding: 4px 6px 6px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
  }

  .pane-actions,
  .inline {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .translate-footer {
    padding: 8px 0 0;
    display: grid;
    gap: 8px;
  }

  .footer-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    min-height: 18px;
  }

  .footer-bar.minimal {
    min-height: 10px;
    opacity: 0.45;
  }

  .footer-bar .inline-note {
    margin: 0;
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
  }

  .footer-toggle :global(svg) {
    transition: transform 0.15s ease;
  }

  .footer-toggle.open :global(svg) {
    transform: rotate(180deg);
  }

  .accel-banner {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 12px;
    border-radius: 8px;
    font-size: 12px;
    border: 1px solid var(--warn-border);
    background: var(--warn-bg);
    color: var(--warn-text);
  }

  .accel-banner.fast {
    border-color: var(--ok-border);
    background: var(--ok-bg);
    color: var(--ok-text);
  }

  .accel-banner :global(svg) {
    flex-shrink: 0;
  }

  .accel-message {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .accel-message strong {
    font-weight: 600;
  }

  .accel-cta {
    margin-left: auto;
    flex-shrink: 0;
    padding: 5px 12px;
    font-size: 12px;
    border-radius: 7px;
    border: 1px solid var(--warn-border);
    background: var(--surface);
    color: var(--warn-text);
    cursor: pointer;
  }

  .accel-cta:hover {
    background: var(--button-hover);
  }

  .accel-modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    background: rgba(0, 0, 0, 0.45);
  }

  .accel-modal {
    width: min(440px, 100%);
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 20px;
    border-radius: 12px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 18px 48px var(--shadow);
  }

  .accel-modal-head {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--primary);
  }

  .accel-modal-head h2 {
    margin: 0;
    font-size: 16px;
    color: var(--text);
  }

  .accel-modal p {
    margin: 0;
    font-size: 13px;
    line-height: 1.5;
    color: var(--muted-text);
  }

  .accel-modal .accel-soon {
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--surface-soft);
    color: var(--text);
  }

  .accel-modal .primary {
    align-self: flex-end;
    margin-top: 4px;
  }

  .accel-modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }

  .accel-modal-actions .ghost {
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text);
    border-radius: 8px;
    padding: 8px 14px;
    cursor: pointer;
  }

  .accel-modal-actions .ghost:hover {
    background: var(--surface-soft);
  }

  .accel-progress {
    height: 8px;
    border-radius: 999px;
    background: var(--surface-soft);
    overflow: hidden;
  }

  .accel-progress-fill {
    height: 100%;
    border-radius: 999px;
    background: var(--accent, #3b82f6);
    transition: width 0.2s ease;
  }

  .range-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 8px;
    align-items: center;
  }

  .range-value {
    min-width: 52px;
    color: var(--text);
    font-size: 12px;
    text-align: right;
  }

  .pane-actions {
    justify-content: flex-start;
  }

  .pane-actions.end {
    justify-content: flex-end;
  }

  .model-note {
    margin: 0;
    padding: 10px 12px;
    display: flex;
    gap: 8px;
    align-items: center;
    color: var(--muted-text);
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
  }

  .model-note span {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .settings-grid {
    min-height: 0;
    padding: 0;
    display: grid;
    grid-template-columns: minmax(0, 1.5fr) minmax(220px, 380px);
    align-items: start;
    align-content: start;
    gap: 12px;
    overflow: auto;
  }

  .group {
    padding: 12px;
    display: grid;
    gap: 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
  }

  .muted {
    margin: 0;
    color: var(--muted-text);
    font-size: 13px;
    line-height: 1.4;
  }

  .running-on-line {
    margin: 4px 0 0;
    font-size: 12px;
    color: var(--muted-text);
  }

  .pill {
    min-height: 21px;
    padding: 2px 7px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    border: 1px solid var(--border);
    border-radius: 999px;
    color: var(--text);
    background: var(--surface-soft);
    font-size: 11px;
    font-weight: 700;
  }

  .pill.ok {
    color: var(--text);
    border-color: var(--border);
    background: var(--surface-soft);
  }

  h3 {
    margin: 0;
    color: var(--text);
    font-size: 13px;
  }

  .model-manager {
    display: grid;
    gap: 8px;
  }

  .model-card {
    margin: 0;
    padding: 9px;
    display: grid;
    gap: 7px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--surface);
  }

  .model-card.active {
    border-color: var(--primary);
  }

  .model-card-head,
  .progress-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .model-card-head strong {
    min-width: 0;
    color: var(--text);
    font-size: 13px;
  }

  .model-card-head span {
    color: var(--muted-text);
    font-size: 11px;
  }

  .model-card p {
    margin: 0;
    color: var(--muted-text);
    font-size: 12px;
    line-height: 1.35;
  }

  .model-card .detail {
    color: var(--text);
    opacity: 0.88;
  }

  .model-card dl {
    margin: 0;
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 6px;
  }

  .model-card dl div {
    min-width: 0;
    padding: 5px 6px;
    border: 1px solid var(--border);
    border-radius: 6px;
  }

  .model-card dt {
    color: var(--muted-text);
    font-size: 10px;
    font-weight: 700;
  }

  .model-card dd {
    margin: 1px 0 0;
    color: var(--text);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .download-progress {
    display: grid;
    gap: 6px;
  }

  .model-card-actions {
    display: flex;
    gap: 8px;
  }

  .model-card-actions button {
    flex: 1;
  }

  .progress-meta {
    color: var(--muted-text);
    font-size: 11px;
  }

  progress {
    width: 100%;
    height: 8px;
    accent-color: var(--primary);
  }

  .custom-local-hint {
    padding: 8px 10px;
    border-left: 3px solid var(--primary);
    border-radius: 0 6px 6px 0;
    background: var(--surface-soft);
    font-size: 12px;
  }

  details {
    display: grid;
    gap: 10px;
  }

  summary {
    list-style: none;
    cursor: pointer;
    color: var(--text);
    font-size: 13px;
    font-weight: 800;
  }

  summary::-webkit-details-marker {
    display: none;
  }

  details[open] {
    gap: 12px;
  }

  details[open] label {
    margin-top: 10px;
  }

  h2 {
    margin: 0 0 2px;
    color: var(--text);
    font-size: 15px;
  }

  .check {
    grid-template-columns: 18px 1fr;
    align-items: center;
    color: var(--text);
    font-size: 14px;
    font-weight: 500;
  }

  .check input {
    width: 16px;
    height: 16px;
  }

  .field-hint {
    color: var(--muted-text);
    font-size: 11px;
    font-weight: 500;
  }

  .settings-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }

  .probe-result {
    padding: 10px 12px;
    display: grid;
    gap: 6px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
  }

  .probe-result strong,
  .probe-result p {
    margin: 0;
  }

  .inline-note {
    margin: 0;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 7px;
    color: var(--text);
    background: var(--surface-soft);
    font-size: 12px;
  }

  .inline-note.error-note {
    border-color: var(--border);
    color: var(--text);
    background: var(--surface-soft);
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

  .history-item-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  article {
    padding: 12px;
    display: grid;
    gap: 6px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
  }

  article p,
  article strong {
    margin: 0;
    white-space: pre-wrap;
  }

  article small {
    color: var(--muted-text);
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

  .device-info {
    margin: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .device-info div {
    padding: 5px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    min-width: 120px;
  }

  .device-info dt {
    color: var(--muted-text);
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
  }

  .device-info dd {
    margin: 2px 0 0;
    color: var(--text);
    font-size: 13px;
    font-weight: 700;
  }

  .device-gpu {
    color: var(--primary) !important;
  }

  @media (max-width: 760px) {
    :global(body) {
      min-width: 0;
    }

    .shell,
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
      margin: 8px;
      border-bottom: 1px solid var(--border);
    }

    nav {
      display: flex;
    }

    textarea {
      min-height: 140px;
    }
  }
</style>
