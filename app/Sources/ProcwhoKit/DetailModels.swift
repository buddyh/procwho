import Foundation

/// `procwho why <pid|key> --json`.
public struct WhyDoc: Decodable, Sendable {
    public var schemaVersion: Int?
    public var observedAtUnix: Double?
    public var collectMs: Double?
    public var workload: Workload
    public var suggestedRule: NamingRule?
    public var declaredOwner: String?
    public var declaredNotes: String?
    public var totalsInclusive: InclusiveTotals?
    public var history: WhyHistory?
    public var process: ProcessDetail?
    @LossyList public var reveal: [RevealAction]
    public var readOnly: String?

    public var inclusive: (children: Int64, footprintBytes: Int64, processes: Int64)? {
        guard let t = totalsInclusive else { return nil }
        return (t.nestedWorkloads, t.footprintBytes, t.processes)
    }
}

public struct InclusiveTotals: Decodable, Sendable {
    public var accounting: String
    public var nestedWorkloads: Int64
    public var footprintBytes: Int64
    public var processes: Int64
}

public struct WhyHistory: Decodable, Sendable {
    public var samples: Int?
    public var spanMin: Double?
    public var footprintFirstBytes: Int64?
    public var footprintLastBytes: Int64?
    public var avgCpuPct: Double?
}

public struct ProcessDetail: Decodable, Sendable {
    public var pid: Int
    public var instanceId: Int64?
    public var started: String?
    public var ppid: Int?
    public var command: String?
    public var cwd: String?
    public var footprintBytes: Int64?
    public var exe: String?
    public var tty: String?
    public var cpuTimeS: Double?
    @LossyList public var ancestry: [Ancestor]
    public var originalParent: String?
    public var descendants: Int?
}

public struct Ancestor: Decodable, Sendable, Equatable {
    public var pid: Int
    public var command: String?
}

/// A reveal suggestion from the CLI. The app never executes `argv`; it shows the command as
/// copyable text and, when the action is a plain `open <path>`, offers Reveal in Finder for
/// that path via NSWorkspace.
public struct RevealAction: Decodable, Sendable, Identifiable, Equatable {
    public var label: String
    @LossyList public var argv: [String]
    public var command: String?
    public var note: String?

    public var id: String { label + "|" + (command ?? argv.joined(separator: " ")) }

    public var displayCommand: String { command ?? argv.joined(separator: " ") }

    /// The filesystem path when this is `open <abs path>` or `open -R <abs path>`.
    public var openablePath: String? {
        guard argv.first == "open", let last = argv.last, last.hasPrefix("/") else { return nil }
        let middle = argv.dropFirst().dropLast()
        guard middle.allSatisfy({ $0 == "-R" }) else { return nil }
        return last
    }
}

/// `procwho worktrees --json` (schema_version 2).
public struct WorktreesDoc: Decodable, Sendable {
    public var schemaVersion: Int?
    public var observedAtUnix: Double?
    public var collectMs: Double?
    public var generatedAt: Double?
    public var scanSecs: Double?
    public var reposFound: Int?
    public var worktreesFound: Int?
    public var searchedFolders: [String]?
    @LossyList public var totals: [VerdictTotal]
    @LossyList public var repos: [RepoInfo]
    @LossyList public var worktrees: [Worktree]
    @LossyList public var notes: [String]

    /// "reclaimable if all merged-idle were removed" arithmetic.
    public var mergedIdle: VerdictTotal? { totals.first { $0.verdict == "merged-idle" } }
}

public struct VerdictTotal: Decodable, Sendable, Identifiable, Equatable {
    public var verdict: String
    public var count: Int?
    public var sizeBytes: Int64?
    public var rebuildableBytes: Int64?
    public var sizeUnknown: Int?
    public var id: String { verdict }
}

public struct RepoInfo: Decodable, Sendable {
    public var discoveredVia: String?
    public var name: String?
    public var path: String?
    public var defaultBranch: String?
    public var fetchedAt: Double?
    public var fetchAgeSecs: Double?
    @LossyList public var notes: [String]
}

public struct Worktree: Decodable, Sendable, Identifiable, Equatable {
    public var discoveredVia: String?
    public var path: String
    public var repo: String?
    public var branch: String?
    public var head: String?
    public var verdict: String
    @LossyList public var flags: [String]
    @LossyList public var reasons: [String]
    @LossyList public var notes: [String]
    public var sizeBytes: Int64?
    public var rebuildableBytes: Int64?
    public var sizeCached: Bool?
    public var sizeMeasuredUnix: Double?
    public var lastActivityUnix: Double?
    public var idleSecs: Double?
    public var inUse: Bool?
    public var exists: Bool?

    public var id: String { path }
}

/// Read-only CLI configuration inspection supplies exact default folders before a scan.
public struct WorktreeSearchConfiguration: Decodable, Sendable {
    public struct Values: Decodable, Sendable { public var worktreeRoots: [String] }
    public struct Scan: Decodable, Sendable {
        public struct Folder: Decodable, Sendable { public var path: String }
        public var containers: [Folder]
    }
    public var values: Values
    public var worktreeScan: Scan
    public func folders(roots: [String]) -> [String] {
        (roots.isEmpty ? values.worktreeRoots : roots) + worktreeScan.containers.map(\.path)
    }
}
