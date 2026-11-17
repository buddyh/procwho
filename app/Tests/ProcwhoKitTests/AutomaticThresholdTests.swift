import Foundation
import Testing
@testable import ProcwhoKit

struct AutomaticThresholdTests {
    @Test func installedRAMScaling() {
        let cases: [(UInt64, Double, Double, Double)] = [
            (8, 2, 1, 100), (16, 2.5, 1.5, 100), (24, 4, 2, 150),
            (48, 8, 4, 300), (96, 16, 8, 600), (192, 32, 16, 1250),
        ]
        for (gb, memory, missing, badge) in cases {
            let limits = AutomaticThresholds(installedRAMBytes: gb * 1_073_741_824)
            #expect(limits.memoryGB == memory)
            #expect(limits.notInRAMGB == missing)
            #expect(limits.swappedMinimumMB == badge)
        }
    }

    @Test func customValuesAndReturnToAutomatic() throws {
        var settings = AppSettings()
        let ram: UInt64 = 16 * 1_073_741_824
        #expect(settings.memoryLimitGB(ram: ram) == 2.5)
        settings.memoryGB = 8
        settings.notInRAMGB = 4
        settings.swappedMinimumMB = 300
        #expect(!settings.usesAutomaticMemory)
        #expect(settings.memoryLimitGB(ram: ram) == 8)
        #expect(settings.notInRAMLimitGB(ram: ram) == 4)
        #expect(settings.swappedLimitMB(ram: ram) == 300)
        settings.memoryAutomatic = true
        settings.notInRAMAutomatic = true
        settings.swappedAutomatic = true
        let decoded = try JSONDecoder().decode(AppSettings.self, from: JSONEncoder().encode(settings.validated()))
        #expect(decoded.memoryLimitGB(ram: ram) == 2.5)
        #expect(decoded.notInRAMLimitGB(ram: ram) == 1.5)
        #expect(decoded.swappedLimitMB(ram: ram) == 100)
    }

    @Test func oldPreferencesKeepCustomNumbers() throws {
        var object = try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(AppSettings())) as? [String: Any])
        object["memoryGB"] = 12.0
        object.removeValue(forKey: "memoryAutomatic")
        object.removeValue(forKey: "notInRAMAutomatic")
        object.removeValue(forKey: "swappedAutomatic")
        object.removeValue(forKey: "onboardingDismissed")
        let settings = try JSONDecoder().decode(AppSettings.self, from: JSONSerialization.data(withJSONObject: object))
        #expect(settings.memoryLimitGB(ram: 16 * 1_073_741_824) == 12)
        #expect(settings.usesAutomaticNotInRAM)
    }

    @Test func alertsUseSampleCapacityAndStrictBoundary() {
        var settings = AppSettings()
        settings.alertsEnabled = true
        settings.reviewAlert = false
        var row = Workload(key: "x", name: "Editor", kind: "app", confidence: "high", footprintBytes: 3 * 1_073_741_824)
        row.rssBytes = 0
        let sample = StatusDoc(system: SystemInfo(memTotalBytes: 16 * 1_073_741_824), workloads: [row])
        let result = AlertEngine.evaluate(previous: .init(), status: sample, settings: settings, now: .distantPast)
        #expect(result.active.count == 2)
        var fallback = sample
        fallback.system = nil
        #expect(AlertEngine.evaluate(previous: .init(), status: fallback, settings: settings, now: .distantPast,
                                     installedRAMBytes: 16 * 1_073_741_824).active.count == 2)
        settings.memoryGB = 3
        settings.notInRAMGB = 3
        #expect(AlertEngine.evaluate(previous: .init(), status: sample, settings: settings, now: .distantPast).active.isEmpty)
    }
}
