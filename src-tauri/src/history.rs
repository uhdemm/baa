//! Dictation history + stats in a local SQLite file.

use parking_lot::Mutex;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;

#[derive(Serialize, Clone)]
pub struct Entry {
    pub id: i64,
    pub ts: i64,
    pub text: String,
    pub raw: String,
    pub app: Option<String>,
    pub words: i64,
    pub audio_ms: i64,
    pub model: Option<String>,
}

#[derive(Serialize, Clone, Default)]
pub struct Stats {
    pub total_words: i64,
    pub total_dictations: i64,
    pub words_today: i64,
    pub words_week: i64,
    pub streak_days: i64,
    /// Minutes saved vs typing at 40 words per minute, minus the time spent talking.
    pub minutes_saved: f64,
}

pub struct History {
    db: Mutex<Connection>,
}

impl History {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let db = Connection::open(path)?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS entries (
                id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, text TEXT NOT NULL, raw TEXT NOT NULL,
                app TEXT, words INTEGER NOT NULL, audio_ms INTEGER NOT NULL, model TEXT);
             CREATE INDEX IF NOT EXISTS entries_ts ON entries(ts);",
        )?;
        Ok(History { db: Mutex::new(db) })
    }

    pub fn add(&self, text: &str, raw: &str, app: Option<&str>, words: usize, audio_ms: u64, model: Option<&str>) -> anyhow::Result<()> {
        self.db.lock().execute(
            "INSERT INTO entries (ts, text, raw, app, words, audio_ms, model) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![chrono::Utc::now().timestamp(), text, raw, app, words as i64, audio_ms as i64, model],
        )?;
        Ok(())
    }

    pub fn list(&self, limit: i64, offset: i64, query: Option<String>) -> anyhow::Result<Vec<Entry>> {
        let db = self.db.lock();
        let q = format!("%{}%", query.unwrap_or_default());
        let mut st = db.prepare(
            "SELECT id, ts, text, raw, app, words, audio_ms, model FROM entries
             WHERE text LIKE ?1 ORDER BY ts DESC LIMIT ?2 OFFSET ?3",
        )?;
        let rows = st.query_map(params![q, limit, offset], |r| {
            Ok(Entry {
                id: r.get(0)?, ts: r.get(1)?, text: r.get(2)?, raw: r.get(3)?, app: r.get(4)?,
                words: r.get(5)?, audio_ms: r.get(6)?, model: r.get(7)?,
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn last_text(&self) -> Option<String> {
        self.db.lock().query_row("SELECT text FROM entries ORDER BY ts DESC LIMIT 1", [], |r| r.get(0)).ok()
    }

    pub fn delete(&self, id: i64) -> anyhow::Result<()> {
        self.db.lock().execute("DELETE FROM entries WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn clear(&self) -> anyhow::Result<()> {
        self.db.lock().execute("DELETE FROM entries", [])?;
        Ok(())
    }

    pub fn stats(&self) -> anyhow::Result<Stats> {
        use chrono::{Duration, Local, NaiveDate, TimeZone};
        let db = self.db.lock();
        let (total_words, total_dictations, audio_ms): (i64, i64, i64) = db.query_row(
            "SELECT COALESCE(SUM(words),0), COUNT(*), COALESCE(SUM(audio_ms),0) FROM entries",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let today = Local::now().date_naive();
        let start_of = |d: NaiveDate| Local.from_local_datetime(&d.and_hms_opt(0, 0, 0).unwrap()).unwrap().timestamp();
        let words_since = |ts: i64| -> i64 {
            db.query_row("SELECT COALESCE(SUM(words),0) FROM entries WHERE ts >= ?1", params![ts], |r| r.get(0))
                .unwrap_or(0)
        };
        let words_today = words_since(start_of(today));
        let words_week = words_since(start_of(today - Duration::days(6)));

        // Streak: consecutive local days with at least one dictation, ending today or yesterday.
        let mut st = db.prepare("SELECT DISTINCT date(ts, 'unixepoch', 'localtime') FROM entries ORDER BY 1 DESC LIMIT 400")?;
        let days: Vec<NaiveDate> = st
            .query_map([], |r| r.get::<_, String>(0))?
            .filter_map(Result::ok)
            .filter_map(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
            .collect();
        let mut streak = 0;
        let mut expect = today;
        if days.first() == Some(&(today - Duration::days(1))) {
            expect = today - Duration::days(1);
        }
        for d in days {
            if d == expect {
                streak += 1;
                expect -= Duration::days(1);
            } else if d < expect {
                break;
            }
        }
        let minutes_saved = (total_words as f64 / 40.0 - audio_ms as f64 / 60_000.0).max(0.0);
        Ok(Stats { total_words, total_dictations, words_today, words_week, streak_days: streak, minutes_saved })
    }
}
