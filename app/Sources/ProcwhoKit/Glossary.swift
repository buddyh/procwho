import Foundation

/// Short definitions shared by the app sheet and tooltips.
public enum Glossary {
    public struct Entry: Identifiable, Sendable {
        public let id: String
        public let section: String
        public let title: String
        public let text: String
    }

    public static let entries: [Entry] = [
        Entry(id: "agents-associated", section: "Workloads", title: "Agents + associated", text: "Agent workloads, known agent host apps, and processes linked by launching evidence, counted once per process."),
        Entry(id: "linked-to-agents", section: "Workloads", title: "Linked to agents", text: "A workload linked to agents by its original parent, responsible process, or known host app."),
        Entry(id: "family:agents", section: "Workloads", title: "Agents", text: "AI coding sessions and their supporting processes."),
        Entry(id: "kind:agent-session", section: "Workloads", title: "Agent session", text: "One identified AI coding session and its child processes."),
        Entry(id: "kind:agent-toolset", section: "Workloads", title: "Tool set", text: "Supporting tools grouped with an agent without enough evidence to name one session."),
        Entry(id: "kind:agent-host", section: "Workloads", title: "Shared agent host", text: "A process that serves several agent sessions."),
        Entry(id: "kind:agent-command", section: "Workloads", title: "Agent command", text: "A command recognized as related to an agent."),
        Entry(id: "family:leftovers", section: "Workloads", title: "Leftovers", text: "Processes that appear detached from their original owner."),
        Entry(id: "kind:detached", section: "Workloads", title: "Detached", text: "A process that appears separated from its original owner."),
        Entry(id: "family:unknown", section: "Workloads", title: "Unknown owner", text: "Processes whose owner could not be identified."),
        Entry(id: "kind:unknown", section: "Workloads", title: "Unknown owner", text: "A workload whose owner could not be identified."),
        Entry(id: "family:services", section: "Workloads", title: "Services", text: "Background jobs, file watchers, and custom groups."),
        Entry(id: "kind:service", section: "Workloads", title: "Service", text: "A background job identified through macOS launchd."),
        Entry(id: "kind:watcher", section: "Workloads", title: "Watcher", text: "A background tool that watches files for changes."),
        Entry(id: "kind:declared", section: "Workloads", title: "Declared owner", text: "An owner supplied by local configuration."),
        Entry(id: "family:apps", section: "Workloads", title: "Apps", text: "Applications and terminal sessions with their supporting processes."),
        Entry(id: "kind:app", section: "Workloads", title: "App", text: "A macOS application and processes assigned to it."),
        Entry(id: "kind:terminal", section: "Workloads", title: "Terminal", text: "A terminal session and the commands assigned to it."),
        Entry(id: "kind:terminal-pane", section: "Workloads", title: "Terminal pane", text: "A pane in a terminal multiplexer and its assigned commands."),
        Entry(id: "family:system", section: "Workloads", title: "System", text: "macOS processes and processes restricted by current permissions."),
        Entry(id: "kind:system", section: "Workloads", title: "System process", text: "A process attributed to macOS."),
        Entry(id: "kind:system-opaque", section: "Workloads", title: "Restricted system process", text: "A system process that macOS does not allow Procwho to inspect fully."),
        Entry(id: "family:simulators", section: "Workloads", title: "Simulators", text: "Virtual Apple devices and their supporting processes."),
        Entry(id: "kind:simulator", section: "Workloads", title: "Simulator", text: "A virtual device used for app development."),
        Entry(id: "memory", section: "Measurements", title: "Memory", text: "Process footprint, as shown in Activity Monitor’s Memory column."),
        Entry(id: "not-in-ram", section: "Measurements", title: "Not in RAM", text: "Estimated compressed and swapped memory, calculated as footprint minus resident memory, floored at zero."),
        Entry(id: "mostly-not-in-ram", section: "Measurements", title: "Mostly not in RAM", text: "Most of this workload’s footprint is estimated to be outside RAM and exceeds the configured minimum."),
        Entry(id: "cpu", section: "Measurements", title: "CPU", text: "Percent of one core over the sample interval, which can exceed 100% across several cores."),
        Entry(id: "memory-pressure", section: "Measurements", title: "Memory pressure", text: "The kernel’s assessment of how well macOS can meet memory demand."),
        Entry(id: "load", section: "Measurements", title: "Load", text: "Average number of tasks running or waiting to run over one minute."),
        Entry(id: "swap", section: "Measurements", title: "Swap", text: "System-wide disk space used to hold memory pages outside RAM."),
        Entry(id: "reclaimable", section: "Measurements", title: "Reclaimable", text: "Measured disk use of merged, idle worktrees, excluding unmeasured sizes."),
        Entry(id: "pid", section: "Measurements", title: "PID", text: "The numeric identifier of a running process."),
        Entry(id: "cwd", section: "Measurements", title: "Working folder", text: "The current working folder of a process."),
        Entry(id: "rss", section: "Measurements", title: "Resident memory", text: "Memory currently resident in RAM."),
        Entry(id: "apfs", section: "Measurements", title: "APFS clones", text: "APFS clones share disk blocks, so removing a worktree can free less than its measured size."),
        Entry(id: "port", section: "Measurements", title: "Port", text: "A numbered network endpoint on which a process listens."),
        Entry(id: "age", section: "Measurements", title: "Age", text: "Time since the oldest process in the workload started."),
        Entry(id: "processes", section: "Measurements", title: "Processes", text: "Running programs assigned to this workload."),
        Entry(id: "confidence:exact", section: "Identity confidence", title: "Exact", text: "Direct identifying evidence ties this process to its owner."),
        Entry(id: "confidence:high", section: "Identity confidence", title: "High", text: "Strong evidence supports this owner."),
        Entry(id: "confidence:medium", section: "Identity confidence", title: "Medium", text: "Indirect evidence suggests this owner."),
        Entry(id: "confidence:low", section: "Identity confidence", title: "Low", text: "Only weak clues suggest this owner."),
        Entry(id: "confidence:none", section: "Identity confidence", title: "None", text: "There is not enough evidence to identify an owner."),
        Entry(id: "confidence:n/a", section: "Identity confidence", title: "Not applicable", text: "Ownership confidence is not assigned to this group."),
        Entry(id: "confidence:declared", section: "Identity confidence", title: "Declared", text: "The owner comes from local configuration."),
        Entry(id: "verdict:merged-idle", section: "Worktree verdicts", title: "Merged, idle", text: "Branch changes are merged, no unsaved work was found, and no observed process uses the folder."),
        Entry(id: "verdict:merged-locked", section: "Worktree verdicts", title: "Merged, locked", text: "The worktree meets the merged, idle checks but Git has locked it."),
        Entry(id: "verdict:has-work", section: "Worktree verdicts", title: "Has unsaved work", text: "Changed files, untracked files, or unmerged commits without a remote copy were found."),
        Entry(id: "verdict:in-use", section: "Worktree verdicts", title: "In use now", text: "An observed live process has its working folder here."),
        Entry(id: "verdict:stale", section: "Worktree verdicts", title: "Stale", text: "The worktree is clean, unmerged, not observed in use, and older than the configured activity limit."),
        Entry(id: "verdict:active", section: "Worktree verdicts", title: "Active", text: "The worktree has recent activity and is not known to be merged."),
        Entry(id: "verdict:missing", section: "Worktree verdicts", title: "Folder missing", text: "Git records a worktree whose folder no longer exists."),
        Entry(id: "verdict:orphaned-dir", section: "Worktree verdicts", title: "Orphaned folder", text: "A worktree folder has missing or inconsistent Git registration."),
        Entry(id: "verdict:unknown", section: "Worktree verdicts", title: "Unknown", text: "A fact needed to decide the verdict could not be read."),
        Entry(id: "worktree", section: "Measurements", title: "Worktree", text: "A separate working folder linked to a Git repository."),
        Entry(id: "branch", section: "Measurements", title: "Branch", text: "A named line of Git history."),
        Entry(id: "detached-head", section: "Measurements", title: "Detached HEAD", text: "A Git checkout at a specific commit without a branch name."),
        Entry(id: "idle", section: "Measurements", title: "Idle", text: "Time since observed file or Git activity in a worktree."),
        Entry(id: "upstream-gone", section: "Measurements", title: "Upstream gone", text: "The remote branch tracked by this worktree is no longer in local Git records."),
        Entry(id: "locked", section: "Measurements", title: "Locked", text: "Git marks this worktree as locked against removal or pruning."),
        Entry(id: "cli", section: "Measurements", title: "CLI", text: "The local command-line program that supplies the app’s reports."),
        Entry(id: "pr", section: "Measurements", title: "Pull request", text: "A proposed set of changes on GitHub."),
        Entry(id: "snapshot", section: "Measurements", title: "Snapshot", text: "Evidence from one process sample."),
        Entry(id: "history", section: "Measurements", title: "History", text: "Observations recorded locally over time."),
        Entry(id: "trend", section: "Measurements", title: "Trend", text: "A change across several stored samples."),
        Entry(id: "gb", section: "Measurements", title: "GB", text: "A gigabyte is 1,024 MB, matching Activity Monitor’s binary units."),
        Entry(id: "mb", section: "Measurements", title: "MB", text: "A megabyte is 1,048,576 bytes."),
        Entry(id: "physical-memory", section: "Measurements", title: "Memory used", text: "Physical RAM used by apps, wired memory, and compressed memory, excluding purgeable and file-backed caches."),
        Entry(id: "%", section: "Measurements", title: "Percent", text: "A share out of 100."),
        Entry(id: "min", section: "Measurements", title: "Minutes", text: "A duration in minutes."),
    ]

    public static let footprintSumHelp = "Sum of process footprints, not physical RAM use; shared memory can be counted twice."
    public static let sections = ["Workloads", "Measurements", "Identity confidence", "Worktree verdicts"]
    public static let verdicts = ["merged-idle", "merged-locked", "has-work", "in-use", "stale", "active", "missing", "orphaned-dir", "unknown"]
    public static let confidenceLevels = ["exact", "high", "medium", "low", "none", "n/a", "declared"]

    public static func entry(_ id: String) -> Entry? { entries.first { $0.id == id } }
    public static func help(_ id: String) -> String {
        entry(id)?.text ?? "A label supplied by the CLI."
    }
    public static func kindHelp(_ kind: String) -> String {
        entry("kind:" + kind)?.text ?? help("family:services")
    }
    public static func verdictTitle(_ verdict: String) -> String { entry("verdict:" + verdict)?.title ?? verdict }
    public static func verdictHelp(_ verdict: String) -> String { verdict + ": " + help("verdict:" + verdict) }

    /// Resolve display labels as well as raw technical names for common view components.
    public static func labelHelp(_ label: String) -> String {
        let key = label.lowercased()
        if let kind = (KindOrder.order + ["terminal", "declared"]).first(where: { KindOrder.title($0).lowercased() == key }) { return kindHelp(kind) }
        if key.hasPrefix("identity: ") { return "Identity confidence: " + String(label.dropFirst(10)) + ". " + help("confidence:" + key.dropFirst(10)) }
        if key.hasPrefix("snapshot") { return help("snapshot") }
        if key.hasPrefix("trend") { return help("trend") }
        if key == "follow the panel" { return "Use the memory view selected in Overview, including its no-agent fallback." }
        if key == "agent memory" { return help("family:agents") + " " + footprintSumHelp }
        if key == "swap used" { return help("swap") }
        if key == "ports" { return help("port") }
        if key.contains("not in ram") { return help("not-in-ram") }
        if let entry = entries.first(where: { $0.title.lowercased() == key || $0.id == key || $0.id == "kind:" + key || $0.id == "confidence:" + key }) { return entry.text }
        return help(key)
    }

    public static var markdown: String {
        var text = "# What the labels mean\n\nLabels describe evidence.\n"
        for section in sections {
            text += "\n## " + section + "\n"
            for entry in entries where entry.section == section {
                text += "\n### " + entry.title + " (`" + entry.id + "`)\n\n" + entry.text + "\n"
            }
        }
        return text
    }
}
