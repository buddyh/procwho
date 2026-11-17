import Foundation

/// Display order and titles for workload kinds. Kinds the app does not know sort last,
/// alphabetically, under their raw name.
public enum KindOrder {
    public static let order = [
        "agent-session", "agent-toolset", "agent-command", "agent-host",
        "detached", "unknown", "simulator", "service", "app", "terminal-pane", "watcher",
        "system", "system-opaque",
    ]

    public static func rank(_ kind: String) -> Int {
        order.firstIndex(of: kind) ?? order.count
    }

    public static func sorted(_ kinds: some Sequence<String>) -> [String] {
        kinds.sorted { a, b in
            let (ra, rb) = (rank(a), rank(b))
            return ra != rb ? ra < rb : a < b
        }
    }

    public static func title(_ kind: String) -> String {
        switch kind {
        case "agent-session": return "Agent session"
        case "agent-toolset": return "Agent tool set"
        case "agent-command": return "Agent command"
        case "agent-host": return "Shared agent host"
        case "detached": return "Detached"
        case "unknown": return "Unknown owner"
        case "simulator": return "Simulator"
        case "service": return "Service"
        case "terminal": return "Terminal"
        case "declared": return "Declared owner"
        case "app": return "App"
        case "terminal-pane": return "Terminal pane"
        case "watcher": return "Watcher"
        case "system": return "System"
        case "system-opaque": return "Restricted system process"
        default: return kind
        }
    }
}
