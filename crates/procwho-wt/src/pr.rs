//! Optional GitHub confirmation of merged pull requests (`--pr`), via the `gh` CLI.
//!
//! One `gh pr list --state merged --limit 200` per repository with a GitHub remote, cached in
//! `<state>/worktree-prs.json` for [`PR_TTL_SECS`]. Only the repository's `owner/name` (taken
//! from its own remote URL) is passed to `gh`. Tokens never pass through procwho: `gh` uses its
//! own stored login, and any token-like word in its error output is redacted.

use crate::git::{self, GitOut};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Cached PR lists are reused for six hours.
pub const PR_TTL_SECS: i64 = 6 * 3600;
/// Timeout for one `gh` call.
pub const GH_TIMEOUT: Duration = Duration::from_secs(10);

/// One merged PR as `gh pr list --json number,headRefName,headRefOid,mergedAt` prints it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergedPr {
    pub number: u64,
    pub head_ref_name: String,
    pub head_ref_oid: String,
    pub merged_at: String,
}

/// What a worktree's branch matched among the merged PRs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrMerged {
    pub number: u64,
    pub merged_at: String,
    /// The PR was merged with exactly the worktree's current tip as its head.
    pub head_matches_tip: bool,
}

/// `owner/name` from a GitHub remote URL (https, ssh or scp-like), else `None`.
///
/// ```
/// use procwho_wt::pr::github_slug;
/// assert_eq!(github_slug("git@github.com:o/r.git").as_deref(), Some("o/r"));
/// assert_eq!(github_slug("https://github.com/o/r").as_deref(), Some("o/r"));
/// assert_eq!(github_slug("ssh://git@github.com/o/r.git").as_deref(), Some("o/r"));
/// assert_eq!(github_slug("https://gitlab.com/o/r.git"), None);
/// ```
pub fn github_slug(url: &str) -> Option<String> {
    let url = url.trim();
    let rest = [
        "https://github.com/",
        "http://github.com/",
        "ssh://git@github.com/",
        "git@github.com:",
        "git://github.com/",
    ]
    .iter()
    .find_map(|p| url.strip_prefix(p))?;
    let rest = rest.trim_end_matches('/');
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let mut it = rest.split('/');
    let (owner, name) = (it.next()?, it.next()?);
    let ok = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    };
    (it.next().is_none() && ok(owner) && ok(name)).then(|| format!("{owner}/{name}"))
}

/// The merged PR for `branch`: one merged at exactly `tip` if there is one, else the most
/// recently merged PR with that head branch name.
pub fn match_branch(prs: &[MergedPr], branch: &str, tip: Option<&str>) -> Option<PrMerged> {
    let same: Vec<&MergedPr> = prs.iter().filter(|p| p.head_ref_name == branch).collect();
    let at_tip = tip.and_then(|t| same.iter().find(|p| p.head_ref_oid == t));
    let pick = at_tip.or_else(|| same.iter().max_by(|a, b| a.merged_at.cmp(&b.merged_at)))?;
    Some(PrMerged {
        number: pick.number,
        merged_at: pick.merged_at.clone(),
        head_matches_tip: at_tip.is_some(),
    })
}

/// Replace token-like words (`ghp_...`, `gho_...`, `github_pat_...`) with `[redacted]`.
///
/// ```
/// assert_eq!(procwho_wt::pr::redact("bad ghp_abc123 x"), "bad [redacted] x");
/// ```
pub fn redact(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let t = w.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
            let token = t.starts_with("github_pat_")
                || (t.len() > 4
                    && t.starts_with("gh")
                    && t.as_bytes()[3] == b'_'
                    && b"pousr".contains(&t.as_bytes()[2]));
            if token {
                "[redacted]"
            } else {
                w
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CacheEntry {
    fetched_at: i64,
    prs: Vec<MergedPr>,
}

/// PR lists per `owner/name`, persisted in `<state>/worktree-prs.json`.
#[derive(Debug, Default)]
pub struct PrCache {
    path: Option<PathBuf>,
    entries: BTreeMap<String, CacheEntry>,
    dirty: bool,
}

impl PrCache {
    pub fn load(state_dir: &Path) -> PrCache {
        let path = state_dir.join("worktree-prs.json");
        let entries = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        PrCache {
            path: Some(path),
            entries,
            dirty: false,
        }
    }

    /// Write the cache if it changed. Entries older than the TTL are dropped.
    pub fn save(&mut self, now: i64) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if !self.dirty {
            return Ok(());
        }
        self.entries.retain(|_, e| now - e.fetched_at < PR_TTL_SECS);
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_vec(&self.entries).map_err(std::io::Error::other)?,
        )?;
        std::fs::rename(&tmp, path)
    }

    fn fresh(&self, slug: &str, now: i64) -> Option<(Vec<MergedPr>, i64)> {
        let e = self.entries.get(slug)?;
        let age = now - e.fetched_at;
        (0..PR_TTL_SECS)
            .contains(&age)
            .then(|| (e.prs.clone(), age))
    }
}

/// Merged PRs of `slug`, from the cache or one `gh` call. `Ok((prs, cache age))`; the age is
/// `None` when `gh` was just called.
pub fn merged_prs(
    gh: &Path,
    slug: &str,
    cache: &std::sync::Mutex<PrCache>,
    now: i64,
) -> Result<(Vec<MergedPr>, Option<i64>), String> {
    if let Some((prs, age)) = cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .fresh(slug, now)
    {
        return Ok((prs, Some(age)));
    }
    let mut cmd = Command::new(gh);
    cmd.args([
        "pr", "list", "-R", slug, "--state", "merged", "--limit", "200",
    ])
    .args(["--json", "number,headRefName,headRefOid,mergedAt"])
    .current_dir("/")
    .env("GH_PROMPT_DISABLED", "1")
    .env("GH_NO_UPDATE_NOTIFIER", "1")
    .env("GH_SPINNER_DISABLED", "1")
    .env("NO_COLOR", "1");
    let prs: Vec<MergedPr> = match git::exec(cmd, GH_TIMEOUT) {
        GitOut::Done {
            code: 0, stdout, ..
        } => serde_json::from_str(&stdout)
            .map_err(|e| format!("gh pr list: unexpected output ({e})"))?,
        GitOut::Done { code, stderr, .. } => {
            let first = stderr
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("")
                .trim();
            let first: String = first.chars().take(160).collect();
            return Err(format!("gh pr list exit {code}: {}", redact(&first)));
        }
        GitOut::TimedOut => {
            return Err(format!(
                "gh pr list timed out after {}s",
                GH_TIMEOUT.as_secs()
            ))
        }
        GitOut::NotInstalled => {
            return Err("GitHub PR metadata: optional gh tool not installed".into())
        }
        GitOut::SpawnFailed(_) => return Err("GitHub PR metadata: gh could not be started".into()),
    };
    let mut c = cache.lock().unwrap_or_else(|e| e.into_inner());
    c.entries.insert(
        slug.to_string(),
        CacheEntry {
            fetched_at: now,
            prs: prs.clone(),
        },
    );
    c.dirty = true;
    Ok((prs, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pr(n: u64, b: &str, oid: &str, at: &str) -> MergedPr {
        MergedPr {
            number: n,
            head_ref_name: b.into(),
            head_ref_oid: oid.into(),
            merged_at: at.into(),
        }
    }

    #[test]
    fn absent_optional_gh_is_informational() {
        let cache = std::sync::Mutex::new(PrCache::default());
        let note = merged_prs(
            Path::new("/nonexistent/procwho-gh"),
            "example/repo",
            &cache,
            0,
        )
        .unwrap_err();
        assert_eq!(note, "GitHub PR metadata: optional gh tool not installed");
        assert!(!note.contains("unavailable") && !note.contains("failed"));
    }

    #[test]
    fn branch_matching() {
        let prs = vec![
            pr(1, "feat", "aaa", "2026-01-01T00:00:00Z"),
            pr(2, "feat", "bbb", "2026-02-01T00:00:00Z"),
            pr(3, "other", "ccc", "2026-03-01T00:00:00Z"),
        ];
        let m = match_branch(&prs, "feat", Some("aaa")).unwrap();
        assert_eq!((m.number, m.head_matches_tip), (1, true));
        let m = match_branch(&prs, "feat", Some("zzz")).unwrap();
        assert_eq!((m.number, m.head_matches_tip), (2, false));
        assert_eq!(match_branch(&prs, "nope", Some("aaa")), None);
        assert!(!match_branch(&prs, "feat", None).unwrap().head_matches_tip);
    }

    #[test]
    fn slug_and_redaction() {
        assert_eq!(github_slug("git@github.com:o/r/extra.git"), None);
        assert_eq!(github_slug("/local/path/origin.git"), None);
        assert_eq!(
            github_slug("https://github.com/o/r.git/").as_deref(),
            Some("o/r")
        );
        assert_eq!(
            redact("token gho_XYZ, github_pat_11AB rest"),
            "token [redacted] [redacted] rest"
        );
        assert_eq!(redact("gh auth login"), "gh auth login");
    }

    #[test]
    fn cache_ttl() {
        let mut c = PrCache::default();
        c.entries.insert(
            "o/r".into(),
            CacheEntry {
                fetched_at: 1000,
                prs: vec![pr(1, "b", "x", "t")],
            },
        );
        assert!(c.fresh("o/r", 1000 + PR_TTL_SECS - 1).is_some());
        assert!(c.fresh("o/r", 1000 + PR_TTL_SECS).is_none());
        assert!(c.fresh("x/y", 1000).is_none());
    }
}
