import Foundation

/// Stable presentation families, ordered as the composition legend appears.
public enum KindFamily: String, CaseIterable, Codable, Sendable, Identifiable {
    case agents, leftovers, unknown, simulators, services, apps, system
    public var id: String { rawValue }
    public var title: String { self == .unknown ? "Unknown owner" : rawValue.capitalized }

    public init(kind: String) {
        switch kind {
        case "agent-session", "agent-toolset", "agent-command", "agent-host": self = .agents
        case "detached": self = .leftovers
        case "unknown": self = .unknown
        case "simulator": self = .simulators
        case "app", "terminal", "terminal-pane": self = .apps
        case "system", "system-opaque": self = .system
        default: self = .services
        }
    }
}

extension Workload {
    /// Estimated compressed or swapped memory; absent when either measurement is missing.
    public var notInRamBytes: Int64? {
        guard let footprintBytes, let rssBytes else { return nil }
        return max(0, max(0, footprintBytes) - max(0, rssBytes))
    }

    /// Requires both substantial nonresident memory and a nonresident majority.
    public var showsSwappedBadge: Bool {
        AppSettings().showsSwappedBadge(for: self)
    }
}
