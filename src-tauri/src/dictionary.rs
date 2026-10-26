use crate::AppState;
use chrono::Utc;
use reqwest::Url;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{fs::OpenOptions, io::Write, path::Path, time::Duration};
use tauri::State;

const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_WORD_LENGTH: usize = 64;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordStatus {
    pub seen_count: i64,
    pub saved: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VocabularyItem {
    pub word: String,
    pub language: String,
    pub translation: String,
    pub definition: String,
    pub example: String,
    pub seen_count: i64,
    pub first_seen: String,
    pub last_seen: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Meaning {
    pub part_of_speech: String,
    pub definition: String,
    pub examples: Vec<String>,
    pub synonyms: Vec<String>,
    pub translations: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryResult {
    pub word: String,
    pub language: String,
    pub pronunciations: Vec<String>,
    pub meanings: Vec<Meaning>,
    pub source_url: Option<String>,
    pub previous_lookups: i64,
    pub saved: bool,
}

#[derive(Debug, Deserialize)]
struct ApiResult {
    #[serde(default)]
    entries: Vec<ApiEntry>,
    source: Option<ApiSource>,
}

#[derive(Debug, Deserialize)]
struct ApiSource {
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiEntry {
    language: ApiLanguage,
    #[serde(default)]
    part_of_speech: String,
    #[serde(default)]
    pronunciations: Vec<ApiPronunciation>,
    #[serde(default)]
    senses: Vec<ApiSense>,
    #[serde(default)]
    synonyms: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ApiLanguage {
    code: String,
}

#[derive(Debug, Deserialize)]
struct ApiPronunciation {
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiSense {
    definition: String,
    #[serde(default)]
    examples: Vec<String>,
    #[serde(default)]
    synonyms: Vec<String>,
    #[serde(default)]
    translations: Vec<ApiTranslation>,
}

#[derive(Debug, Deserialize)]
struct ApiTranslation {
    language: ApiLanguage,
    word: String,
}

fn normalize_word(word: &str) -> Result<String, String> {
    let word = word.trim();
    if word.is_empty()
        || word.chars().count() > MAX_WORD_LENGTH
        || !word.chars().next().is_some_and(char::is_alphabetic)
        || !word.chars().last().is_some_and(char::is_alphabetic)
        || !word
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '\'' | '’' | '-'))
    {
        return Err("Dictionary lookup accepts one word only (up to 64 characters).".into());
    }
    Ok(word.to_string())
}

fn language_code(raw: &str) -> String {
    let short = raw.split('_').next().unwrap_or("auto").to_ascii_lowercase();
    match short.as_str() {
        "eng" => "en".into(),
        "rus" => "ru".into(),
        "slk" => "sk".into(),
        "ces" => "cs".into(),
        "deu" => "de".into(),
        "fra" => "fr".into(),
        "spa" => "es".into(),
        "ita" => "it".into(),
        "por" => "pt".into(),
        "ukr" => "uk".into(),
        "pol" => "pl".into(),
        "auto" => "all".into(),
        _ if (2..=3).contains(&short.len()) && short.chars().all(|c| c.is_ascii_lowercase()) => {
            short
        }
        _ => "all".into(),
    }
}

fn db_open(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let connection = Connection::open(path).map_err(|e| e.to_string())?;
    connection
        .busy_timeout(Duration::from_secs(2))
        .map_err(|e| e.to_string())?;
    db_init(&connection)?;
    Ok(connection)
}

fn db_init(db: &Connection) -> Result<(), String> {
    db.execute_batch(
        "create table if not exists vocabulary (
            language text not null,
            key text not null,
            word text not null,
            translation text not null default '',
            definition text not null default '',
            example text not null default '',
            seen_count integer not null default 0,
            saved integer not null default 0,
            first_seen text not null,
            last_seen text not null,
            primary key (language, key)
        );
        create index if not exists saved_vocabulary on vocabulary(saved, last_seen);",
    )
    .map_err(|e| e.to_string())
}

fn db_status(db: &Connection, language: &str, word: &str) -> Result<WordStatus, String> {
    let key = word.to_lowercase();
    let query = if language == "all" {
        "select coalesce(sum(seen_count), 0), coalesce(max(saved), 0) from vocabulary where key = ?1"
    } else {
        "select coalesce(sum(seen_count), 0), coalesce(max(saved), 0) from vocabulary where key = ?1 and language = ?2"
    };
    let status = if language == "all" {
        db.query_row(query, [&key], |row| {
            Ok(WordStatus {
                seen_count: row.get(0)?,
                saved: row.get::<_, i64>(1)? != 0,
            })
        })
    } else {
        db.query_row(query, params![key, language], |row| {
            Ok(WordStatus {
                seen_count: row.get(0)?,
                saved: row.get::<_, i64>(1)? != 0,
            })
        })
    };
    status.map_err(|e| e.to_string())
}

fn db_record(
    db: &Connection,
    language: &str,
    word: &str,
    translation: &str,
) -> Result<WordStatus, String> {
    let previous = db_status(db, language, word)?;
    let now = Utc::now().to_rfc3339();
    db.execute(
        "insert into vocabulary (language, key, word, translation, seen_count, first_seen, last_seen)
         values (?1, ?2, ?3, ?4, 1, ?5, ?5)
         on conflict(language, key) do update set
           word = excluded.word,
           translation = excluded.translation,
           seen_count = vocabulary.seen_count + 1,
           last_seen = excluded.last_seen",
        params![language, word.to_lowercase(), word, translation, now],
    ).map_err(|e| e.to_string())?;
    Ok(previous)
}

fn db_save(
    db: &Connection,
    language: &str,
    word: &str,
    translation: &str,
    definition: &str,
    example: &str,
    saved: bool,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    db.execute(
        "insert into vocabulary (language, key, word, translation, definition, example, saved, first_seen, last_seen)
         values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
         on conflict(language, key) do update set
           saved = excluded.saved,
           translation = case when excluded.translation != '' then excluded.translation else vocabulary.translation end,
           definition = case when excluded.definition != '' then excluded.definition else vocabulary.definition end,
           example = case when excluded.example != '' then excluded.example else vocabulary.example end",
        params![language, word.to_lowercase(), word, translation, definition, example, i64::from(saved), now],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

fn db_list(db: &Connection, limit: i64) -> Result<Vec<VocabularyItem>, String> {
    let mut query = db.prepare(
        "select word, language, translation, definition, example, seen_count, first_seen, last_seen
         from vocabulary where saved = 1 order by last_seen desc limit ?1",
    ).map_err(|e| e.to_string())?;
    let rows = query
        .query_map([limit], |row| {
            Ok(VocabularyItem {
                word: row.get(0)?,
                language: row.get(1)?,
                translation: row.get(2)?,
                definition: row.get(3)?,
                example: row.get(4)?,
                seen_count: row.get(5)?,
                first_seen: row.get(6)?,
                last_seen: row.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

fn dictionary_enabled(state: &State<'_, AppState>) -> Result<(), String> {
    if !crate::config::load(&state.paths)?.dictionary_enabled {
        return Err("Enable the dictionary in Settings first.".into());
    }
    Ok(())
}

fn parse_dictionary(
    raw: &[u8],
    word: &str,
    requested_lang: &str,
    target_lang: &str,
) -> Result<DictionaryResult, String> {
    let mut api: ApiResult =
        serde_json::from_slice(raw).map_err(|e| format!("Invalid dictionary response: {e}"))?;
    let language = if requested_lang == "all" {
        let preferred = if word.chars().any(|c| ('\u{0400}'..='\u{052f}').contains(&c)) {
            "ru"
        } else {
            "en"
        };
        if api.entries.iter().any(|e| e.language.code == preferred) {
            preferred.to_string()
        } else {
            api.entries
                .first()
                .map(|e| e.language.code.clone())
                .unwrap_or_else(|| "all".into())
        }
    } else {
        requested_lang.to_string()
    };
    api.entries.retain(|e| e.language.code == language);
    let mut pronunciations = Vec::new();
    let mut meanings = Vec::new();
    for entry in api.entries {
        for pron in entry.pronunciations {
            if let Some(text) = pron.text.filter(|t| !t.is_empty()) {
                if !pronunciations.contains(&text) && pronunciations.len() < 3 {
                    pronunciations.push(text);
                }
            }
        }
        for sense in entry.senses {
            if meanings.len() >= 6 {
                break;
            }
            if sense.definition.trim().is_empty() {
                continue;
            }
            let mut synonyms = sense.synonyms;
            if synonyms.is_empty() {
                synonyms = entry.synonyms.clone();
            }
            let translations = sense
                .translations
                .into_iter()
                .filter(|t| t.language.code == target_lang)
                .map(|t| t.word)
                .take(5)
                .collect();
            meanings.push(Meaning {
                part_of_speech: entry.part_of_speech.clone(),
                definition: sense.definition,
                examples: sense.examples.into_iter().take(2).collect(),
                synonyms: synonyms.into_iter().take(6).collect(),
                translations,
            });
        }
    }
    let source_url = api
        .source
        .and_then(|s| s.url)
        .filter(|u| u.starts_with("https://en.wiktionary.org/wiki/"));
    Ok(DictionaryResult {
        word: word.into(),
        language,
        pronunciations,
        meanings,
        source_url,
        previous_lookups: 0,
        saved: false,
    })
}

fn dictionary_url(word: &str, language: &str) -> Result<Url, String> {
    let mut url =
        Url::parse("https://freedictionaryapi.com/api/v1/entries/").map_err(|e| e.to_string())?;
    url.path_segments_mut()
        .map_err(|_| "Invalid dictionary URL")?
        .pop_if_empty()
        .push(language)
        .push(word);
    url.query_pairs_mut().append_pair("translations", "true");
    Ok(url)
}

async fn fetch_dictionary(
    word: &str,
    language: &str,
    target: &str,
) -> Result<DictionaryResult, String> {
    let url = dictionary_url(word, language)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .user_agent("Waylate/0.1 (dictionary feature; https://github.com/GolovIaroslav/waylate)")
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Dictionary connection: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Dictionary service returned HTTP {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_RESPONSE_BYTES as u64)
    {
        return Err("Dictionary entry is too large to display.".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err("Dictionary entry is too large to display.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    parse_dictionary(&bytes, word, language, target)
}

#[tauri::command]
pub fn dictionary_word_status(
    state: State<'_, AppState>,
    word: String,
    language: String,
) -> Result<WordStatus, String> {
    dictionary_enabled(&state)?;
    let word = normalize_word(&word)?;
    db_status(
        &db_open(&state.paths.data_dir.join("vocabulary.sqlite3"))?,
        &language_code(&language),
        &word,
    )
}

#[tauri::command]
pub async fn lookup_dictionary_word(
    state: State<'_, AppState>,
    word: String,
    language: String,
    target_lang: String,
    translation: String,
) -> Result<DictionaryResult, String> {
    dictionary_enabled(&state)?;
    let word = normalize_word(&word)?;
    let language = language_code(&language);
    let target = language_code(&target_lang);
    let path = state.paths.data_dir.join("vocabulary.sqlite3");
    let mut result = fetch_dictionary(&word, &language, &target).await?;
    let previous = db_record(&db_open(&path)?, &result.language, &word, &translation)?;
    result.previous_lookups = previous.seen_count;
    result.saved = previous.saved;
    Ok(result)
}

#[tauri::command]
pub fn save_vocabulary_word(
    state: State<'_, AppState>,
    word: String,
    language: String,
    translation: String,
    definition: String,
    example: String,
    saved: bool,
) -> Result<(), String> {
    dictionary_enabled(&state)?;
    let word = normalize_word(&word)?;
    db_save(
        &db_open(&state.paths.data_dir.join("vocabulary.sqlite3"))?,
        &language_code(&language),
        &word,
        &translation,
        &definition,
        &example,
        saved,
    )
}

#[tauri::command]
pub fn list_vocabulary(state: State<'_, AppState>) -> Result<Vec<VocabularyItem>, String> {
    dictionary_enabled(&state)?;
    db_list(
        &db_open(&state.paths.data_dir.join("vocabulary.sqlite3"))?,
        5000,
    )
}

fn spreadsheet_safe(value: &str, delimiter: char) -> String {
    let cleaned = value.replace(['\r', '\n', '\t'], " ");
    let cleaned = if cleaned.trim_start().starts_with(['=', '+', '-', '@']) {
        format!("'{cleaned}")
    } else {
        cleaned
    };
    if delimiter == ',' {
        format!("\"{}\"", cleaned.replace('"', "\"\""))
    } else {
        cleaned
    }
}

fn export_body(items: &[VocabularyItem], format: &str) -> Result<String, String> {
    if format == "json" {
        return serde_json::to_string_pretty(items).map_err(|e| e.to_string());
    }
    let delimiter = match format {
        "csv" => ',',
        "tsv" => '\t',
        _ => return Err("Unsupported export format.".into()),
    };
    let mut lines = vec![format!("word{delimiter}language{delimiter}translation{delimiter}definition{delimiter}example{delimiter}seen_count")];
    for item in items {
        let fields = [
            item.word.as_str(),
            &item.language,
            &item.translation,
            &item.definition,
            &item.example,
        ];
        let mut row = fields
            .iter()
            .map(|s| spreadsheet_safe(s, delimiter))
            .collect::<Vec<_>>();
        row.push(item.seen_count.to_string());
        lines.push(row.join(&delimiter.to_string()));
    }
    Ok(lines.join("\n") + "\n")
}

#[tauri::command]
pub fn export_vocabulary(state: State<'_, AppState>, format: String) -> Result<String, String> {
    dictionary_enabled(&state)?;
    if !matches!(format.as_str(), "json" | "csv" | "tsv") {
        return Err("Unsupported export format.".into());
    }
    let words = db_list(
        &db_open(&state.paths.data_dir.join("vocabulary.sqlite3"))?,
        50000,
    )?;
    let output = export_body(&words, &format)?;
    let home = std::env::var_os("HOME").ok_or("HOME is not set.")?;
    let downloads = Path::new(&home).join("Downloads");
    std::fs::create_dir_all(&downloads).map_err(|e| e.to_string())?;
    let date = Utc::now().format("%Y-%m-%d");
    for suffix in 0..1000 {
        let name = if suffix == 0 {
            format!("Waylate-vocabulary-{date}.{format}")
        } else {
            format!("Waylate-vocabulary-{date}-{suffix}.{format}")
        };
        let path = downloads.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(output.as_bytes())
                    .map_err(|e| e.to_string())?;
                return Ok(path.display().to_string());
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err.to_string()),
        }
    }
    Err("Too many vocabulary export files for today.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a live network connection to FreeDictionaryAPI"]
    fn public_api_prickly_contract() {
        let entry = tauri::async_runtime::block_on(fetch_dictionary("prickly", "en", "ru"))
            .expect("English dictionary lookup should work");
        assert!(entry.meanings.len() >= 2, "expected multiple word meanings");
        assert!(
            entry.source_url.is_some(),
            "expected Wiktionary attribution"
        );
    }

    #[test]
    fn dictionary_url_has_no_duplicate_path_separator() {
        assert_eq!(
            dictionary_url("prickly", "en").unwrap().as_str(),
            "https://freedictionaryapi.com/api/v1/entries/en/prickly?translations=true"
        );
    }

    #[test]
    fn validates_words_and_normalizes_languages() {
        assert_eq!(normalize_word("  prickly  ").unwrap(), "prickly");
        assert_eq!(normalize_word("človek").unwrap(), "človek");
        assert_eq!(normalize_word("молоко").unwrap(), "молоко");
        assert!(normalize_word("two words").is_err());
        assert!(normalize_word("a/b").is_err());
        assert_eq!(language_code("eng_Latn"), "en");
        assert_eq!(language_code("slk_Latn"), "sk");
        assert_eq!(language_code("auto"), "all");
    }

    #[test]
    fn saves_and_records_independently_of_full_history() {
        let db = Connection::open_in_memory().unwrap();
        db_init(&db).unwrap();
        assert_eq!(db_status(&db, "en", "prickly").unwrap().seen_count, 0);
        assert_eq!(
            db_record(&db, "en", "Prickly", "колючий")
                .unwrap()
                .seen_count,
            0
        );
        db_save(
            &db,
            "en",
            "prickly",
            "колючий",
            "Sharp",
            "a prickly cactus",
            true,
        )
        .unwrap();
        assert_eq!(
            db_record(&db, "en", "prickly", "раздражительный")
                .unwrap()
                .seen_count,
            1
        );
        let words = db_list(&db, 20).unwrap();
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].seen_count, 2);
        assert_eq!(words[0].definition, "Sharp");
        db_save(&db, "en", "prickly", "", "", "", false).unwrap();
        assert!(db_list(&db, 20).unwrap().is_empty());
        assert_eq!(db_status(&db, "en", "prickly").unwrap().seen_count, 2);
    }

    #[test]
    fn parses_multisense_entry_and_target_translations() {
        let raw = r#"{"entries":[{"language":{"code":"en"},"partOfSpeech":"adjective","pronunciations":[{"text":"/test/"}],"senses":[{"definition":"sharp","examples":["a prickly plant"],"translations":[{"language":{"code":"ru"},"word":"колючий"}]}],"synonyms":["spiny"]}],"source":{"url":"https://en.wiktionary.org/wiki/prickly"}}"#;
        let parsed = parse_dictionary(raw.as_bytes(), "prickly", "en", "ru").unwrap();
        assert_eq!(parsed.meanings[0].definition, "sharp");
        assert_eq!(parsed.meanings[0].translations, vec!["колючий"]);
        assert_eq!(parsed.meanings[0].synonyms, vec!["spiny"]);
        assert_eq!(parsed.pronunciations, vec!["/test/"]);
        assert!(parsed.source_url.is_some());
    }

    #[test]
    fn exports_escaped_and_spreadsheet_safe() {
        let item = VocabularyItem {
            word: "prickly".into(),
            language: "en".into(),
            translation: "=SUM(1,1)".into(),
            definition: "say \"hello\"".into(),
            example: "one\ntwo".into(),
            seen_count: 2,
            first_seen: "date".into(),
            last_seen: "date".into(),
        };
        let csv = export_body(&[item.clone()], "csv").unwrap();
        assert!(csv.contains("\"'=SUM(1,1)\""));
        assert!(csv.contains("\"say \"\"hello\"\"\""));
        assert!(export_body(&[item], "json").unwrap().contains("=SUM(1,1)"));
        assert!(export_body(&[], "xml").is_err());
    }
}
