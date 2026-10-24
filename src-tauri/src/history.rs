//! Dictation history, stats and notes in a local SQLite file.

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
    /// Speaking speed over all dictations, words per minute.
    pub wpm: f64,
    /// Words per local day for the last 14 days, oldest first ("2026-10-24", words).
    pub daily: Vec<(String, i64)>,
    /// Apps you dictate into most, (exe, words), top 5.
    pub top_apps: Vec<(String, i64)>,
}

/// A note: typed or dictated in the Notes page, or written from a meeting.
#[derive(Serialize, Clone)]
pub struct Note {
    pub id: i64,
    pub created: i64,
    pub updated: i64,
    pub title: String,
    pub body: String,
    /// "note" | "meeting"
    pub kind: String,
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
             CREATE INDEX IF NOT EXISTS entries_ts ON entries(ts);
             CREATE TABLE IF NOT EXISTS notes (
                id INTEGER PRIMARY KEY, created INTEGER NOT NULL, updated INTEGER NOT NULL,
                title TEXT NOT NULL, body TEXT NOT NULL, kind TEXT NOT NULL DEFAULT 'note');",
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
        let wpm = if audio_ms > 0 { total_words as f64 / (audio_ms as f64 / 60_000.0) } else { 0.0 };
        let daily = (0..14)
            .rev()
            .map(|back| {
                let d = today - Duration::days(back);
                let from = start_of(d);
                let to = start_of(d + Duration::days(1));
                let w: i64 = db
                    .query_row(
                        "SELECT COALESCE(SUM(words),0) FROM entries WHERE ts >= ?1 AND ts < ?2",
                        params![from, to],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                (d.format("%Y-%m-%d").to_string(), w)
            })
            .collect();
        let mut st = db.prepare(
            "SELECT app, SUM(words) FROM entries WHERE app IS NOT NULL GROUP BY app ORDER BY 2 DESC LIMIT 5",
        )?;
        let top_apps = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.filter_map(Result::ok).collect();
        Ok(Stats { total_words, total_dictations, words_today, words_week, streak_days: streak, minutes_saved, wpm, daily, top_apps })
    }

    /// Every dictation, newest first, as CSV or plain text, written to `path`.
    pub fn export(&self, path: &Path) -> anyhow::Result<usize> {
        let all = self.list(i64::MAX, 0, None)?;
        let csv = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("csv"));
        let when = |ts: i64| {
            use chrono::TimeZone;
            chrono::Local.timestamp_opt(ts, 0).single().map(|d| d.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default()
        };
        let mut out = String::new();
        if csv {
            let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
            out.push_str("time,app,words,seconds,text,raw\r\n");
            for e in &all {
                out.push_str(&format!(
                    "{},{},{},{:.1},{},{}\r\n",
                    when(e.ts),
                    q(e.app.as_deref().unwrap_or("")),
                    e.words,
                    e.audio_ms as f64 / 1000.0,
                    q(&e.text),
                    q(&e.raw)
                ));
            }
        } else {
            for e in &all {
                out.push_str(&format!("{}  {}\r\n{}\r\n\r\n", when(e.ts), e.app.as_deref().unwrap_or(""), e.text));
            }
        }
        std::fs::write(path, out)?;
        Ok(all.len())
    }

    // ---------- notes ----------

    pub fn notes(&self, query: Option<String>) -> anyhow::Result<Vec<Note>> {
        let db = self.db.lock();
        let q = format!("%{}%", query.unwrap_or_default());
        let mut st = db.prepare(
            "SELECT id, created, updated, title, body, kind FROM notes
             WHERE title LIKE ?1 OR body LIKE ?1 ORDER BY updated DESC LIMIT 500",
        )?;
        let rows = st.query_map(params![q], |r| {
            Ok(Note { id: r.get(0)?, created: r.get(1)?, updated: r.get(2)?, title: r.get(3)?, body: r.get(4)?, kind: r.get(5)? })
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Saves a note; with no id it makes a new one. Returns the note's id.
    pub fn save_note(&self, id: Option<i64>, title: &str, body: &str, kind: &str) -> anyhow::Result<i64> {
        let db = self.db.lock();
        let now = chrono::Utc::now().timestamp();
        match id {
            Some(id) => {
                db.execute("UPDATE notes SET title = ?1, body = ?2, updated = ?3 WHERE id = ?4", params![title, body, now, id])?;
                Ok(id)
            }
            None => {
                db.execute(
                    "INSERT INTO notes (created, updated, title, body, kind) VALUES (?1,?1,?2,?3,?4)",
                    params![now, title, body, kind],
                )?;
                Ok(db.last_insert_rowid())
            }
        }
    }

    pub fn delete_note(&self, id: i64) -> anyhow::Result<()> {
        self.db.lock().execute("DELETE FROM notes WHERE id = ?1", params![id])?;
        Ok(())
    }
}
