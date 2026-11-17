import Foundation
import Testing
import UserNotifications
import ProcwhoKit
@testable import ProcwhoBar

@Suite @MainActor struct SettingsApplicationTests {
    @Test func preferencesApplyImmediatelyToPresentation() {
        let model = AppModel(snapshotMode: true)
        model.settings.value.menuBarMode = .icon
        #expect(model.labelText.isEmpty)
        model.settings.value.menuBarMode = .agents
        let rows = [Workload(key: "small", name: "Small", kind: "app", footprintBytes: 10 * 1_048_576, cpuPct: 20),
                    Workload(key: "large", name: "Large", kind: "agent-session", footprintBytes: 9 * 1_073_741_824, cpuPct: 1)]
        model.seedSnapshot(status: StatusDoc(system: SystemInfo(memUsedBytes: 20 * 1_073_741_824, swapUsedBytes: 2 * 1_073_741_824), workloads: rows), why: nil, worktrees: nil, tab: .workloads)
        model.settings.value.defaultSort = .cpu
        #expect(model.sort == .cpu)
        #expect(model.rankedWorkloads.first?.key == "small")
        model.settings.value.hideBelowMB = 100
        #expect(model.rankedWorkloads.map(\.key) == ["large"])
        model.settings.value.collapsedFamilies = [.apps]
        #expect(model.collapsedFamilies == [.apps])
        model.settings.value.menuBarMode = .swap
        #expect(model.labelText == "2 GB")
        model.settings.value.menuBarMode = .total
        #expect(model.labelText == "20 GB")
        model.settings.value.menuBarMode = .icon
        #expect(model.labelText.isEmpty)
        model.settings.value.alertsEnabled = true
        #expect(model.activeAlerts.count == 1)
        model.settings.value.alertsEnabled = false
        #expect(model.activeAlerts.isEmpty)
    }

    @Test func resetRestoresPresentationAndDisablesAlerts() {
        let model = AppModel(snapshotMode: true)
        model.settings.value.alertsEnabled = true
        model.settings.value.defaultSort = .age
        model.settings.value.collapsedFamilies = [.agents]
        model.resetSettings()
        #expect(model.settings.value == AppSettings())
        #expect(model.sort == .memory)
        #expect(model.collapsedFamilies == [.system])
        #expect(!model.services.loginEnabled)
    }

    @Test func notificationPermissionCrossesIsolationAsValue() {
        #expect(SettingsServices.Permission(.notDetermined) == .notDetermined)
        #expect(SettingsServices.Permission(.denied) == .denied)
        #expect(SettingsServices.Permission(.authorized) == .authorized)
        #expect(SettingsServices.Permission(.provisional) == .provisional)
        #expect(SettingsServices.Permission(UNAuthorizationStatus(rawValue: 4)!) == .ephemeral)
        #expect(SettingsServices.Permission(UNAuthorizationStatus(rawValue: 999)!) == .unknown)
    }

    @Test func snapshotServicesNeverRegisterOrRequestPermission() async {
        let services = SettingsServices(snapshotMode: true)
        services.setLogin(true)
        await services.enableAlerts()
        #expect(!services.loginEnabled)
        #expect(services.notificationStatus == "Notifications: not requested yet")
    }

    @Test func diagnosticsExcludeSnapshotAndRawErrors() async {
        let model = AppModel(snapshotMode: true)
        model.seedSnapshot(status: StatusDoc(workloads: [Workload(key: "private", name: "Private process", kind: "app")]), why: nil, worktrees: nil, tab: .workloads, error: "Private process /private/path")
        let diagnostics = await model.diagnostics()
        #expect(!diagnostics.contains("Private process"))
        #expect(!diagnostics.contains("/private/path"))
        #expect(diagnostics.contains("CLI path:"))
        #expect(diagnostics.contains("Last error:"))
    }
}
