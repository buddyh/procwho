//! Verdict per worktree: a review label with every fact behind it. Never an instruction.

use serde::Serialize;

/// Review label, decided in this precedence:
/// `missing` > `orphaned-dir` > `in-use` > `has-work` > `merged-idle` > `merged-locked` >
/// `stale` > `active`,
/// with `unknown` whenever a fact needed for the decision could not be determined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    Missing,
    OrphanedDir,
    InUse,
    HasWork,
    MergedIdle,
    /// Merged, clean and idle, but `git worktree lock`ed.
    MergedLocked,
    Stale,
    Active,
    Unknown,
}

impl Verdict {
    pub const ALL: [Verdict; 9] = [
        Verdict::Missing,
        Verdict::OrphanedDir,
        Verdict::InUse,
        Verdict::HasWork,
        Verdict::MergedIdle,
        Verdict::MergedLocked,
        Verdict::Stale,
        Verdict::Active,
        Verdict::Unknown,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Missing => "missing",
            Verdict::OrphanedDir => "orphaned-dir",
            Verdict::InUse => "in-use",
            Verdict::HasWork => "has-work",
            Verdict::MergedIdle => "merged-idle",
            Verdict::MergedLocked => "merged-locked",
            Verdict::Stale => "stale",
            Verdict::Active => "active",
            Verdict::Unknown => "unknown",
        }
    }

    /// Parse a verdict name as printed by [`Verdict::as_str`].
    ///
    /// ```
    /// use procwho_wt::Verdict;
    /// assert_eq!(Verdict::parse("merged-idle"), Some(Verdict::MergedIdle));
    /// assert_eq!(Verdict::parse("nope"), None);
    /// ```
    pub fn parse(s: &str) -> Option<Verdict> {
        Verdict::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

/// The facts a verdict is decided from. `None` means "could not be determined".
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub exists: bool,
    /// Text explaining why git calls it prunable, if it does.
    pub prunable: Option<String>,
    /// Why this directory is an orphan, if it is one.
    pub orphan: Option<String>,
    pub in_use: Option<bool>,
    pub in_use_workloads: Vec<String>,
    pub in_use_procs: usize,
    pub tracked: Option<u32>,
    pub untracked: Option<u32>,
    pub unpushed: Option<u32>,
    pub merged: Option<bool>,
    /// Every change on the branch is already in the default branch (squash/rebase merge).
    pub content_merged: Option<bool>,
    /// Number of a GitHub PR merged at exactly this tip (`--pr`).
    pub pr_merged_at_tip: Option<u64>,
    /// Ref the merge check used, e.g. `origin/main`.
    pub default_ref: Option<String>,
    pub locked: bool,
    pub lock_reason: Option<String>,
    /// Seconds since last activity.
    pub idle_secs: Option<i64>,
    pub stale_days: u32,
    /// Extra context lines (upstream gone, no remotes, ...). Added to reasons as-is.
    pub context: Vec<String>,
}

fn days(secs: i64) -> String {
    if secs >= 86400 {
        format!("{}d", secs / 86400)
    } else {
        format!("{}h", secs / 3600)
    }
}

/// Decide the verdict. Returns the label and the reasons: every fact used, in order.
pub fn decide(f: &Facts) -> (Verdict, Vec<String>) {
    let mut r: Vec<String> = Vec::new();
    if f.locked {
        match f.lock_reason.as_deref() {
            Some(s) if !s.is_empty() => r.push(format!("locked: {s}")),
            _ => r.push("locked".into()),
        }
    }
    if !f.exists {
        r.push("directory does not exist".into());
        if let Some(p) = &f.prunable {
            r.push(format!("git lists it as prunable: {p}"));
        }
        return (Verdict::Missing, r);
    }
    if let Some(p) = &f.prunable {
        r.push(format!("git lists it as prunable: {p}"));
    }
    if let Some(o) = &f.orphan {
        r.push(o.clone());
        if f.in_use == Some(true) {
            r.push(format!(
                "in use by {} process(es): {}",
                f.in_use_procs,
                f.in_use_workloads.join(", ")
            ));
        }
        return (Verdict::OrphanedDir, r);
    }
    match f.in_use {
        Some(true) => {
            r.push(format!(
                "in use by {} process(es): {}",
                f.in_use_procs,
                f.in_use_workloads.join(", ")
            ));
            return (Verdict::InUse, r);
        }
        Some(false) => r.push("no live process has its cwd here".into()),
        None => r.push("in-use not checked".into()),
    }

    let target = f.default_ref.as_deref().unwrap_or("default branch");
    let idle = f.in_use == Some(false);
    // Merged by ancestry, or its changes are all in the default branch.
    let merged_by = if f.merged == Some(true) {
        Some(format!("tip is contained in {target} (as of last fetch)"))
    } else if f.content_merged == Some(true) {
        Some(format!(
            "all changes on this branch are already in {target} (squash or rebase merge), as of last fetch"
        ))
    } else {
        f.pr_merged_at_tip
            .map(|n| format!("GitHub PR #{n} was merged with exactly this tip as its head"))
    };

    // has-work: any known positive is enough. Unpushed commits whose changes are already in
    // the default branch are not work.
    let mut unknown: Vec<&str> = Vec::new();
    let mut work: Vec<String> = Vec::new();
    for (v, what, label) in [
        (f.tracked, "tracked change(s)", "tracked changes"),
        (f.untracked, "untracked file(s)", "untracked files"),
    ] {
        match v {
            Some(n) if n > 0 => work.push(format!("{n} {what}")),
            Some(_) => {}
            None => unknown.push(label),
        }
    }
    let mut clean = "clean, nothing unpushed".to_string();
    match f.unpushed {
        Some(n) if n > 0 && merged_by.is_some() => {
            clean = format!("clean; {n} commit(s) on no remote, but their changes are in {target}")
        }
        Some(n) if n > 0 => work.push(format!("{n} commit(s) on no remote")),
        Some(_) => {}
        None if merged_by.is_some() => clean = "clean".into(),
        None => unknown.push("unpushed commits"),
    }
    r.extend(f.context.iter().cloned());
    if !work.is_empty() {
        r.extend(work);
        return (Verdict::HasWork, r);
    }
    if !unknown.is_empty() {
        r.push(format!("could not determine: {}", unknown.join(", ")));
        return (Verdict::Unknown, r);
    }
    r.push(clean);

    if let Some(why) = merged_by {
        r.push(why);
        if !idle {
            r.push("would be merged-idle, but in-use was not checked".into());
            return (Verdict::Unknown, r);
        }
        if f.locked {
            r.push("locked, so merged-locked rather than merged-idle".into());
            return (Verdict::MergedLocked, r);
        }
        return (Verdict::MergedIdle, r);
    }
    match f.merged {
        None => {
            r.push(format!("could not determine whether merged into {target}"));
            return (Verdict::Unknown, r);
        }
        Some(_) => match f.content_merged {
            Some(_) => r.push(format!(
                "not merged into {target}, and some changes are not in it"
            )),
            None => r.push(format!(
                "not merged into {target} (whether its changes are in it is unknown)"
            )),
        },
    }
    let Some(secs) = f.idle_secs else {
        r.push("last activity unknown".into());
        return (Verdict::Unknown, r);
    };
    let stale = secs > i64::from(f.stale_days) * 86400;
    r.push(format!("last activity {} ago", days(secs)));
    if stale {
        if !idle {
            r.push("would be stale, but in-use was not checked".into());
            return (Verdict::Unknown, r);
        }
        r.push(format!("older than {} days", f.stale_days));
        return (Verdict::Stale, r);
    }
    (Verdict::Active, r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean() -> Facts {
        Facts {
            exists: true,
            in_use: Some(false),
            tracked: Some(0),
            untracked: Some(0),
            unpushed: Some(0),
            merged: Some(false),
            default_ref: Some("origin/main".into()),
            idle_secs: Some(3600),
            stale_days: 7,
            ..Default::default()
        }
    }

    #[test]
    fn precedence_table() {
        let d = |f: Facts| decide(&f).0;
        let base = clean();
        assert_eq!(d(base.clone()), Verdict::Active);

        // Each row: modification -> expected verdict.
        type Row = (Box<dyn Fn(&mut Facts)>, Verdict);
        let rows: Vec<Row> = vec![
            (
                Box::new(|f| {
                    f.exists = false;
                    f.in_use = Some(true);
                    f.tracked = Some(3)
                }),
                Verdict::Missing,
            ),
            (
                Box::new(|f| {
                    f.orphan = Some("o".into());
                    f.in_use = Some(true)
                }),
                Verdict::OrphanedDir,
            ),
            (
                Box::new(|f| {
                    f.in_use = Some(true);
                    f.tracked = Some(3)
                }),
                Verdict::InUse,
            ),
            (
                Box::new(|f| {
                    f.tracked = Some(1);
                    f.merged = Some(true)
                }),
                Verdict::HasWork,
            ),
            (Box::new(|f| f.untracked = Some(1)), Verdict::HasWork),
            (
                Box::new(|f| {
                    f.unpushed = Some(2);
                    f.tracked = None
                }),
                Verdict::HasWork,
            ),
            (Box::new(|f| f.merged = Some(true)), Verdict::MergedIdle),
            (
                Box::new(|f| {
                    f.merged = Some(true);
                    f.idle_secs = Some(90 * 86400)
                }),
                Verdict::MergedIdle,
            ),
            (Box::new(|f| f.idle_secs = Some(8 * 86400)), Verdict::Stale),
            (Box::new(|f| f.idle_secs = Some(6 * 86400)), Verdict::Active),
            // Unknowns stay unknown, never a guessed "no".
            (Box::new(|f| f.tracked = None), Verdict::Unknown),
            (Box::new(|f| f.merged = None), Verdict::Unknown),
            (Box::new(|f| f.idle_secs = None), Verdict::Unknown),
            (
                Box::new(|f| {
                    f.in_use = None;
                    f.merged = Some(true)
                }),
                Verdict::Unknown,
            ),
            (
                Box::new(|f| {
                    f.in_use = None;
                    f.idle_secs = Some(30 * 86400)
                }),
                Verdict::Unknown,
            ),
            (
                Box::new(|f| {
                    f.in_use = None;
                    f.tracked = Some(1)
                }),
                Verdict::HasWork,
            ),
            (Box::new(|f| f.in_use = None), Verdict::Active),
            // Locked is never merged-idle: merged-locked instead; locked keeps other verdicts.
            (
                Box::new(|f| {
                    f.locked = true;
                    f.merged = Some(true)
                }),
                Verdict::MergedLocked,
            ),
            (
                Box::new(|f| {
                    f.locked = true;
                    f.merged = Some(true);
                    f.in_use = None
                }),
                Verdict::Unknown,
            ),
            (
                Box::new(|f| {
                    f.locked = true;
                    f.merged = Some(true);
                    f.in_use = Some(true)
                }),
                Verdict::InUse,
            ),
            (
                Box::new(|f| {
                    f.locked = true;
                    f.merged = Some(true);
                    f.untracked = Some(1)
                }),
                Verdict::HasWork,
            ),
            // Content merged (squash / rebase): unpushed commits are not work.
            (
                Box::new(|f| {
                    f.unpushed = Some(4);
                    f.content_merged = Some(true)
                }),
                Verdict::MergedIdle,
            ),
            (
                Box::new(|f| {
                    f.unpushed = None;
                    f.content_merged = Some(true)
                }),
                Verdict::MergedIdle,
            ),
            (
                Box::new(|f| {
                    f.unpushed = Some(4);
                    f.content_merged = Some(true);
                    f.locked = true
                }),
                Verdict::MergedLocked,
            ),
            (
                Box::new(|f| {
                    f.unpushed = Some(4);
                    f.content_merged = Some(true);
                    f.tracked = Some(1)
                }),
                Verdict::HasWork,
            ),
            (
                Box::new(|f| {
                    f.unpushed = Some(4);
                    f.content_merged = Some(true);
                    f.untracked = Some(1)
                }),
                Verdict::HasWork,
            ),
            (
                Box::new(|f| {
                    f.unpushed = Some(4);
                    f.content_merged = Some(true);
                    f.in_use = None
                }),
                Verdict::Unknown,
            ),
            (
                Box::new(|f| {
                    f.unpushed = Some(4);
                    f.content_merged = Some(true);
                    f.in_use = Some(true)
                }),
                Verdict::InUse,
            ),
            (
                Box::new(|f| {
                    f.unpushed = Some(4);
                    f.content_merged = Some(false)
                }),
                Verdict::HasWork,
            ),
            (
                Box::new(|f| {
                    f.unpushed = Some(4);
                    f.content_merged = None
                }),
                Verdict::HasWork,
            ),
            (
                Box::new(|f| {
                    f.unpushed = None;
                    f.content_merged = Some(false)
                }),
                Verdict::Unknown,
            ),
            (
                Box::new(|f| {
                    f.content_merged = Some(false);
                    f.idle_secs = Some(30 * 86400)
                }),
                Verdict::Stale,
            ),
            (
                Box::new(|f| {
                    f.content_merged = None;
                    f.idle_secs = Some(30 * 86400)
                }),
                Verdict::Stale,
            ),
            // A PR merged at exactly this tip counts as merged, even if the content check says no.
            (
                Box::new(|f| {
                    f.unpushed = Some(4);
                    f.content_merged = Some(false);
                    f.pr_merged_at_tip = Some(9)
                }),
                Verdict::MergedIdle,
            ),
            (
                Box::new(|f| {
                    f.unpushed = Some(4);
                    f.pr_merged_at_tip = Some(9);
                    f.tracked = Some(1)
                }),
                Verdict::HasWork,
            ),
            // Ancestry-merged with commits on no remote (local default branch): merged-idle.
            (
                Box::new(|f| {
                    f.unpushed = Some(2);
                    f.merged = Some(true)
                }),
                Verdict::MergedIdle,
            ),
            (
                Box::new(|f| {
                    f.locked = true;
                    f.idle_secs = Some(30 * 86400)
                }),
                Verdict::Stale,
            ),
            (
                Box::new(|f| {
                    f.locked = true;
                    f.tracked = Some(1)
                }),
                Verdict::HasWork,
            ),
        ];
        for (i, (m, want)) in rows.iter().enumerate() {
            let mut f = base.clone();
            m(&mut f);
            assert_eq!(d(f), *want, "row {i}");
        }
    }

    #[test]
    fn reasons_list_facts() {
        let mut f = clean();
        f.merged = Some(true);
        f.locked = true;
        f.lock_reason = Some("agent".into());
        let (_, r) = decide(&f);
        assert!(r.iter().any(|s| s == "locked: agent"));
        assert!(r.iter().any(|s| s.contains("origin/main")));
        assert!(r
            .iter()
            .all(|s| !s.to_lowercase().contains("safe to delete")));

        let mut f = clean();
        f.content_merged = Some(true);
        f.unpushed = Some(3);
        let (v, r) = decide(&f);
        assert_eq!(v, Verdict::MergedIdle);
        assert!(r
            .iter()
            .any(|s| s.contains("3 commit(s) on no remote, but their changes are in origin/main")));
        assert!(
            r.iter().any(|s| s.contains("squash or rebase merge")),
            "{r:?}"
        );
    }
}
