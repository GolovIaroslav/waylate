use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: i64,
    pub created_at: DateTime<Utc>,
    pub source_lang: String,
    pub target_lang: String,
    pub model_id: String,
    pub source_text: String,
    pub translated_text: String,
}

pub fn init(db_path: &Path) -> Result<(), String> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let conn = Connection::open(db_path).map_err(|err| err.to_string())?;
    conn.execute_batch(
        "create table if not exists translations (
            id integer primary key autoincrement,
            created_at text not null,
            source_lang text not null,
            target_lang text not null,
            model_id text not null,
            source_text text not null,
            translated_text text not null
        );",
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

pub fn insert(db_path: &Path, entry: &HistoryEntry) -> Result<(), String> {
    init(db_path)?;
    let conn = Connection::open(db_path).map_err(|err| err.to_string())?;
    conn.execute(
        "insert into translations (created_at, source_lang, target_lang, model_id, source_text, translated_text)
         values (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            entry.created_at.to_rfc3339(),
            entry.source_lang,
            entry.target_lang,
            entry.model_id,
            entry.source_text,
            entry.translated_text
        ],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

pub fn list(db_path: &Path, limit: i64) -> Result<Vec<HistoryEntry>, String> {
    // SQLite treats a negative LIMIT as "no limit", which would load the entire table
    // into memory and freeze the UI. Clamp to a non-negative bound.
    let limit = limit.max(0);
    init(db_path)?;
    let conn = Connection::open(db_path).map_err(|err| err.to_string())?;
    let mut stmt = conn
        .prepare(
            "select id, created_at, source_lang, target_lang, model_id, source_text, translated_text
             from translations order by id desc limit ?1",
        )
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map([limit], |row| {
            let created: String = row.get(1)?;
            Ok(HistoryEntry {
                id: row.get(0)?,
                created_at: created
                    .parse::<DateTime<Utc>>()
                    .unwrap_or_else(|_| Utc::now()),
                source_lang: row.get(2)?,
                target_lang: row.get(3)?,
                model_id: row.get(4)?,
                source_text: row.get(5)?,
                translated_text: row.get(6)?,
            })
        })
        .map_err(|err| err.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
}

pub fn clear(db_path: &Path) -> Result<(), String> {
    init(db_path)?;
    let conn = Connection::open(db_path).map_err(|err| err.to_string())?;
    conn.execute("delete from translations", [])
        .map_err(|err| err.to_string())?;
    Ok(())
}

pub fn delete(db_path: &Path, id: i64) -> Result<(), String> {
    init(db_path)?;
    let conn = Connection::open(db_path).map_err(|err| err.to_string())?;
    conn.execute("delete from translations where id = ?1", [id])
        .map_err(|err| err.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    // A fresh, unique sqlite path per test so parallel runs never share a database.
    fn temp_db() -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("waylate-history-{}-{}.sqlite3", std::process::id(), n))
    }

    fn entry(text: &str) -> HistoryEntry {
        HistoryEntry {
            id: 0, // ignored on insert
            created_at: "2026-06-30T12:00:00Z".parse().unwrap(),
            source_lang: "auto".into(),
            target_lang: "eng_Latn".into(),
            model_id: "nllb-200-distilled-600m-onnx".into(),
            source_text: text.into(),
            translated_text: format!("translated:{text}"),
        }
    }

    #[test]
    fn insert_then_list_round_trips_fields() {
        let db = temp_db();
        insert(&db, &entry("привет")).expect("insert");
        let rows = list(&db, 10).expect("list");
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.source_text, "привет");
        assert_eq!(row.translated_text, "translated:привет");
        assert_eq!(row.target_lang, "eng_Latn");
        assert_eq!(row.created_at, entry("привет").created_at);
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn list_returns_newest_first() {
        let db = temp_db();
        insert(&db, &entry("first")).expect("insert");
        insert(&db, &entry("second")).expect("insert");
        let rows = list(&db, 10).expect("list");
        assert_eq!(rows.len(), 2);
        // Ordered by id desc, so the most recently inserted row comes first.
        assert_eq!(rows[0].source_text, "second");
        assert_eq!(rows[1].source_text, "first");
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn list_respects_positive_limit() {
        let db = temp_db();
        for text in ["a", "b", "c"] {
            insert(&db, &entry(text)).expect("insert");
        }
        assert_eq!(list(&db, 2).expect("list").len(), 2);
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn negative_limit_is_clamped_not_unbounded() {
        let db = temp_db();
        for text in ["a", "b", "c"] {
            insert(&db, &entry(text)).expect("insert");
        }
        // Regression: a negative LIMIT in SQLite means "no limit". It must be clamped to 0
        // instead of dumping the whole table.
        assert!(list(&db, -1).expect("list").is_empty());
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn list_on_missing_db_returns_empty() {
        let db = temp_db();
        // No file exists yet; init() should create the schema and return no rows.
        assert!(list(&db, 10).expect("list").is_empty());
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn corrupt_db_file_errors_without_panicking() {
        let db = temp_db();
        std::fs::write(&db, b"this is not a sqlite database").expect("write garbage");
        // A corrupt file must surface an Err, not panic the process.
        assert!(list(&db, 10).is_err());
        let _ = std::fs::remove_file(&db);
    }
}
