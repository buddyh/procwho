import Foundation

// Shapes of `procwho status --json` (and the parts of `top --json` / `findings --json` the
// fallback composes from). Every field the app does not strictly need is optional, and
// unknown keys are ignored, so newer CLI versions keep working.

public struct StatusDoc: Decodable, Sendable {
    public var observedAtUnix: Double? = nil
    public var schemaVersion: Int?
    public var generatedUnix: Double?
    public var collectMs: Double?
    public var cpuIntervalS: Double?
    public var system: SystemInfo?
    /// Optional snapshot input metadata; otherwise use the supplied worktree inventory.
    public var worktreeRoots: [String]? = nil
    public var totals: Totals?
    @LossyList public var byKind: [KindSummary]
    @LossyList public var workloads: [Workload]
    @LossyList public var findings: [Finding]
    public var sources: [String: SourceState]?

    public init(
        schemaVersion: Int? = nil, generatedUnix: Double? = nil, collectMs: Double? = nil,
        cpuIntervalS: Double? = nil, system: SystemInfo? = nil, totals: Totals? = nil,
        byKind: [KindSummary] = [], workloads: [Workload] = [], findings: [Finding] = [],
        sources: [String: SourceState]? = nil
    ) {
        self.schemaVersion = schemaVersion
        self.generatedUnix = generatedUnix
        self.collectMs = collectMs
        self.cpuIntervalS = cpuIntervalS
        self.system = system
        self.totals = totals
        self.byKind = byKind
        self.workloads = workloads
        self.findings = findings
        self.sources = sources
    }
}

public struct SystemInfo: Decodable, Sendable {
    public var ncpu: Int?
    public var memTotalBytes: Int64?
    public var memoryPressure: String?
    public var memAvailableBytes: Int64?
    public var memCompressedBytes: Int64?
    public var memWiredBytes: Int64?
    public var memUsedBytes: Int64?
    public var swapUsedBytes: Int64?
    public var swapTotalBytes: Int64?
    public var swapDiskFreeBytes: Int64?
    public var swapDiskTotalBytes: Int64?
    public var load1: Double?
    public var diskFreeBytes: Int64?
    public var diskTotalBytes: Int64?

    public init(ncpu: Int? = nil, memTotalBytes: Int64? = nil, memUsedBytes: Int64? = nil, swapUsedBytes: Int64? = nil,
                swapTotalBytes: Int64? = nil, load1: Double? = nil, diskFreeBytes: Int64? = nil,
                diskTotalBytes: Int64? = nil, swapDiskFreeBytes: Int64? = nil, swapDiskTotalBytes: Int64? = nil) {
        self.ncpu = ncpu
        self.memTotalBytes = memTotalBytes
        self.memUsedBytes = memUsedBytes
        self.swapUsedBytes = swapUsedBytes
        self.swapTotalBytes = swapTotalBytes
        self.swapDiskFreeBytes = swapDiskFreeBytes
        self.swapDiskTotalBytes = swapDiskTotalBytes
        self.load1 = load1
        self.diskFreeBytes = diskFreeBytes
        self.diskTotalBytes = diskTotalBytes
    }
}

public struct Totals: Decodable, Sendable {
    public var accounting: String? = nil
    public var processes: Int?
    public var workloads: Int?
    public var footprintBytes: Int64?
    public var agentBytes: Int64?
    public var agentAssociatedBytes: Int64?
    public var agentAssociatedMediumBytes: Int64?
    public var unreadableProcesses: Int?

    public init(processes: Int? = nil, workloads: Int? = nil, footprintBytes: Int64? = nil,
                unreadableProcesses: Int? = nil) {
        self.processes = processes
        self.workloads = workloads
        self.footprintBytes = footprintBytes
        self.unreadableProcesses = unreadableProcesses
    }
}

public struct KindSummary: Decodable, Sendable, Equatable {
    public var accounting: String? = nil
    public var kind: String
    public var workloads: Int?
    public var processes: Int?
    public var footprintBytes: Int64?
    public var cpuPct: Double?

    public init(kind: String, workloads: Int?, processes: Int?, footprintBytes: Int64?, cpuPct: Double?) {
        self.kind = kind
        self.workloads = workloads
        self.processes = processes
        self.footprintBytes = footprintBytes
        self.cpuPct = cpuPct
    }
}

public struct AgentLink: Decodable, Sendable, Equatable {
    public var relation: String
    public var via: String
    public var confidence: String
    public var agentProduct: String?

    public init(relation: String, via: String, confidence: String, agentProduct: String? = nil) {
        self.relation = relation; self.via = via; self.confidence = confidence; self.agentProduct = agentProduct
    }
}

/// One workload. Union of the `status` row and the `top`/`why` workload view.
public struct Workload: Decodable, Sendable, Identifiable, Equatable {
    public var members: [ProcessMember]? = nil
    public var bundlePath: String? = nil
    public var agentLink: AgentLink? = nil
    public var accounting: String? = nil
    public var cpuSampleState: String? = nil
    public var cpuIntervalS: Double? = nil
    public var key: String
    public var name: String
    public var kind: String
    public var confidence: String?
    public var processes: Int?
    public var footprintBytes: Int64?
    public var rssBytes: Int64?
    public var cpuPct: Double?
    public var ageSeconds: Double?
    @LossyList public var ports: [String]
    public var inside: String?
    public var sessionId: String?
    public var projectPath: String?
    public var rootPid: Int?
    // Present in `top --json` / `why --json`.
    @LossyList public var evidence: [String]
    @LossyList public var identity: [String]
    @LossyList public var ifStopped: [String]
    public var zombies: Int?

    public var id: String { key }

    /// True for kinds the menu bar total counts as "agent" memory.
    public var isAgent: Bool { kind.hasPrefix("agent-") }

    /// Anything except exact/high/n/a needs an ownership caution, including future levels.
    public var weakConfidence: Bool {
        switch confidence?.lowercased() {
        case "exact", "high", "n/a": return false
        default: return true
        }
    }

    public init(key: String, name: String, kind: String, confidence: String? = nil,
                processes: Int? = nil, footprintBytes: Int64? = nil, cpuPct: Double? = nil) {
        self.key = key
        self.name = name
        self.kind = kind
        self.confidence = confidence
        self.processes = processes
        self.footprintBytes = footprintBytes
        self.cpuPct = cpuPct
    }
}

public struct Finding: Decodable, Sendable, Identifiable, Equatable {
    public var severity: String
    public var title: String
    public var workloadKey: String?
    @LossyList public var reasons: [String]
    @LossyList public var notProven: [String]
    public var basis: String?

    public var id: String { "\(severity)|\(title)|\(workloadKey ?? "")" }
    public var isReview: Bool { severity.lowercased() == "review" }

    enum CodingKeys: String, CodingKey {
        case severity, title, workloadKey, workload, reasons, notProven, basis
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        severity = (try? c.decodeIfPresent(String.self, forKey: .severity)) ?? "info"
        title = try c.decode(String.self, forKey: .title)
        // `status` uses workload_key; `findings --json` uses workload.
        workloadKey = (try? c.decodeIfPresent(String.self, forKey: .workloadKey))
            ?? (try? c.decodeIfPresent(String.self, forKey: .workload))
        _reasons = try c.decode(LossyList<String>.self, forKey: .reasons)
        _notProven = try c.decode(LossyList<String>.self, forKey: .notProven)
        basis = try? c.decodeIfPresent(String.self, forKey: .basis)
    }
}

/// `procwho top --json` (used only by the fallback when `status` is not available).
public struct TopDoc: Decodable, Sendable {
    public var processes: Int?
    public var workloadsTotal: Int?
    public var footprintSumBytes: Int64?
    public var notInspectableProcesses: Int?
    public var cpuIntervalS: Double?
    public var collectMs: Double?
    public var sources: [String: SourceState]?
    @LossyList public var workloads: [Workload]
}

extension StatusDoc {
    /// Builds a status document from `top --json` and `findings --json` when the binary has no
    /// `status` command. `system` is limited to what the app can know without reading process
    /// tables (core count, physical memory).
    public static func compose(top: TopDoc, findings: [Finding], now: Date = Date(),
                               ncpu: Int = ProcessInfo.processInfo.activeProcessorCount,
                               memTotal: UInt64 = ProcessInfo.processInfo.physicalMemory) -> StatusDoc {
        var groups: [String: KindSummary] = [:]
        for w in top.workloads {
            var g = groups[w.kind] ?? KindSummary(kind: w.kind, workloads: 0, processes: 0, footprintBytes: 0, cpuPct: 0)
            g.workloads = (g.workloads ?? 0) + 1
            g.processes = (g.processes ?? 0) + (w.processes ?? 0)
            g.footprintBytes = (g.footprintBytes ?? 0) + (w.footprintBytes ?? 0)
            g.cpuPct = (g.cpuPct ?? 0) + (w.cpuPct ?? 0)
            groups[w.kind] = g
        }
        return StatusDoc(
            schemaVersion: nil,
            generatedUnix: now.timeIntervalSince1970,
            collectMs: top.collectMs,
            cpuIntervalS: top.cpuIntervalS,
            system: SystemInfo(ncpu: ncpu, memTotalBytes: Int64(clamping: memTotal)),
            totals: Totals(processes: top.processes, workloads: top.workloadsTotal,
                           footprintBytes: top.footprintSumBytes,
                           unreadableProcesses: top.notInspectableProcesses),
            byKind: groups.values.sorted { ($0.footprintBytes ?? 0) > ($1.footprintBytes ?? 0) },
            workloads: top.workloads,
            findings: findings,
            sources: top.sources
        )
    }

    /// Total footprint of agent workloads (kinds starting with `agent-`).
    public var agentFootprintBytes: Int64 {
        if let bytes = totals?.agentBytes { return bytes }
        if !byKind.isEmpty {
            return byKind.filter { $0.kind.hasPrefix("agent-") }.reduce(0) { $0 + ($1.footprintBytes ?? 0) }
        }
        return workloads.filter(\.isAgent).reduce(0) { $0 + ($1.footprintBytes ?? 0) }
    }

    public var reviewFindingCount: Int { findings.filter(\.isReview).count }

    /// Sources that reported `false`.
    public var failedSources: [String] {
        (sources ?? [:]).compactMap { k, v in
            v == .ok(false) && !sourceNotes.contains("\(Self.sourceName(k)): optional tool not installed") ? Self.sourceName(k) : nil
        }.sorted()
    }

    /// Sources that reported a descriptive string.
    public var sourceNotes: [String] {
        (sources ?? [:]).flatMap { _, v -> [String] in
            switch v { case .note(let s): [s]; case .notes(let notes): notes; default: [] }
        }.sorted()
    }
}

/// Timestamped findings document (schema v2).
public struct FindingsDoc: Decodable, Sendable {
    public var observedAtUnix: Double?
    public var collectMs: Double?
    @LossyList public var findings: [Finding]
}
