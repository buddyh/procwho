# Public screenshots

These images use invented workloads, processes, and repositories from
`../../Tests/Fixtures/screenshot-*.json`. They contain no live machine data.

- `overview-light.png` and `overview-dark.png`: Overview with native workload rows.
- `process-outline-*.png`: Expanded workload with every member process.
- `member-search-*.png`: PID search with only matching members visible.
- `workload-detail-*.png`: Identity, grouped measurements, and process list.
- `processes-detail-*.png`: Full process section and root process information.
- `worktrees-measured-*.png` and `worktrees-unmeasured-*.png`: Measured and unmeasured disk states.
- `review-*.png`: Review findings as plain rows.
- `settings-{general,workloads,worktrees,alerts,advanced}-*.png`: Native Settings tabs.

Regenerate from the repository root into a directory outside the checkout:

```sh
swift build --package-path app
app/.build/debug/ProcwhoBar --snapshot /tmp/procwho-screenshots
```

The default fixtures are resolved from the source checkout. Explicit fixture
paths are also supported:

```sh
app/.build/debug/ProcwhoBar --snapshot /tmp/procwho-screenshots \
  --status app/Tests/Fixtures/screenshot-status.json \
  --why app/Tests/Fixtures/screenshot-why.json \
  --worktrees app/Tests/Fixtures/screenshot-worktrees.json
```

Inspect light and dark images before copying them here. Keep public filenames
stable to preserve README links. `memory-views-receipt.json` records the fixture
inputs and rendered scenarios. It contains no live report data.

Additional scenarios cover the memory lenses, pressure and swap states, naming
rules, folder selection, empty states, filters, denied notification permission,
and compatibility with CLIs that do not report process members. Settings captures
use native AppKit toolbar tabs with the same SwiftUI Forms as the live window.
Snapshot settings are isolated; rendering never registers login or requests
notification permission.

Live checks use the bundled CLI:

```sh
app/scripts/bundle.sh
app/build/Procwho.app/Contents/MacOS/ProcwhoBar \
  --snapshot /tmp/procwho-live-screenshots --live
```

Never copy live snapshots into this directory.
