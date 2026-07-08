# Model profile notes

Waylate keeps model support explicit. Each built-in profile declares its engine,
language list and setup hint. This is less magical than accepting any Hugging
Face id, but it avoids a bad first-run experience where the app downloads a
model it cannot execute.

## Hy-MT GGUF

Use this profile with a local OpenAI-compatible server. For example, start
`llama.cpp` server with a translation GGUF model and keep Waylate pointed at:

```text
http://127.0.0.1:8080/v1/chat/completions
```

Waylate sends a strict translation prompt and expects a Chat Completions style
response.

## NLLB / CTranslate2

Use this profile when you have a converted CTranslate2 model directory. The
helper command stays separate:

```bash
waylate-ct2-translate --model ./nllb-ct2 --tokenizer facebook/nllb-200-distilled-600M --source eng_Latn --target rus_Cyrl "hello"
```

This keeps the desktop app small. Python ML dependencies stay optional instead of
bundled into the GUI.

## API profiles

Waylate includes DeepL and Google as off-by-default providers for comparison and
fallback. Selecting them requires enabling network providers in Settings and
storing an API key in Secret Service.
