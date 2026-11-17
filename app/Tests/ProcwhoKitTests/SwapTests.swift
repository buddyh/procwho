import Foundation
import Testing
@testable import ProcwhoKit

struct SwapTests {
    let gb: Int64 = 1_073_741_824

    @Test(arguments: [8, 16, 48]) func meterAndStrictStates(ramGB: Int64) {
        var s = SystemInfo(memTotalBytes: ramGB * gb, swapUsedBytes: ramGB * gb / 2, swapTotalBytes: ramGB * gb)
        #expect(Presentation.swapRatio(used: s.swapUsedBytes, ram: s.memTotalBytes) == 0.5)
        #expect(Presentation.swapState(s) == .normal)
        s.swapUsedBytes! += 1
        #expect(Presentation.swapState(s) == .attention)
        s.swapUsedBytes = ramGB * gb
        #expect(Presentation.swapState(s) == .attention)
        s.swapUsedBytes! += 1
        #expect(Presentation.swapState(s) == .critical)
        #expect(Presentation.swapRatio(used: s.swapUsedBytes, ram: s.memTotalBytes) == 1)
        s.swapUsedBytes = 0
        s.memoryPressure = "warning"
        #expect(Presentation.swapState(s) == .attention)
        s.memoryPressure = "critical"
        #expect(Presentation.swapState(s) == .critical)
        s.memoryPressure = "normal"
        s.swapDiskTotalBytes = 1000; s.swapDiskFreeBytes = 100
        #expect(Presentation.swapState(s) == .normal)
        s.swapDiskFreeBytes = 99
        #expect(Presentation.swapState(s) == .critical)
    }

    @Test(arguments: [8, 16, 48]) func automaticAlertsUseRAM(ramGB: Int64) {
        var settings = AppSettings(); settings.alertsEnabled = true
        let limit = max(2, Double(ramGB) / 4)
        #expect(AutomaticThresholds(installedRAMBytes: UInt64(ramGB * gb)).swapGB == limit)
        var status = StatusDoc(system: SystemInfo(memTotalBytes: ramGB * gb, swapUsedBytes: Int64(limit) * gb))
        #expect(AlertEngine.evaluate(previous: .init(), status: status, settings: settings, now: .distantPast).active.isEmpty)
        status.system?.swapUsedBytes! += 1
        #expect(AlertEngine.evaluate(previous: .init(), status: status, settings: settings, now: .distantPast).active.map(\.id) == ["swap"])
        status.system?.swapUsedBytes = 0; status.system?.memoryPressure = "critical"
        #expect(AlertEngine.evaluate(previous: .init(), status: status, settings: settings, now: .distantPast).active.map(\.id) == ["memory-pressure"])
        settings.criticalPressureAlert = false
        #expect(AlertEngine.evaluate(previous: .init(), status: status, settings: settings, now: .distantPast).active.isEmpty)
        settings.swapGB = 1
        #expect(!settings.validated().usesAutomaticSwap)
        #expect(settings.swapLimitGB(ram: UInt64(ramGB * gb)) == 1)
    }

    @Test func missingAndTooltip() {
        #expect(Presentation.swapRatio(used: -1, ram: 8 * gb) == nil)
        #expect(Presentation.swapRatio(used: 1, ram: 0) == nil)
        #expect(Presentation.swapState(nil) == .normal)
        let lowDisk = SystemInfo(swapDiskFreeBytes: 1, swapDiskTotalBytes: 100)
        #expect(Presentation.machineState(lowDisk) == .critical)
        #expect(Presentation.machineAttention(StatusDoc(system: lowDisk)) == "Swap volume below 10% free")
        var pressure = SystemInfo(memTotalBytes: 8 * gb, swapUsedBytes: 5 * gb)
        pressure.memoryPressure = "warning"
        #expect(Presentation.machineAttention(StatusDoc(system: pressure))?.hasPrefix("Memory pressure is high") == true)
        let system = SystemInfo(swapUsedBytes: 9 * gb, swapTotalBytes: 10 * gb, swapDiskFreeBytes: 52 * gb)
        #expect(Presentation.swapGaugeHelp(system).contains("Swap files allocated: 10 GB"))
        #expect(Presentation.swapGaugeHelp(system).contains("Free disk space: 52 GB"))
        #expect(AutomaticThresholds(installedRAMBytes: 10 * UInt64(gb)).swapGB == 2.5)
    }

    @Test @MainActor func migratesOldSwapPercentageAndPreservesOtherPreferences() throws {
        let name = "dev.procwho.app.tests.\(UUID())"
        let defaults = try #require(UserDefaults(suiteName: name))
        defer { defaults.removePersistentDomain(forName: name) }
        var old = try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(AppSettings())) as? [String: Any])
        old["swapPercent"] = 32
        old["memoryGB"] = 6
        old["memoryAutomatic"] = false
        defaults.set(try JSONSerialization.data(withJSONObject: old), forKey: SettingsStore.storageKey)
        let store = SettingsStore(defaults: defaults)
        #expect(store.value.usesAutomaticSwap)
        #expect(store.value.memoryGB == 6)
        #expect(store.value.worktreeFoldersChosen != true)
        store.value.swapGB = 3.5
        #expect(!SettingsStore(defaults: defaults).value.usesAutomaticSwap)
        #expect(SettingsStore(defaults: defaults).value.swapGB == 3.5)
        #expect(!store.value.alertsEnabled)
        #expect(!store.value.launchAtLogin)
    }

}
