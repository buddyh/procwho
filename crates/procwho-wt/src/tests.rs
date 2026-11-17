//! Integration test with real git repositories under the system temp dir.

use super::*;
use std::process::Command;

fn sh_git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "user.name=t",
            "-c",
            "user.email=fixture",
        ])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn by_path<'a>(r: &'a Report, p: &Path) -> &'a Worktree {
    let p = p.display().to_string();
    r.worktrees
        .iter()
        .find(|w| w.path == p)
        .unwrap_or_else(|| panic!("{p} not in report: {:#?}", r.worktrees))
}

#[test]
fn real_git_worktrees() {
    let base = std::env::temp_dir().join(format!("procwho wt it-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let base = canon(&base);
    let (origin, main) = (base.join("origin.git"), base.join("main"));
    std::fs::create_dir_all(&origin).unwrap();
    std::fs::create_dir_all(&main).unwrap();
    sh_git(&origin, &["init", "-q", "--bare", "-b", "main"]);
    sh_git(&main, &["init", "-q", "-b", "main"]);
    std::fs::write(main.join("a.txt"), "a\n").unwrap();
    sh_git(&main, &["add", "a.txt"]);
    sh_git(&main, &["commit", "-q", "-m", "a"]);
    sh_git(
        &main,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    sh_git(&main, &["push", "-q", "-u", "origin", "main"]);
    sh_git(
        &main,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    );

    let wt = base.join("wts");
    let (feat, done, dirty, gone) = (
        wt.join("feat"),
        wt.join("done"),
        wt.join("dirty"),
        wt.join("gone"),
    );
    for (p, b) in [
        (&feat, "feat"),
        (&done, "done"),
        (&dirty, "dirty"),
        (&gone, "gone"),
    ] {
        sh_git(
            &main,
            &["worktree", "add", "-q", "-b", b, p.to_str().unwrap()],
        );
    }
    // feat: one local commit plus an untracked file.
    std::fs::write(feat.join("b.txt"), "b\n").unwrap();
    sh_git(&feat, &["add", "b.txt"]);
    sh_git(&feat, &["commit", "-q", "-m", "b"]);
    std::fs::write(feat.join("scratch.txt"), "x\n").unwrap();
    // dirty: a tracked modification.
    std::fs::write(dirty.join("a.txt"), "changed\n").unwrap();
    // gone: directory removed behind git's back.
    std::fs::remove_dir_all(&gone).unwrap();
    // orphan: a container dir whose .git file points at a worktree entry that does not exist.
    let orphan = base.join("container").join("proj").join("ghost");
    std::fs::create_dir_all(&orphan).unwrap();
    std::fs::write(
        orphan.join(".git"),
        format!("gitdir: {}/.git/worktrees/ghost\n", main.display()),
    )
    .unwrap();

    let index_before = mtime(&main.join(".git/worktrees/dirty/index"));
    let opts = Options {
        roots: vec![(base.clone(), 1)],
        containers: vec![(base.join("container"), 2)],
        state_dir: base.join("state"),
        jobs: 2,
        ..Default::default()
    };
    let live = vec![LiveUse {
        agent_related: true,
        pid: 1,
        cwd: done.join("sub").display().to_string(),
        workload: "Claude session: test".into(),
        kind: "agent-session".into(),
    }];
    let r = scan(&opts, Some(&live));

    let f = by_path(&r, &feat);
    assert_eq!(f.verdict, Verdict::HasWork, "{:?}", f.reasons);
    assert_eq!(
        (f.tracked_changes, f.untracked_files, f.unpushed_commits),
        (Some(0), Some(1), Some(1))
    );
    assert_eq!(f.merged, Some(false));
    assert_eq!(f.default_branch.as_deref(), Some("origin/main"));

    let d = by_path(&r, &dirty);
    assert_eq!(d.verdict, Verdict::HasWork);
    assert_eq!(
        (d.tracked_changes, d.untracked_files, d.unpushed_commits),
        (Some(1), Some(0), Some(0))
    );
    assert_eq!(d.merged, Some(true));

    let dn = by_path(&r, &done);
    assert_eq!(dn.verdict, Verdict::InUse);
    assert_eq!(
        dn.in_use_workloads,
        vec!["Claude session: test".to_string()]
    );

    assert_eq!(by_path(&r, &gone).verdict, Verdict::Missing);
    let o = by_path(&r, &orphan);
    assert_eq!(o.verdict, Verdict::OrphanedDir, "{:?}", o.reasons);
    assert!(
        r.worktrees.iter().all(|w| w.kind != "main"),
        "main checkout excluded by default"
    );
    assert!(dn.size_bytes.is_some() && !dn.size_cached);
    let measured = dn.size_measured_unix;
    assert!(measured.is_some());

    // Second scan: no live users -> done is merged-idle; sizes come from the cache.
    let r2 = scan(&opts, Some(&[]));
    let dn = by_path(&r2, &done);
    assert_eq!(dn.verdict, Verdict::MergedIdle, "{:?}", dn.reasons);
    assert!(dn.size_cached);
    assert_eq!(dn.size_measured_unix, measured);
    let text = render(&r2, 10);
    assert!(
        text.contains("Removing all merged-idle worktrees could free up to "),
        "{text}"
    );
    assert!(text.contains("APFS clones"));
    assert!(
        !text.contains("rm -") && !text.contains("worktree remove"),
        "no deletion commands"
    );
    // Without a process snapshot nothing may be labelled idle.
    let r3 = scan(
        &Options {
            size: false,
            ..opts.clone()
        },
        None,
    );
    assert_eq!(by_path(&r3, &done).verdict, Verdict::Unknown);
    assert_eq!(by_path(&r3, &done).in_use, None);
    // --all includes the main checkout.
    let r4 = scan(
        &Options {
            include_main: true,
            size: false,
            ..opts.clone()
        },
        Some(&[]),
    );
    assert!(r4
        .worktrees
        .iter()
        .any(|w| w.kind == "main" && w.path == main.display().to_string()));

    // Read-only: the scan did not touch the worktree index; JSON serializes.
    assert_eq!(
        index_before,
        mtime(&main.join(".git/worktrees/dirty/index"))
    );
    assert!(serde_json::to_string(&r)
        .unwrap()
        .contains("\"verdict\":\"has-work\""));
    assert!(!render(&r, 10).to_lowercase().contains("safe to delete"));

    std::fs::remove_dir_all(&base).unwrap();
}

fn git_out(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Every file under `dir` with its size and mtime (nanoseconds), sorted.
fn snapshot(dir: &Path) -> Vec<(PathBuf, u64, i64, i64)> {
    use std::os::unix::fs::MetadataExt;
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let m = std::fs::symlink_metadata(&d).unwrap();
        out.push((d.clone(), m.len(), m.mtime(), m.mtime_nsec()));
        if m.is_dir() {
            for e in std::fs::read_dir(&d).unwrap() {
                stack.push(e.unwrap().path());
            }
        }
    }
    out.sort();
    out
}

/// Repo with origin, plus branches `squash` (squash-merged into main), `newwork` (unmerged)
/// and `conflict` (edits a line main later changed differently), each in a linked worktree.
fn squash_fixture(base: &Path) -> (PathBuf, PathBuf) {
    let (origin, main, wt) = (base.join("origin.git"), base.join("main"), base.join("wts"));
    std::fs::create_dir_all(&origin).unwrap();
    std::fs::create_dir_all(&main).unwrap();
    sh_git(&origin, &["init", "-q", "--bare", "-b", "main"]);
    sh_git(&main, &["init", "-q", "-b", "main"]);
    std::fs::write(main.join("a.txt"), "one\ntwo\n").unwrap();
    sh_git(&main, &["add", "a.txt"]);
    sh_git(&main, &["commit", "-q", "-m", "a"]);
    sh_git(
        &main,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    sh_git(&main, &["push", "-q", "-u", "origin", "main"]);
    sh_git(
        &main,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    );
    for b in ["squash", "newwork", "conflict", "lockedsq"] {
        sh_git(
            &main,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                b,
                wt.join(b).to_str().unwrap(),
            ],
        );
    }
    for (b, file, text) in [
        ("squash", "s.txt", "s1\n"),
        ("lockedsq", "l.txt", "l1\n"),
        ("newwork", "n.txt", "n\n"),
        ("conflict", "a.txt", "one\nbranch\n"),
    ] {
        std::fs::write(wt.join(b).join(file), text).unwrap();
        sh_git(&wt.join(b), &["add", file]);
        sh_git(&wt.join(b), &["commit", "-q", "-m", b]);
    }
    // Two commits on squash, so a squash merge is not the same commit as the tip.
    std::fs::write(wt.join("squash").join("s.txt"), "s1\ns2\n").unwrap();
    sh_git(&wt.join("squash"), &["commit", "-q", "-am", "squash 2"]);
    // Pushed, then the remote branch deleted (as GitHub does after merging a PR).
    sh_git(
        &wt.join("squash"),
        &["push", "-q", "-u", "origin", "squash"],
    );
    sh_git(&main, &["push", "-q", "origin", "--delete", "squash"]);
    sh_git(&main, &["merge", "-q", "--squash", "squash"]);
    sh_git(&main, &["commit", "-q", "-m", "squash merge"]);
    sh_git(&main, &["merge", "-q", "--squash", "lockedsq"]);
    sh_git(&main, &["commit", "-q", "-m", "lockedsq merge"]);
    std::fs::write(main.join("a.txt"), "one\nmain\n").unwrap();
    sh_git(&main, &["commit", "-q", "-am", "main edit"]);
    sh_git(&main, &["push", "-q", "origin", "main"]);
    sh_git(
        &main,
        &[
            "worktree",
            "lock",
            "--reason",
            "agent",
            wt.join("lockedsq").to_str().unwrap(),
        ],
    );
    (main, wt)
}

#[test]
fn content_merge_is_isolated() {
    let base = std::env::temp_dir().join(format!("procwho-wt-cm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let base = canon(&base);
    let (main, _) = squash_fixture(&base);
    let gd = main.join(".git");
    let objects = PathBuf::from(git_out(
        &main,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "objects",
        ],
    ));
    let tree = git_out(&main, &["rev-parse", "origin/main^{tree}"]);
    let tmp = base.join("tmp");
    std::fs::create_dir_all(&tmp).unwrap();
    let before = snapshot(&gd);
    let t = Duration::from_secs(10);
    let check = |b: &str| {
        let tip = git_out(&main, &["rev-parse", b]);
        git::content_merged_in(&tmp, &gd, &objects, "origin/main", &tree, &tip, t)
    };
    assert_eq!(check("squash"), Ok(true));
    assert_eq!(check("newwork"), Ok(false));
    assert_eq!(check("conflict"), Ok(false));
    let bogus = "1234567890123456789012345678901234567890";
    assert!(git::content_merged_in(&tmp, &gd, &objects, "origin/main", &tree, bogus, t).is_err());
    // Nothing in the repository changed (objects, refs, index, logs), and no temp dir is left.
    assert_eq!(before, snapshot(&gd), "repository .git changed");
    assert_eq!(
        std::fs::read_dir(&tmp).unwrap().count(),
        0,
        "temp object dir left behind"
    );
    std::fs::remove_dir_all(&base).unwrap();
}

#[test]
fn scan_reports_content_merged() {
    let base = std::env::temp_dir().join(format!("procwho-wt-sq-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let base = canon(&base);
    let (main, wt) = squash_fixture(&base);
    let objects = snapshot(&main.join(".git/objects"));
    let heads: Vec<_> = ["squash", "newwork"]
        .iter()
        .map(|b| {
            (
                mtime(&main.join(format!(".git/worktrees/{b}/index"))),
                mtime(&main.join(format!(".git/worktrees/{b}/HEAD"))),
            )
        })
        .collect();
    let opts = Options {
        roots: vec![(base.clone(), 1)],
        containers: vec![],
        state_dir: base.join("state"),
        size: false,
        jobs: 2,
        ..Default::default()
    };
    let r = scan(&opts, Some(&[]));
    let sq = by_path(&r, &wt.join("squash"));
    assert_eq!(
        (sq.merged, sq.content_merged, sq.unpushed_commits),
        (Some(false), Some(true), Some(2))
    );
    assert_eq!(sq.verdict, Verdict::MergedIdle, "{:?}", sq.reasons);
    assert_eq!(sq.flags, vec!["upstream-gone"]);
    assert!(render(&r, 10).contains("[upstream-gone]"));
    let lk = by_path(&r, &wt.join("lockedsq"));
    assert_eq!(
        (lk.content_merged, lk.verdict),
        (Some(true), Verdict::MergedLocked),
        "{:?}",
        lk.reasons
    );
    assert_eq!(lk.flags, vec!["locked"]);
    let nw = by_path(&r, &wt.join("newwork"));
    assert_eq!(
        (nw.content_merged, nw.verdict),
        (Some(false), Verdict::HasWork)
    );
    let cf = by_path(&r, &wt.join("conflict"));
    assert_eq!(
        (cf.content_merged, cf.verdict),
        (Some(false), Verdict::HasWork)
    );
    assert_eq!(
        objects,
        snapshot(&main.join(".git/objects")),
        "scan changed the object directory"
    );
    let after: Vec<_> = ["squash", "newwork"]
        .iter()
        .map(|b| {
            (
                mtime(&main.join(format!(".git/worktrees/{b}/index"))),
                mtime(&main.join(format!(".git/worktrees/{b}/HEAD"))),
            )
        })
        .collect();
    assert_eq!(heads, after);
    std::fs::remove_dir_all(&base).unwrap();
}

#[test]
fn pr_confirmation_with_fake_gh() {
    use std::os::unix::fs::PermissionsExt;
    let base = std::env::temp_dir().join(format!("procwho-wt-pr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let base = canon(&base);
    let (main, wt) = squash_fixture(&base);
    // A GitHub remote that is never contacted: only the fake gh below sees its name.
    sh_git(
        &main,
        &["remote", "add", "gh", "https://github.com/example/proj.git"],
    );
    let conflict_tip = git_out(&main, &["rev-parse", "conflict"]);
    let json = format!(
        r#"[{{"number":7,"headRefName":"conflict","headRefOid":"{conflict_tip}","mergedAt":"2026-11-01T00:00:00Z"}},
            {{"number":8,"headRefName":"newwork","headRefOid":"0000","mergedAt":"2026-11-02T00:00:00Z"}}]"#
    );
    let (data, calls, gh) = (
        base.join("prs.json"),
        base.join("gh-calls.txt"),
        base.join("fake-gh"),
    );
    std::fs::write(&data, json).unwrap();
    std::fs::write(
        &gh,
        format!(
            "#!/bin/sh\necho \"$*\" >> '{}'\ncat '{}'\n",
            calls.display(),
            data.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
    let opts = Options {
        roots: vec![(base.clone(), 1)],
        containers: vec![],
        state_dir: base.join("state"),
        size: false,
        jobs: 2,
        pr: true,
        gh: gh.clone(),
        ..Default::default()
    };
    let r = scan(&opts, Some(&[]));
    // Merged at exactly this tip: merged, although merge-tree reports a conflict.
    let cf = by_path(&r, &wt.join("conflict"));
    assert_eq!(cf.content_merged, Some(false));
    assert_eq!(
        cf.pr_merged,
        Some(pr::PrMerged {
            number: 7,
            merged_at: "2026-11-01T00:00:00Z".into(),
            head_matches_tip: true
        })
    );
    assert_eq!(cf.verdict, Verdict::MergedIdle, "{:?}", cf.reasons);
    // Same branch name, different tip: recorded, verdict unchanged.
    let nw = by_path(&r, &wt.join("newwork"));
    assert_eq!(
        nw.pr_merged
            .as_ref()
            .map(|p| (p.number, p.head_matches_tip)),
        Some((8, false))
    );
    assert_eq!(nw.verdict, Verdict::HasWork);
    assert!(nw
        .reasons
        .iter()
        .any(|s| s.contains("PR #8") && s.contains("different tip")));
    assert_eq!(by_path(&r, &wt.join("squash")).pr_merged, None);
    // JSON: schema version and the documented per-worktree keys.
    let j: serde_json::Value = serde_json::to_value(&r).unwrap();
    assert_eq!(j["schema_version"], SCHEMA_VERSION);
    let keys = [
        "path",
        "repo",
        "branch",
        "verdict",
        "flags",
        "reasons",
        "notes",
        "size_bytes",
        "rebuildable_bytes",
        "last_activity_unix",
        "idle_secs",
        "in_use",
        "merged",
        "content_merged",
        "pr_merged",
        "unpushed_commits",
        "tracked_changes",
        "untracked_files",
        "locked",
        "upstream_gone",
        "default_branch",
    ];
    for w in j["worktrees"].as_array().unwrap() {
        for k in keys {
            assert!(w.get(k).is_some(), "missing key {k}");
        }
    }
    let cfj = j["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["branch"] == "conflict")
        .unwrap();
    assert_eq!(cfj["pr_merged"]["head_matches_tip"], true);
    assert_eq!(cfj["verdict"], "merged-idle");
    // gh got the repo's own name and nothing else.
    let expect = "pr list -R example/proj --state merged --limit 200 --json number,headRefName,headRefOid,mergedAt\n";
    assert_eq!(std::fs::read_to_string(&calls).unwrap(), expect);
    // Second scan within the TTL uses the cache: no second gh call.
    let r2 = scan(&opts, Some(&[]));
    assert_eq!(
        by_path(&r2, &wt.join("conflict")).verdict,
        Verdict::MergedIdle
    );
    assert_eq!(std::fs::read_to_string(&calls).unwrap(), expect);
    // Without --pr the field is null and a note says so; a missing gh is a note, not a failure.
    let r3 = scan(
        &Options {
            pr: false,
            ..opts.clone()
        },
        Some(&[]),
    );
    assert_eq!(by_path(&r3, &wt.join("conflict")).pr_merged, None);
    assert!(r3
        .notes
        .iter()
        .any(|n| n.contains("PR state was not checked")));
    let r4 = scan(
        &Options {
            gh: base.join("no-such-gh"),
            state_dir: base.join("state2"),
            ..opts.clone()
        },
        Some(&[]),
    );
    assert_eq!(by_path(&r4, &wt.join("conflict")).verdict, Verdict::HasWork);
    assert!(
        r4.repos
            .iter()
            .flat_map(|r| &r.notes)
            .any(|n| n.contains("optional gh tool not installed")),
        "{:?}",
        r4.repos
    );
    std::fs::remove_dir_all(&base).unwrap();
}

#[test]
fn live_process_discovery_remembers_repos_and_finds_arbitrary_linked_paths() {
    let base = std::env::temp_dir().join(format!("procwho-live-seeds-{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    let base = canon(&base);
    let repo = base.join("unconventional-project-location");
    std::fs::create_dir_all(&repo).unwrap();
    sh_git(&repo, &["init", "-q", "-b", "main"]);
    sh_git(&repo, &["commit", "--allow-empty", "-qm", "fixture"]);
    let linked = base.join("elsewhere/isolated-task");
    sh_git(
        &repo,
        &["worktree", "add", "-qb", "task", linked.to_str().unwrap()],
    );
    std::fs::create_dir_all(linked.join("src/nested")).unwrap();
    let opts = Options {
        roots: vec![],
        containers: vec![],
        state_dir: base.join("state"),
        size: false,
        ..Options::default()
    };
    let live: Vec<_> = (1..=1000)
        .map(|pid| LiveUse {
            pid,
            cwd: linked.join("src/nested").display().to_string(),
            workload: "Fixture helper".into(),
            kind: "app".into(),
            agent_related: true,
        })
        .collect();
    assert_eq!(discover::live_repositories(&live, &base).len(), 1);
    let report = scan(&opts, Some(&live));
    assert_eq!(report.repos_found, 1);
    assert_eq!(report.worktrees_found, 1);
    assert_eq!(
        report.worktrees[0].discovered_via,
        discover::DiscoverySource::LiveProcess
    );
    assert_eq!(report.worktrees[0].path, linked.display().to_string());
    let known = known::load_known(&opts.state_dir);
    assert_eq!(known.len(), 1);
    assert_eq!(known[0].path, repo);
    assert!(known[0].last_seen_unix > 0);
    let later = scan(&opts, Some(&[]));
    assert_eq!(
        later.worktrees[0].discovered_via,
        discover::DiscoverySource::Remembered
    );
    let unrelated: Vec<_> = live
        .into_iter()
        .map(|mut u| {
            u.agent_related = false;
            u
        })
        .collect();
    assert!(discover::live_repositories(&unrelated, &base).is_empty());
    known::forget_known(&opts.state_dir).unwrap();
    assert_eq!(scan(&opts, Some(&[])).repos_found, 0);
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn known_repos_are_bounded_deduplicated_pruned_and_newest_first() {
    let base = std::env::temp_dir().join(format!("procwho-known-{}", std::process::id()));
    let mut entries = Vec::new();
    for i in 0..510 {
        let path = base.join(format!("project-{i}"));
        std::fs::create_dir_all(&path).unwrap();
        entries.push(known::KnownRepo {
            path,
            last_seen_unix: i,
        });
    }
    entries.push(known::KnownRepo {
        path: base.join("missing"),
        last_seen_unix: 999,
    });
    entries.push(known::KnownRepo {
        path: base.join("project-509"),
        last_seen_unix: 1000,
    });
    let pruned = known::prune_known(entries, &base);
    assert_eq!(pruned.len(), 500);
    assert_eq!(pruned[0].last_seen_unix, 1000);
    assert_eq!(pruned.last().unwrap().last_seen_unix, 10);
    known::save_known(&base.join("state"), &pruned).unwrap();
    assert_eq!(known::load_known(&base.join("state")).len(), 500);
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn root_probe_never_opens_protected_folders_and_stops_at_depth_two() {
    use std::os::unix::fs::symlink;
    let base = std::env::temp_dir().join(format!("procwho-probe-{}", std::process::id()));
    std::fs::create_dir_all(base.join("code/one/.git")).unwrap();
    std::fs::create_dir_all(base.join("code/group/two/.git")).unwrap();
    std::fs::create_dir_all(base.join("code/group/deep/three/.git")).unwrap();
    // Dangling protected paths: no metadata is required, and counts must stay absent.
    symlink(base.join("must-not-open"), base.join("Documents")).unwrap();
    symlink(base.join("must-not-open"), base.join("Desktop")).unwrap();
    let suggestions = discover::suggest_roots(&base);
    assert_eq!(
        suggestions
            .candidates
            .iter()
            .find(|c| c.path == base.join("code"))
            .unwrap()
            .repo_count,
        Some(2)
    );
    for c in suggestions.candidates.iter().filter(|c| c.protected) {
        assert_eq!(c.repo_count, None);
    }
    assert!(!discover::default_roots(&base)
        .iter()
        .any(|(p, _)| discover::protected(p, &base)));
    let fast = discover::suggest_roots_with_budget(&base, Duration::ZERO);
    assert!(fast.candidates.iter().all(|c| c.repo_count.is_none()));
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn protected_git_indirection_requires_an_explicit_folder_choice() {
    let base = std::env::temp_dir().join(format!("procwho-privacy-{}", std::process::id()));
    let repo = base.join("Documents/project");
    let gitdir = repo.join(".git/worktrees/task");
    let container = base.join("agent-worktrees/task");
    std::fs::create_dir_all(&gitdir).unwrap();
    std::fs::create_dir_all(&container).unwrap();
    std::fs::write(gitdir.join("commondir"), "../..\n").unwrap();
    std::fs::write(
        container.join(".git"),
        format!("gitdir: {}\n", gitdir.display()),
    )
    .unwrap();
    let access = discover::Access::new(base.clone(), &[]);
    assert!(!access.allows(&gitdir));
    assert!(!access.allows(&base.join("agent-worktrees/../Documents/project")));
    assert!(access.common_dir(&gitdir).is_none());
    let mut found = Found {
        access,
        ..Found::default()
    };
    discover::walk(&base.join("agent-worktrees"), 2, &mut found);
    assert!(found.repos.is_empty());
    assert_eq!(found.protected_skipped, 1);
    let mut opted_in = Found {
        access: discover::Access::new(base.clone(), &[base.join("Documents")]),
        ..Found::default()
    };
    discover::walk(&base.join("agent-worktrees"), 2, &mut opted_in);
    assert_eq!(
        opted_in.repos,
        [canon(&repo.join(".git"))].into_iter().collect()
    );
    assert!(!discover::Access::default().allows(Path::new("/Users/user/Documents/project/.git")));
    std::fs::remove_dir_all(base).unwrap();
}
