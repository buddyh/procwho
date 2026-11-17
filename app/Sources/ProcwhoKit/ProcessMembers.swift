import Foundation

/// A process assigned to a workload; optional measurements support older CLI reports.
public struct ProcessMember: Decodable, Sendable, Equatable, Identifiable {
    public var pid: Int
    public var ppid: Int?
    public var name: String
    public var command: String
    public var footprintBytes: Int64?
    public var cpuPct: Double?
    public var ageSeconds: Double?
    public var id: Int { pid }

    public init(pid: Int, ppid: Int? = nil, name: String, command: String,
                footprintBytes: Int64? = nil, cpuPct: Double? = nil, ageSeconds: Double? = nil) {
        self.pid = pid; self.ppid = ppid; self.name = name; self.command = command
        self.footprintBytes = footprintBytes; self.cpuPct = cpuPct; self.ageSeconds = ageSeconds
    }
}

extension Presentation {
    /// Removes CLI-only ownership decorations from app display names.
    public static func displayName(_ name: String) -> String {
        var result = name
        if result.hasPrefix("UNKNOWN owner: ") { result.removeFirst("UNKNOWN owner: ".count) }
        if result.hasSuffix(" (app)") { result.removeLast(" (app)".count) }
        return result
    }

    /// Matches workload identity independently of member matches.
    public static func workloadMatches(_ row: Workload, query: String) -> Bool {
        [displayName(row.name), row.kind, row.key, row.projectPath ?? "", row.inside ?? ""]
            .contains { $0.localizedCaseInsensitiveContains(query) }
    }

    /// Process search covers names and literal process identifiers.
    public static func memberMatches(_ member: ProcessMember, query: String) -> Bool {
        member.name.localizedCaseInsensitiveContains(query) || String(member.pid).contains(query)
    }

    /// Member-only search results open temporarily without changing saved expansion state.
    public static func memberOnlyMatch(_ row: Workload, search: String) -> Bool {
        let query = search.trimmingCharacters(in: .whitespacesAndNewlines)
        return !query.isEmpty && !workloadMatches(row, query: query)
            && (row.members ?? []).contains { memberMatches($0, query: query) }
    }

    /// Sort all members by the selected metric; unavailable measurements sort last.
    public static func members(_ row: Workload, sort: WorkloadSort, search: String = "") -> [ProcessMember] {
        let query = search.trimmingCharacters(in: .whitespacesAndNewlines)
        let filter = memberOnlyMatch(row, search: query)
        func value(_ member: ProcessMember) -> Double {
            let number: Double? = switch sort {
            case .cpu: member.cpuPct
            case .age: member.ageSeconds
            case .memory, .notInRAM: member.footprintBytes.map(Double.init)
            }
            return number.flatMap { $0.isFinite ? max(0, $0) : nil } ?? -.infinity
        }
        return (row.members ?? []).filter { !filter || memberMatches($0, query: query) }.sorted {
            if value($0) != value($1) { return value($0) > value($1) }
            if $0.name != $1.name { return $0.name < $1.name }
            return $0.pid < $1.pid
        }
    }
}
