import Foundation
import Testing
@testable import ProcwhoKit

@Suite @MainActor struct SettingsTests {
    @Test func defaultsAndPersistenceAreIsolated() throws {
        let name = "dev.procwho.app.tests.\(UUID())"
        let defaults = try #require(UserDefaults(suiteName: name))
        defer { defaults.removePersistentDomain(forName: name) }
        let store = SettingsStore(defaults: defaults)
        #expect(store.value == AppSettings())
        #expect(!store.value.alertsEnabled)
        #expect(!store.value.checkPullRequests)
        #expect(store.value.groupByFamily != true)
        #expect(store.value.hideSystemWorkloads != false)
        store.value.groupByFamily = true
        store.value.hideSystemWorkloads = false
        store.value.menuBarMode = .swap
        store.value.searchRoots = ["/tmp/web-app"]
        #expect(SettingsStore(defaults: defaults).value == store.value)
        store.reset()
        #expect(SettingsStore(defaults: defaults).value == AppSettings())
    }

    @Test func clampsInvalidNumbersAndNormalizesRoots() {
        var settings = AppSettings()
        settings.swapGB = 0
        settings.memoryGB = -.infinity
        settings.notInRAMGB = -1
        settings.cooldownMinutes = -1
        settings.hideBelowMB = .nan
        settings.swappedSharePercent = -5
        settings.swappedMinimumMB = -1
        settings.staleDays = 0
        settings.searchRoots = [" /tmp/repo ", "/tmp/repo", "relative", ""]
        settings.cliOverride = "relative"
        let value = settings.validated()
        #expect(value.swapGB == 0.5)
        #expect(value.memoryGB == 8)
        #expect(value.notInRAMGB == 0.1)
        #expect(value.cooldownMinutes == 0)
        #expect(value.hideBelowMB == 0)
        #expect(value.swappedSharePercent == 0)
        #expect(value.swappedMinimumMB == 0)
        #expect(value.staleDays == 1)
        #expect(value.searchRoots == ["/tmp/repo"])
        #expect(value.cliOverride == nil)
    }

    @Test func migratesLegacyIntervalAndResetOverridesIt() throws {
        let name = "dev.procwho.app.tests.\(UUID())"
        let defaults = try #require(UserDefaults(suiteName: name))
        defer { defaults.removePersistentDomain(forName: name) }
        defaults.set(30, forKey: "refreshInterval")
        let store = SettingsStore(defaults: defaults)
        #expect(store.value.openRefresh == .thirty)
        store.reset()
        #expect(SettingsStore(defaults: defaults).value.openRefresh == .ten)
    }

    @Test func introductionDismissalPersists() throws {
        let name = "dev.procwho.app.tests.\(UUID())"
        let defaults = try #require(UserDefaults(suiteName: name))
        defer { defaults.removePersistentDomain(forName: name) }
        let store = SettingsStore(defaults: defaults)
        #expect(store.value.onboardingDismissed != true)
        store.value.onboardingDismissed = true
        #expect(SettingsStore(defaults: defaults).value.onboardingDismissed == true)
        #expect(!store.value.alertsEnabled)
        #expect(!store.value.launchAtLogin)
    }

    @Test func corruptStorageFallsBackToDefaults() throws {
        let name = "dev.procwho.app.tests.\(UUID())"
        let defaults = try #require(UserDefaults(suiteName: name))
        defer { defaults.removePersistentDomain(forName: name) }
        defaults.set(Data("invalid".utf8), forKey: SettingsStore.storageKey)
        #expect(SettingsStore(defaults: defaults).value == AppSettings())
    }

    @Test func worktreeArgumentsAndBadgeThresholds() {
        var settings = AppSettings()
        #expect(settings.worktreeArguments == ["worktrees", "--json", "--stale-days", "7"])
        settings.searchRoots = ["/tmp/a", "/tmp/b"]
        settings.checkPullRequests = true
        #expect(settings.worktreeArguments.suffix(5) == ["--root", "/tmp/a", "--root", "/tmp/b", "--pr"])
        var workload = Workload(key: "a", name: "A", kind: "app", footprintBytes: 1024 * 1_048_576)
        workload.rssBytes = 100 * 1_048_576
        #expect(settings.showsSwappedBadge(for: workload, ram: 48 * 1_073_741_824))
        settings.showSwappedBadge = false
        #expect(!settings.showsSwappedBadge(for: workload, ram: 48 * 1_073_741_824))
    }
}
