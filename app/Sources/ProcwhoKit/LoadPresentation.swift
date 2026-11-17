import Foundation

/// Semantic load state, independent of SwiftUI colors.
public enum LoadState: Sendable {
    case normal, attention, critical
}

extension Presentation {
    /// Swap has no fixed allocated capacity. Installed RAM supplies meter context.
    public static func swapUsageText(used: Int64, allocated: Int64) -> String { Fmt.bytes(used) }

    public static func swapRatio(used: Int64?, ram: Int64?) -> Double? {
        guard let used, used >= 0, let ram, ram > 0 else { return nil }
        return min(1, Double(used) / Double(ram))
    }

    /// Pressure and low disk take precedence; exact 50% and 100% remain in the lower state.
    public static func swapState(_ system: SystemInfo?) -> LoadState {
        guard let system else { return .normal }
        if system.memoryPressure == "critical" { return .critical }
        if let free = system.swapDiskFreeBytes, let total = system.swapDiskTotalBytes,
           free >= 0, total > 0, Double(free) / Double(total) < 0.1 { return .critical }
        let ratio = system.swapUsedBytes.flatMap { used in
            system.memTotalBytes.flatMap { ram in ram > 0 && used >= 0 ? Double(used) / Double(ram) : nil }
        }
        if let ratio, ratio > 1 { return .critical }
        if system.memoryPressure == "warning" || (ratio.map { $0 > 0.5 } ?? false) { return .attention }
        return .normal
    }

    public static func swapGaugeHelp(_ system: SystemInfo) -> String {
        "Swap files allocated: \(Fmt.bytes(system.swapTotalBytes)). Free disk space: \(Fmt.bytes(system.swapDiskFreeBytes))."
    }

    /// Load uses one unit per logical core; malformed samples have no presentation.
    public static func loadRatio(_ load: Double?, cores: Int?) -> Double? {
        guard let load, load.isFinite, load >= 0, let cores, cores > 0 else { return nil }
        return min(1, load / Double(cores))
    }

    /// Exact core capacity stays neutral; exact twice capacity stays attention.
    public static func loadState(_ load: Double?, cores: Int?) -> LoadState {
        guard loadRatio(load, cores: cores) != nil, let load, let cores else { return .normal }
        if load > Double(cores) * 2 { return .critical }
        return load > Double(cores) ? .attention : .normal
    }

}
