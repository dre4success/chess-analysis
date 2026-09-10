use super::{Job, Result};
use crate::review::GameReview;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter, types::Value};
use std::{path::Path, sync::Mutex};

pub struct Store(Mutex<Connection>);

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let db = Connection::open(path)?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS jobs (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               username TEXT NOT NULL, pace TEXT NOT NULL, status TEXT NOT NULL,
               snapshot TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS jobs_queue ON jobs(status, id);
             CREATE INDEX IF NOT EXISTS jobs_player ON jobs(username, pace, id);
             CREATE TABLE IF NOT EXISTS analyses (
               cache_key TEXT PRIMARY KEY, review TEXT NOT NULL
             );",
        )?;
        let store = Self(Mutex::new(db));
        // The worker owns one process lock. Any running jobs here belonged to
        // the previous process and can safely resume from the per-game cache.
        let ids = store.ids_with_status("running")?;
        for id in ids {
            store.update(id, |job| {
                job.status = "queued".into();
                job.message = "Resuming saved review".into();
            })?;
        }
        Ok(store)
    }

    fn ids_with_status(&self, status: &str) -> Result<Vec<i64>> {
        let db = self.0.lock().map_err(|_| "database lock poisoned")?;
        Ok(db
            .prepare("SELECT id FROM jobs WHERE status = ?1 ORDER BY id")?
            .query_map([status], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn enqueue(&self, mut job: Job) -> Result<Job> {
        let mut connection = self.0.lock().map_err(|_| "database lock poisoned")?;
        let db = connection.transaction()?;
        // One active job per player/pace. Repeated clicks never launch another
        // engine process or lose the ID needed to reconnect to a review.
        let existing: Option<String> = db
            .query_row(
                "SELECT snapshot FROM jobs WHERE username=?1 AND pace=?2
             AND status IN ('queued','running') ORDER BY id DESC LIMIT 1",
                params![job.username, job.pace],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(snapshot) = existing {
            return Ok(serde_json::from_str(&snapshot)?);
        }
        let count: i64 = db.query_row(
            "SELECT count(*) FROM jobs WHERE status IN ('queued','running')",
            [],
            |r| r.get(0),
        )?;
        if count >= 32 {
            return Err("review queue is full; try again later".into());
        }
        db.execute(
            "INSERT INTO jobs(username,pace,status,snapshot) VALUES(?1,?2,?3,?4)",
            params![
                job.username,
                job.pace,
                job.status,
                serde_json::to_string(&job)?
            ],
        )?;
        job.id = db.last_insert_rowid();
        db.execute(
            "UPDATE jobs SET snapshot=?1 WHERE id=?2",
            params![serde_json::to_string(&job)?, job.id],
        )?;
        db.commit()?;
        Ok(job)
    }

    pub fn get(&self, id: i64) -> Result<Option<Job>> {
        let db = self.0.lock().map_err(|_| "database lock poisoned")?;
        let snapshot: Option<String> = db
            .query_row("SELECT snapshot FROM jobs WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .optional()?;
        snapshot
            .map(|value| serde_json::from_str(&value).map_err(Into::into))
            .transpose()
    }

    pub fn history(
        &self,
        usernames: &[String],
        offset: i64,
        limit: i64,
    ) -> Result<(Vec<Job>, i64)> {
        if usernames.is_empty() {
            return Ok((Vec::new(), 0));
        }
        let db = self.0.lock().map_err(|_| "database lock poisoned")?;
        let filter = format!("username IN ({})", vec!["?"; usernames.len()].join(","));
        let mut parameters: Vec<Value> = usernames.iter().cloned().map(Value::from).collect();
        let total = db.query_row(
            &format!("SELECT count(*) FROM jobs WHERE {filter}"),
            params_from_iter(&parameters),
            |row| row.get(0),
        )?;
        parameters.extend([Value::from(limit), Value::from(offset)]);
        let snapshots = db
            .prepare(&format!(
                "SELECT snapshot FROM jobs WHERE {filter} ORDER BY id DESC LIMIT ? OFFSET ?"
            ))?
            .query_map(params_from_iter(&parameters), |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let jobs = snapshots
            .iter()
            .map(|s| serde_json::from_str(s).map_err(Into::into))
            .collect::<Result<Vec<Job>>>()?;
        Ok((jobs, total))
    }

    pub fn queue_status(&self) -> Result<serde_json::Value> {
        let db = self.0.lock().map_err(|_| "database lock poisoned")?;
        let (queued, running, failed, current_job): (i64, i64, i64, Option<i64>) = db.query_row(
            "SELECT coalesce(sum(status='queued'),0), coalesce(sum(status='running'),0),
                    coalesce(sum(status='failed'),0), min(CASE WHEN status='running' THEN id END)
             FROM jobs",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        let last_error: Option<String> = db
            .query_row(
                "SELECT json_extract(snapshot,'$.error') FROM jobs
             WHERE status='failed' ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        Ok(serde_json::json!({
            "queued": queued, "running": running, "failed": failed,
            "current_job": current_job, "last_error": last_error,
        }))
    }

    pub fn next(&self) -> Result<Option<Job>> {
        self.ids_with_status("queued")?
            .first()
            .map(|id| self.get(*id))
            .transpose()
            .map(Option::flatten)
    }

    pub fn update(&self, id: i64, change: impl FnOnce(&mut Job)) -> Result<()> {
        let db = self.0.lock().map_err(|_| "database lock poisoned")?;
        let value: String =
            db.query_row("SELECT snapshot FROM jobs WHERE id=?1", [id], |r| r.get(0))?;
        let mut job: Job = serde_json::from_str(&value)?;
        // Cancellation is terminal, including if a worker finishes a position
        // at the same moment as the request arrives.
        if job.status == "cancelled" {
            return Ok(());
        }
        change(&mut job);
        job.updated_at = chrono::Utc::now().to_rfc3339();
        db.execute(
            "UPDATE jobs SET status=?1,snapshot=?2 WHERE id=?3",
            params![job.status, serde_json::to_string(&job)?, id],
        )?;
        Ok(())
    }

    pub fn cached(&self, key: &str) -> Result<Option<GameReview>> {
        let db = self.0.lock().map_err(|_| "database lock poisoned")?;
        let value: Option<String> = db
            .query_row(
                "SELECT review FROM analyses WHERE cache_key=?1",
                [key],
                |r| r.get(0),
            )
            .optional()?;
        value
            .map(|s| serde_json::from_str(&s).map_err(Into::into))
            .transpose()
    }

    pub fn cache(&self, key: &str, review: &GameReview) -> Result<()> {
        self.0.lock().map_err(|_| "database lock poisoned")?.execute(
            "INSERT INTO analyses(cache_key,review) VALUES(?1,?2) ON CONFLICT(cache_key) DO UPDATE SET review=excluded.review",
            params![key,serde_json::to_string(review)?],
        )?;
        Ok(())
    }
}
