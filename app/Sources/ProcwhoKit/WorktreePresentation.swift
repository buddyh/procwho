import Foundation

/// Local ranking over the complete loaded worktree report; absent measurements sort last.
public enum WorktreeSort: String, CaseIterable, Identifiable, Sendable {
    case size = "Size", idle = "Idle time", repo = "Repo"
    public var id: String { rawValue }
}

extension Presentation {
    public static func worktrees(_ rows: [Worktree], search: String, verdict: String?, sort: WorktreeSort) -> [Worktree] {
        let query = search.trimmingCharacters(in: .whitespacesAndNewlines)
        return rows.filter { row in
            (verdict == nil || row.verdict == verdict) && (query.isEmpty || [row.repo ?? "", row.branch ?? "", row.path].contains { $0.localizedCaseInsensitiveContains(query) })
        }.sorted { a, b in
            switch sort {
            case .size:
                if a.sizeBytes != b.sizeBytes { return (a.sizeBytes ?? -1) > (b.sizeBytes ?? -1) }
            case .idle:
                if a.idleSecs != b.idleSecs { return (a.idleSecs ?? -1) > (b.idleSecs ?? -1) }
            case .repo:
                if a.repo != b.repo { return (a.repo ?? "").localizedStandardCompare(b.repo ?? "") == .orderedAscending }
            }
            return a.path < b.path
        }
    }

    /// Live processes make idle age inapplicable; recent idle activity needs no seconds counter.
    public static func worktreeIdleCaption(_ row: Worktree) -> String? {
        guard row.inUse != true, row.verdict != "in-use",
              let idle = row.idleSecs, idle.isFinite else { return nil }
        return idle < 60 ? "just now" : "Idle \(Fmt.age(idle))"
    }

    public static func sizingFailure(_ row: Worktree) -> String? {
        guard row.sizeBytes == nil else { return nil }
        return row.notes.first { $0.hasPrefix("size:") }.map { "Sizing failed: " + $0.dropFirst(5).trimmingCharacters(in: .whitespaces) }
    }
}
