# Procwho menu bar app

Procwho maps running processes to named workloads. The app is read-only and never
executes commands shown in its reports. It has no stop, kill, or delete controls.

## Native macOS presentation

Use system fonts, semantic system colours, SF Symbols, app icons, and standard
macOS controls. Use grouped Forms in Settings and grouped label/value rows for
measurements. Avoid decorative cards, tinted icon tiles, pills, custom chrome,
gradients, shadows, and emojis. Text uses primary or secondary colour. Attention
and critical states use orange and red symbols sparingly.

Keep prose short and in sentence case. Section headers are 11 pt semibold in
secondary colour, without tracking. Tooltips contain at most two short sentences.
Labels have no estimate prefix or parenthetical qualifiers. Estimated numbers
retain `~`. The glossary intro is “Labels describe evidence.” Each definition is
one sentence of at most 20 words. The first-launch card and About each contain
one read-only statement; other screens do not repeat it.

## Tokens

Visual constants live in `Sources/ProcwhoBar/Theme.swift`. Native Form spacing
and typography are the exception to panel tokens.

- Panel: 440 × 640 pt; Settings: 520 × 640 pt; About: 440 × 360 pt.
- Insets: 14 pt at the panel edge; spacing: 4, 8, 12, and 16 pt; text gap: 2 pt.
- System type: hero 30 pt semibold, identity 15 pt semibold, row title 13 pt
  regular, number 13 pt semibold, section 11 pt semibold, caption 11 pt.
- Numbers use monospaced digits. PIDs, paths, and commands use 11 pt monospaced type.
- App icons: 20 pt in rows, 32 pt in detail. Cache NSWorkspace icons in memory
  per bundle path; missing paths use plain secondary SF Symbols.
- Workload rows: minimum 40 pt. Process rows: minimum 24 pt, with no extra gaps.
- Disclosure control: 12 pt wide; members are indented 48 pt from the list inset.
- Numeric columns: memory 64 pt, CPU 44 pt, both right-aligned; gauge 108 pt.
  Root process labels use 84 pt; numeric settings fields use 72 pt.
- Composition strip: 10 pt high, 1 pt gaps, 4 pt corner radius. Row share bars:
  56 × 3 pt, quaternary track, family colour fill, 4 pt radius.
- Hover selection: quaternary fill, 6 pt radius. Increase Contrast adds a
  secondary outline. Intro and source warnings may use a neutral grouped surface.
- Motion: 120 ms hover/bar transitions, 220 ms detail transition. Reduce Motion
  removes slides and bar animation. Numeric changes use numeric text transitions.

Family colours identify composition and share bars; symbols remain secondary.

| Family | Kinds | Colour |
|---|---|---|
| Agents | agent-session, agent-toolset, agent-command, agent-host | Teal |
| Leftovers | detached | Orange |
| Unknown owner | unknown | Yellow |
| Simulators | simulator | Pink |
| Services | service, watcher, declared, custom | Blue |
| Apps | app, terminal, terminal-pane | Indigo at 70%, full in Increase Contrast |
| System | system, system-opaque | Secondary |

Agent SF Symbols distinguish session (`sparkles`), tool set
(`wrench.and.screwdriver`), command (`terminal`), and shared host (`server.rack`).
Other kinds use `Theme.symbol(kind:)`.

## Measurements

Memory is process footprint, as shown in Activity Monitor. Footprint sums can
count shared memory more than once and are distinct from physical RAM use.
“Not in RAM” estimates compressed plus swapped memory using footprint minus
resident memory, floored at zero. It is part of footprint, not an extra amount.
CPU is a percent of one core and can exceed 100%.

Display binary sizes as MB, GB, and TB. Switch to TB at 1024 GB; use at most one
decimal for GB/TB, drop trailing zeroes, and use whole numbers above 100.
Positive sizes below 1 MB read “<1 MB”. Installed RAM is whole GB. Counts have
thousands separators; PIDs and ports retain their literal form. Missing values
are dashes, never numeric zeroes.

Pressure is normal, warning, or critical. When unavailable, use one-minute load
relative to logical CPU count. Load above one task per core is attention; above
two tasks per core is critical. Swap's meter compares used swap with installed
RAM. Swap above 50% of RAM is attention, above 100% is critical; less than 10%
free space on the swap volume is critical. Exact thresholds stay in the lower
state. Swap tooltips show allocated swap files and free disk space.

## Overview

The segmented control switches Overview, Review, and Worktrees. The Overview
hero cycles between “Agents + associated”, “Agents”, and “Memory used” by
clicking its label, chevron, or value. Persist the choice. With no agents, the
associated view uses physical memory instead. Associated memory counts agents,
known host apps, and workloads linked by launching evidence, once per process.

The hero has one caption line with at most two facts, such as
“12 sessions · 48 GB RAM”. Helper groups, host apps, and medium-confidence linked
bytes belong in its tooltip. Pressure and swap gauges sit alongside it.

Keep the composition strip and wrapping filter legend. Its denominator is the
sum of attributed footprints, not physical RAM. The total and accounting note
belong in the tooltip. Omit zero and unmeasured segments. The hero lens dims
excluded bytes to 35% without changing widths. There is no legend help button;
“What the labels mean” is available in the gear menu.

A machine warning alone is one plain line, such as “Swap is high: 9.6 GB”,
without a card or header. Other measurements and the largest two footprint
families belong in its tooltip. Alerts and review findings appear as plain rows
under “Needs attention”. Do not repeat a machine alert already represented by
the machine line. Show up to three findings with an “All findings” link.

Search and sort precede one list header: count on the left, “Memory” and “CPU”
over their columns. Not-in-RAM sorting changes the memory heading accordingly.
The list is flat by default; optional family grouping ranks by the selected
metric and has sticky, collapsible family headers. Search and family filters
flatten the list. Filters are clearable plain text controls. System workloads
are hidden by default, with a count and Show control below the list.

### Workload outline

Every workload has a small secondary `chevron.right`, rotated 90 degrees when
open. Its button is separate from the rest of the row, which opens detail.
VoiceOver labels are “Show processes” and “Hide processes”. Keep expansion by
workload key across refreshes; each refresh replaces members with current data.

Titles use `Presentation.displayName`, stripping `UNKNOWN owner: ` and ` (app)`.
Unknown workloads start their subtitle with “Owner unknown”. Ordinary subtitles
retain process count and age, such as “9 processes · 2d 17h”. “Mostly not in RAM”
is plain subtitle text, shown only at the configured share and size thresholds.
Weak confidence and agent links can have small secondary symbols with tooltips.
Parent rows retain their share bar.

Expanded members show every process in a lazy list: name in regular 13 pt type,
PID in 11 pt monospaced secondary type, footprint and CPU in the parent's numeric
columns. No icons, bars, cards, or extra separators. Sort members with the current
sort; Not-in-RAM uses footprint because member reports do not contain RSS.
Hover shows the command. Context menus offer Copy PID and Copy Command; member
text is not selectable directly.

Search also matches member names and PIDs. A member-only match opens its parent
while search is active and shows only matching members, without changing saved
expansion. A workload identity match keeps all members available.

Keyboard: Up/Down highlight workloads, Right expands, Left collapses, Return
opens detail, Escape goes back or clears filters, Cmd-F focuses search, Cmd-R
refreshes, Cmd-1/2/3 switch tabs. Left/Right retain text-editing behaviour in search.

### Data compatibility

Request `status --json --limit 0 --members`. If an older CLI rejects `--members`,
retry without it and periodically probe again. Missing members show
“Update the procwho CLI to list processes.” on expansion. Older CLIs without
status retain the top/findings fallback. `why` and status workload objects accept
optional `members` and `bundle_path`; missing fields remain valid.

## Workload detail

Use the status identity while why loads. Show a 32 pt icon, name, and one
secondary kind line. Weak confidence alone gets one warning sentence and the
“What the labels mean” link.

Measurements use one grouped box with label/value rows and hairline dividers:
Memory, Not in RAM, CPU, Processes, Age, Ports. Hide Not in RAM when zero or
missing; hide empty Ports. CPU uses the latest matching status sample and keeps
sample age in its tooltip; missing CPU reads “Not sampled”.

Sections are “How it was identified”, “If it stopped”, “Processes”, “History”,
and “Root process”. Processes uses all why members when available, otherwise
status members, with the same rows as the outline. Root process fields have copy
controls. Reveal commands remain copyable text and are never executed.

The bottom buttons form one row: Copy naming rule, Reveal in Finder, Copy PID,
and Copy Command, when available. There is no Actions heading. Naming-rule
confirmation reads “Copied. Paste it into ~/.config/procwho/registry.json.” with
the existing configuration link. No registry file is written.

## Review

Show review findings as plain rows with evidence, unproven facts, and a workload
link. When there are no review findings, show “Nothing needs review.” above any
collapsed “Informational” group. No celebratory empty-state illustration or
explanatory disclaimer.

## Worktrees

Before sizes are measured, the hero is the worktree count labelled “Worktrees”,
with a merged/idle count and one Measure sizes button. Once measured, show the
estimated reclaimable size labelled “Reclaimable”. Never use “Not measured” as
the hero. Keep one capacity line, such as “18.2 GB free of 926 GB”. Capacity is
for the home volume; the tooltip names it.

Hide disk composition until a size is measured. Keep the verdict filter legend
without a share caption or unmeasured-count paragraph. APFS clone sharing is
explained in the size tooltip. Unknown sizes show an en dash and no share bar.
The list header is a sentence-case count such as “286 worktrees”.

Rows show repository, branch, a small verdict-coloured dot with plain text,
applicable idle time, flags, and size. Verdict colours: merged-idle teal, in-use
blue, has-work orange, stale pink, active indigo, other secondary. Hide idle time
for live processes; recent idle time is “just now”. Expand rows for paths,
reasons, and measurement age. Keep Finder and Copy path controls on hover.
Scan status and Rescan remain in the bottom caption row.

## Settings and chrome

Settings uses native toolbar tabs and grouped Forms. General contains launch at
login, refresh, and menu bar controls. Show login status only for approval or an
error. Workloads includes sort, family grouping, system visibility, a disclosure
for default collapsed families, and the nonresident thresholds. Automatic limits
use installed RAM and remain automatic until edited as Custom.

Worktrees contains folders, stale age, PR checking, and rescan cadence. Its folder
sheet shows one instruction, folder choices, protected-folder permission notes,
automatic sources, Add folder, Not now, and Scan.

Alerts has one “Alerts” toggle; enabling it requests notification permission.
Only denied permission gets a status row with Open System Settings. Conditions
use plain labels including “Not in RAM above”. The interval row reads
“Repeat an alert at most every”. Advanced contains the CLI picker, diagnostics,
reset, and data sources. Reset and notifications preserve their existing system
behaviour; snapshot mode never registers login or requests permission.

The footer shows update age and the gear menu: Refresh now, Refresh settings,
Settings, What the labels mean, About Procwho, Quit. Failed sources and CLI errors
remain visible as dismissible warnings; informational source notes stay in
Advanced and About. Loading uses skeletons; refresh errors keep stale data visible
with repair controls. Menu bar modes follow the selected hero or a fixed measure.

## Snapshots

`ProcwhoBar --snapshot <directory>` renders the invented screenshot fixtures from
this checkout in light and dark. Explicit status/why/worktrees fixture arguments
remain supported. `--live` uses the bundled CLI and must never supply public
screenshots. Snapshot preferences are isolated and do not change user settings.

Capture Overview, expanded processes, member search, detail including its process
section and actions, Review, measured/unmeasured Worktrees, every Settings tab,
and the folder chooser. Inspect both appearances for wrapping, clipping,
alignment, and leftover decorative elements. Commit public screenshots only from
`Tests/Fixtures/screenshot-*.json`; keep live output outside the repository.
