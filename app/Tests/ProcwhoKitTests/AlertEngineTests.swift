import Foundation
import Testing
@testable import ProcwhoKit

struct AlertEngineTests {
    let now = Date(timeIntervalSince1970: 10_000)
    var enabled: AppSettings { var s = AppSettings(); s.alertsEnabled = true; return s }
    func status(bytes: Int64 = 9 * 1_073_741_824, key: String = "one") -> StatusDoc {
        StatusDoc(system: SystemInfo(memTotalBytes: 48 * 1_073_741_824), workloads: [Workload(key: key, name: "Claude session: web-app", kind: "agent-session", footprintBytes: bytes)])
    }
    @Test func disabledAndExactThresholdDoNotFire() {
        #expect(AlertEngine.evaluate(previous: .init(), status: status(), settings: .init(), now: now).active.isEmpty)
        #expect(AlertEngine.evaluate(previous: .init(), status: status(bytes: 8 * 1_073_741_824), settings: enabled, now: now).active.isEmpty)
    }
    @Test func requiresFalseEvaluationAndCooldown() {
        let first = AlertEngine.evaluate(previous: .init(), status: status(), settings: enabled, now: now)
        #expect(first.notifications.count == 1)
        let continuous = AlertEngine.evaluate(previous: first.state, status: status(), settings: enabled, now: now.addingTimeInterval(3600))
        #expect(continuous.notifications.isEmpty)
        let cleared = AlertEngine.evaluate(previous: continuous.state, status: status(bytes: 0), settings: enabled, now: now.addingTimeInterval(60))
        let early = AlertEngine.evaluate(previous: cleared.state, status: status(), settings: enabled, now: now.addingTimeInterval(120))
        #expect(early.notifications.isEmpty)
        let due = AlertEngine.evaluate(previous: early.state, status: status(), settings: enabled, now: now.addingTimeInterval(1800))
        #expect(due.notifications.count == 1)
    }
    @Test func keysSeparateWorkloadsAndDisappearancesRearm() {
        let first = AlertEngine.evaluate(previous: .init(), status: status(), settings: enabled, now: now)
        let other = AlertEngine.evaluate(previous: first.state, status: status(key: "two"), settings: enabled, now: now)
        #expect(other.notifications.count == 1)
        let returned = AlertEngine.evaluate(previous: other.state, status: status(), settings: enabled, now: now.addingTimeInterval(1800))
        #expect(returned.notifications.count == 1)
    }
    @Test func swapAndNotInRAMAndPrivacy() {
        var sample = status()
        sample.workloads[0].rssBytes = 1_073_741_824
        sample.workloads[0].name = "Command /Users/user/private/file --token secret"
        sample.system = SystemInfo(memTotalBytes: 48 * 1_073_741_824, swapUsedBytes: 13 * 1_073_741_824, swapTotalBytes: 14 * 1_073_741_824)
        let result = AlertEngine.evaluate(previous: .init(), status: sample, settings: enabled, now: now)
        #expect(result.active.count == 3)
        #expect(result.notifications.count == 3)
        #expect(result.active.contains { $0.text.contains("not in RAM") })
        #expect(!result.notifications.contains { $0.text.contains("/Users") || $0.text.contains("secret") })
    }
    @Test func findingEdgesAndSummaryBudget() throws {
        let finding = try ProcwhoJSON.decode(Finding.self, from: Data(#"{"severity":"review","title":"Private detail","workload_key":"one","reasons":[],"not_proven":[]}"#.utf8))
        var sample = status()
        sample.findings = [finding]
        let result = AlertEngine.evaluate(previous: .init(), status: sample, settings: enabled, now: now)
        #expect(result.notifications.count == 2)
        #expect(!result.notifications.contains { $0.text.contains("Private detail") })
        #expect(AlertEngine.evaluate(previous: result.state, status: sample, settings: enabled, now: now).notifications.isEmpty)
        sample.workloads += (2...5).map { Workload(key: "\($0)", name: "Tool set", kind: "agent-toolset", footprintBytes: 10 * 1_073_741_824) }
        let many = AlertEngine.evaluate(previous: .init(), status: sample, settings: enabled, now: now)
        #expect(many.notifications.count == 1)
        #expect(many.notifications.first?.id == "summary")
        #expect(many.active.count == 6)
    }
    @Test func estimatesAndUncertainOwnersAreExplicit() {
        var sample = status()
        sample.workloads[0].rssBytes = 0
        sample.workloads[0].confidence = "medium"
        let result = AlertEngine.evaluate(previous: .init(), status: sample, settings: enabled, now: now)
        #expect(result.active.allSatisfy { $0.text.contains("(owner uncertain)") })
        #expect(result.notifications.contains { $0.text.contains("~9 GB not in RAM") })
        sample.workloads[0].confidence = "high"
        #expect(!AlertEngine.evaluate(previous: .init(), status: sample, settings: enabled, now: now).active.contains { $0.text.contains("owner uncertain") })
        #expect(Fmt.estimatedBytes(nil) == "—")
        #expect(Fmt.estimatedBytes(0) == "~0 MB")
    }

    @Test func togglesAndMissingMeasurements() {
        var settings = enabled
        settings.memoryAlert = false
        #expect(AlertEngine.evaluate(previous: .init(), status: status(), settings: settings, now: now).active.isEmpty)
        let sample = StatusDoc(system: SystemInfo(swapUsedBytes: 1, swapTotalBytes: 0), workloads: [Workload(key: "x", name: "X", kind: "app")])
        #expect(AlertEngine.evaluate(previous: .init(), status: sample, settings: enabled, now: now).notifications.isEmpty)
    }
}
