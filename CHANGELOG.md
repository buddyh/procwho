# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-11-16

### Added

- Plain-language ownership evidence, impact descriptions, findings and CLI summaries,
  with complete redaction at report output boundaries.

- App attribution for macOS code-sign clones, including Chrome, using matching
  installed bundle IDs and executable paths at medium confidence.

- Nullable, existing outer app `bundle_path` in workload JSON for native app icons.

- Exclusive process members in `status --json --members` and `why`, with sorted,
  redacted commands and bounded MCP explanations.

- Worktree discovery from deduplicated live-agent and associated-process directories,
  bounded remembered repositories, per-row discovery sources, and verified Cursor,
  Conductor, Windsurf, Factory, OpenCode, Gemini, Kilo and Vibe Kanban containers.
- First-scan project folder chooser, bounded CLI root suggestions, protected-folder
  permission notes, custom folders and an empty-results chooser shortcut.
- RAM-relative swap meter and pressure/disk-aware colour, GB-based Automatic swap
  alerts, critical-pressure alerts and migration from legacy percentage preferences.
- Conventional `git`, `github`, `Code`, `Sites` and `go/src` root candidates;
  Documents/Desktop are opt-in and never probed. `worktrees --forget-known` clears
  procwho-owned repository memory.

- Physical Memory Used, available, wired and compressed bytes from macOS VM
  counters, plus kernel memory pressure in CLI, MCP and the menu bar app.
- Evidence-based agent associations and exclusive agent/associated/medium totals.
- Three persisted Overview views, matching menu bar choices, partial strip
  highlights, linked workload badges and a Pressure gauge with Load fallback.
- Copy naming rule in workload detail, `why --rule`, and MCP `suggested_rule`,
  with safe matcher selection, current match counts and a worked naming guide.
- Schema v3 status, workload reports and MCP results; shared glossary and neutral
  screenshots cover the new memory views and naming workflow.

- First public release of the macOS CLI and Swift menu bar app.
- Process ownership labels for apps, launchd services, coding-agent sessions,
  shared hosts, tool sets, detached leftovers, and unknown workloads.
- Globally ranked workloads with optional family grouping, a separate Unknown owner
  family, session/helper counts, visible filters and system visibility controls.
- Footprint accounting, estimated nonresident memory, explicit composition and RAM-relative swap
  context, CPU sample states and ages, ownership evidence and bounded history.
- Neutral review scope and machine pressure context before individual findings.
- Searchable and sortable git worktrees with usage, measured disk sizes and ages,
  home-volume free space, git state and descriptive review labels.
- Timestamped, versioned JSON and redacted text output with named inclusive and
  exclusive accounting; complete workload names and descriptive impact.
- Read-only MCP over stdio with untrusted-host-data warnings and instance-bound
  protection for the calling connection's ancestors, session, host and tool helpers.
- A "~mostly not in RAM" estimate badge and explicit identity-confidence labels.
- Alerts that default off, with separate macOS notification permission controls.
- Optional configuration for scan roots, label providers, and additional agent rules.
- MIT licence, contribution guidance, CI, and local release packaging scripts.

[0.1.0]: https://github.com/buddyh/procwho/releases/tag/v0.1.0
