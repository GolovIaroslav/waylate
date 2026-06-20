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
    quantization: string;
    size: string;
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
    ct2ModelPath: string;
    ct2TokenizerPath: string;
    ct2HelperCommand: string;
    ct2Device: string;
    apiProviderEnabled: boolean;
    yandexFolderId: string;
    uiLanguage: string;
    theme: string;
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
      hasPython: boolean;
      hasNvidiaSmi: boolean;
      hasRocmSmi: boolean;
      hasLlamaServer: boolean;
      ct2CudaDevices: number;
      llamaCudaReported: boolean;
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
      ct2CudaDevices: number;
      llamaBinaryFound: boolean;
      llamaCudaReported: boolean;
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

  type DownloadProgress = {
    modelId: string;
    status: "starting" | "downloading" | "preparing" | "done" | "cancelled";
    message: string;
    progress: number;
    downloadedBytes: number;
    totalBytes?: number;
  };

  type UiLang = "en" | "ru" | "sk";

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
  let downloadState: DownloadProgress | null = null;
  let activeHelp: keyof typeof helpTexts | null = null;
  let helpCloseTimer: number | undefined;
  let uiLang: UiLang = "en";

  $: selectedModel = snapshot?.catalog.find((model) => model.id === config?.modelId);
  $: languages = selectedModel?.languages ?? [];
  $: localModelReady = isCurrentModelReady();
  $: downloadableModels = snapshot?.catalog.filter((model) => model.downloadable) ?? [];
  $: selectableModels = snapshot?.catalog.filter((model) => model.provider !== "open-ai-compatible") ?? [];
  $: uiLang = config?.uiLanguage === "ru" || config?.uiLanguage === "sk" ? config.uiLanguage : "en";
  $: if (config) applyTheme(config.theme);

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
      en: "For custom setups only.",
      ru: "Только для своей ручной настройки.",
      sk: "Len pre vlastné ručné nastavenie.",
    },
    openaiModel: {
      en: "Name used by a custom local setup.",
      ru: "Имя модели для ручной настройки.",
      sk: "Názov modelu pre ručné nastavenie.",
    },
    ct2ModelPath: {
      en: "Filled after download.",
      ru: "Заполняется после скачивания.",
      sk: "Vyplní sa po stiahnutí.",
    },
    ct2TokenizerPath: {
      en: "Filled after download.",
      ru: "Заполняется после скачивания.",
      sk: "Vyplní sa po stiahnutí.",
    },
    ct2HelperCommand: {
      en: "Filled after download.",
      ru: "Заполняется после скачивания.",
      sk: "Vyplní sa po stiahnutí.",
    },
    device: {
      en: "auto tries CUDA when CTranslate2 sees a CUDA device, otherwise CPU. CUDA is faster but uses VRAM while translating.",
      ru: "auto пробует CUDA, если CTranslate2 видит CUDA-устройство, иначе CPU. CUDA быстрее, но во время перевода занимает VRAM.",
      sk: "auto skúsi CUDA, keď CTranslate2 vidí CUDA zariadenie, inak CPU. CUDA je rýchlejšia, ale počas prekladu používa VRAM.",
    },
    localModelPolicy: {
      en: "Balanced keeps the model warm for a while, Fast preloads it, Memory saver unloads it after each translation.",
      ru: "Balanced держит модель тёплой некоторое время, Fast прогружает заранее, Memory saver выгружает после каждого перевода.",
      sk: "Balanced nechá model chvíľu nahratý, Fast ho prednačíta, Memory saver ho po každom preklade uvoľní.",
    },
    localModelIdleTimeout: {
      en: "How long the warm local runtime stays loaded after the last translation in Balanced mode.",
      ru: "Сколько тёплый локальный runtime остаётся загруженным после последнего перевода в режиме Balanced.",
      sk: "Ako dlho zostane lokálny runtime po poslednom preklade nahratý v režime Balanced.",
    },
    customBackendMode: {
      en: "External OpenAI-compatible uses your own server. Managed GGUF starts a hidden llama-server process for a local GGUF file.",
      ru: "External OpenAI-compatible использует твой готовый сервер. Managed GGUF сам запускает скрытый llama-server для локального GGUF файла.",
      sk: "External OpenAI-compatible používa tvoj vlastný server. Managed GGUF spustí skrytý llama-server pre lokálny GGUF súbor.",
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
      en: "ID of your Yandex Cloud folder.",
      ru: "ID твоей папки в Yandex Cloud.",
      sk: "ID tvojho priečinka v Yandex Cloud.",
    },
    localBearer: {
      en: "Only needed for a custom local setup.",
      ru: "Нужно только для ручной локальной настройки.",
      sk: "Treba len pre ručné lokálne nastavenie.",
    },
    uiLanguage: {
      en: "Changes the visible interface language immediately. Save settings to keep it after restart.",
      ru: "Меняет язык интерфейса сразу. Сохрани настройки, чтобы оставить язык после перезапуска.",
      sk: "Okamžite zmení jazyk rozhrania. Ulož nastavenia, aby zostal aj po reštarte.",
    },
    theme: {
      en: "Switches the interface between light and dark mode. Save settings to keep it after restart.",
      ru: "Переключает интерфейс между светлой и тёмной темой. Сохрани настройки, чтобы оставить выбор после перезапуска.",
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
      localModel: "Local model",
      runtimeLoaded: "Loaded",
      runtimeCold: "Cold start",
      loadingModel: "Loading model...",
      ready: "Ready",
      setupNeeded: "Setup needed",
      onboardingTitle: "Local setup",
      onboardingText: "Download a local model once. After that, translation works offline.",
      download: "Download",
      downloading: "Downloading",
      cancel: "Cancel",
      retry: "Retry",
      saveSettings: "Save settings",
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
      ct2ModelPath: "Model folder",
      tokenizerPath: "Tokenizer folder",
      helperCommand: "Translator helper",
      privacyApis: "Privacy and APIs",
      interfaceLanguage: "Interface language",
      theme: "Theme",
      light: "Light",
      dark: "Dark",
      saveHistory: "Save translation history locally",
      autostart: "Start Waylate in background",
      networkApis: "Allow network API providers",
      apiKeysNote: "DeepL, Google and Yandex need your own key.",
      deeplKey: "DeepL API key",
      googleKey: "Google API key",
      yandexKey: "Yandex API key",
      yandexFolderId: "Yandex Folder ID",
      localBearer: "Local bearer token",
      storedSecret: "Saved key",
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
      settingsSaved: "Settings saved.",
      keySaved: "API key saved.",
      keyRemoved: "API key removed.",
      backendOk: "Translation works.",
      downloaded: "Ready",
      quantization: "Version",
      size: "Size",
      languages: "Languages",
      modelManager: "Model manager",
      runtime: "Runtime",
      diagnostics: "Diagnostics",
      activeRuntime: "Active runtime",
      none: "None",
      localModelReadyHint: "This model is ready to translate.",
      localModelMissingHint: "This model is not installed - Download it in Settings.",
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
      sourcePlaceholder: "Вставь текст для перевода",
      translationPlaceholder: "Перевод",
      readSelection: "Взять выделенный текст",
      pasteClipboard: "Вставить из буфера",
      copyTranslation: "Скопировать перевод",
      localModel: "Локальная модель",
      runtimeLoaded: "Загружена",
      runtimeCold: "Холодный старт",
      loadingModel: "Загрузка модели...",
      ready: "Готово",
      setupNeeded: "Нужна настройка",
      onboardingTitle: "Локальная настройка",
      onboardingText: "Скачай локальную модель один раз. После этого перевод работает офлайн.",
      download: "Скачать",
      downloading: "Скачивается",
      cancel: "Отмена",
      retry: "Повторить",
      saveSettings: "Сохранить настройки",
      testBackend: "Проверить перевод",
      modelPath: "Путь модели",
      tokenizer: "Токенизатор",
      python: "Python",
      device: "Устройство",
      modelMemory: "Память модели",
      balanced: "Balanced",
      fast: "Fast",
      memorySaver: "Memory saver",
      idleTimeout: "Тайм-аут",
      minutesShort: "мин",
      advancedLocalBackend: "Дополнительно",
      customBackendMode: "Режим custom",
      externalOpenAi: "External OpenAI-compatible",
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
      ct2ModelPath: "Папка модели",
      tokenizerPath: "Папка tokenizer",
      helperCommand: "Помощник перевода",
      privacyApis: "Приватность и API",
      interfaceLanguage: "Язык интерфейса",
      theme: "Тема",
      light: "Светлая",
      dark: "Тёмная",
      saveHistory: "Сохранять историю переводов локально",
      autostart: "Запускать Waylate в фоне",
      networkApis: "Разрешить сетевые API-провайдеры",
      apiKeysNote: "Для DeepL, Google и Yandex нужен твой ключ.",
      deeplKey: "DeepL API key",
      googleKey: "Google API key",
      yandexKey: "Yandex API key",
      yandexFolderId: "Yandex Folder ID",
      localBearer: "Local bearer token",
      storedSecret: "Ключ сохранён",
      optionalLocalServer: "Необязательно",
      system: "Система",
      config: "Конфиг",
      models: "Модели",
      localHistory: "Локальная история",
      clear: "Очистить",
      historyDisabled: "История выключена. Включи её в настройках, если нужна локальная SQLite-история.",
      noHistory: "Сохранённых переводов пока нет.",
      loading: "Загрузка Waylate...",
      nothingToTranslate: "Нечего переводить.",
      translationCopied: "Перевод скопирован.",
      settingsSaved: "Настройки сохранены.",
      keySaved: "API-ключ сохранён.",
      keyRemoved: "API-ключ удалён.",
      backendOk: "Перевод работает.",
      downloaded: "Готово",
      quantization: "Версия",
      size: "Размер",
      languages: "Языки",
      modelManager: "Менеджер моделей",
      runtime: "Runtime",
      diagnostics: "Диагностика",
      activeRuntime: "Активный runtime",
      none: "Нет",
      localModelReadyHint: "Эта модель готова к переводу.",
      localModelMissingHint: "Эта модель не установлена. Скачай её в настройках.",
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
      localModel: "Lokálny model",
      runtimeLoaded: "Nahraté",
      runtimeCold: "Studený štart",
      loadingModel: "Načítava sa model...",
      ready: "Pripravené",
      setupNeeded: "Treba nastaviť",
      onboardingTitle: "Lokálne nastavenie",
      onboardingText: "Stiahni lokálny model raz. Potom preklad funguje offline.",
      download: "Stiahnuť",
      downloading: "Sťahuje sa",
      cancel: "Zrušiť",
      retry: "Skúsiť znova",
      saveSettings: "Uložiť nastavenia",
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
      ct2ModelPath: "Priečinok modelu",
      tokenizerPath: "Priečinok tokenizeru",
      helperCommand: "Pomocník prekladu",
      privacyApis: "Súkromie a API",
      interfaceLanguage: "Jazyk rozhrania",
      theme: "Téma",
      light: "Svetlá",
      dark: "Tmavá",
      saveHistory: "Ukladať históriu prekladov lokálne",
      autostart: "Spúšťať Waylate na pozadí",
      networkApis: "Povoliť sieťových API providerov",
      apiKeysNote: "DeepL, Google a Yandex potrebujú tvoj kľúč.",
      deeplKey: "DeepL API key",
      googleKey: "Google API key",
      yandexKey: "Yandex API key",
      yandexFolderId: "Yandex Folder ID",
      localBearer: "Local bearer token",
      storedSecret: "Kľúč uložený",
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
      settingsSaved: "Nastavenia uložené.",
      keySaved: "API kľúč uložený.",
      keyRemoved: "API kľúč odstránený.",
      backendOk: "Preklad funguje.",
      downloaded: "Pripravené",
      quantization: "Verzia",
      size: "Veľkosť",
      languages: "Jazyky",
      modelManager: "Správca modelov",
      runtime: "Runtime",
      diagnostics: "Diagnostika",
      activeRuntime: "Aktívny runtime",
      none: "Žiadny",
      localModelReadyHint: "Tento model je pripravený na preklad.",
      localModelMissingHint: "Tento model nie je nainštalovaný. Stiahni ho v nastaveniach.",
    },
  } as const;

  onMount(() => {
    let unlisten: (() => void) | undefined;
    let unlistenDownload: (() => void) | undefined;
    const savedScale = Number(localStorage.getItem("waylate-ui-scale"));
    setUiScale(Number.isFinite(savedScale) && savedScale >= 1 ? savedScale : 1);
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
      if (!localModelReady) {
        tab = "settings";
      }
      await consumePending();
      unlisten = await listen("waylate-pending", consumePending);
      unlistenDownload = await listen<DownloadProgress>("model-download-progress", (event) => {
        downloadState = event.payload;
      });
    })();
    return () => {
      unlisten?.();
      unlistenDownload?.();
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
      error = t("nothingToTranslate");
      return;
    }
    if (selectedModel?.provider === "c-translate2" && !localModelReady) {
      error = t("localModelMissingHint");
      tab = "settings";
      return;
    }
    if (isLocalProfile(config.modelId)) {
      status = snapshot?.runtime.selectedModelLoaded ? t("runtimeLoaded") : t("loadingModel");
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
    status = t("translationCopied");
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
      status = t("settingsSaved");
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
      status = t("keySaved");
    } catch (err) {
      error = String(err);
    }
  }

  async function clearKey(provider: string) {
    error = "";
    try {
      await invoke("clear_api_key", { provider });
      await refresh();
      status = t("keyRemoved");
    } catch (err) {
      error = String(err);
    }
  }

  async function downloadModel(modelId: string) {
    if (!config) return;
    busy = true;
    error = "";
    downloadState = {
      modelId,
      status: "starting",
      message: "Starting download",
      progress: 0.02,
      downloadedBytes: 0,
    };
    try {
      const path = await invoke<string>("download_catalog_model", { modelId });
      await refresh();
      status = `${t("downloaded")}: ${path}`;
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  async function cancelDownload() {
    await invoke("cancel_model_download");
  }

  async function testBackend() {
    if (!config) return;
    error = "";
    status = "";
    busy = true;
    try {
      await invoke("translate_text", {
        request: {
          text: "Hello",
          sourceLang:
            config.sourceLang === "auto"
              ? selectedModel?.provider === "c-translate2"
                ? "eng_Latn"
                : "en"
              : config.sourceLang,
          targetLang: config.targetLang,
          modelId: config.modelId,
        },
      });
      status = t("backendOk");
      await refresh();
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

  function isLocalProfile(modelId: string) {
    const profile = snapshot?.catalog.find((item) => item.id === modelId);
    if (profile?.provider === "c-translate2") return true;
    if (profile?.provider === "custom") return config?.customBackendMode === "managed-gguf";
    return false;
  }

  function isCurrentModelReady() {
    if (!config || !selectedModel) return false;
    if (selectedModel.provider === "c-translate2") {
      return Boolean(config.ct2ModelPath && config.ct2TokenizerPath);
    }
    if (selectedModel.provider === "custom") {
      if (config.customBackendMode === "managed-gguf") {
        return Boolean(config.customModelPath);
      }
      return Boolean(config.openaiEndpoint);
    }
    return false;
  }

  function help(key: keyof typeof helpTexts) {
    return helpTexts[key][uiLang];
  }

  function t(key: keyof typeof uiTexts.en) {
    return uiTexts[uiLang][key];
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

</script>

<svelte:head>
  <title>Waylate</title>
</svelte:head>

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
        <section class="toolbar" aria-label="Translation options">
          <label>
            {t("model")}
            <select value={config.modelId} on:change={(event) => changeModel(event.currentTarget.value)}>
              {#each selectableModels as model}
                <option value={model.id}>{model.name}</option>
              {/each}
            </select>
          </label>
          <label class="combo-label">
            {t("from")}
            <div class="combo">
              <button type="button" class="combo-button" on:click={() => (sourceLanguageOpen = !sourceLanguageOpen)}>
                <span>{languageLabel(config.sourceLang)}</span>
                <ChevronDown size={14} />
              </button>
              {#if sourceLanguageOpen}
                <div class="combo-menu">
                  <input bind:value={sourceLanguageQuery} placeholder={t("searchLanguage")} />
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
          <button class="icon" title={t("swapLanguages")} on:click={swapLanguages} disabled={config.sourceLang === "auto"}>
            <Repeat2 size={15} />
          </button>
          <label class="combo-label">
            {t("to")}
            <div class="combo">
              <button type="button" class="combo-button" on:click={() => (targetLanguageOpen = !targetLanguageOpen)}>
                <span>{languageLabel(config.targetLang)}</span>
                <ChevronDown size={14} />
              </button>
              {#if targetLanguageOpen}
                <div class="combo-menu">
                  <input bind:value={targetLanguageQuery} placeholder={t("searchLanguage")} />
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
            <span class:spin={busy}><RefreshCw size={15} /></span> {t("translate")}
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
              <span>{t("source")}</span>
            </div>
            <textarea bind:value={sourceText} spellcheck="false" placeholder={t("sourcePlaceholder")}></textarea>
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
            <div class="pane-head">
              <span>{t("translation")}</span>
            </div>
            <textarea bind:value={translatedText} spellcheck="false" readonly placeholder={t("translationPlaceholder")}></textarea>
            <div class="pane-actions end">
              <button class="icon small" title={t("copyTranslation")} aria-label={t("copyTranslation")} on:click={copyTranslation} disabled={!translatedText.trim()}>
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
              <h2>{t("localModel")}</h2>
              {#if localModelReady}
                <span class="pill ok"><CheckCircle2 size={13} /> {t("ready")}</span>
              {:else}
                <span class="pill">{t("setupNeeded")}</span>
              {/if}
            </div>
            <p class="muted">{localModelReady ? t("localModelReadyHint") : t("onboardingText")}</p>
            <div class="setup-list">
              <span class:ok={Boolean(config.ct2ModelPath) || Boolean(config.customModelPath)}>{t("modelPath")}</span>
              <span class:ok={Boolean(config.ct2TokenizerPath) || config.customBackendMode === "managed-gguf"}>{t("tokenizer")}</span>
              <span class:ok={snapshot.environment.hasPython}>{t("python")}</span>
              <span class:ok={Boolean(snapshot.runtime.selectedDevice) || config.ct2Device === "cpu"}>{t("device")}</span>
              <span class:ok={snapshot.runtime.selectedModelLoaded}>{snapshot.runtime.selectedModelLoaded ? t("runtimeLoaded") : t("runtimeCold")}</span>
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
            <h3>{t("modelManager")}</h3>
            <div class="model-manager">
              {#each downloadableModels as model}
                <article class:active={config.modelId === model.id} class="model-card">
                  <div class="model-card-head">
                    <strong>{model.name}</strong>
                    <span>{model.size}</span>
                  </div>
                  <p>{model.description}</p>
                  <dl>
                    <div><dt>{t("quantization")}</dt><dd>{model.quantization}</dd></div>
                    <div><dt>{t("size")}</dt><dd>{model.size}</dd></div>
                    <div><dt>{t("languages")}</dt><dd>{model.languages.length - 1}</dd></div>
                  </dl>
                  {#if downloadState?.modelId === model.id && downloadState.status !== "done" && downloadState.status !== "cancelled"}
                    <div class="download-progress">
                      <div class="progress-meta">
                        <span>{downloadState.status === "preparing" ? downloadState.message : t("downloading")}: {formatBytes(downloadState.downloadedBytes)}{downloadState.totalBytes ? ` / ${formatBytes(downloadState.totalBytes)}` : ""}</span>
                        <span>{Math.round(downloadState.progress * 100)}%</span>
                      </div>
                      <progress max="1" value={downloadState.progress}></progress>
                      <button on:click={cancelDownload}>{t("cancel")}</button>
                    </div>
                  {:else}
                    <button class="primary" on:click={() => downloadModel(model.id)} disabled={busy}>
                      <Download size={16} /> {downloadState?.modelId === model.id && error ? t("retry") : t("download")}
                    </button>
                  {/if}
                </article>
              {/each}
            </div>
            <div class="actions">
              <button on:click={testBackend} disabled={busy}><RefreshCw size={16} /> {t("testBackend")}</button>
              <button on:click={saveSettings}><Save size={16} /> {t("saveSettings")}</button>
            </div>
            <details>
              <summary>{t("advancedLocalBackend")}</summary>
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
              <label>
                <span>{t("ct2ModelPath")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "ct2ModelPath")} on:mouseenter={() => showHelp("ct2ModelPath")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "ct2ModelPath"}<span class="help-popover">{help("ct2ModelPath")}</span>{/if}</button></span>
                <input bind:value={config.ct2ModelPath} placeholder="/home/user/.local/share/Waylate/models/nllb-200-ct2" />
              </label>
              <label>
                <span>{t("tokenizerPath")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "ct2TokenizerPath")} on:mouseenter={() => showHelp("ct2TokenizerPath")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "ct2TokenizerPath"}<span class="help-popover">{help("ct2TokenizerPath")}</span>{/if}</button></span>
                <input bind:value={config.ct2TokenizerPath} placeholder="same as model path" />
              </label>
              <label>
                <span>{t("helperCommand")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "ct2HelperCommand")} on:mouseenter={() => showHelp("ct2HelperCommand")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "ct2HelperCommand"}<span class="help-popover">{help("ct2HelperCommand")}</span>{/if}</button></span>
                <input bind:value={config.ct2HelperCommand} placeholder="waylate-ct2-translate" />
              </label>
              <label>
                <span>{t("device")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "device")} on:mouseenter={() => showHelp("device")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "device"}<span class="help-popover">{help("device")}</span>{/if}</button></span>
                <select bind:value={config.ct2Device}>
                  <option value="auto">auto</option>
                  <option value="cuda">cuda</option>
                  <option value="cpu">cpu</option>
                </select>
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
            <label>
              <span>{t("deeplKey")} {snapshot.hasDeeplKey ? "(saved)" : ""} <button type="button" class="help" on:click={(event) => toggleHelp(event, "deeplKey")} on:mouseenter={() => showHelp("deeplKey")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "deeplKey"}<span class="help-popover">{help("deeplKey")}</span>{/if}</button></span>
              <div class="inline">
                <input bind:value={deeplKey} type="password" placeholder={t("storedSecret")} />
                <button on:click={() => saveKey("deepl", deeplKey)} disabled={!deeplKey}><Save size={15} /></button>
                <button on:click={() => clearKey("deepl")}><Trash2 size={15} /></button>
              </div>
            </label>
            <label>
              <span>{t("googleKey")} {snapshot.hasGoogleKey ? "(saved)" : ""} <button type="button" class="help" on:click={(event) => toggleHelp(event, "googleKey")} on:mouseenter={() => showHelp("googleKey")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "googleKey"}<span class="help-popover">{help("googleKey")}</span>{/if}</button></span>
              <div class="inline">
                <input bind:value={googleKey} type="password" placeholder={t("storedSecret")} />
                <button on:click={() => saveKey("google", googleKey)} disabled={!googleKey}><Save size={15} /></button>
                <button on:click={() => clearKey("google")}><Trash2 size={15} /></button>
              </div>
            </label>
            <label>
              <span>{t("yandexKey")} {snapshot.hasYandexKey ? "(saved)" : ""} <button type="button" class="help" on:click={(event) => toggleHelp(event, "yandexKey")} on:mouseenter={() => showHelp("yandexKey")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "yandexKey"}<span class="help-popover">{help("yandexKey")}</span>{/if}</button></span>
              <div class="inline">
                <input bind:value={yandexKey} type="password" placeholder={t("storedSecret")} />
                <button on:click={() => saveKey("yandex", yandexKey)} disabled={!yandexKey}><Save size={15} /></button>
                <button on:click={() => clearKey("yandex")}><Trash2 size={15} /></button>
              </div>
            </label>
            <label>
              <span>{t("yandexFolderId")} <button type="button" class="help" on:click={(event) => toggleHelp(event, "yandexFolderId")} on:mouseenter={() => showHelp("yandexFolderId")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "yandexFolderId"}<span class="help-popover">{help("yandexFolderId")}</span>{/if}</button></span>
              <input bind:value={config.yandexFolderId} placeholder="Folder ID" />
            </label>
            <label>
              <span>{t("localBearer")} {snapshot.hasLocalKey ? "(saved)" : ""} <button type="button" class="help" on:click={(event) => toggleHelp(event, "localBearer")} on:mouseenter={() => showHelp("localBearer")} on:mouseleave={scheduleHelpClose}><CircleHelp size={13} />{#if activeHelp === "localBearer"}<span class="help-popover">{help("localBearer")}</span>{/if}</button></span>
              <div class="inline">
                <input bind:value={localKey} type="password" placeholder={t("optionalLocalServer")} />
                <button on:click={() => saveKey("openai-compatible", localKey)} disabled={!localKey}><Save size={15} /></button>
                <button on:click={() => clearKey("openai-compatible")}><Trash2 size={15} /></button>
              </div>
            </label>
          </div>

          <div class="group wide">
            <h2>{t("diagnostics")}</h2>
            <div class="facts">
              <span class:ok={snapshot.environment.hasWlClipboard}>wl-clipboard</span>
              <span class:ok={snapshot.environment.hasPython}>python3</span>
              <span class="ok">HF HTTP</span>
              <span class:ok={snapshot.environment.hasNvidiaSmi}>CUDA/NVIDIA</span>
              <span class:ok={snapshot.environment.hasRocmSmi}>ROCm</span>
              <span class:ok={snapshot.runtime.ct2CudaDevices > 0}>CT2 CUDA: {snapshot.runtime.ct2CudaDevices}</span>
              <span class:ok={snapshot.runtime.llamaBinaryFound}>llama-server</span>
              <span class:ok={snapshot.runtime.llamaCudaReported}>llama CUDA</span>
              <span class:ok={snapshot.runtime.selectedModelLoaded}>{t("activeRuntime")}: {snapshot.runtime.selectedModelLoaded ? (snapshot.runtime.selectedDevice ?? t("runtimeLoaded")) : t("none")}</span>
            </div>
            {#if snapshot.runtime.activeProfiles.length}
              <div class="runtime-list">
                {#each snapshot.runtime.activeProfiles as item}
                  <span class="runtime-chip">{item.profileId} / {item.kind} / {item.device} / {item.idleSeconds}s</span>
                {/each}
              </div>
            {/if}
            <div class="actions">
              <button on:click={revealConfigDir}><FolderOpen size={16} /> {t("config")}</button>
              <button on:click={revealModelsDir}><FolderOpen size={16} /> {t("models")}</button>
            </div>
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
              <small>{item.sourceLang} -> {item.targetLang} / {item.modelId}</small>
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
    min-width: 680px;
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
    border-right: 1px solid var(--border);
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
    border-bottom: 1px solid var(--border);
    background: var(--surface-soft);
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

  .help:hover,
  .help:focus-visible {
    color: var(--text);
    border-color: var(--control-border);
    background: var(--button-hover);
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
    justify-content: space-between;
    border-color: transparent;
    background: transparent;
  }

  .combo-options button:hover,
  .combo-options button.active {
    border-color: var(--border);
    background: var(--surface-soft);
  }

  .combo-options small {
    color: var(--muted-text);
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
    color: var(--text);
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
    margin: 0 12px 8px;
    display: flex;
    gap: 8px;
    align-items: center;
    color: var(--muted-text);
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

  .pill {
    min-height: 21px;
    padding: 2px 7px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    border: 1px solid var(--warn-border);
    border-radius: 999px;
    color: var(--warn-text);
    background: var(--warn-bg);
    font-size: 11px;
    font-weight: 700;
  }

  .pill.ok {
    color: var(--ok-text);
    border-color: var(--ok-border);
    background: var(--ok-bg);
  }

  .setup-list {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }

  .setup-list span {
    min-height: 22px;
    padding: 2px 7px;
    border: 1px solid var(--warn-border);
    border-radius: 6px;
    color: var(--warn-text);
    background: var(--warn-bg);
    font-size: 11px;
    font-weight: 700;
  }

  .setup-list span.ok {
    color: var(--ok-text);
    background: var(--ok-bg);
    border-color: var(--ok-border);
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

  .progress-meta {
    color: var(--muted-text);
    font-size: 11px;
  }

  progress {
    width: 100%;
    height: 8px;
    accent-color: var(--primary);
  }

  details {
    display: grid;
    gap: 10px;
  }

  summary {
    cursor: pointer;
    color: var(--text);
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
    border: 1px solid var(--warn-border);
    border-radius: 6px;
    color: var(--warn-text);
    background: var(--warn-bg);
  }

  .facts span.ok {
    color: var(--ok-text);
    background: var(--ok-bg);
    border-color: var(--ok-border);
  }

  .runtime-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .runtime-chip {
    min-height: 26px;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 999px;
    color: var(--text);
    background: var(--surface-soft);
    font-size: 11px;
    white-space: nowrap;
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

  .status,
  .error {
    margin: 0;
    padding: 8px 18px;
    font-size: 13px;
    border-top: 1px solid var(--border);
  }

  .status {
    color: var(--ok-text);
    background: var(--ok-bg);
  }

  .error {
    color: var(--warn-text);
    background: var(--warn-bg);
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
