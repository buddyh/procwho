import Foundation

extension StatusDoc {
    /// Friendly names for schema source keys; custom sources retain their labels.
    public static func sourceName(_ key: String) -> String {
        switch key {
        case "lsof_ports": "listening ports"
        case "launchctl": "launchd jobs"
        case "tmux": "tmux sessions"
        case "label_providers": "label providers"
        default: key
        }
    }

    /// Notes have no structured severity. Only explicit failure/unavailability text
    /// supplements the source's boolean failure state; stale metadata is informational.
    public var sourceFailureNotes: [String] {
        sourceNotes.filter {
            $0.localizedCaseInsensitiveContains("failed") || $0.localizedCaseInsensitiveContains("failure")
                || $0.localizedCaseInsensitiveContains("unavailable")
        }
    }

    /// Source ages are unstructured CLI strings, so preserve them verbatim.
    public var dataSourceDescriptions: [String] {
        (sources ?? [:]).sorted { $0.key < $1.key }.map { key, state in
            let rawName = Self.sourceName(key)
            let name = rawName.prefix(1).capitalized + rawName.dropFirst()
            return switch state {
            case .ok(let ok):
                "\(name): " + (sourceNotes.contains("\(Self.sourceName(key)): optional tool not installed") ? "Optional tool not installed" : ok ? "Available" : "Unavailable")
            case .note(let note): "\(name): \(note)"
            case .notes(let notes): notes.joined(separator: "; ")
            case .other: "\(name): Not reported"
            }
        }
    }
}
