import Foundation
import Observation

public enum BackgroundRefresh: Int, Codable, CaseIterable, Sendable {
    case off = 0, minute = 60, fiveMinutes = 300, fifteenMinutes = 900
    public var title: String {
        switch self { case .off: "Off"; case .minute: "1 min"; case .fiveMinutes: "5 min"; case .fifteenMinutes: "15 min" }
    }
}

public enum MenuBarMode: String, Codable, CaseIterable, Sendable {
    case followPanel = "Follow the panel", associated = "Agents + associated"
    case agents = "Agent memory", total = "Total memory in use", swap = "Swap used", icon = "Icon only"
    public var title: String {
        switch self { case .agents: "Agents"; case .total: "Memory used"; default: rawValue }
    }
}

public enum WorktreeRescan: Int, Codable, CaseIterable, Sendable {
    case never = 0, halfHour = 1800, twoHours = 7200
    public var title: String {
        switch self { case .never: "Never"; case .halfHour: "Every 30 min"; case .twoHours: "Every 2 h" }
    }
}

/// Validated user preferences. Binary units match Activity Monitor throughout the app.
public struct AppSettings: Codable, Equatable, Sendable {
    public var launchAtLogin = false
    public var openRefresh: RefreshInterval = .ten
    public var backgroundRefresh: BackgroundRefresh = .minute
    public var menuBarMode: MenuBarMode = .followPanel
    public var heroView: HeroView?
    public var showReviewCount = true
    public var groupByFamily: Bool?
    public var hideSystemWorkloads: Bool?
    public var defaultSort: WorkloadSort = .memory
    public var collapsedFamilies: Set<KindFamily> = [.system]
    public var showSwappedBadge = true
    public var swappedSharePercent = 70.0
    public var swappedMinimumMB = 300.0 { didSet { swappedAutomatic = false } }
    public var swappedAutomatic: Bool?
    public var hideBelowMB = 0.0
    public var searchRoots: [String] = []
    public var staleDays = 7
    public var checkPullRequests = false
    public var worktreeRescan: WorktreeRescan = .never
    public var cliOverride: String?
    public var alertsEnabled = false
    public var swapAlert = true
    public var swapGB: Double? { didSet { swapAutomatic = false } }
    public var swapAutomatic: Bool?
    public var criticalPressureAlert: Bool?
    public var worktreeFoldersChosen: Bool?
    public var memoryAlert = true
    public var memoryGB = 8.0 { didSet { memoryAutomatic = false } }
    public var memoryAutomatic: Bool?
    public var notInRAMAlert = true
    public var notInRAMGB = 4.0 { didSet { notInRAMAutomatic = false } }
    public var notInRAMAutomatic: Bool?
    public var onboardingDismissed: Bool?
    public var reviewAlert = true
    public var cooldownMinutes = 30.0

    public init() {}

    /// Clamp all numeric inputs and normalize paths before use or persistence.
    public func validated() -> Self {
        var result = self
        func clamp(_ value: Double, _ range: ClosedRange<Double>, _ fallback: Double) -> Double {
            value.isFinite ? min(range.upperBound, max(range.lowerBound, value)) : fallback
        }
        result.swappedSharePercent = clamp(swappedSharePercent, 0...100, 70)
        result.swappedMinimumMB = clamp(swappedMinimumMB, 0...1_048_576, 300)
        result.hideBelowMB = clamp(hideBelowMB, 0...1_048_576, 0)
        result.staleDays = min(3650, max(1, staleDays))
        result.swapGB = swapGB.map { clamp($0, 0.5...1_048_576, 2) }
        result.swapAutomatic = swapAutomatic
        result.memoryGB = clamp(memoryGB, 0.1...1_048_576, 8)
        result.notInRAMGB = clamp(notInRAMGB, 0.1...1_048_576, 4)
        result.cooldownMinutes = clamp(cooldownMinutes, 0...10080, 30)
        var seen = Set<String>()
        result.searchRoots = searchRoots.map { ($0.trimmingCharacters(in: .whitespacesAndNewlines) as NSString).expandingTildeInPath }
            .filter { $0.hasPrefix("/") && seen.insert($0).inserted }
        result.cliOverride = cliOverride.flatMap {
            let path = ($0.trimmingCharacters(in: .whitespacesAndNewlines) as NSString).expandingTildeInPath
            return path.hasPrefix("/") ? path : nil
        }
        // Validation must not turn an Automatic limit into a custom preference.
        result.memoryAutomatic = memoryAutomatic
        result.notInRAMAutomatic = notInRAMAutomatic
        result.swappedAutomatic = swappedAutomatic
        return result
    }

    public var usesAutomaticSwap: Bool { swapAutomatic ?? (swapGB == nil) }
    public func swapLimitGB(ram: UInt64) -> Double {
        usesAutomaticSwap ? AutomaticThresholds(installedRAMBytes: ram).swapGB : (swapGB ?? 2)
    }
    public var usesAutomaticMemory: Bool { memoryAutomatic ?? (memoryGB == 8) }
    public var usesAutomaticNotInRAM: Bool { notInRAMAutomatic ?? (notInRAMGB == 4) }
    public var usesAutomaticSwapped: Bool { swappedAutomatic ?? (swappedMinimumMB == 300) }

    public func memoryLimitGB(ram: UInt64) -> Double {
        usesAutomaticMemory ? AutomaticThresholds(installedRAMBytes: ram).memoryGB : memoryGB
    }
    public func notInRAMLimitGB(ram: UInt64) -> Double {
        usesAutomaticNotInRAM ? AutomaticThresholds(installedRAMBytes: ram).notInRAMGB : notInRAMGB
    }
    public func swappedLimitMB(ram: UInt64) -> Double {
        usesAutomaticSwapped ? AutomaticThresholds(installedRAMBytes: ram).swappedMinimumMB : swappedMinimumMB
    }

    public var worktreeArguments: [String] {
        ["worktrees", "--json", "--stale-days", String(staleDays)]
            + (worktreeFoldersChosen == true ? ["--no-default-roots"] : [])
            + searchRoots.flatMap { ["--root", $0] } + (checkPullRequests ? ["--pr"] : [])
    }

    public func showsSwappedBadge(for workload: Workload, ram: UInt64 = ProcessInfo.processInfo.physicalMemory) -> Bool {
        guard showSwappedBadge, let missing = workload.notInRamBytes,
              let footprint = workload.footprintBytes, footprint > 0 else { return false }
        return Double(missing) >= swappedLimitMB(ram: ram) * 1_048_576
            && Double(missing) / Double(footprint) * 100 >= swappedSharePercent
    }
}

/// One typed preferences boundary; callers can inject an isolated UserDefaults suite.
@MainActor @Observable
public final class SettingsStore {
    public static let storageKey = "dev.procwho.app.settings.v1"
    private let defaults: UserDefaults
    private let persists: Bool
    public var onChange: ((AppSettings, AppSettings) -> Void)?
    private var storedValue: AppSettings
    public var value: AppSettings {
        get { storedValue }
        set {
            let old = storedValue
            storedValue = newValue.validated()
            if persists, let data = try? JSONEncoder().encode(storedValue) { defaults.set(data, forKey: Self.storageKey) }
            if old != storedValue { onChange?(old, storedValue) }
        }
    }

    public init(defaults: UserDefaults, persists: Bool = true) {
        self.defaults = defaults
        self.persists = persists
        storedValue = defaults.data(forKey: Self.storageKey)
            .flatMap { try? JSONDecoder().decode(AppSettings.self, from: $0) }?.validated() ?? AppSettings()
        if defaults.data(forKey: Self.storageKey) == nil,
           let legacy = RefreshInterval(rawValue: defaults.integer(forKey: "refreshInterval")) {
            storedValue.openRefresh = legacy
        }
    }

    /// Restores preferences only; service registration remains an explicit system action.
    public func reset() { value = AppSettings() }
}
