# Git worktrees

`procwho worktrees` (alias `wt`) lists every git worktree on the machine with its disk usage,
rebuildable bytes, time since last activity, git status, merge state and whether a live process
has its cwd inside it, plus a review label and the facts behind that label.

```sh
procwho wt                               # largest first, 40 rows, totals per verdict
procwho wt --verdict merged-idle -n 100
procwho wt --sort age --repo example-project
procwho wt --no-size                     # skip disk usage (git status only)
procwho wt --json > worktrees.json
procwho wt --pr                          # also ask GitHub (gh) which PRs were merged
procwho wt --all                         # include main checkouts
procwho wt --root ~/code --container ~/agent-worktrees
```

## Discovery and folder choice

Repositories are found from running agent workloads and workloads linked to agents,
agent worktree containers, selected project roots, and procwho's remembered repositories.
Agent cwds are deduplicated before resolving at most 256 directories per scan. Resolution
reads Git's `.git` file/directory and `commondir` layout without a subprocess per process.
Each distinct common Git directory is scanned once. `git worktree list --porcelain` finds
all its linked worktrees, wherever they live. Git's safe read flags still apply.

`repos[].discovered_via` and `worktrees[].discovered_via` report `live_process`, `container`,
`root`, or `remembered`. The first source wins in that order. Orphaned directories use their
walk source. `repos[]` includes discovered repositories with no linked worktrees too.
`--no-procs` disables live seeding and live-use checks; remembered discovery still applies.

Live-discovered repo roots are kept in `<state>/known-repos.json`, with `last_seen_unix`.
The list retains the 500 most recently seen unique roots and drops paths that no longer exist.
Later scans can find their worktrees without a running agent. `procwho worktrees --forget-known`
clears only this procwho-owned list and exits; a later scan can learn live repos again.

Without config, existing conventional candidates are searched at depth 3: `~/repos`, `~/code`,
`~/src`, `~/dev`, `~/Developer`, `~/Projects`, `~/projects`, `~/workspace`, `~/work`,
`~/git`, `~/github`, `~/Code`, `~/Sites`, and `~/go/src`. Case-equivalent paths are deduplicated.
`~/Documents` and `~/Desktop` remain selectable candidates but are never default roots or probed.
The home directory itself is no longer walked. Automatic Git-file pointers and linked-worktree
entries into protected folders are not read unless that folder was explicitly selected as a
root/container. Such skipped linked entries produce a scan note. Configured `worktree_roots` replaces the default
roots; explicit `--root` wins. `--no-default-roots` permits an empty explicit selection.
Containers and live/remembered discovery apply independently of that selection.

The app's first **Scan worktrees** opens **Where are your projects?**, also reachable from
Settings > Worktrees > **Choose folders…**. A bounded, off-main-thread CLI probe counts repos
at depth 2 with a shared two-second/20,000-entry budget. Counts are suggestions, not the
full depth-3 scan. Folders with repos start selected; empty folders do not. Documents and Desktop
start unchecked with “macOS will ask for permission” and are not probed, even in the scriptable
`procwho worktrees --suggest-roots --json` output. Their `repo_count` is null; existence is
assumed so listing candidates never opens protected folders. Other unprobed/timed-out counts
are null too. Add folder accepts other locations, including external drives. **Scan** saves
roots in UserDefaults and scans; **Not now** leaves them unchanged. An empty selection still
finds automatic sources. The empty result names folders searched and offers the chooser again.

## Worktrees found automatically

Only existing containers are walked. Locations below are observed filesystem layouts or
supported by the linked official docs/source; these do not prove every version uses them.
Repo-local folders are entered when their main repo is discovered. Custom locations are found
through live/remembered repos or `--container PATH`.

| Product | Location | Evidence / scope |
|---|---|---|
| T3 Code | `~/.t3/worktrees` (depth 3) | Observed managed and temporary checkouts on macOS; historical defaults, custom roots vary |
| Codex app | `~/.codex/worktrees` (depth 3) | Observed local linked checkouts; [worktree docs](https://developers.openai.com/codex/app/worktrees) |
| Cursor | `~/.cursor/worktrees` (depth 3) | Observed local `.git` file checkouts; [parallel/worktree docs](https://cursor.com/docs/configuration/worktrees) |
| Claude Code / Desktop | `<repo>/.claude/worktrees` (depth 2); `~/.claude/worktrees` (depth 3, legacy compatibility) | [Documented repo-local default](https://code.claude.com/docs/en/desktop#run-a-session-in-its-own-worktree); global layout retained from prior local observation, absent on this check |
| Conductor | `~/conductor/workspaces` (depth 3) | [Documented default since 0.25.0](https://www.conductor.build/changelog/0.25.0-workspace-storage-improvements-ai-response-metadata-and-slash-commands) |
| Windsurf / Cascade | `~/.windsurf/worktrees` (depth 3) | [Documented legacy Cascade location](https://docs.devin.ai/desktop/cascade#location) |
| Factory / Droid | `~/.factory/worktrees` (depth 3) | [Documented desktop default](https://docs.factory.com/factory-app/worktrees); CLI supports a [custom managed root](https://github.com/Factory-AI/factory/blob/main/docs/reference/cli-reference.mdx#git-worktrees) |
| OpenCode | `~/.local/share/opencode/worktree` (depth 3) | [Upstream default under global data](https://github.com/anomalyco/opencode/blob/dev/packages/core/src/worktree/strategies.ts); XDG relocation/custom strategies require `--container` or live seeding |
| Gemini CLI | `<repo>/.gemini/worktrees` (depth 2) | [Documented experimental worktree mode](https://github.com/google-gemini/gemini-cli/blob/main/docs/cli/git-worktrees.md) |
| Kilo Code | `<repo>/.kilo/worktrees` (depth 2) | [Upstream WorktreeManager](https://github.com/Kilo-Org/kilocode/blob/main/packages/kilo-vscode/src/agent-manager/WorktreeManager.ts) |
| Vibe Kanban | `$TMPDIR/vibe-kanban/worktrees`, `$TMPDIR/vibe-kanban-dev/worktrees` (depth 3) | [macOS temp root](https://github.com/BloopAI/vibe-kanban/blob/main/crates/utils/src/path.rs), [worktree suffix](https://github.com/BloopAI/vibe-kanban/blob/main/crates/worktree-manager/src/worktree_manager.rs); custom workspace overrides need live seeding or `--container` |

No guessed global container is added for Antigravity, Grok, Kiro, Amp, Zed, pi or Crush.
[Antigravity documents worktree support](https://antigravity.google/docs/features) but a stable
full container path was not verified; `.system_generated/worktrees` alone is not a root.
[Kiro workflows](https://kiro.dev/docs/workflows/) can use worktrees but no fixed global root
was verified. [Amp creates sibling folders](https://ampcode.com/news/one-runner-many-worktrees);
[Zed defaults to a project-relative `../worktrees` location](https://zed.dev/docs/reference/all-settings).
Those repos are covered by Git enumeration after root/live discovery. Upstream
[pi](https://github.com/badlogic/pi-mono) and [Crush](https://github.com/charmbracelet/crush)
source trees supplied no native worktree-container module; extension-defined paths remain custom.
Grok's observed grove directory and worktree database filename do not establish a checkout container.
The local `.gemini`, `.antigravity`, `.kiro`, OpenCode config/data and selected agent application
support directories supplied no additional `.git` file container in the bounded directory-only check.
Cloud/background agents on remote machines are outside the local process/disk inventory.

| Verdict | Meaning (precedence order) |
|---|---|
| `missing` | directory gone; git calls the entry prunable |
| `orphaned-dir` | has a `.git` file, but its repo does not list it (or the repo is gone); copies of a worktree land here |
| `in-use` | a live process cwd is inside it (`--no-procs` skips this check) |
| `has-work` | tracked changes, untracked files, or commits on no remote-tracking ref whose changes are not in the default branch |
| `merged-idle` | merged (see below), clean, idle, not locked |
| `merged-locked` | as `merged-idle`, but `git worktree lock`ed |
| `stale` | not merged, clean, idle, no activity for `--stale-days` (default 7) |
| `active` | none of the above |
| `unknown` | a fact the decision needed could not be determined; the reason says which |

These labels are for review, not instructions. Unknown is never reported as "no": a field that
could not be read is `null` in JSON, with a note. Limits to keep in mind:

- Merged, ahead/behind and unpushed are only as fresh as each repo's last fetch
  (`.git/FETCH_HEAD`, shown per repo). procwho never fetches.
- Merged means any of: the tip is an ancestor of the default branch (`merged`); every change
  on the branch is already in the default branch (`content_merged`, see below); or, with
  `--pr`, a GitHub PR was merged with exactly this tip as its head. The reasons say which.
  Unpushed commits of a merged branch do not make it `has-work`; tracked changes and
  untracked files always do.
- `content_merged` catches squash and rebase merges offline: procwho merges the tip into the
  default branch in memory (`git merge-tree --write-tree`, git 2.38+) and compares the result
  tree with the default branch's tree. Equal trees mean merging would change nothing:
  "all changes on this branch are already in origin/main (squash or rebase merge), as of last
  fetch". A conflict or error means not contained (or unknown, with a note). It is only run
  for tips that are not ancestors of the default branch. Caveats: it compares against the
  local copy of the default branch ref (as of the last fetch), so something merged since
  shows as not contained; it is also true for a branch with no net changes; and if the
  default branch later changed the same lines again, a squash-merged branch shows as not
  contained (the cautious direction).
- Flags (facts, not verdicts) are shown on the reasons line: `upstream-gone` (the branch's
  upstream was deleted, typically after its PR was merged), `locked`, `detached`.
- `--pr` (off by default) runs one `gh pr list --state merged --limit 200` per repo with a
  GitHub remote, passing only that remote's `owner/name`; 10 s timeout; cached in
  `<state>/worktree-prs.json` for 6 hours. A missing, logged-out or failing `gh` is a note.
  The match is by branch name; only a PR merged at exactly the worktree's tip
  (`head_matches_tip`) counts as merged, a same-named PR at another tip is a reason line.
  Without `--pr`, `pr_merged` is `null` and the notes say PR state was not checked.
- Sizes are allocated blocks (`st_blocks * 512`, lstat, hard links counted once, symlinks not
  followed, one filesystem). APFS clones share blocks, so the sum can be larger than what
  removing the worktree would free. Sizes are cached in `<state>/worktree-sizes.json` for
  `--size-ttl` minutes (default 30) and marked `size cached` with their age.
- Last activity is the newest of: tip commit date, the worktree's `index`, `HEAD` and
  `logs/HEAD` mtimes, and the worktree directory mtime.
- Under the totals, one line of arithmetic: "reclaimable if all merged-idle were removed: X
  (of which Y is rebuildable caches)", with the APFS clone caveat. procwho never deletes
  anything and prints no deletion commands.

See [JSON output](JSON.md#worktrees-schema-v2) for the worktree fields.

Read-only and low impact: only read commands run (`worktree list`, `for-each-ref`,
`symbolic-ref`, `branch --merged`, `merge-base --is-ancestor`, `log`, `status`, `rev-list`,
`rev-parse`, `config --get-regexp`), plus `merge-tree --write-tree`, which writes objects. It
runs with `GIT_OBJECT_DIRECTORY` set to a fresh temp dir and the repo's objects as an
alternate, under `sandbox-exec` with every file write denied outside that temp dir. The
sandbox is needed because git "freshens" (touches the mtime of) an existing object instead of
writing a copy. The temp dir is removed afterwards, and a test checks that the repo's `.git`
is byte- and mtime-identical. Every git call runs with `-c core.fsmonitor=false -c core.untrackedCache=false --no-optional-locks`, so no
`git fsmonitor--daemon` is started and no index lock is taken. Each call has a 10 s timeout
(the field becomes unknown), work runs on `--jobs` threads (default 4), and git children and
walker threads run with iCloud materialization turned off, so a worktree under an
iCloud-synced folder reports unknown instead of downloading evicted files. Nothing is pruned,
removed, fetched or checked out. The size cache, known-repo list and, with `--pr`, the PR cache are procwho-owned files.

Library: `procwho_wt::scan(&Options::default(), Some(&live))` returns a serializable `Report`;
`procwho_wt::render(&report, n)` formats it. `live` is a list of `procwho_wt::LiveUse`
(the CLI maps `procwho_proc::Snapshot::collect().live_uses()` into it).


The menu bar app searches repo, branch and path locally and sorts by Size,
Idle time or Repo. The verdict legend filters the list; the merged-idle hero
caption applies that filter too. Clearable chips show active filters even when
no rows match. Free space is the home volume's latest status measurement,
not a claim about every worktree volume. Measure sizes runs a read-only sizing
scan; errors explain the failed read, and cached sizes retain measurement time.

Shared system temporary directories (`/tmp`, `/private/tmp`) are not default containers:
they can hold other users' data and large unrelated trees. Agents and review tools sometimes
create throwaway worktrees there. Select a specific folder with `--root` or `--container`;
live agent repositories and remembered repositories can also identify them without walking
the shared directory.
