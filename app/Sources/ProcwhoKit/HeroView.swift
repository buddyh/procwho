import Foundation

/// Persisted Overview lens. It changes the headline, never exclusive workload ownership.
public enum HeroView: String, Codable, CaseIterable, Sendable {
    case associated, agents, memoryUsed
    public var title: String {
        switch self { case .associated: "Agents + associated"; case .agents: "Agents"; case .memoryUsed: "Memory used" }
    }
    public var next: Self {
        switch self { case .associated: .agents; case .agents: .memoryUsed; case .memoryUsed: .associated }
    }
    /// Skip an associated view that would resolve back to the same no-agent headline.
    public func next(for status: StatusDoc) -> Self {
        Presentation.agentWorkloadCount(status) == 0 && effective(for: status) == .memoryUsed ? .agents : next
    }
    public func effective(for status: StatusDoc) -> Self {
        self == .associated && Presentation.agentWorkloadCount(status) == 0 ? .memoryUsed : self
    }
}

extension Presentation {
    /// Lit fraction of a family segment; the remainder stays visible at 35% opacity.
    public static func highlightedShare(_ segment: CompositionSegment, status: StatusDoc, view: HeroView) -> Double {
        let effective = view.effective(for: status)
        if effective == .memoryUsed { return 1 }
        if effective == .agents { return segment.id == KindFamily.agents.rawValue ? 1 : 0 }
        let rows = status.workloads.filter { KindFamily(kind: $0.kind).rawValue == segment.id && ($0.agentLink != nil || $0.isAgent) }
        return share(sum(rows.compactMap(\.footprintBytes)), of: segment.bytes)
    }
}
