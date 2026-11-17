//! Text table for a [`Report`].

use crate::{tilde, Report, Verdict, Worktree};
use std::fmt::Write;

/// Bytes as `812M`, `1.5G`, `12K`; `-` when unknown.
///
/// ```
/// use procwho_wt::fmt_bytes;
/// assert_eq!(fmt_bytes(Some(1536 * 1024 * 1024)), "1.5G");
/// assert_eq!(fmt_bytes(Some(300 * 1024 * 1024)), "300M");
/// assert_eq!(fmt_bytes(None), "-");
/// ```
pub fn fmt_bytes(b: Option<u64>) -> String {
    let Some(b) = b else { return "-".into() };
    let k = 1024.0;
    let f = b as f64;
    if f >= k * k * k {
        format!("{:.1}G", f / k / k / k)
    } else if f >= k * k {
        format!("{:.0}M", f / k / k)
    } else if f >= k {
        format!("{:.0}K", f / k)
    } else {
        format!("{b}B")
    }
}

/// A duration in seconds as `45m`, `5h`, `12d`.
///
/// ```
/// assert_eq!(procwho_wt::fmt_age(90 * 60), "1h");
/// assert_eq!(procwho_wt::fmt_age(3 * 86400 + 5), "3d");
/// ```
pub fn fmt_age(secs: i64) -> String {
    if secs >= 86400 {
        format!("{}d", secs / 86400)
    } else if secs >= 3600 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}m", secs / 60)
    }
}

fn cut(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(n.saturating_sub(1)).collect();
        t.push('~');
        t
    }
}

/// One-line summary of why a worktree has its verdict.
fn summary(w: &Worktree) -> String {
    let mut parts: Vec<String> = Vec::new();
    match w.verdict {
        Verdict::InUse => parts.push(format!(
            "{} proc: {}",
            w.in_use_procs.unwrap_or(0),
            w.in_use_workloads.join(", ")
        )),
        Verdict::HasWork => {
            for (n, what) in [
                (w.tracked_changes, "changed"),
                (w.untracked_files, "untracked"),
                (w.unpushed_commits, "unpushed"),
            ] {
                if let Some(n) = n.filter(|n| *n > 0) {
                    parts.push(format!("{n} {what}"));
                }
            }
        }
        Verdict::MergedIdle | Verdict::MergedLocked => {
            let how = if w.merged == Some(true) {
                "merged"
            } else {
                "squash/rebase-merged"
            };
            parts.push(format!(
                "{how} into {}",
                w.default_branch.as_deref().unwrap_or("default")
            ));
            if let Some(n) = w.unpushed_commits.filter(|n| *n > 0) {
                parts.push(format!("{n} unpushed, content in it"));
            }
        }
        Verdict::Stale => parts.push(format!(
            "not merged, idle {}",
            w.idle_secs.map(fmt_age).unwrap_or_default()
        )),
        Verdict::Missing | Verdict::OrphanedDir | Verdict::Unknown | Verdict::Active => {
            let skip = [
                "no live process",
                "in-use not checked",
                "clean, nothing",
                "main checkout",
            ];
            if let Some(r) = w
                .reasons
                .iter()
                .rev()
                .find(|r| !skip.iter().any(|s| r.starts_with(s)))
            {
                parts.push(r.clone());
            }
        }
    }
    if !w.flags.is_empty() {
        parts.push(format!("[{}]", w.flags.join(", ")));
    }
    if w.size_cached {
        parts.push(format!(
            "size cached {}",
            w.size_cache_age_secs.map(fmt_age).unwrap_or_default()
        ));
    }
    parts.join("; ")
}

/// Render the report as a table with at most `limit` worktrees, then totals and notes.
pub fn render(report: &Report, limit: usize) -> String {
    let mut o = String::new();
    let shown = report.worktrees.len().min(limit);
    let _ = writeln!(
        o,
        "Procwho worktrees: {} worktrees in {} repositories{}. Scan took {:.1}s ({}); sizes: {} measured, {} cached.",
        report.worktrees.len(),
        report.repos_found,
        report.filter.as_ref().map(|f| format!(" [{f}, {} before filter]", report.worktrees_found)).unwrap_or_default(),
        report.scan_secs,
        report.phase_secs.iter().map(|(k, v)| format!("{} {v:.1}s", &k[2..])).collect::<Vec<_>>().join(", "),
        report.sizes_walked,
        report.sizes_cached
    );
    let _ = writeln!(o);
    let rw = report
        .worktrees
        .iter()
        .take(shown)
        .map(|w| w.repo.chars().count())
        .max()
        .unwrap_or(4)
        .clamp(4, 24);
    let _ = writeln!(
        o,
        "{:>6} {:>7} {:>4}  {:<13} {:<rw$} Branch",
        "Size", "Rebuild", "Age", "Verdict", "Repo"
    );
    for w in report.worktrees.iter().take(shown) {
        let branch = match (&w.branch, &w.head) {
            (Some(b), _) => b.clone(),
            (None, Some(h)) if w.detached => format!("(detached {})", &h[..h.len().min(8)]),
            _ => "-".into(),
        };
        let _ = writeln!(
            o,
            "{:>6} {:>7} {:>4}  {:<13} {:<rw$} {}",
            fmt_bytes(w.size_bytes),
            fmt_bytes(w.rebuildable_bytes),
            w.idle_secs.map(fmt_age).unwrap_or_else(|| "?".into()),
            w.verdict.as_str(),
            cut(&w.repo, rw),
            cut(&branch, 48)
        );
        let s = summary(w);
        let _ = writeln!(
            o,
            "       {}{}",
            tilde(&w.path),
            if s.is_empty() {
                String::new()
            } else {
                format!("  - {s}")
            }
        );
    }
    if shown < report.worktrees.len() {
        let _ = writeln!(
            o,
            "       ... {} more (use -n)",
            report.worktrees.len() - shown
        );
    }

    let _ = writeln!(
        o,
        "\n{:<13} {:>5} {:>8} {:>8}",
        "Verdict", "Count", "Size", "Rebuild"
    );
    let (mut c, mut s, mut r, mut u) = (0, 0u64, 0u64, 0);
    for t in &report.totals {
        let _ = writeln!(
            o,
            "{:<13} {:>5} {:>8} {:>8}{}",
            t.verdict.as_str(),
            t.count,
            fmt_bytes((t.size_unknown < t.count).then_some(t.size_bytes)),
            fmt_bytes((t.size_unknown < t.count).then_some(t.rebuildable_bytes)),
            if t.size_unknown > 0 {
                format!("  ({} unsized)", t.size_unknown)
            } else {
                String::new()
            }
        );
        c += t.count;
        s += t.size_bytes;
        r += t.rebuildable_bytes;
        u += t.size_unknown;
    }
    let _ = writeln!(
        o,
        "{:<13} {:>5} {:>8} {:>8}{}",
        "Total",
        c,
        fmt_bytes((u < c).then_some(s)),
        fmt_bytes((u < c).then_some(r)),
        if u > 0 {
            format!("  ({u} unsized)")
        } else {
            String::new()
        }
    );
    if let Some(t) = report
        .totals
        .iter()
        .find(|t| t.verdict == Verdict::MergedIdle && t.size_unknown < t.count)
    {
        let _ = writeln!(
            o,
            "\nRemoving all merged-idle worktrees could free up to {}, including {} in rebuildable caches{}.",
            fmt_bytes(Some(t.size_bytes)),
            fmt_bytes(Some(t.rebuildable_bytes)),
            if t.size_unknown > 0 { format!(", plus {} unsized", t.size_unknown) } else { String::new() }
        );
        let _ = writeln!(
            o,
            "  This is the sum of allocated blocks; APFS clones can reduce the space freed."
        );
    }
    if !report.notes.is_empty() {
        let _ = writeln!(o, "\nNotes");
        for n in &report.notes {
            let _ = writeln!(o, "  - {n}");
        }
    }
    o
}
