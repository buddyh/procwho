# Supported coding agents

procwho recognizes agent runtimes from executable and package paths, guarded command
positions, and process ancestry. It does not read conversations, transcripts, session
databases, or IDE state. A recognized installation path is evidence, not a code-signature
check. Copied binaries and other installation layouts can remain unknown.

A **session** is a recognized local runtime with process-instance identity. Its provider
session ID can still be unresolved. A **host** serves multiple sessions or agent clients;
its PID, cwd, and inherited environment do not identify those sessions. A **tool set** groups
helpers beneath a proven provider host; a **command** has an established provider parent.
Neither timing nor a nearby session filename supplies a thread ID.

Identity confidence means:

- **Exact:** an explicit, validated session ID in argv, or an allowlisted T3 thread ID with
  T3 application ancestry. Host-thread and provider-session IDs remain separate fields.
- **High:** a product-specific executable, bundle, or package-launcher path. Shared hosts
  normally have high confidence and unresolved session identity.
- **Medium:** executable-name recognition with an additional command/module guard.
  tmux ancestry supplies context, not exact provider identity.

The verification column separates **verified on a real install** (observed binary,
package, launcher, or bundle evidence) from **from documentation**. Installed-file evidence
alone does not mean an agent was running or that all modes were tested. Every enabled rule
also has synthetic positive and negative tests. Server modes and ID options can come from
documentation even when the installation path was observed. Research checked 2026-11-16.

| Agent | What is recognized | How identity is established | Identity confidence | Verification status |
|---|---|---|---|---|
| [Claude Code](https://github.com/anthropics/claude-agent-sdk-python/blob/main/src/claude_agent_sdk/_internal/transport/subprocess_cli.py) | CLI/headless/SDK/Claude-desktop embedded sessions | Versioned native, desktop or SDK bundle, or `@anthropic-ai/claude-code/cli.js`; validated UUID from `--resume` / `--session-id` | High; exact with explicit ID | verified on a real install (CLI and desktop; SDK/legacy launcher from documentation) |
| [Codex](https://developers.openai.com/codex/cli/reference) | CLI sessions; `app-server` / `exec-server` hosts, including ChatGPT embedded; code-mode / computer-use helper tool sets | `@openai/codex` native package or launcher, ChatGPT CodexCLI bundle; UUID after `resume` / `exec resume` | High; exact with explicit ID; hosts high | verified on a real install (including running hosts) |
| [Cursor](https://docs.cursor.com/en/cli/reference/parameters) | CLI sessions; desktop host only with a recognized provider descendant | Versioned `cursor-agent` executable / `index.js`; explicit `--resume` chat ID; guarded desktop bundle | High; exact CLI ID when supplied | verified on a real install (launcher/bundle; no live agent chain observed) |
| [Gemini CLI](https://geminicli.com/docs/cli/cli-reference/) | CLI sessions | Google package `bundle/gemini.js` / `dist/index.js`; full UUID from `--resume` / `-r`, never an index or filename prefix | High; exact with full ID | verified on a real install (package/launcher) |
| [Google Antigravity](https://antigravity.google/docs/cli/reference/) | `agy` sessions; bundled desktop `language_server` host | Official `.local/bin/agy` install shape; UUID from `--conversation`; full Antigravity service bundle path | High; exact CLI ID when supplied | verified on a real install (binary/service path) |
| [Grok CLI / Grok Build](https://docs.x.ai/build/cli/reference) | Native sessions; `agent stdio` host | `.grok/bin` / versioned download path; UUID from `--session-id` / `--resume` | High; exact with UUID | verified on a real install (native install paths) |
| [OpenCode](https://opencode.ai/docs/cli/) | Package runtimes; explicit `serve` / `web` hosts | `opencode-ai/bin/opencode` including macOS `.exe`; explicit `-s` / `--session` ID | High; exact explicit ID; hosts high | verified on a real install (native package) |
| [pi](https://github.com/earendil-works/pi-mono/blob/main/packages/coding-agent/docs/cli.md) | CLI sessions; `--mode rpc` host | Current `@earendil-works` and older `@mariozechner` coding-agent launchers; UUID from `--session-id` / UUID-valued `--session` | High; exact with UUID; RPC host high | verified on a real install (both package namespaces) |
| [Kiro](https://kiro.dev/docs/cli/reference/cli-commands/) | Explicit local `kiro-cli chat` sessions | Distinct basename plus `chat` command; UUID from `--resume-id` | Medium; exact with UUID | from documentation |
| [Amp](https://ampcode.com/docs/cli) | Local native/package runtimes | `.amp/bin/amp` or `@ampcode/cli/bin/amp.exe`; explicit `threads continue T-…` ID | High; exact explicit thread ID | from documentation |
| [Aider](https://aider.chat/docs/config/options) | CLI sessions | Console script in uv/pipx `aider-chat` environment, or Python executable plus exact `-m aider`; no verified session ID | High for scoped script; medium for module guard | from documentation |
| [Goose](https://github.com/aaif-goose/goose/blob/main/crates/goose-cli/src/cli.rs) | Homebrew sessions; explicit `serve` host | `Cellar/block-goose-cli/.../bin/goose`; explicit `--session-id`, including non-UUID IDs | High; exact explicit ID; host high | from documentation |
| [GitHub Copilot CLI](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference) | CLI sessions; SDK `--headless` host | `@github/copilot/npm-loader.js`; explicit `--resume` ID; SDK mode parsed as an option | High; exact explicit ID; host high | from documentation |
| [Cline](https://docs.cline.bot/cli/cli-reference) | Local CLI/package sessions | `cline/bin/cline`, `.cline`, or `@cline/cli-darwin-*` native executable; explicit `--id` | High; exact explicit ID | verified on a real install (launcher/platform package) |
| [Windsurf / Cascade](https://docs.windsurf.com/windsurf/cascade/hooks) | Shared backend host in Devin desktop | Full observed `Devin.app/.../extensions/windsurf/bin/language_server_macos_arm` path; no conversation ID | High host; conversations unresolved | verified on a real install (bundled component; no live Cascade session observed) |
| [Zed external agents](https://zed.dev/docs/ai/external-agents) | Zed host only with a recognized external provider descendant; provider keeps its own family | Zed/Preview/Nightly bundle executable plus provider descendant; provider-specific identity rules | High host; provider-specific session confidence | from documentation; [bundle executable source](https://github.com/zed-industries/zed/blob/main/script/bundle-mac) |
| [Droid / Factory](https://docs.factory.com/droid-cli/cli-reference) | Local CLI / `exec` sessions | Observed `.local/bin/droid` install shape; UUID from `--resume` / `-r` / `exec -s` | High; exact with UUID | verified on a real install (native binary) |
| [Qwen Code](https://github.com/QwenLM/qwen-code/blob/main/packages/cli/src/config/config.ts) | CLI sessions; `serve` / ACP hosts | `@qwen-code/qwen-code/dist/index.js`; explicit `--session-id` | High; exact explicit ID; hosts high | from documentation |
| [Crush](https://github.com/charmbracelet/crush) | Package/Homebrew runtimes; `server` host | `@charmland/crush/run-crush.js` or `Cellar/crush/.../bin/crush`; no ID inferred from an abbreviated selector | High; provider session unresolved | verified on a real install (npm launcher; Homebrew/server from documentation) |
| [Kilo Code](https://kilo.ai/docs/code-with-ai/platforms/cli) | CLI sessions; `serve` / `acp` hosts | `@kilocode/cli/bin/kilo`; no unverified native package child path or ID parser | High; provider session unresolved | from documentation |
| [Continue](https://github.com/continuedev/continue) | `cn` CLI sessions | `@continuedev/cli/dist/cn.js`; no exact ID from resume-latest | High; provider session unresolved | from documentation |
| [Devin local CLI](https://docs.devin.ai/cli/index.md) | Versioned local CLI sessions | `.local/share/{devin,cognition}/cli/_versions/.../bin/devin`; explicit `--resume` / `-r` ID | High; exact explicit ID | from documentation (desktop install does not verify the CLI) |
| [Auggie](https://docs.augmentcode.com/cli/reference.md) | CLI / ACP runtime | `@augmentcode/auggie/augment.mjs`; a session-file path is not promoted to an ID | High; provider session unresolved | from documentation |
| [T3 Code](https://github.com/pingdotgg/t3code) | Application/server host; separately recognized provider children | T3 Code/Nightly bundle path; validated `T3CODE_THREAD_ID` only on a proven provider below T3 | Host high; child exact host-thread context when available | verified on a real install (including running host/provider chains) |
| [Conductor](https://www.conductor.build/docs/reference/harnesses) | Known provider children, with ancestor app origin | Provider signature remains primary; app ancestry supplies origin, not a provider ID; no standalone Conductor worker rule | Provider-specific; shared host/session mapping unresolved | from documentation |

## Deliberate limits

- Bare `pi`, `amp`, `goose`, `crush`, `droid`, `zed`, `q`, generic Node/Bun/Python, and arbitrary
  `gemini`/`codex`/`claude` paths do not establish an agent. Kiro's unqualified default launch
  stays unknown until a native install path is verified; the enabled name rule requires `chat`.
- Cursor renderers/GPU helpers and ordinary editor processes are not sessions. Native Zed
  conversations run in the editor; no process-only per-conversation join is known. An external
  provider below an editor does not prove that its in-process native agent is active.
- Copilot in VS Code, Roo Code, and VS Code Agent Host can share extension/utility processes.
  Recognized provider children are supported; the shared generic process is not assigned to
  one extension or conversation. Installed extension directories do not prove activity.
- OpenHands/Agent Canvas and Warp have no sufficiently verified local worker/launcher signature
  in this set. A browser, terminal, Docker VM, or cloud client is not its remote agent worker.
- Conductor's bundled provider filenames, Kiro desktop workers, Cline's hub daemon, OpenCode's
  automatic background-service invocation, and dynamically unpacked Crush native children
  need more process evidence. Explicit documented server modes are recognized; other modes
  are not asserted to be dedicated one-session workers. No guessed rule is enabled.
- Known management/help/update commands, remote-client modes, Cline `--zen`, and Droid
  `--list-tools` are excluded. Cline `-p` means plan, not headless print. Headless mode is
  product-specific; stream JSON alone does not make Claude a shared host.
- ID pickers, `latest`, abbreviated IDs, titles, cwd encodings, and session-file paths are not
  exact IDs. Opaque explicit selectors must pass the rule's character/length validation and
  existing secret redactor. No database or transcript is read to resolve an abbreviation.
- Proven launcher/native pairs are counted once. Independently launched agents keep separate
  workloads, including agents launched by another provider. Helper timing can group a Codex
  tool set, but cannot assign a session ID or project.

## Verify a change

```sh
cargo build --release
cargo test --workspace
target/release/procwho top --kind agent -n 60
target/release/procwho unknown -n 40
target/release/procwho top --kind detached -n 100
```

Compare only agents already running with their installation signatures; do not start agents
for the inventory. Inspect a matched root with `why <PID> --json` to check session versus
shared-host mode. `unknown` also lists already classified hosts, tool sets, and commands
whose session identity is unresolved; it is not limited to kind `unknown`. Benchmark
`status --json` with alternating before/after release binaries on the same machine, because
helper-command timing and process churn can dominate a single run.

## Add your agent

`procwho config --example` prints the supported configuration shape. Add a narrowly scoped
rule to `extra_agents` in `~/.config/procwho/config.json` (or `PROCWHO_CONFIG`). This illustrative
rule uses the same fields as built-ins; change the package path and ID contract to the ones
verified for your installation:

```json
{
  "extra_agents": [
    {
      "name": "Local Coding Agent",
      "argv_patterns": ["/@acme/coding-agent/bin/cli\\.js$"],
      "host_commands": [["serve"]],
      "session_flags": ["--session-id"],
      "session_id_pattern": "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"
    }
  ]
}
```

Executable names, executable patterns, and launcher patterns are alternative recognition
seeds. Do not add `"executables": ["node"]` beside a narrow package pattern: the name would
also match unrelated Node processes. `required_argv` and `ancestor_patterns` add mandatory
guards. `role: "host"` declares a shared service; `role: "toolset"` names a verified helper
with an appropriate ancestor guard; `require_agent_descendant` guards editor
hosts. `host_commands` parses command prefixes; `host_flags` accepts alternative sets of
required options (for example `[["--mode=rpc"]]`). `value_flags` declares options that consume
values. `headless_flags` / `headless_commands` control the display mode. Exclusion fields
reject known helpers, management commands, or remote clients. Unknown options are parsed
conservatively. Patterns are Rust regular expressions, compiled outside process matching.
The legacy `session_identity: "claude_codex"` marker is reserved for built-ins; custom rules
can use the explicit validated selectors shown above.

Use `procwho why <PID> --json` to check evidence. Do not post environment values, private
arguments, or transcript content. Report a missing signature with the
[agent not detected issue template](https://github.com/buddyh/procwho/issues/new?template=agent_not_detected.md),
including a sanitized executable/package path, relevant mode flags, OS/architecture, and
which similar non-agent process must remain excluded.

## Worktrees found automatically

Process recognition and worktree storage are separate capabilities. Every recognized agent
can seed a repository from its cwd, including associated workloads; remembered repositories
remain discoverable later. The container table in [Worktrees](WORKTREES.md#worktrees-found-automatically)
contains sources and evidence limits.

| Agent / host | Automatic worktree location | Evidence |
|---|---|---|
| T3 Code | `~/.t3/worktrees`; temporary folders via explicit roots or live sessions | Observed |
| Codex app | `~/.codex/worktrees` | Observed |
| Cursor | `~/.cursor/worktrees` | Observed |
| Claude Code / Desktop | `<repo>/.claude/worktrees`; legacy `~/.claude/worktrees` | Documented / prior observation |
| Conductor | `~/conductor/workspaces` | Documented |
| Windsurf / Cascade | `~/.windsurf/worktrees` | Documented |
| Factory / Droid | `~/.factory/worktrees` | Documented desktop default |
| OpenCode | `~/.local/share/opencode/worktree` | Official source |
| Gemini CLI | `<repo>/.gemini/worktrees` | Documented |
| Kilo Code | `<repo>/.kilo/worktrees` | Official source |
| Vibe Kanban | `$TMPDIR/vibe-kanban/worktrees`, debug variant | Official source |
| Amp / Zed | Project-relative worktrees via root/live Git enumeration | Documented; no global container |
| Antigravity / Grok / Kiro / pi / Crush | Live/remembered repos or explicit roots/containers | No verified fixed container added |

Other custom worktree paths are found by `git worktree list` after their repository is
discovered. Remote cloud-agent worktrees require an inventory on that machine.
