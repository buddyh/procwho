import Foundation

extension Presentation {
    /// Family totals prefer complete kind summaries over the limited workload sample.
    public static func familyTotals(_ family: KindFamily, status: StatusDoc) -> (memory: Int64?, cpu: Double?) {
        let groups = status.byKind.filter { KindFamily(kind: $0.kind) == family }
        let rows = status.workloads.filter { KindFamily(kind: $0.kind) == family }
        let memory = groups.isEmpty ? rows.compactMap(\.footprintBytes) : groups.compactMap(\.footprintBytes)
        let cpu = (groups.isEmpty ? rows.compactMap(\.cpuPct) : groups.compactMap(\.cpuPct)).filter(\.isFinite)
        return (memory.isEmpty ? nil : sum(memory), cpu.isEmpty ? nil : cpu.reduce(0) { $0 + max(0, $1) })
    }

    /// Use full kind counts where available, with a sample-only fallback.
    public static func agentWorkloadCount(_ status: StatusDoc) -> Int {
        status.byKind.isEmpty ? status.workloads.filter(\.isAgent).count
            : status.byKind.filter { $0.kind.hasPrefix("agent-") }.reduce(0) { $0 + max(0, $1.workloads ?? 0) }
    }

    /// Sessions and helpers are distinct counts; omit absent parts and pluralize each.
    public static func agentCountsCaption(_ status: StatusDoc) -> String {
        func count(_ kind: String) -> Int {
            status.byKind.isEmpty ? status.workloads.filter { $0.kind == kind }.count
                : status.byKind.filter { $0.kind == kind }.reduce(0) { $0 + max(0, $1.workloads ?? 0) }
        }
        let sessions = count("agent-session")
        let helpers = ["agent-toolset", "agent-command", "agent-host"].reduce(0) { $0 + count($1) }
        return [sessions > 0 ? "\(Fmt.count(sessions)) agent \(sessions == 1 ? "session" : "sessions")" : nil,
                helpers > 0 ? "\(Fmt.count(helpers)) helper \(helpers == 1 ? "group" : "groups")" : nil]
            .compactMap { $0 }.joined(separator: " · ")
    }

    /// Sum of footprints, not a physical RAM measurement. Preserve absent measurements.
    public static func totalFootprint(_ status: StatusDoc) -> Int64? {
        if let total = status.totals?.footprintBytes { return total }
        let values = status.byKind.isEmpty ? status.workloads.compactMap(\.footprintBytes) : status.byKind.compactMap(\.footprintBytes)
        return values.isEmpty ? nil : sum(values)
    }

    public static func associatedMemory(_ status: StatusDoc) -> Int64 {
        status.totals?.agentAssociatedBytes ?? sum(status.workloads.filter { $0.agentLink != nil || $0.isAgent }.compactMap(\.footprintBytes))
    }

    public static func mediumAssociatedMemory(_ status: StatusDoc) -> Int64 {
        status.totals?.agentAssociatedMediumBytes ?? sum(status.workloads.filter { $0.agentLink?.confidence == "medium" }.compactMap(\.footprintBytes))
    }

    public static func heroMemory(_ status: StatusDoc, view: HeroView = .associated) -> Int64? {
        switch view.effective(for: status) {
        case .associated: associatedMemory(status)
        case .agents: status.agentFootprintBytes
        case .memoryUsed: status.system?.memUsedBytes
        }
    }

    /// Capacity, association counts and physical use remain distinct measurements.
    public static func heroCaption(_ status: StatusDoc, view: HeroView = .associated) -> String {
        var parts: [String] = []
        switch view.effective(for: status) {
        case .associated, .agents:
            let sessions = status.byKind.isEmpty ? status.workloads.filter { $0.kind == "agent-session" }.count
                : status.byKind.filter { $0.kind == "agent-session" }.reduce(0) { $0 + max(0, $1.workloads ?? 0) }
            parts.append("\(Fmt.count(sessions)) \(sessions == 1 ? "session" : "sessions")")
        case .memoryUsed:
            if let available = status.system?.memAvailableBytes { parts.append("\(Fmt.bytes(available)) available") }
            else { parts.append("Pressure " + (status.system?.memoryPressure ?? "unknown")) }
        }
        if let ram = status.system?.memTotalBytes { parts.append("\(Fmt.installedRAM(ram)) RAM") }
        return parts.joined(separator: " · ")
    }

    /// Extra association counts belong in the tooltip, outside the one-line caption.
    public static func heroHelp(_ status: StatusDoc, view: HeroView) -> String {
        if view.effective(for: status) == .memoryUsed { return Glossary.help("physical-memory") }
        let counts = agentCountsCaption(status).replacingOccurrences(of: " · ", with: ", ")
        let hosts = status.workloads.filter { $0.agentLink?.relation == "agent_host_app" }.count
        let parts = [counts.isEmpty ? nil : counts, hosts > 0 ? "\(Fmt.count(hosts)) host \(hosts == 1 ? "app" : "apps")" : nil].compactMap { $0 }
        let linked = mediumAssociatedMemory(status)
        return parts.joined(separator: ", ") + (parts.isEmpty ? "" : ".")
            + (view == .associated && linked > 0 ? " Includes ~\(Fmt.bytes(linked)) linked by launching app." : "")
    }

    public static let associationHelp = "Launching-app evidence links these processes to agents; it does not identify a specific task."

}

extension Presentation {
    /// The same swap/pressure rule drives the gauge and machine-level attention icon.
    public static func machineState(_ system: SystemInfo?) -> LoadState {
        let swap = swapState(system)
        let load = system?.memoryPressure == nil ? loadState(system?.load1, cores: system?.ncpu) : .normal
        if swap == .critical || load == .critical { return .critical }
        return swap == .attention || load == .attention ? .attention : .normal
    }

    /// Machine measurements precede individual findings; family footprints are context, not causality.
    public static func machineAttention(_ status: StatusDoc) -> String? {
        if let pressure = status.system?.memoryPressure, ["warning", "critical"].contains(pressure) {
            return "Memory pressure is " + (pressure == "critical" ? "critical" : "high")
        }
        if let free = status.system?.swapDiskFreeBytes, let total = status.system?.swapDiskTotalBytes,
           free >= 0, total > 0, Double(free) / Double(total) < 0.1 { return "Swap volume below 10% free" }
        if status.system?.memoryPressure == nil,
           loadState(status.system?.load1, cores: status.system?.ncpu) != .normal, let load = status.system?.load1 {
            return String(format: "Load is high: %.1f", load)
        }
        if swapState(status.system) != .normal, let used = status.system?.swapUsedBytes {
            return "Swap is high: \(Fmt.bytes(used))"
        }
        return nil
    }

    /// Footprint context stays available without lengthening the machine warning.
    public static func machineAttentionHelp(_ status: StatusDoc) -> String {
        let largest = composition(status).filter { $0.bytes > 0 }.sorted { $0.bytes > $1.bytes }
            .prefix(2).map { "\($0.title) \(Fmt.bytes($0.bytes))" }.joined(separator: ", ")
        let measurements = [status.system?.memTotalBytes.map { "\(Fmt.installedRAM($0)) RAM" },
                            status.system?.swapUsedBytes.map { "Swap \(Fmt.bytes($0))" }].compactMap { $0 }
        return measurements.joined(separator: ", ") + (measurements.isEmpty ? "" : ".")
            + (largest.isEmpty ? "" : " Largest footprints: \(largest).")
    }

}

extension Presentation {
    public static func pressureState(_ pressure: String?) -> LoadState {
        switch pressure { case "warning": .attention; case "critical": .critical; default: .normal }
    }
    public static func pressureFraction(_ pressure: String) -> Double {
        switch pressure { case "critical": 1; case "warning": 0.67; default: 0.33 }
    }
}
