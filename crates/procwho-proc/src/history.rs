//! Bounded observation history in SQLite (system libsqlite3). 72 hour retention.
//!
//! Tables: `sample` (one row per observation), `wlmeta` (workload ids), `wl` (one compact row
//! per workload per observation) and `inst` (one row per process instance, keyed by the kernel
//! unique id). The database lives only in the state directory.

use crate::model::*;
use crate::snapshot::Snapshot;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS sample(ts REAL PRIMARY KEY, procs INT, collect_ms REAL, total_fp INT);
CREATE TABLE IF NOT EXISTS wlmeta(id INTEGER PRIMARY KEY, key TEXT UNIQUE, name TEXT, kind TEXT, last_ts REAL);
CREATE TABLE IF NOT EXISTS wl(ts REAL, id INT, procs INT, fp INT, rss INT,
    cpu_delta_ns INT, interval_s REAL, births INT, deaths INT, PRIMARY KEY(ts,id)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS inst(uniq INT PRIMARY KEY, pid INT, start REAL, key TEXT, sig TEXT, cmd TEXT,
    first_ts REAL, last_ts REAL, cpu_first INT, cpu_last INT, fp_first INT, fp_last INT, fp_peak INT);
CREATE INDEX IF NOT EXISTS inst_last_ts ON inst(last_ts);
";

/// One stored observation of one workload.
#[derive(Debug, Clone, Copy)]
pub struct TrendRow {
    pub ts: f64,
    pub procs: i64,
    pub fp: i64,
    /// CPU nanoseconds used since the previous observation; None for the first one.
    pub cpu_delta_ns: Option<i64>,
    pub interval_s: Option<f64>,
    pub births: i64,
    pub deaths: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct RecordInfo {
    pub samples: i64,
    pub interval_s: Option<f64>,
    pub db_bytes: u64,
}

pub struct History {
    conn: Connection,
    path: PathBuf,
}

fn err(e: rusqlite::Error) -> String {
    format!("history database: {e}")
}

fn db_path(state: &Path) -> PathBuf {
    state.join("history.db")
}

impl History {
    /// Opens (creating if needed) the database in `state`.
    pub fn open(state: &Path) -> Result<History, String> {
        std::fs::create_dir_all(state)
            .map_err(|e| format!("state dir {}: {e}", state.display()))?;
        let path = db_path(state);
        let conn = Connection::open(&path).map_err(err)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(err)?;
        // WAL: readers (`why`, `findings`) never block the sampler, and a crash mid-write
        // cannot corrupt earlier samples.
        let _mode: String = conn
            .query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))
            .map_err(err)?;
        conn.execute_batch(SCHEMA).map_err(err)?;
        Ok(History { conn, path })
    }

    /// Opens the database only if it already exists (read paths must not create state).
    pub fn open_existing(state: &Path) -> Option<History> {
        if db_path(state).exists() {
            History::open(state).ok()
        } else {
            None
        }
    }

    /// Stores one observation. Instances are keyed by the kernel's never-reused unique id.
    pub fn record(&mut self, snap: &Snapshot) -> Result<RecordInfo, String> {
        let tx = self.conn.transaction().map_err(err)?;
        let ts = snap.ts;
        let prev: Option<f64> = tx
            .query_row("SELECT max(ts) FROM sample", [], |r| r.get(0))
            .map_err(err)?;
        // uniq -> (key, cpu_last) for instances seen at the previous observation.
        let mut old: HashMap<i64, (String, i64)> = HashMap::new();
        if let Some(pv) = prev {
            let mut st = tx
                .prepare("SELECT uniq,key,cpu_last FROM inst WHERE last_ts=?")
                .map_err(err)?;
            let rows = st
                .query_map([pv], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                })
                .map_err(err)?;
            for r in rows.flatten() {
                old.insert(r.0, (r.1, r.2));
            }
        }
        // per workload key: cpu delta, births, deaths
        let mut agg: HashMap<&str, [i64; 3]> = HashMap::new();
        let mut live: std::collections::HashSet<i64> = std::collections::HashSet::new();
        {
            let mut upd = tx
                .prepare_cached("UPDATE inst SET last_ts=?,cpu_last=?,fp_last=?,fp_peak=max(fp_peak,?),key=? WHERE uniq=?")
                .map_err(err)?;
            let mut ins = tx
                .prepare_cached("INSERT OR REPLACE INTO inst VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .map_err(err)?;
            for p in &snap.procs {
                if !p.readable || p.uniq == 0 {
                    continue;
                }
                let u = p.uniq as i64;
                live.insert(u);
                let key = snap.workloads[p.wl].key.as_str();
                let a = agg.entry(key).or_insert([0; 3]);
                if let Some(o) = old.get(&u) {
                    a[0] += (p.cpu_ns as i64 - o.1).max(0);
                    upd.execute(params![
                        ts,
                        p.cpu_ns as i64,
                        p.fp as i64,
                        p.fp as i64,
                        key,
                        u
                    ])
                    .map_err(err)?;
                } else {
                    if let Some(pv) = prev {
                        a[1] += 1;
                        if p.start > pv {
                            a[0] += p.cpu_ns as i64;
                        }
                    }
                    let cmd: String = p.cmd.chars().take(300).collect();
                    ins.execute(params![
                        u,
                        p.pid,
                        p.start,
                        key,
                        crate::attribute::signature(p),
                        cmd,
                        ts,
                        ts,
                        p.cpu_ns as i64,
                        p.cpu_ns as i64,
                        p.fp as i64,
                        p.fp as i64,
                        p.fp as i64
                    ])
                    .map_err(err)?;
                }
            }
        }
        for (u, o) in &old {
            if !live.contains(u) {
                if let Some(a) = agg.get_mut(o.0.as_str()) {
                    a[2] += 1;
                } else {
                    // The workload no longer exists in this snapshot; its death count has no row to live in.
                }
            }
        }
        let iv = prev.map(|pv| ts - pv);
        {
            let mut meta = tx
                .prepare_cached("INSERT INTO wlmeta(key,name,kind,last_ts) VALUES(?,?,?,?) ON CONFLICT(key) DO UPDATE SET name=excluded.name, last_ts=excluded.last_ts")
                .map_err(err)?;
            let mut gid = tx
                .prepare_cached("SELECT id FROM wlmeta WHERE key=?")
                .map_err(err)?;
            let mut wl = tx
                .prepare_cached("INSERT OR REPLACE INTO wl VALUES(?,?,?,?,?,?,?,?,?)")
                .map_err(err)?;
            for w in &snap.workloads {
                let a = agg.get(w.key.as_str()).copied().unwrap_or([0; 3]);
                meta.execute(params![w.key, w.name, w.kind, ts])
                    .map_err(err)?;
                let id: i64 = gid.query_row([&w.key], |r| r.get(0)).map_err(err)?;
                let cpu = prev.map(|_| a[0]);
                wl.execute(params![
                    ts,
                    id,
                    w.procs as i64,
                    w.fp as i64,
                    w.rss as i64,
                    cpu,
                    iv,
                    a[1],
                    a[2]
                ])
                .map_err(err)?;
            }
        }
        tx.execute(
            "INSERT OR REPLACE INTO sample VALUES(?,?,?,?)",
            params![
                ts,
                snap.procs.len() as i64,
                snap.t_total * 1000.0,
                snap.footprint_sum() as i64
            ],
        )
        .map_err(err)?;
        let cut = ts - RETENTION_H * 3600.0;
        tx.execute("DELETE FROM sample WHERE ts<?", [cut])
            .map_err(err)?;
        tx.execute("DELETE FROM wl WHERE ts<?", [cut])
            .map_err(err)?;
        tx.execute("DELETE FROM inst WHERE last_ts<?", [cut])
            .map_err(err)?;
        tx.execute("DELETE FROM wlmeta WHERE last_ts<?", [cut])
            .map_err(err)?;
        let n: i64 = tx
            .query_row("SELECT count(*) FROM sample", [], |r| r.get(0))
            .map_err(err)?;
        tx.commit().map_err(err)?;
        let db_bytes = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        Ok(RecordInfo {
            samples: n,
            interval_s: iv,
            db_bytes,
        })
    }

    fn row(r: &rusqlite::Row<'_>, o: usize) -> rusqlite::Result<TrendRow> {
        Ok(TrendRow {
            ts: r.get(o)?,
            procs: r.get(o + 1)?,
            fp: r.get(o + 2)?,
            cpu_delta_ns: r.get(o + 3)?,
            interval_s: r.get(o + 4)?,
            births: r.get::<_, Option<i64>>(o + 5)?.unwrap_or(0),
            deaths: r.get::<_, Option<i64>>(o + 6)?.unwrap_or(0),
        })
    }

    /// Stored observations of one workload over the last `hours`, oldest first.
    pub fn trend(&self, key: &str, hours: f64) -> Vec<TrendRow> {
        let cut = crate::sys::now() - hours * 3600.0;
        let q = "SELECT ts,procs,fp,cpu_delta_ns,interval_s,births,deaths FROM wl \
                 WHERE id=(SELECT id FROM wlmeta WHERE key=?) AND ts>? ORDER BY ts";
        let Ok(mut st) = self.conn.prepare(q) else {
            return Vec::new();
        };
        let rows = st.query_map(params![key, cut], |r| Self::row(r, 0));
        rows.map(|it| it.flatten().collect()).unwrap_or_default()
    }

    /// Every workload's observations over the last `hours` in one pass, keyed by workload key.
    pub fn trends_all(&self, hours: f64) -> HashMap<String, Vec<TrendRow>> {
        let cut = crate::sys::now() - hours * 3600.0;
        let mut ids: HashMap<i64, String> = HashMap::new();
        if let Ok(mut st) = self.conn.prepare("SELECT id,key FROM wlmeta") {
            if let Ok(it) = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            {
                ids.extend(it.flatten());
            }
        }
        let mut out: HashMap<String, Vec<TrendRow>> = HashMap::new();
        let q = "SELECT id,ts,procs,fp,cpu_delta_ns,interval_s,births,deaths FROM wl WHERE ts>? ORDER BY ts";
        if let Ok(mut st) = self.conn.prepare(q) {
            if let Ok(it) = st.query_map([cut], |r| Ok((r.get::<_, i64>(0)?, Self::row(r, 1)?))) {
                for (id, row) in it.flatten() {
                    if let Some(k) = ids.get(&id) {
                        out.entry(k.clone()).or_default().push(row);
                    }
                }
            }
        }
        out
    }

    #[cfg(test)]
    fn pragma(&self, q: &str) -> String {
        self.conn.query_row(q, [], |r| r.get(0)).unwrap_or_default()
    }

    pub fn sample_count(&self) -> i64 {
        self.conn
            .query_row("SELECT count(*) FROM sample", [], |r| r.get(0))
            .optional()
            .ok()
            .flatten()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_in_wal_mode_with_last_ts_index() {
        let dir = std::env::temp_dir().join(format!("procwho-wal-{}", std::process::id()));
        let h = History::open(&dir).unwrap();
        assert_eq!(h.pragma("PRAGMA journal_mode"), "wal");
        let idx = h.pragma("SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='inst' AND sql LIKE '%last_ts%'");
        assert_eq!(idx, "inst_last_ts");
        drop(h);
        let _ = std::fs::remove_dir_all(dir);
    }
}
