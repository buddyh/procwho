import AppKit
import Foundation
import IOKit.ps
import Observation
import OSLog
import ProcwhoKit
import UserNotifications

let log = Logger(subsystem: "dev.procwho.app", category: "app")

enum Tab: String, CaseIterable, Identifiable {
    case workloads = "Overview"
    case findings = "Review"
    case worktrees = "Worktrees"
    var id: String { rawValue }
}

/// Owns all polling and CLI state. Created by the App (not by a view), so the menu bar label
/// stays fresh before the window is ever opened.
@MainActor
@Observable
final class AppModel {
    // Polling policy.
    static let closedInterval = 60.0
    static let worktreesMaxAge = 15.0 * 60
    static let lowBatteryPercent = 20

    let settings: SettingsStore
    let services: SettingsServices
    @ObservationIgnored var settingsOpener: (() -> Void)?
    @ObservationIgnored weak var settingsWindow: NSWindow?
    @ObservationIgnored private let notificationDelegate = NotificationDelegate()
    private var alertState = AlertState()
    private(set) var activeAlerts: [WorkloadAlert] = []
    private(set) var binarySource = "Not found"
    var installedRAMBytes: UInt64 {
        snapshotMode ? UInt64(max(0, status?.system?.memTotalBytes ?? 0)) : ProcessInfo.processInfo.physicalMemory
    }
    private(set) var configuredFolders: [String]?
    private var automaticFolders: [String] = []
    @ObservationIgnored private var configurationTask: Task<Void, Never>?
    private var binaryGeneration = 0
    private var pendingWorktreeRefresh = false
    private var worktreesAttempt: Date?
    private var diagnosticsError = "None"
    var refreshInterval: RefreshInterval {
        get { settings.value.openRefresh }
        set { settings.value.openRefresh = newValue }
    }
    func openSettings() {
        guard !snapshotMode else { return }
        services.refreshLogin()
        settings.value.launchAtLogin = services.loginEnabled
        NSApp.activate(ignoringOtherApps: true)
        settingsOpener?()
        DispatchQueue.main.async { [weak self] in
            self?.settingsWindow?.makeKeyAndOrderFront(nil)
            self?.settingsWindow?.orderFrontRegardless()
        }
    }

    let snapshotMode: Bool
    var settingsTab: SettingsTab = .general
    var showingGlossary = false
    var search = ""
    var familyFilter: String?
    var sort: WorkloadSort = .memory
    var expandedWorkloadKeys: Set<String> = []
    var highlightedKey: String?
    var snapshotWorktreeVerdict: String?
    var snapshotDetailSection: String?
    var snapshotRuleCopied = false
    var snapshotWorktreeSearch = ""
    var snapshotWorktreeExpanded: String?
    var searchFocusRequest = 0
    var collapsedFamilies: Set<KindFamily> = [.system]
    var dismissedWarning: String?

    var rankedWorkloads: [Workload] {
        Presentation.workloads((status?.workloads ?? []).filter { settings.value.hideBelowMB == 0 || ($0.footprintBytes.map { Double($0) >= settings.value.hideBelowMB * 1_048_576 } ?? true) }, search: search,
                               family: familyFilter.flatMap(KindFamily.init(rawValue:)), sort: sort, hideSystem: settings.value.hideSystemWorkloads != false)
    }

    var flatWorkloads: Bool { settings.value.groupByFamily != true || !search.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || familyFilter != nil }

    var visibleWorkloads: [Workload] {
        let rows = rankedWorkloads
        if flatWorkloads { return rows }
        return Presentation.rankedFamilies(rows, sort: sort).flatMap { family in
            collapsedFamilies.contains(family) ? [] : rows.filter { KindFamily(kind: $0.kind) == family }
        }
    }

    func moveSelection(_ offset: Int) {
        guard selectedKey == nil, tab == .workloads else { return }
        let rows = visibleWorkloads
        guard !rows.isEmpty else { highlightedKey = nil; return }
        let index = highlightedKey.flatMap { key in rows.firstIndex { $0.key == key } }
        let next = index.map { min(rows.count - 1, max(0, $0 + offset)) } ?? (offset > 0 ? 0 : rows.count - 1)
        highlightedKey = rows[next].key
    }

    func escape() {
        if selectedKey != nil { select(nil) }
        else if !search.isEmpty { search = "" }
        else { familyFilter = nil }
    }

    func switchTab(_ next: Tab) {
        select(nil)
        tab = next
    }

    var warningText: String? {
        var messages: [String] = []
        if let statusError { messages.append(statusError) }
        if let pauseReason { messages.append(pauseReason) }
        if let status {
            if !status.failedSources.isEmpty { messages.append("Sources unavailable: " + status.failedSources.joined(separator: ", ")) }
            messages += status.sourceFailureNotes
        }
        return messages.isEmpty ? nil : messages.joined(separator: ". ")
    }

    // Binary.
    private(set) var binaryPath: String?
    private(set) var triedPaths: [String] = []
    private var client: ProcwhoClient?

    // Status.
    private(set) var status: StatusDoc?
    private(set) var statusMode: ProcwhoClient.StatusMode?
    private(set) var statusError: String?
    private(set) var statusAt: Date?
    private(set) var lastRefreshSeconds: Double?
    private(set) var isRefreshing = false
    private(set) var pauseReason: String?

    // Window and navigation.
    var windowVisible = false {
        didSet { if windowVisible != oldValue { restartLoop() } }
    }
    var tab: Tab = .workloads

    // Detail.
    private(set) var selectedKey: String?
    private(set) var why: WhyDoc?
    private(set) var whyError: String?
    private(set) var whyLoading = false

    var showingProjectFolders = false
    var folderSuggestions: ProjectFolders?
    var folderSuggestionError: String?
    var snapshotFoldersDocumentsSelected = false
    var snapshotFoldersProbing = false
    var folderSuggestionsProbing = false

    func loadFolderSuggestions() async {
        guard !snapshotMode, let client else { return }
        let generation = binaryGeneration
        folderSuggestions = nil
        folderSuggestionError = nil
        do {
            let initial = try await client.runner.run(["worktrees", "--suggest-roots", "--json", "--no-probe"], key: "suggest-roots-initial", timeout: 5)
            guard generation == binaryGeneration, !Task.isCancelled else { return }
            folderSuggestions = try ProcwhoJSON.decode(ProjectFolders.self, from: initial)
            folderSuggestionsProbing = true
            defer { folderSuggestionsProbing = false }
            let data = try await client.runner.run(["worktrees", "--suggest-roots", "--json"], key: "suggest-roots", timeout: 5)
            guard generation == binaryGeneration, !Task.isCancelled else { return }
            folderSuggestions = try ProcwhoJSON.decode(ProjectFolders.self, from: data)
        } catch {
            folderSuggestions = ProjectFolders()
            folderSuggestionError = "Could not check folders. Add a folder to scan."
        }
    }

    func chooseProjectFolders() { showingProjectFolders = true }

    // Worktrees.
    private(set) var worktrees: WorktreesDoc?
    private(set) var worktreesError: String?
    private(set) var worktreesAt: Date?
    private(set) var worktreesScanning = false

    @ObservationIgnored private var loopTask: Task<Void, Never>?
    @ObservationIgnored private var statusTask: Task<Void, Never>?
    @ObservationIgnored private var whyTask: Task<Void, Never>?
    @ObservationIgnored private var worktreesTask: Task<Void, Never>?
    @ObservationIgnored private var lastAttempt: Date?
    @ObservationIgnored private var screensAsleep = false
    @ObservationIgnored private var observers: [NSObjectProtocol] = []

    init(snapshotMode: Bool = false, defaults: UserDefaults? = nil) {
        self.snapshotMode = snapshotMode
        // Snapshots and panel tests use memory-only defaults and never read real preferences.
        settings = SettingsStore(defaults: defaults ?? (snapshotMode ? UserDefaults(suiteName: "dev.procwho.app.snapshot.\(UUID())")! : .standard), persists: !snapshotMode)
        services = SettingsServices(snapshotMode: snapshotMode)
        sort = settings.value.defaultSort
        collapsedFamilies = settings.value.collapsedFamilies
        resolveBinary()
        settings.onChange = { [weak self] old, new in self?.applySettings(old: old, new: new) }
    }

    private func applySettings(old: AppSettings, new: AppSettings) {
        if old.defaultSort != new.defaultSort { sort = new.defaultSort }
        if old.collapsedFamilies != new.collapsedFamilies { collapsedFamilies = new.collapsedFamilies }
        if old.cliOverride != new.cliOverride {
            binaryGeneration += 1
            statusTask?.cancel(); whyTask?.cancel(); worktreesTask?.cancel()
            resolveBinary()
            lastAttempt = nil
        }
        if old.openRefresh != new.openRefresh || old.backgroundRefresh != new.backgroundRefresh || old.cliOverride != new.cliOverride {
            restartLoop()
        }
        if old.worktreeArguments != new.worktreeArguments || old.worktreeRescan != new.worktreeRescan || old.cliOverride != new.cliOverride {
            if new.worktreeFoldersChosen == true { refreshWorktrees() }
        }
        if let status {
            // Preference edits update visible conditions immediately; only fresh samples notify.
            activeAlerts = AlertEngine.evaluate(previous: alertState, status: status, settings: new, now: Date(), installedRAMBytes: installedRAMBytes).active
        }
        StatusItemTooltip.apply(tooltipText)
    }

    private func resolveBinary() {
        if snapshotMode {
            binaryPath = "/Applications/Procwho.app/Contents/Resources/procwho"
            binarySource = "Bundled CLI"
            return
        }
        let candidates = settings.value.cliOverride.map { [$0] } ?? BinaryLocator.defaultCandidates()
        triedPaths = candidates
        binaryPath = BinaryLocator.locate(candidates) { FileManager.default.isExecutableFile(atPath: $0) }
        client = binaryPath.map(ProcwhoClient.init(binary:))
        configurationTask?.cancel()
        configuredFolders = nil
        let generation = binaryGeneration
        if let client {
            configurationTask = Task { [weak self] in
                guard let data = try? await client.runner.run(["config"], key: "config", timeout: 5),
                      let config = try? ProcwhoJSON.decode(WorktreeSearchConfiguration.self, from: data),
                      let self, generation == binaryGeneration else { return }
                automaticFolders = config.worktreeScan.containers.map(\.path)
                configuredFolders = config.folders(roots: settings.value.searchRoots)
            }
        }
        if settings.value.cliOverride != nil { binarySource = "User override" }
        else if binaryPath == ProcessInfo.processInfo.environment["PROCWHO_BIN"] { binarySource = "PROCWHO_BIN environment" }
        else if binaryPath == Bundle.main.resourceURL?.appendingPathComponent("procwho").path { binarySource = "Bundled CLI" }
        else { binarySource = binaryPath == nil ? "Not found" : "Local installation or development checkout" }
    }

    func resetSettings() {
        if services.loginEnabled { services.setLogin(false) }
        settings.reset()
        sort = settings.value.defaultSort
        collapsedFamilies = settings.value.collapsedFamilies
        settings.value.launchAtLogin = services.loginEnabled
    }

    func useBundledCLI() {
        guard !snapshotMode else { return }
        // Explicitly prefer the bundle even when PROCWHO_BIN is set.
        let bundled = Bundle.main.resourceURL?.appendingPathComponent("procwho").path
        if ProcessInfo.processInfo.environment["PROCWHO_BIN"] != nil { settings.value.cliOverride = bundled }
        else { settings.value.cliOverride = nil }
    }

    func diagnostics() async -> String {
        var version = "Not available"
        if !snapshotMode, let client,
           let data = try? await client.runner.run(["--version"], key: "version", timeout: 5) {
            // Version commands should only produce a version, never include arbitrary stdout.
            let raw = String(decoding: data, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
            if raw.range(of: #"^procwho [0-9]+\.[0-9]+\.[0-9]+(?:[-+][A-Za-z0-9.-]+)?$"#, options: .regularExpression) != nil { version = raw }
        }
        return "Procwho: \(AppVersion.current)\nCLI: \(version)\nmacOS: \(ProcessInfo.processInfo.operatingSystemVersionString)\nApp: \(snapshotMode ? "/Applications/Procwho.app" : Bundle.main.bundlePath)\nCLI path: \(binaryPath ?? "Not found")\nCLI source: \(binarySource)\nLast error: \(diagnosticsError)"
    }

    // MARK: lifecycle

    func start() {
        guard !snapshotMode, loopTask == nil else { return }
        log.notice("start; binary=\(self.binaryPath ?? "none", privacy: .public)")
        let ws = NSWorkspace.shared.notificationCenter
        observers.append(ws.addObserver(forName: NSWorkspace.screensDidSleepNotification, object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.screensAsleep = true }
        })
        observers.append(ws.addObserver(forName: NSWorkspace.screensDidWakeNotification, object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.screensAsleep = false
                self?.restartLoop()
            }
        })
        observers.append(NotificationCenter.default.addObserver(forName: NSApplication.willTerminateNotification, object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.stop() }
        })
        if Bundle.main.bundleURL.pathExtension == "app" {
            UNUserNotificationCenter.current().delegate = notificationDelegate
        }
        restartLoop()
    }

    /// Cancels everything; cancelling a task terminates the CLI child it is waiting on.
    func stop() {
        loopTask?.cancel()
        statusTask?.cancel()
        whyTask?.cancel()
        worktreesTask?.cancel()
        configurationTask?.cancel()
        loopTask = nil
    }

    private var interval: Double { windowVisible ? Double(refreshInterval.rawValue) : Double(settings.value.backgroundRefresh.rawValue) }

    private func restartLoop() {
        loopTask?.cancel()
        guard !snapshotMode, client != nil else { loopTask = nil; return }
        loopTask = Task { [weak self] in
            while !Task.isCancelled {
                guard let self else { return }
                let wait = await self.tick()
                try? await Task.sleep(for: .seconds(wait))
            }
        }
    }

    /// One loop step. Returns how long to sleep before the next step.
    private func tick() async -> Double {
        pauseReason = currentPauseReason()
        if pauseReason != nil { return Self.closedInterval }
        let rescan = Double(settings.value.worktreeRescan.rawValue)
        if settings.value.worktreeFoldersChosen == true, rescan > 0, worktreesAttempt.map({ Date().timeIntervalSince($0) >= rescan }) ?? true { refreshWorktrees() }
        if interval == 0 { return Self.closedInterval }
        let due = lastAttempt.map { Date().timeIntervalSince($0) >= interval - 0.5 } ?? true
        if due { await refreshStatus() }
        let since = lastAttempt.map { Date().timeIntervalSince($0) } ?? 0
        return max(1, min(interval - since, 60))
    }

    private func currentPauseReason() -> String? {
        if screensAsleep { return "Paused while the display sleeps." }
        if let pct = Self.batteryPercentWhenDischarging(), pct < Self.lowBatteryPercent {
            return "Paused: on battery at \(pct)%."
        }
        return nil
    }

    /// Battery percent when running on battery power; nil when on AC or no battery.
    static func batteryPercentWhenDischarging() -> Int? {
        guard let info = IOPSCopyPowerSourcesInfo()?.takeRetainedValue(),
              let list = IOPSCopyPowerSourcesList(info)?.takeRetainedValue() as? [CFTypeRef] else { return nil }
        for ps in list {
            guard let d = IOPSGetPowerSourceDescription(info, ps)?.takeUnretainedValue() as? [String: Any],
                  d[kIOPSTypeKey] as? String == kIOPSInternalBatteryType,
                  d[kIOPSPowerSourceStateKey] as? String == kIOPSBatteryPowerValue,
                  let cur = d[kIOPSCurrentCapacityKey] as? Int,
                  let max = d[kIOPSMaxCapacityKey] as? Int, max > 0 else { continue }
            return cur * 100 / max
        }
        return nil
    }

    // MARK: status

    /// Refreshes status. A call while a run is in flight joins it instead of starting another.
    func refreshStatus() async {
        if let running = statusTask {
            await running.value
            return
        }
        let t = Task { await self.performStatus() }
        statusTask = t
        await t.value
        statusTask = nil
    }

    func refreshNow() {
        guard !snapshotMode else { return }
        Task { await refreshStatus() }
        if tab == .worktrees { refreshWorktrees() }
    }

    private func performStatus() async {
        guard let client else { return }
        lastAttempt = Date()
        isRefreshing = true
        defer { isRefreshing = false }
        let generation = binaryGeneration
        let started = Date()
        log.notice("status run start")
        do {
            let (doc, mode) = try await client.status()
            guard generation == binaryGeneration else { return }
            status = doc
            statusMode = mode
            statusError = nil
            statusAt = Date()
            diagnosticsError = "None"
            let evaluation = AlertEngine.evaluate(previous: alertState, status: doc, settings: settings.value, now: Date(), installedRAMBytes: installedRAMBytes)
            alertState = evaluation.state
            activeAlerts = evaluation.active
            await services.post(evaluation.notifications)
            lastRefreshSeconds = Date().timeIntervalSince(started)
            log.notice("status run ok mode=\(mode.rawValue, privacy: .public) workloads=\(doc.workloads.count) processes=\(doc.totals?.processes ?? 0) secs=\(Date().timeIntervalSince(started), privacy: .public)")
        } catch CLIError.cancelled {
            log.notice("status run cancelled")
        } catch {
            guard generation == binaryGeneration else { return }
            statusError = String(describing: error)
            diagnosticsError = "Status CLI request failed"
            log.error("status run failed: \(String(describing: error), privacy: .public)")
        }
        StatusItemTooltip.apply(tooltipText)
    }

    /// Seeds deterministic offscreen rendering without starting CLI work or polling.
    func seedSnapshot(status: StatusDoc?, why: WhyDoc?, worktrees: WorktreesDoc?, tab: Tab,
                      key: String? = nil, search: String = "", family: String? = nil,
                      error: String? = nil, loading: Bool = false, missingCLI: Bool = false) {
        precondition(snapshotMode)
        self.status = status
        if missingCLI { binaryPath = nil }
        triedPaths = ["/Applications/Procwho.app/Contents/Resources/procwho", "/Users/user/.local/bin/procwho"]
        if let status { activeAlerts = AlertEngine.evaluate(previous: .init(), status: status, settings: settings.value, now: Date(), installedRAMBytes: installedRAMBytes).active }
        self.why = why
        self.worktrees = worktrees
        self.tab = tab
        self.selectedKey = key
        self.search = search
        familyFilter = family
        statusError = error
        isRefreshing = loading
        statusAt = status == nil ? nil : Date().addingTimeInterval(-4)
        worktreesAt = worktrees == nil ? nil : Date().addingTimeInterval(-30)
    }

    // MARK: label

    var labelText: String {
        if settings.value.menuBarMode == .icon { return "" }
        guard let status else { return binaryPath == nil ? "?" : "..." }
        switch settings.value.menuBarMode {
        case .followPanel: return Fmt.bytes(Presentation.heroMemory(status, view: settings.value.heroView ?? .associated))
        case .associated: return Fmt.bytes(Presentation.heroMemory(status, view: .associated))
        case .agents: return Fmt.bytes(status.agentFootprintBytes)
        case .total: return Fmt.bytes(status.system?.memUsedBytes)
        case .swap: return Fmt.bytes(status.system?.swapUsedBytes)
        case .icon: return ""
        }
    }

    var reviewCount: Int { settings.value.showReviewCount ? (status?.reviewFindingCount ?? 0) : 0 }

    var tooltipText: String {
        guard let s = status else { return statusError ?? "procwho: no data yet" }
        let selected: HeroView = switch settings.value.menuBarMode {
        case .agents: .agents
        case .associated: .associated
        case .total: .memoryUsed
        default: settings.value.heroView ?? .associated
        }
        let value = "\(selected.effective(for: s).title): \(Fmt.bytes(Presentation.heroMemory(s, view: selected)))."
        return value + (statusAt.map { " Updated \(Fmt.age(Date().timeIntervalSince($0))) ago." } ?? "")
    }

    // MARK: detail

    func select(_ key: String?) {
        selectedKey = key
        why = nil
        whyError = nil
        let previous = whyTask
        previous?.cancel()
        guard !snapshotMode, let key, let client else { whyLoading = false; return }
        // Prefer the root PID when the status row has one; otherwise the workload key.
        let target = status?.workloads.first { $0.key == key }?.rootPid.map(String.init) ?? key
        whyLoading = true
        whyTask = Task {
            await previous?.value
            do {
                let doc = try await client.why(target)
                guard selectedKey == key else { return }
                why = doc
            } catch CLIError.cancelled {
                return
            } catch {
                guard selectedKey == key else { return }
                whyError = String(describing: error)
                diagnosticsError = "Why CLI request failed"
            }
            if selectedKey == key { whyLoading = false }
        }
    }

    func workload(forKey key: String) -> Workload? {
        status?.workloads.first { $0.key == key }
    }

    // MARK: worktrees

    /// Capacity is scoped to the home volume, as in the CLI's status collector.
    var diskVolumeName: String {
        if snapshotMode { return "Example disk" }
        return (try? URL(fileURLWithPath: NSHomeDirectory()).resourceValues(forKeys: [.volumeNameKey]).volumeName) ?? "Home volume"
    }

    var worktreeFoldersText: String {
        let folders = worktrees?.searchedFolders ?? (settings.value.worktreeFoldersChosen == true ? settings.value.searchRoots + automaticFolders : configuredFolders)
        let names = folders ?? (settings.value.searchRoots.isEmpty ? [] : settings.value.searchRoots)
        if names.isEmpty, settings.value.worktreeFoldersChosen == true { return "Automatic sources only" }
        return names.isEmpty ? "Default project folders" : names.map {
            $0.replacingOccurrences(of: snapshotMode ? "/Users/user" : NSHomeDirectory(), with: "~")
        }.joined(separator: ", ")
    }

    func refreshWorktrees() {
        guard !snapshotMode, let client else { return }
        guard settings.value.worktreeFoldersChosen == true else { chooseProjectFolders(); return }
        if worktreesScanning { pendingWorktreeRefresh = true; return }
        worktreesScanning = true
        worktreesAttempt = Date()
        worktreesTask = Task {
            defer {
                worktreesScanning = false
                if pendingWorktreeRefresh { pendingWorktreeRefresh = false; refreshWorktrees() }
            }
            let scanSettings = settings.value
            let generation = binaryGeneration
            let started = Date()
            log.notice("worktrees run start")
            do {
                let result = try await client.worktrees(settings: scanSettings)
                guard generation == binaryGeneration else { return }
                worktrees = result
                worktreesAt = Date()
                worktreesError = nil
                log.notice("worktrees run ok secs=\(Date().timeIntervalSince(started), privacy: .public)")
            } catch CLIError.cancelled {
                return
            } catch {
                worktreesError = String(describing: error)
                diagnosticsError = "Worktrees CLI request failed"
            }
        }
    }
}

/// MenuBarExtra has no tooltip API; set it on the status bar button directly. Harmless if the
/// button is not found.
@MainActor
enum StatusItemTooltip {
    static func apply(_ text: String) {
        for window in NSApp?.windows ?? [] where String(describing: type(of: window)).contains("StatusBar") {
            if let button = findButton(in: window.contentView) { button.toolTip = text }
        }
    }

    private static func findButton(in view: NSView?) -> NSButton? {
        guard let view else { return nil }
        if let b = view as? NSButton { return b }
        for sub in view.subviews { if let b = findButton(in: sub) { return b } }
        return nil
    }
}
