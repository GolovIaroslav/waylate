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
