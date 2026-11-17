# Procwho menu bar app

The macOS 14+ app is a read-only SwiftUI client of the bundled Rust CLI.
[DESIGN.md](DESIGN.md) defines the UI. `Theme.swift` owns visual tokens;
`Components/` contains shared surfaces and meters; `ProcwhoKit` owns decoding,
ranking, family composition, shares, and binary GB/MB formatting.

## Build and verify

From the repository root:

```sh
cd app
swift build -c release
swift test
scripts/bundle.sh
```

The bundle is `app/build/Procwho.app`. If that bundle is running, use
`scripts/bundle.sh --verify` to build `build/verification/Procwho.app` without
replacing it. The script bundles the existing
`target/release/procwho` CLI, signs the CLI and app ad hoc, and verifies the
signature. No dependencies need installing. The app uses local CLI output;
it does not execute suggested reveal commands. Remote pull-request checks are
optional and off by default.

## Developer snapshots

The hidden snapshot entry point renders the same `ContentView` in an offscreen
440×640 `NSWindow`/`NSHostingView`. Panel PNGs are 880×1280; About is 880×720.
It writes 51 states in both light and dark, including each settings tab,
Alerts with individual conditions enabled or disabled, Overview with an active alert, detail, Review, Worktrees, loading, error and About.
Settings images include the native toolbar and are 1040×1440. Snapshot models do not poll, update preferences, or create menu bar items.

From the repository root, use deterministic fixture inputs:

```sh
app/build/Procwho.app/Contents/MacOS/ProcwhoBar \
  --snapshot app/build/integration-snapshots \
  --status app/Tests/Fixtures/design-review-status.json \
  --why app/Tests/Fixtures/why.json \
  --worktrees app/Tests/Fixtures/design-review-worktrees.json
```

All three input files are required in file mode. Installed RAM comes from
`status.system.mem_total_bytes`; roots come from optional `status.worktree_roots`,
then `worktrees.searched_folders` (or an empty list). Fixture mode stays deterministic.
To collect local data through the bundled CLI, use:

```sh
app/build/Procwho.app/Contents/MacOS/ProcwhoBar --snapshot app/build/live-snapshots --live
```

`--live` cannot be combined with input paths. It reads RAM and configured roots from
the machine. Both modes apply explicit scenario overrides (pressure, swap and folder
picker examples); their receipt identifies the source and original RAM/roots.
Neither mode writes real preferences or enables login/alerts. `receipt.json` records counts and
dimensions. `build/` is ignored. The dedicated `screenshot-*.json` fixtures contain
invented public examples; the design-review fixtures cover long titles and unknown
measurements. Do not use captured machine data for published screenshots.

## Interaction

Cmd-1/2/3 switches Overview/Review/Worktrees. Cmd-F focuses workload search;
Cmd-R refreshes. Up/Down selects visible workloads, Return opens detail, and
Esc returns or clears search/filter. Text editing shortcuts stay native.
The gear menu opens Settings (Cmd-,), with a shortcut to refresh settings.
General holds launch at login, open/background polling and menu bar display.
Workloads holds ranking, optional family grouping, system visibility, collapsed
groups, minimum size and "~mostly not in RAM" badges. Unknown owner is a separate
yellow family after Leftovers.
Worktrees holds Choose folders, stale age, optional pull-request checks and rescans.
The first Scan worktrees opens the project folder sheet. The CLI probes existing candidates
at depth 2 with a shared two-second budget; Documents/Desktop are listed unchecked and
never probed. Agent containers and live/remembered repositories are automatic.
The selection is saved in UserDefaults, including an empty selection.
Alerts separates its on/off switch from macOS permission, with an explicit
Request permission or Open System Settings button, plus thresholds and cooldown. Advanced holds CLI
selection, safe diagnostics, reset and data source notes. Sleep and low battery
pause automatic sampling; stale data remains visible. Launch at login and
notifications are never enabled by tests or snapshot rendering.

Total memory in use reads macOS VM counters (app, wired and compressed memory,
excluding purgeable and file-backed cache). Other memory values come from the CLI.
The optional GitHub setting asks the CLI to use `gh pr list`; it is off by default.

Public README images live in `docs/screenshots/`. Full verification captures,
including all five settings tabs in light and dark and Overview with an active
alert row, live in `build/integration-snapshots/`.

Offscreen settings captures use AppKit’s toolbar tab controller with the same
SwiftUI forms; the live window uses SwiftUI’s Settings scene and TabView. This
avoids the standalone TabView’s vibrancy drawing artifacts in bitmap capture.

Overview ranks across families by default. Group by family ranks group totals by
the selected metric. Filter chips remain visible with no matches. Worktrees adds
repo/branch/path search, Size/Idle time/Repo sorting, home-volume free space,
clearable verdicts, size ages and a Measure sizes action. Detail CPU uses the latest
matching status sample, independently of the unsampled why explanation.

Swap shows used bytes, with meter length relative to installed RAM. Colour follows memory
pressure, swap strictly above 50%/100% of RAM, and swap-volume free space below 10%.
The tooltip labels swap files as allocated so far and gives free disk space. Swap alerts
use GB, with Automatic at 25% of RAM rounded to 0.5 GB (floor 2 GB). Old percentage
preferences migrate to Automatic. Critical memory pressure has a separate default-on
condition inside the optional, initially disabled alert system.
