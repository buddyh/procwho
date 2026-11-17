# Contributing

Use macOS, Rust stable, and Xcode 16.4 or later (Swift 6.1.2+, Swift 6 language mode). The CLI supports macOS
13 or later; the menu bar app needs macOS 14 or later. Matching Command Line Tools
alone support `swift build`, `swift test`, and `bundle.sh`; universal release packaging
requires full Xcode. The package uses Swift tools 6.1 and Swift 6 language mode.

Compatibility CI uses `macos-15` with Xcode 16.4 (Swift 6.1.2, macOS SDK 15.5),
plus `macos-latest` with its default Xcode. GitHub retired macOS 14 runners on
November 2, 2026. See the [runner image inventory](https://github.com/actions/runner-images/blob/main/images/macos/macos-15-arm64-Readme.md#xcode)
and [retirement notice](https://github.com/actions/runner-images/issues/13518).
The minimum toolchain is checked by CI; newer SDKs are checked locally and on latest.

From the repository root, with [Rust installed](https://rustup.rs/):

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --release
cargo test --workspace
cd app && swift build && swift test
cd ..
app/scripts/bundle.sh
```

Keep these rules in every change:

- Read-only: no code that stops, signals, restarts, or deletes observed processes,
  services, repositories, or worktrees. Local history and caches belong in procwho's state directory.
- Privacy: redact argv before display or storage. No environment dumps, transcript
  reads, or new network access. Preserve the narrow identity allowlist. The existing
  opt-in `worktrees --pr` lookup is documented separately; do not expand it silently.
- Fixtures: use synthetic data. No personal paths, project names, hostnames, or credentials.

To add agent recognition, start with the detection table and rule contract in
[Architecture](docs/ARCHITECTURE.md#agent-table). Every rule needs an observed or
documented source. Add a synthetic fixture and explain the confidence and identity
limits. An executable match alone does not prove a session identity.

Use small conventional commits, such as `fix(proc): redact provider labels`.
Run the relevant tests and explain any check you could not run in your pull request.
