import Foundation

/// Descending rank, with absent/nonfinite measurements last and stable name/key ties.
public enum WorkloadSort: String, CaseIterable, Codable, Sendable, Identifiable {
    case memory = "Memory", notInRAM = "Not in RAM", cpu = "CPU", age = "Age"
    public var title: String { rawValue }
    public var id: String { rawValue }

    public func sorted(_ rows: [Workload]) -> [Workload] {
        rows.sorted { a, b in
            let av = value(a), bv = value(b)
            if av != bv { return av > bv }
            if a.name != b.name { return a.name < b.name }
            return a.key < b.key
        }
    }

    fileprivate func value(_ row: Workload) -> Double {
        let value: Double?
        switch self {
        case .memory: value = row.footprintBytes.map(Double.init)
        case .notInRAM: value = row.notInRamBytes.map(Double.init)
        case .cpu: value = row.cpuPct
        case .age: value = row.ageSeconds
        }
        guard let value, value.isFinite else { return -.infinity }
        return max(0, value)
    }
}

/// A presentation-independent part of a composition strip. Bytes are measured values only.
public struct CompositionSegment: Identifiable, Equatable, Sendable {
    public var id: String
    public var title: String
    public var bytes: Int64
    public var count: Int
    public var unknownCount: Int

    public init(id: String, title: String, bytes: Int64, count: Int, unknownCount: Int = 0) {
        self.id = id
        self.title = title
        self.bytes = max(0, bytes)
        self.count = max(0, count)
        self.unknownCount = max(0, unknownCount)
    }

    public var valueLabel: String {
        if count > 0, unknownCount >= count { return "Not measured" }
        return Fmt.bytes(bytes) + (unknownCount > 0 ? " + \(Fmt.count(unknownCount)) unknown" : "")
    }
}

public enum Presentation {
    /// The denominator is the sum of measured segments, never installed capacity.
    public static func stripCaption(_ segments: [CompositionSegment], disk: Bool = false) -> String {
        "Share of " + (disk ? "measured disk" : "attributed memory") + " · " + Fmt.bytes(sum(segments.map(\.bytes)))
    }

    /// A safe fraction for bar widths, including malformed or absent measurements.
    public static func share(_ bytes: Int64?, of maximum: Int64?) -> Double {
        guard let bytes, let maximum, maximum > 0 else { return 0 }
        return min(1, max(0, Double(bytes) / Double(maximum)))
    }

    /// Prefer by-kind totals: a status document can contain a limited workload sample.
    public static func composition(_ status: StatusDoc) -> [CompositionSegment] {
        KindFamily.allCases.compactMap { family in
            let groups = status.byKind.filter { KindFamily(kind: $0.kind) == family }
            let rows = status.workloads.filter { KindFamily(kind: $0.kind) == family }
            guard !groups.isEmpty || !rows.isEmpty else { return nil }
            return CompositionSegment(
                id: family.rawValue, title: family.title,
                bytes: groups.isEmpty ? sum(rows.compactMap(\.footprintBytes)) : sum(groups.compactMap(\.footprintBytes)),
                count: groups.isEmpty ? rows.count : groups.reduce(0) { $0 + max(0, $1.workloads ?? 0) },
                unknownCount: groups.isEmpty ? rows.filter { $0.footprintBytes == nil }.count
                    : groups.filter { $0.footprintBytes == nil }.reduce(0) { $0 + max(0, $1.workloads ?? 0) })
        }
    }

    /// Keep unknown sizes out of disk proportions, while preserving their count for the unmeasured caption.
    public static func composition(_ doc: WorktreesDoc) -> [CompositionSegment] {
        let order = ["merged-idle", "in-use", "has-work", "stale", "active", "missing", "unknown"]
        return doc.totals.map { total in
            CompositionSegment(id: total.verdict, title: Glossary.verdictTitle(total.verdict),
                               bytes: total.sizeBytes ?? 0, count: total.count ?? 0,
                               unknownCount: total.sizeUnknown ?? (total.sizeBytes == nil ? total.count ?? 0 : 0))
        }.sorted {
            let a = order.firstIndex(of: $0.id) ?? order.count
            let b = order.firstIndex(of: $1.id) ?? order.count
            return a == b ? $0.id < $1.id : a < b
        }
    }

    /// Saturating sum protects presentation from corrupt or very large CLI values.
    public static func sum(_ bytes: [Int64]) -> Int64 {
        bytes.reduce(0) { sum, value in
            let (next, overflow) = sum.addingReportingOverflow(max(0, value))
            return overflow ? .max : next
        }
    }

    /// Rank groups by the same metric as rows; unknown-only groups sort last.
    public static func rankedFamilies(_ rows: [Workload], sort: WorkloadSort) -> [KindFamily] {
        let grouped = Dictionary(grouping: rows) { KindFamily(kind: $0.kind) }
        func total(_ family: KindFamily) -> Double {
            let values = (grouped[family] ?? []).map { sort.value($0) }.filter(\.isFinite)
            return values.isEmpty ? -.infinity : values.reduce(0, +)
        }
        return grouped.keys.sorted { a, b in
            total(a) == total(b) ? a.rawValue < b.rawValue : total(a) > total(b)
        }
    }

    public static func hiddenSystemCount(_ status: StatusDoc, hidden: Bool) -> Int {
        guard hidden else { return 0 }
        let kinds = status.byKind.filter { KindFamily(kind: $0.kind) == .system }
        return kinds.isEmpty ? status.workloads.filter { KindFamily(kind: $0.kind) == .system }.count
            : kinds.reduce(0) { $0 + max(0, $1.workloads ?? 0) }
    }

    public static func workloads(_ rows: [Workload], search: String, family: KindFamily?,
                                 sort: WorkloadSort, hideSystem: Bool = false) -> [Workload] {
        let query = search.trimmingCharacters(in: .whitespacesAndNewlines)
        return sort.sorted(rows.filter { row in
            (!hideSystem || KindFamily(kind: row.kind) != .system)
                && (family == nil || KindFamily(kind: row.kind) == family)
                && (query.isEmpty || workloadMatches(row, query: query) || (row.members ?? []).contains { memberMatches($0, query: query) })
        })
    }
}

/// Only supported open-panel polling cadences; the closed-panel cadence stays separate.
public enum RefreshInterval: Int, CaseIterable, Codable, Sendable, Identifiable {
    case five = 5, ten = 10, thirty = 30, sixty = 60
    public var id: Int { rawValue }
}

extension Presentation {
    public static func sizeAge(_ row: Worktree, now: Double) -> String {
        guard row.sizeBytes != nil, let measured = row.sizeMeasuredUnix else { return "Measurement time unavailable" }
        return "measured \(Fmt.age(max(0, now - measured))) ago"
    }

    public static func sizeRangeCaption(_ doc: WorktreesDoc, now: Double) -> String? {
        let times = doc.worktrees.filter { $0.sizeBytes != nil }.compactMap(\.sizeMeasuredUnix)
        guard let scan = doc.generatedAt, times.contains(where: { abs($0 - scan) > 300 }),
              let oldest = times.min(), let newest = times.max() else { return nil }
        return "sizes measured between \(Fmt.age(max(0, now - oldest))) and \(Fmt.age(max(0, now - newest))) ago"
    }
}
