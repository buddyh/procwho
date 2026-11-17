# What the labels mean

Procwho only reads. Labels describe evidence; they do not tell you to stop a process or remove a worktree.

## Workloads

### Agents + associated (`agents-associated`)

Exclusive footprints of agent workloads, known agent host apps and processes with launching evidence. Medium-confidence links are marked with ~; each process counts once even inside a host app.

### Linked to agents (`linked-to-agents`)

A non-agent workload linked by a recorded original parent or macOS responsible process, or a known agent host app. The tooltip names the evidence. Simulators and launchd services are excluded; launching context does not identify a task.

### Agents (`family:agents`)

AI coding sessions and their supporting processes. This groups memory by owner; it does not show how useful a session is.

### Agent session (`kind:agent-session`)

One identified AI coding session and its child processes. Read the evidence and confidence before attributing it to a particular task.

### Tool set (`kind:agent-toolset`)

Supporting tools grouped with an agent, without enough evidence to name one session. They can belong to several sessions.

### Shared agent host (`kind:agent-host`)

A process that serves several agent sessions. Its memory cannot reliably be assigned to one thread.

### Agent command (`kind:agent-command`)

A command recognized as agent-related. This does not prove which session started it.

### Leftovers (`family:leftovers`)

Processes that appear detached from their original owner. They may still be needed; this is not proof they are safe to stop.

### Detached (`kind:detached`)

A process that appears separated from its original owner. It may still be doing useful work.

### Unknown owner (`family:unknown`)

Procwho could not identify the owner. This says nothing about whether the process is needed.

### Unknown owner (`kind:unknown`)

Procwho could not identify the owner. This says nothing about whether the process is needed. Inspect the command and parent processes before drawing a conclusion.

### Services (`family:services`)

Background jobs, watchers and custom groups. They can support apps or system features even without an open window.

### Service (`kind:service`)

A background job identified through macOS launchd. Its evidence names the service registration.

### Watcher (`kind:watcher`)

A background tool that watches files for changes. It can be needed by an active development task.

### Declared owner (`kind:declared`)

An owner supplied by local configuration. It is a declaration, not independent proof of ownership.

### Apps (`family:apps`)

Applications and terminal sessions with their supporting processes. A running app may have no visible window.

### App (`kind:app`)

A macOS application and processes assigned to it. Read the evidence to see how they were grouped.

### Terminal (`kind:terminal`)

A terminal session and the commands assigned to it. Closing a window is not proof all its commands have finished.

### Terminal pane (`kind:terminal-pane`)

A pane identified in a terminal multiplexer such as tmux. It groups commands by the pane that owns them.

### System (`family:system`)

macOS processes and processes that cannot be inspected with current permissions. Missing information is not zero use.

### System process (`kind:system`)

A process attributed to macOS. Its role can affect the whole machine.

### System (not inspectable) (`kind:system-opaque`)

macOS does not allow this process to be fully inspected. Its owner or measurements may be unavailable.

### Simulators (`family:simulators`)

Virtual Apple devices and their supporting processes. A simulator can keep running when its window is closed.

### Simulator (`kind:simulator`)

A virtual device used for app development. Its processes and memory are grouped together.

## Measurements

### Memory (`memory`)

The process footprint, the Memory figure used by Activity Monitor. Sum of process footprints; shared memory can be counted more than once.

### Not in RAM (`not-in-ram`)

Estimate from footprint minus resident memory, floored at zero. It can include compressed or swapped memory; macOS does not report swap per process.

### ~mostly not in RAM (`mostly-not-in-ram`)

An estimate: most of this workload’s footprint is not resident in RAM and exceeds the configured minimum. This badge does not prove how much swap the workload uses.

### CPU (`cpu`)

Percent of one core, measured over the sample interval. A workload using several cores can exceed 100%.

### Memory pressure (`memory-pressure`)

The macOS kernel reports normal, warning or critical pressure. It reflects how well memory demand can be served, not the percent of installed RAM used. The gauge is categorical; Load is shown only when pressure cannot be read.

### Load (`load`)

Average number of tasks running or waiting to run over one minute. Compare it with the Mac’s logical core count; it is not CPU percent.

### Swap (`swap`)

System-wide disk space used to hold memory pages outside RAM. macOS allocates more swap files as needed; allocated so far is not a limit. The meter compares used swap with installed RAM. Colour reflects memory pressure, swap above 50% or 100% of RAM, and less than 10% free space on the swap volume. Swap cannot be assigned to individual workloads by this app.

### Reclaimable (`reclaimable`)

Up to the measured disk use of merged, idle worktrees. APFS clones share blocks, so removal can free less; unmeasured sizes are excluded.

### PID (`pid`)

The numeric identifier of a running process. macOS can reuse it after that process exits.

### Working folder (`cwd`)

The current working directory (cwd) of a process. It is evidence of use, not proof that all work is confined to that folder.

### Resident memory (`rss`)

Memory currently resident in RAM (RSS). Subtracting it from footprint gives only an estimate of memory outside RAM.

### APFS clones (`apfs`)

Copies that share disk blocks on Apple’s file system. Adding their measured sizes can overstate the space that removal would free.

### Port (`port`)

A numbered network endpoint a process listens on. A port number alone does not establish ownership.

### Age (`age`)

Time since the oldest process in the workload started. It does not establish when a task last made useful progress.

### Processes (`processes`)

Running programs assigned to this workload. A workload can contain several processes.

### Worktree (`worktree`)

A separate working folder linked to a Git repository. It can have its own branch and files while sharing repository history.

### Branch (`branch`)

A named line of Git history. Its name does not establish whether all its work has been saved elsewhere.

### Detached HEAD (`detached-head`)

A Git checkout at a specific commit without a branch name. This is unrelated to a detached process.

### Idle (`idle`)

Time since observed file or Git activity in a worktree. A quiet folder can still contain valuable work.

### Upstream gone (`upstream-gone`)

The remote branch tracked by this worktree is no longer in local Git records. The folder can still contain useful work; records can be as old as the last fetch.

### Locked (`locked`)

Git marks this worktree as locked against removal or pruning. Read the reason before taking any action outside Procwho.

### CLI (`cli`)

The local command-line program bundled with Procwho. The app reads its JSON reports to populate this panel.

### Pull request (`pr`)

A proposed set of changes on GitHub (PR). A merged pull request counts as merge evidence only when its head matches the worktree’s commit.

### Snapshot (`snapshot`)

Evidence from one process sample. It cannot prove a trend or that a process is making useful progress.

### History (`history`)

Evidence from observations already recorded locally. Repeated measurements can show a trend, but do not establish whether work is useful.

### Trend (`trend`)

A change across several stored samples. It supports a statement about resource use over time, not whether a process is doing useful work.

### GB (`gb`)

A gigabyte is 1,024 MB here, matching Activity Monitor’s binary units. Installed RAM is capacity, not current use.

### MB (`mb`)

A megabyte is 1,048,576 bytes here. Values below 1 MB remain marked as less than 1 MB rather than zero.

### Memory used (`physical-memory`)

Physical RAM used by apps, wired memory and compressed memory, excluding purgeable and file-backed caches. The hero and menu bar use macOS VM counters, rather than summing process footprints.

### Percent (`%`)

A share out of 100. CPU uses one core as 100%. The swap meter uses installed RAM as context, not allocated swap as a limit.

### Minutes (`min`)

A duration in minutes. Alert cooldown starts when a notification fires; the condition must also clear before it can fire again.

## Identity confidence

### Exact (`confidence:exact`)

Direct identifying evidence ties this process to its owner. It establishes attribution, not whether stopping it is safe.

### High (`confidence:high`)

Strong evidence supports this owner. Check the impact and current work before any action outside Procwho.

### Medium (`confidence:medium`)

Indirect evidence suggests this owner, but another owner is possible. Check before you stop anything.

### Low (`confidence:low`)

Only weak clues suggest this owner. Do not use this label alone to decide what to stop.

### None (`confidence:none`)

There is not enough evidence to identify an owner. Inspect the process and its parents.

### Not applicable (`confidence:n/a`)

Ownership confidence is not assigned to this group. This does not establish that stopping it is safe.

### Declared (`confidence:declared`)

The owner comes from local configuration. Verify the declaration against the process before acting.

## Worktree verdicts

### Merged, idle (`verdict:merged-idle`)

The branch’s changes are in the default branch, no unsaved work was found, and no observed process uses the folder. Merge facts can be as old as the last fetch; this is not a deletion instruction.

### Merged, locked (`verdict:merged-locked`)

The worktree meets the merged, idle checks but Git has locked it. The lock can reflect a reason to keep it; read its evidence.

### Has unsaved work (`verdict:has-work`)

There are changed files, untracked files, or commits not on a remote whose changes are not known to be merged. Keep and review this work.

### In use now (`verdict:in-use`)

An observed live process has its working folder here. That is evidence of current use, not proof of all activity in the folder.

### Stale (`verdict:stale`)

The worktree is clean, not merged, not observed in use, and older than the configured activity limit. Age alone does not mean it is disposable.

### Active (`verdict:active`)

The worktree has recent activity and is not known to be merged. This label does not require a live process.

### Folder missing (`verdict:missing`)

Git records a worktree whose folder no longer exists. Procwho does not change or remove that record.

### Orphaned folder (`verdict:orphaned-dir`)

A worktree folder has missing or inconsistent Git registration. It can still contain useful files or be in use.

### Unknown (`verdict:unknown`)

A fact needed to decide the verdict could not be read. Unknown is not evidence that the worktree is unused or safe to remove.
