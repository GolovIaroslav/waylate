# Waylate — план фиксов по сессиям

Источник: code-review ultra (локальный, 10 агентов + sweep), июнь 2026.
Каждая сессия независима. Запускать в порядке нумерации.

---

## Как запускать

Открой новую сессию Claude Code, напиши:

> продолжи docs/fix-sessions.md сессия N

Агент прочитает этот файл, выполнит задачи сессии N, пометит их выполненными.

---

## Сессия 1 — Критические функциональные баги (translation)

**Приоритет: БЛОКИРУЮЩИЙ** — без этих фиксов local model переводы сломаны.

### Задачи

- [ ] **1a. "auto" в LLM промпте** (`src-tauri/src/translation.rs:85`)
  - Проблема: при auto-detect source_lang, строка "auto" не маппится на язык → промпт "Translate from auto to Russian" → мусор.
  - Фикс: в `translate_spec_managed_llama` и `translate_catalog_managed_gguf` перед построением промпта проверить `source_lang == "auto"` — вернуть `Err("Auto-detect is not supported for local models. Please select a source language.")`.
  - То же самое было удалено из TranslateGemma path — восстановить.

- [ ] **1b. translate() guard не видит spec-catalog модели** (`src/routes/+page.svelte:1073`)
  - Проблема: guard проверяет `isModelInstalled(id)` (читает `snapshot.modelStates`, legacy path). Spec модели живут в `modelStatuses` / `specModelState()`. Установленная модель → "not installed".
  - Фикс: в guard условие добавить `|| specModelState(selectedModel.id) === 'installed'` (уже есть в `localModelReady` на строке 253 — привести в соответствие).

- [ ] **1c. Неполная миграция model_id** (`src-tauri/src/config.rs:142`)
  - Проблема: `migrate_legacy_model_selection` переименовывает `config.model_id`, но `config.installed_models` — HashMap с тем же старым ключом. После миграции модель "не найдена".
  - Фикс: в той же функции после переименования model_id также переименовать ключ в `installed_models`: если в map есть старый ключ → вставить его значение под новым ключом → удалить старый.

### Верификация
После фиксов: выбрать NLLB модель, включить auto-detect, нажать Translate → должно вернуть чёткую ошибку о языке (не мусор). Выбрать язык явно → перевод должен работать.

---

## Сессия 2 — GPU path: Float16 и атомарность

**Приоритет: ВЫСОКИЙ** — GPU ускорение фактически не работает.

### Задачи

- [ ] **2a. Float16 KV-cache tensor → GPU перевод всегда падает** (`src-tauri/src/engines/onnx_mt.rs:619`)
  - Проблема: `initial_cache_tensor` возвращает `Err` для `TensorElementType::Float16`. Fp16 ONNX модели могут иметь Float16 KV-cache → каждый GPU перевод падает на первом decode шаге → тихий fallback на CPU.
  - Фикс: добавить ветку `Float16` в `initial_cache_tensor` — создать zero-тензор как `f32` (Float16 значения KV-cache совместимы с f32 zero-init): `Array::zeros(shape).into_dyn()` с `TensorElementType::Float32`, или создать правильный Float16 zero через `ndarray`.
  - Если это нецелесообразно — добавить понятный error с указанием что конкретный тензор не поддерживается, и какой файл модели вызвал проблему.

- [ ] **2b. GPU download не атомарный** (`src-tauri/src/gpu_runtime.rs:121`)
  - Проблема: `libonnxruntime.so` из первого архива скачивается первым → `is_installed()` видит его → возвращает true. Если следующие архивы (CUDA, cuDNN) не докачались — bundle сломан, но `is_installed()` не знает.
  - Фикс: использовать staging dir (например `gpu_runtime_dir + ".staging"`), заполнить его полностью, затем `rename` в `gpu_runtime_dir`. `is_installed()` проверяет только финальный dir. Если staging dir существует — предыдущее скачивание прервалось → удалить и начать заново.

- [ ] **2c. GPU download нельзя отменить** (`src/routes/+page.svelte:991`)
  - Проблема: GPU download events пропускают `downloadState` assignment → `cancelDownload()` не вызывает cancel.
  - Фикс: в listener при `modelId === 'gpu-runtime'` присваивать `downloadState = { modelId: 'gpu-runtime', ... }`. В `cancelDownload()` добавить ветку для `gpu-runtime`: вызывать отдельный Tauri command или установить флаг отмены.
  - Если Rust-side cancel GPU download не реализован — хотя бы задизейблить кнопку Cancel для gpu-runtime и показать "Cannot cancel GPU download" вместо silent fail.

- [ ] **2d. Truncated fp16 файл принимается как готовый** (`src-tauri/src/gpu_runtime.rs:158`)
  - Проблема: resume guard проверяет только `size > 0`. Обрезанный файл принят → corrupt ONNX подаётся в ORT.
  - Фикс: проверять размер против `Content-Length` заголовка (сохранять expected_size в `.part.size` файл рядом). Или: убрать resume совсем для fp16 файла — он маленький (~600 MB), всегда качать в `.part` и переименовывать после успешного завершения.

### Верификация
После 2a: включить GPU, скачать fp16, перевести → device должен показывать "cuda". После 2b: прервать GPU download на 30% → перезапустить → должен начать заново, не висеть.

---

## Сессия 3 — Безопасность данных (config, secrets, history)

**Приоритет: ВЫСОКИЙ** — crash может навсегда сломать конфиг пользователя.

### Задачи

- [ ] **3a. Non-atomic config save** (`src-tauri/src/config.rs:135`)
  - Проблема: `fs::write(path, json)` = truncate + write. Kill в середине → пустой config.json → app не запускается.
  - Фикс: писать во временный файл (`config.json.tmp` в том же dir), затем `fs::rename(tmp, path)`. rename() атомарен на том же FS.

- [ ] **3b. Corrupt config.json → hard error без fallback** (`src-tauri/src/config.rs:128`)
  - Проблема: parse error → `Err` → caller не запускает app. Missing-file ветка создаёт defaults, corrupt-file — нет.
  - Фикс: в `load()` при `serde_json::from_str` ошибке — логировать warning, вернуть `Ok(AppConfig::default())`. Опционально: сохранить сломанный файл как `config.json.bak` для диагностики.

- [ ] **3c. secrets::set("") записывает пустую запись в keyring** (`src-tauri/src/secrets.rs:18`)
  - Проблема: пустая строка пишется в Secret Service вместо удаления. На KWallet может блокировать последующий set().
  - Фикс: в начале `set()` — если `value.is_empty()` → вызвать `delete(service, key)` и вернуть `Ok(())`.

- [ ] **3d. history::list() принимает отрицательный limit** (`src-tauri/src/history.rs:57`)
  - Проблема: SQLite LIMIT -1 = no limit → вся таблица в памяти → OOM / зависание UI при тысячах записей.
  - Фикс: в начале `list()` добавить `let limit = limit.max(0)` или `if limit < 0 { return Ok(vec![]) }`.

### Верификация
После 3a: `kill -9` процесс во время сохранения настроек → перезапустить → должен стартовать с defaults или сохранёнными настройками. После 3c: очистить API ключ → сохранить → открыть KWallet → пустой записи не должно быть.

---

## Сессия 4 — Translation correctness (context, network guard, Mutex)

**Приоритет: СРЕДНИЙ** — тихие регрессии в качестве переводов.

### Задачи

- [ ] **4a. context_size всегда 4096 для spec managed-llama** (`src-tauri/src/translation.rs:119`)
  - Проблема: `entry.min_ram_bytes.map(|_| 4096u32).unwrap_or(4096)` всегда = 4096. `ModelCatalogEntry` имеет `managed_context_size` поле, которое здесь игнорируется.
  - Фикс: заменить на `entry.managed_context_size.unwrap_or(4096)` (или аналог по полю структуры).

- [ ] **4b. api_provider_enabled не проверяется для OpenAiCompatible** (`src-tauri/src/translation.rs:166`)
  - Проблема: `translate_openai_compatible` не вызывает `ensure_network_enabled`. Пользователь отключил "Use online APIs" — но OpenAI-compatible endpoint всё равно работает.
  - Фикс: добавить `ensure_network_enabled(config)?;` в начало `translate_openai_compatible`.

- [ ] **4c. MODEL_CACHE Mutex удерживается на весь inference** (`src-tauri/src/engines/onnx_mt.rs:145`)
  - Проблема: lock держится через 512 decode шагов (~10 сек на CPU). `unload_model()` из UI блокируется всё это время.
  - Фикс: взять модель из кеша (клонировать Arc или взять owned), освободить lock, затем запустить inference без lock. Потребует изменение типа кеша — вместо `LoadedOnnxModel` хранить `Arc<LoadedOnnxModel>`. Если рефактор слишком большой — добавить таймаут на lock с понятной ошибкой.

- [ ] **4d. TranslateGemma prompt format деградировал** (`src-tauri/src/translation.rs:93`)
  - Проблема: `translate_via_spec_llama_chat` удалена. TranslateGemma теперь получает plain text вместо структурированного `{source_lang_code, target_lang_code, text}` content объекта.
  - Фикс: восстановить специальный prompt format для TranslateGemma в `translate_spec_managed_llama` при `entry.prompt_style == PromptStyle::Chat` — либо через отдельный path, либо через поле в `ModelCatalogEntry`.

### Верификация
После 4b: зайти в Settings → отключить "Use online APIs" → попробовать OpenAI-compatible перевод → должна быть ошибка. После 4c: нажать "Unload model" в процессе перевода → кнопка должна реагировать немедленно.

---

## Сессия 5 — UI гонки, bandaids, maintenance traps

**Приоритет: НИЗКИЙ** — edge cases и технический долг.

### Задачи

- [ ] **5a. reinstallModel TOCTOU гонка** (`src/routes/+page.svelte:1276`)
  - Проблема: читает module-level `error` между двумя `await` — другой event handler может изменить `error` в этот момент.
  - Фикс: использовать локальную переменную: `const removeError = await removeModel(id).catch(e => e.message); if (removeError) return;` вместо проверки глобального `error`.

- [ ] **5b. copyTranslation() нет try/catch** (`src/routes/+page.svelte:1121`)
  - Проблема: `invoke('write_clipboard_text')` может упасть (wl-clipboard не установлен) → unhandled rejection → пользователь не видит ошибки.
  - Фикс: обернуть в try/catch, при ошибке установить `error = t('clipboardError')` или показать toast.

- [ ] **5c. LLAMA_SERVER_RELEASE не используется в URL** (`src-tauri/src/runtime.rs:730`)
  - Проблема: тег `b8987` захардкожен три раза прямо в URL строке. Обновление версии требует 4 правки, пропуск одной — тихо качает старый бинарник.
  - Фикс: построить URL из константы: `const LLAMA_SERVER_ZIP_URL: &str = concat!("https://github.com/ggml-org/llama.cpp/releases/download/", LLAMA_SERVER_RELEASE, "/llama-", LLAMA_SERVER_RELEASE, "-bin-ubuntu-x64.tar.gz");` (или через `format!` в lazy_static / `std::sync::OnceLock`).

- [ ] **5d. configure_ort_dylib без OnceLock** (`src-tauri/src/engines/onnx_mt.rs:81`)
  - Проблема: вызов дважды может изменить GPU_ACTIVE и рассинхронизировать его с уже загруженной ORT библиотекой.
  - Фикс: обернуть тело в `static INIT: OnceLock<()> = OnceLock::new(); INIT.get_or_init(|| { ... });`.

- [ ] **5e. WAYLATE_GPU_REEXEC — magic string без константы** (`src-tauri/src/lib.rs:1687`)
  - Проблема: env var имя `"WAYLATE_GPU_REEXEC"` встречается в двух местах как строка. Опечатка в одном → infinite exec loop.
  - Фикс: `const GPU_REEXEC_ENV: &str = "WAYLATE_GPU_REEXEC";` и использовать константу в обоих местах.

### Верификация
После 5c: найти `LLAMA_SERVER_RELEASE` в коде — URL должен содержать только эту константу без дублирующихся строк.

---

## Статус сессий

| Сессия | Тема | Статус |
|--------|------|--------|
| 1 | Translation: auto в промпте, guard, миграция | ⬜ не начата |
| 2 | GPU: Float16, атомарность, cancel, truncation | ⬜ не начата |
| 3 | Data safety: config, secrets, history | ⬜ не начата |
| 4 | Translation quality: context, network guard, Mutex | ⬜ не начата |
| 5 | UI races, bandaids, maintenance | ⬜ не начата |

---

## Промпты для запуска сессий

Скопируй нужный промпт в новую сессию:

**Сессия 1:**
```
Прочитай docs/fix-sessions.md. Выполни все задачи Сессии 1 (1a, 1b, 1c). После каждой задачи отметь её выполненной [x] в файле. В конце собери проект (npm run tauri build) и установи бинарник.
```

**Сессия 2:**
```
Прочитай docs/fix-sessions.md. Выполни все задачи Сессии 2 (2a, 2b, 2c, 2d). После каждой задачи отметь её выполненной [x] в файле. В конце собери проект (npm run tauri build) и установи бинарник.
```

**Сессия 3:**
```
Прочитай docs/fix-sessions.md. Выполни все задачи Сессии 3 (3a, 3b, 3c, 3d). После каждой задачи отметь её выполненной [x] в файле. В конце собери проект (npm run tauri build) и установи бинарник.
```

**Сессия 4:**
```
Прочитай docs/fix-sessions.md. Выполни все задачи Сессии 4 (4a, 4b, 4c, 4d). После каждой задачи отметь её выполненной [x] в файле. В конце собери проект (npm run tauri build) и установи бинарник.
```

**Сессия 5:**
```
Прочитай docs/fix-sessions.md. Выполни все задачи Сессии 5 (5a, 5b, 5c, 5d, 5e). После каждой задачи отметь её выполненной [x] в файле. В конце собери проект (npm run tauri build) и установи бинарник.
```
