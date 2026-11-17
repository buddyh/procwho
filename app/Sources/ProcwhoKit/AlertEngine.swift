import Foundation

public struct WorkloadAlert: Equatable, Identifiable, Sendable {
    public let id: String
    public let workloadKey: String?
    public let text: String
}

public struct AlertState: Equatable, Sendable {
    public struct Condition: Equatable, Sendable {
        public var active = false
        public var armed = true
        public var lastFired: Date?
        public init() {}
    }
    public var conditions: [String: Condition] = [:]
    public init() {}
}

public struct AlertEvaluation: Sendable {
    public var state: AlertState
    public var active: [WorkloadAlert]
    public var notifications: [WorkloadAlert]
}

/// Pure edge detection. A false evaluation rearms a condition; cooldown also has to elapse.
/// Stable workload keys separate workloads that happen to share a display name.
public enum AlertEngine {
    public static func evaluate(previous: AlertState, status: StatusDoc, settings: AppSettings,
                                now: Date, installedRAMBytes: UInt64 = ProcessInfo.processInfo.physicalMemory) -> AlertEvaluation {
        let settings = settings.validated()
        let ram = status.system?.memTotalBytes.flatMap { $0 > 0 ? UInt64($0) : nil } ?? installedRAMBytes
        var active: [WorkloadAlert] = []
        if settings.alertsEnabled {
            if settings.criticalPressureAlert != false, status.system?.memoryPressure == "critical" {
                active.append(WorkloadAlert(id: "memory-pressure", workloadKey: nil, text: "Memory pressure is critical"))
            }
            if settings.swapAlert, let used = status.system?.swapUsedBytes,
               Double(used) > settings.swapLimitGB(ram: ram) * 1_073_741_824 {
                active.append(WorkloadAlert(id: "swap", workloadKey: nil, text: "Swap: \(Fmt.bytes(used)) used"))
            }
            for workload in status.workloads.sorted(by: { $0.key < $1.key }) {
                let name = notificationName(workload) + (workload.weakConfidence ? " (owner uncertain)" : "")
                if settings.memoryAlert, let bytes = workload.footprintBytes,
                   Double(bytes) > settings.memoryLimitGB(ram: ram) * 1_073_741_824 {
                    active.append(WorkloadAlert(id: "memory:\(workload.key)", workloadKey: workload.key,
                                                text: "\(name): \(Fmt.bytes(bytes)) memory"))
                }
                if settings.notInRAMAlert, let bytes = workload.notInRamBytes,
                   Double(bytes) > settings.notInRAMLimitGB(ram: ram) * 1_073_741_824 {
                    active.append(WorkloadAlert(id: "not-in-ram:\(workload.key)", workloadKey: workload.key,
                                                text: "\(name): \(Fmt.estimatedBytes(bytes)) not in RAM"))
                }
            }
            if settings.reviewAlert {
                for finding in status.findings.filter(\.isReview).sorted(by: { $0.id < $1.id }) {
                    let workload = status.workloads.first { $0.key == finding.workloadKey }
                    active.append(WorkloadAlert(id: "review:\(finding.id)", workloadKey: finding.workloadKey,
                                                text: "\(workload.map { notificationName($0) + ($0.weakConfidence ? " (owner uncertain)" : "") } ?? "Workload"): 1 finding needs review"))
                }
            }
        }
        // Duplicated CLI rows must not produce duplicate notifications.
        var seen = Set<String>()
        active = active.filter { seen.insert($0.id).inserted }
        var state = previous
        for key in state.conditions.keys where !seen.contains(key) {
            state.conditions[key]?.active = false
            state.conditions[key]?.armed = true
        }
        var fires: [WorkloadAlert] = []
        for alert in active {
            var condition = state.conditions[alert.id] ?? AlertState.Condition()
            condition.active = true
            if condition.armed && (condition.lastFired.map { now.timeIntervalSince($0) >= settings.cooldownMinutes * 60 } ?? true) {
                fires.append(alert)
                condition.lastFired = now
                condition.armed = false
            }
            state.conditions[alert.id] = condition
        }
        // Removed workloads need no tombstone after the cooldown has elapsed.
        state.conditions = state.conditions.filter { _, condition in
            condition.active || (condition.lastFired.map { now.timeIntervalSince($0) < settings.cooldownMinutes * 60 } ?? false)
        }
        let notifications = fires.count > 3
            ? [WorkloadAlert(id: "summary", workloadKey: nil, text: "\(Fmt.count(fires.count)) new alerts" + (fires.contains { $0.text.contains("(owner uncertain)") } ? " (owner uncertain)" : ""))]
            : fires
        return AlertEvaluation(state: state, active: active, notifications: notifications)
    }

    /// CLI labels can contain a command or path. Notifications never include those labels.
    public static func notificationName(_ workload: Workload) -> String {
        if workload.kind == "agent-command" || workload.name.contains("/") || workload.name.contains("\\")
            || workload.name.contains("\n") || workload.name.contains("\t")
            || workload.name.contains("--") || workload.name.count > 120 {
            return "\(KindFamily(kind: workload.kind).title) workload"
        }
        return Presentation.displayName(workload.name)
    }
}
