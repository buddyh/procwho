import AppKit
import SwiftUI
import ProcwhoKit

/// Hidden developer entry point: render actual panel views, without menu-bar interaction.
@MainActor
enum SnapshotRenderer {
    struct Options {
        let directory: URL
        var status: URL?
        var why: URL?
        var worktrees: URL?
        var live = false

        init(arguments: [String]) throws {
            guard let index = arguments.firstIndex(of: "--snapshot"), index + 1 < arguments.count, !arguments[index + 1].hasPrefix("--") else {
                throw SnapshotError.usage
            }
            directory = URL(fileURLWithPath: arguments[index + 1], isDirectory: true)
            var cursor = index + 2
            while cursor < arguments.count {
                if arguments[cursor] == "--live" {
                    guard !live else { throw SnapshotError.usage }
                    live = true; cursor += 1; continue
                }
                guard cursor + 1 < arguments.count else { throw SnapshotError.usage }
                let url = URL(fileURLWithPath: arguments[cursor + 1])
                switch arguments[cursor] {
                case "--status": status = url
                case "--why": why = url
                case "--worktrees": worktrees = url
                default: throw SnapshotError.usage
                }
                cursor += 2
            }
            if !live && status == nil && why == nil && worktrees == nil {
                let fixtures = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
                    .deletingLastPathComponent().appendingPathComponent("Tests/Fixtures")
                status = fixtures.appendingPathComponent("screenshot-status.json")
                why = fixtures.appendingPathComponent("screenshot-why.json")
                worktrees = fixtures.appendingPathComponent("screenshot-worktrees.json")
            }
            guard live ? (status == nil && why == nil && worktrees == nil) : (status != nil && why != nil && worktrees != nil) else {
                throw SnapshotError.usage
            }
        }
    }

    enum SnapshotError: Error, CustomStringConvertible {
        case usage, bitmap, png, bundledCLI
        var description: String {
            switch self {
            case .usage: "Usage: ProcwhoBar --snapshot <outDir> --status <fixture> --why <fixture> --worktrees <fixture>\n   or: ProcwhoBar --snapshot <outDir> --live"
            case .bundledCLI: "Live snapshots require the bundled CLI. Run app/scripts/bundle.sh first."
            case .bitmap: "Could not allocate the 2x snapshot bitmap."
            case .png: "Could not encode the snapshot PNG."
            }
        }
    }

    static func run(_ options: Options) async throws {
        let app = NSApplication.shared
        app.setActivationPolicy(.prohibited)
        let status: StatusDoc
        let why: WhyDoc
        let worktrees: WorktreesDoc
        let roots: [String]
        if options.live {
            guard let binary = Bundle.main.resourceURL?.appendingPathComponent("procwho").path,
                  FileManager.default.isExecutableFile(atPath: binary) else { throw SnapshotError.bundledCLI }
            let client = ProcwhoClient(binary: binary)
            status = try await client.status().0
            why = try await client.why(String(ProcessInfo.processInfo.processIdentifier))
            // No PR lookups or user preference changes. Default discovery stays local.
            worktrees = try await client.runner.decode(WorktreesDoc.self, ["worktrees", "--json", "--no-size"], key: "worktrees", timeout: ProcwhoClient.worktreesTimeout)
            let config = try await client.runner.decode(WorktreeSearchConfiguration.self, ["config"], key: "config", timeout: 5)
            roots = config.values.worktreeRoots
        } else {
            guard let statusPath = options.status, let whyPath = options.why, let treesPath = options.worktrees else { throw SnapshotError.usage }
            status = try ProcwhoJSON.decode(StatusDoc.self, from: Data(contentsOf: statusPath))
            why = try ProcwhoJSON.decode(WhyDoc.self, from: Data(contentsOf: whyPath))
            worktrees = try ProcwhoJSON.decode(WorktreesDoc.self, from: Data(contentsOf: treesPath))
            roots = status.worktreeRoots ?? worktrees.searchedFolders ?? []
        }

        try FileManager.default.createDirectory(at: options.directory, withIntermediateDirectories: true)
        let scenarios = ["process-outline", "member-search", "processes-detail", "detail-actions", "worktrees-measured", "older-cli-processes", "project-folders-probing", "project-folders-results", "project-folders-documents", "swap-normal", "swap-attention", "swap-critical", "overview-agents", "overview-memory-used", "pressure-warning", "pressure-critical", "pressure-unavailable", "naming-actions", "naming-confirmation", "overview", "overview-filter", "overview-search", "workload-detail", "review", "review-empty", "worktrees", "loading", "error", "about", "overview-alert", "settings-alerts-enabled", "no-agents", "first-launch", "worktrees-never-scanned", "worktrees-empty", "cli-missing", "glossary", "low-confidence-detail", "overview-not-in-ram", "overview-empty-filter", "worktrees-unmeasured", "stale-error", "overview-grouped", "overview-system", "overview-unknown", "overview-empty-family", "cpu-unsampled", "worktrees-filter", "worktrees-search", "worktrees-cached", "worktrees-expanded", "settings-alerts-denied"] + SettingsTab.allCases.map { "settings-" + $0.rawValue.lowercased() }
        for dark in [false, true] {
            for scenario in scenarios {
                let model = AppModel(snapshotMode: true)
                model.settings.value.onboardingDismissed = scenario != "first-launch"
                model.settings.value.searchRoots = roots
                if scenario == "overview-not-in-ram" { model.sort = .notInRAM }
                if scenario == "overview-alert" {
                    model.settings.value.alertsEnabled = true
                    model.settings.value.swapAlert = false
                    model.settings.value.notInRAMAlert = false
                    model.settings.value.reviewAlert = false
                }
                if scenario == "settings-alerts-enabled" {
                    model.settings.value.alertsEnabled = true
                    model.settings.value.notInRAMAlert = false
                }
                if scenario == "overview-grouped" { model.settings.value.groupByFamily = true }
                if scenario == "overview-system" { model.settings.value.hideSystemWorkloads = false }
                if scenario == "settings-alerts-denied" {
                    model.services.notificationNotRequested = false
                    model.services.notificationDenied = true
                    model.services.notificationStatus = "Notifications: denied in System Settings"
                }
                if scenario == "worktrees-filter" { model.snapshotWorktreeVerdict = "merged-idle" }
                if scenario == "worktrees-search" { model.snapshotWorktreeSearch = worktrees.worktrees.first?.repo ?? "example" }
                if scenario == "worktrees-expanded" { model.snapshotWorktreeExpanded = worktrees.worktrees.first?.path }
                if scenario == "overview-agents" { model.settings.value.heroView = .agents }
                if ["overview-memory-used", "pressure-warning", "pressure-critical"].contains(scenario) { model.settings.value.heroView = .memoryUsed }
                model.snapshotRuleCopied = scenario == "naming-confirmation"
                if scenario.hasPrefix("project-folders-") {
                    model.snapshotFoldersProbing = scenario == "project-folders-probing"
                    model.snapshotFoldersDocumentsSelected = scenario == "project-folders-documents"
                    model.folderSuggestions = ProjectFolders(candidates: [
                        .init(path: "/Users/user/code", repoCount: model.snapshotFoldersProbing ? nil : 12),
                        .init(path: "/Users/user/projects", repoCount: model.snapshotFoldersProbing ? nil : 4),
                        .init(path: "/Users/user/Sites", repoCount: model.snapshotFoldersProbing ? nil : 0),
                        .init(path: "/Users/user/Documents", protected: true),
                        .init(path: "/Users/user/Desktop", protected: true)
                    ], automaticFolders: ["/Users/user/.cursor/worktrees", "/Users/user/.codex/worktrees"])
                }
                var sample = status
                if scenario.hasPrefix("swap-") {
                    sample.system?.swapUsedBytes = 9_556_341_760
                    sample.system?.swapTotalBytes = 10_737_418_240
                    sample.system?.swapDiskFreeBytes = 52 * 1_073_741_824
                    sample.system?.swapDiskTotalBytes = 500 * 1_073_741_824
                    sample.system?.memoryPressure = scenario == "swap-critical" ? "critical" : scenario == "swap-attention" ? "warning" : "normal"
                }
                if scenario == "older-cli-processes" {
                    for index in sample.workloads.indices { sample.workloads[index].members = nil }
                }
                if ["process-outline", "older-cli-processes"].contains(scenario), let first = sample.workloads.first {
                    model.expandedWorkloadKeys.insert(first.key)
                }
                var detail = why
                if scenario == "processes-detail" { model.snapshotDetailSection = "processes" }
                if scenario == "detail-actions" { model.snapshotDetailSection = "detail-actions" }
                var trees: WorktreesDoc? = worktrees
                if scenario == "pressure-warning" { sample.system?.memoryPressure = "warning" }
                if scenario == "pressure-critical" { sample.system?.memoryPressure = "critical" }
                if scenario == "pressure-unavailable" { sample.system?.memoryPressure = nil }
                if scenario.hasPrefix("naming-") {
                    detail.workload.kind = "unknown"
                    detail.workload.name = "Local preview"
                    detail.workload.evidence = ["Owner not identified; inspect the suggested rule before naming."]
                    detail.workload.ifStopped = []
                    detail.workload.confidence = "none"
                    detail.process = nil
                    detail.history = nil
                    detail.reveal = []
                }
                if scenario == "low-confidence-detail" { detail.workload.confidence = "medium" }
                if ["no-agents", "overview-empty-family"].contains(scenario) {
                    sample.workloads.removeAll(where: \.isAgent)
                    sample.byKind.removeAll { $0.kind.hasPrefix("agent-") }
                    sample.findings = []
                    sample.totals = Totals(processes: sample.workloads.compactMap(\.processes).reduce(0, +),
                                           workloads: sample.workloads.count,
                                           footprintBytes: Presentation.sum(sample.byKind.compactMap(\.footprintBytes)))
                }
                if scenario == "cpu-unsampled" { sample.workloads.removeAll { $0.key == detail.workload.key } }
                if scenario == "overview-unknown" { model.familyFilter = "unknown" }
                if scenario == "worktrees-cached", let scan = trees?.generatedAt {
                    for index in trees!.worktrees.indices where trees!.worktrees[index].sizeBytes != nil {
                        trees!.worktrees[index].sizeMeasuredUnix = scan - Double((index + 1) * 600)
                    }
                }
                if scenario == "worktrees-never-scanned" { trees = nil }
                if scenario == "worktrees-measured", var measured = trees {
                    for index in measured.worktrees.indices where measured.worktrees[index].sizeBytes == nil {
                        measured.worktrees[index].sizeBytes = 100 * 1_048_576
                        measured.worktrees[index].sizeMeasuredUnix = measured.generatedAt
                    }
                    for index in measured.totals.indices { measured.totals[index].sizeUnknown = 0 }
                    trees = measured
                }
                if scenario == "worktrees-unmeasured", var unmeasured = trees {
                    for index in unmeasured.totals.indices {
                        unmeasured.totals[index].sizeBytes = nil
                        unmeasured.totals[index].sizeUnknown = unmeasured.totals[index].count
                    }
                    for index in unmeasured.worktrees.indices {
                        unmeasured.worktrees[index].sizeBytes = nil
                        unmeasured.worktrees[index].sizeMeasuredUnix = nil
                    }
                    trees = unmeasured
                }
                if scenario == "worktrees-empty" {
                    trees?.worktrees = []; trees?.totals = []; trees?.worktreesFound = 0
                    trees?.searchedFolders = roots
                }
                if scenario == "review-empty" { sample.findings = [] }
                let tab: Tab = scenario.hasPrefix("review") ? .findings : (scenario.hasPrefix("worktrees") ? .worktrees : .workloads)
                let isEmpty = scenario == "loading" || scenario == "error" || scenario == "cli-missing"
                model.seedSnapshot(status: isEmpty ? nil : sample, why: detail, worktrees: trees, tab: tab,
                                   key: (["workload-detail", "processes-detail", "detail-actions", "low-confidence-detail", "cpu-unsampled"].contains(scenario) || scenario.hasPrefix("naming-")) ? detail.workload.key : nil,
                                   search: scenario == "member-search" ? (sample.workloads.first?.members?.first.map { String($0.pid) } ?? "WebKit") : scenario == "overview-empty-filter" ? "no-matching-workload" : scenario == "overview-search" ? (status.workloads.first?.name.components(separatedBy: " ").first ?? "agent") : "",
                                   family: scenario == "overview-empty-family" ? "agents" : scenario == "overview-unknown" ? "unknown" : scenario == "overview-filter" ? (status.workloads.first(where: \.isAgent).map { KindFamily(kind: $0.kind).rawValue } ?? "services") : nil,
                                   error: ["error", "stale-error"].contains(scenario) ? "The procwho sample timed out. Refresh to try again." : nil,
                                   loading: scenario == "loading", missingCLI: scenario == "cli-missing")
                let name = "\(scenario)-\(dark ? "dark" : "light").png"
                let settingsTab: SettingsTab? = ["settings-alerts-enabled", "settings-alerts-denied"].contains(scenario) ? .alerts : SettingsTab.allCases.first { scenario == "settings-" + $0.rawValue.lowercased() }
                try render(model: model, dark: dark, to: options.directory.appendingPathComponent(name), about: scenario == "about", glossary: scenario == "glossary", settingsTab: settingsTab, projectFolders: scenario.hasPrefix("project-folders-"))
            }
        }
        let settingsBitmap = NSBitmapImageRep(data: try Data(contentsOf: options.directory.appendingPathComponent("settings-general-light.png")))
        let receipt: [String: Any] = [
            "source": options.live ? "Live bundled CLI with scenario overrides" : "Supplied JSON with scenario overrides",
            "installedRAMBytes": status.system?.memTotalBytes ?? 0,
            "searchRoots": roots,
            "generatedUnix": status.generatedUnix ?? 0,
            "workloads": status.workloads.count,
            "processes": status.totals?.processes ?? 0,
            "agentBytes": status.agentFootprintBytes,
            "worktrees": worktrees.worktrees.count,
            "screens": scenarios, "appearances": ["light", "dark"],
            "pixelWidth": Int(Theme.width * Theme.snapshotScale), "pixelHeight": Int(Theme.height * Theme.snapshotScale),
            "aboutPixelHeight": Int(Theme.aboutHeight * Theme.snapshotScale),
            "settingsPixelWidth": Int(Theme.settingsWidth * Theme.snapshotScale),
            "settingsPixelHeight": settingsBitmap?.pixelsHigh ?? 0
        ]
        try JSONSerialization.data(withJSONObject: receipt, options: [.prettyPrinted, .sortedKeys])
            .write(to: options.directory.appendingPathComponent("receipt.json"))
        print("Rendered \(scenarios.count * 2) snapshots to \(options.directory.path)")
    }

    private static func render(model: AppModel, dark: Bool, to path: URL, about: Bool = false, glossary: Bool = false, settingsTab: SettingsTab? = nil, projectFolders: Bool = false) throws {
        let contentSize = NSSize(width: settingsTab == nil ? Theme.width : Theme.settingsWidth,
                                 height: settingsTab == nil ? (about ? Theme.aboutHeight : Theme.height) : Theme.settingsHeight)
        let window = NSWindow(contentRect: NSRect(origin: .zero, size: contentSize),
                              styleMask: settingsTab == nil ? .borderless : [.titled, .closable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
        window.setFrameOrigin(NSPoint(x: -Theme.offscreenOffset, y: Theme.zero))
        let capture: NSView
        if let settingsTab {
            // A standalone hosting window lacks SwiftUI Settings scene's toolbar adapter.
            // Use the equivalent native tab controller and the exact same SwiftUI forms.
            let controller = NSTabViewController()
            controller.tabStyle = .toolbar
            for tab in SettingsTab.allCases {
                let host = NSHostingController(rootView: SettingsTabContent(tab: tab)
                    .environment(model).environment(\.colorScheme, dark ? .dark : .light))
                let item = NSTabViewItem(viewController: host)
                item.label = tab.rawValue
                item.image = NSImage(systemSymbolName: tab.symbol, accessibilityDescription: tab.rawValue)
                controller.addTabViewItem(item)
            }
            controller.selectedTabViewItemIndex = SettingsTab.allCases.firstIndex(of: settingsTab) ?? 0
            window.contentViewController = controller
            window.toolbar?.displayMode = .iconAndLabel
            window.title = "Procwho Settings"
            window.setContentSize(contentSize)
            capture = window.contentView?.superview ?? controller.view
        } else {
            let view = NSHostingView(rootView: Group {
                if projectFolders { ProjectFoldersView() }
                else if about { AboutView(sources: model.status?.dataSourceDescriptions ?? []) }
                else if glossary { GlossaryView() }
                else { ContentView() }
            }.environment(model).environment(\.colorScheme, dark ? .dark : .light)
                // Capture settled content rather than a tab/detail transition.
                .transaction { $0.disablesAnimations = true })
            view.frame = NSRect(origin: .zero, size: contentSize)
            window.contentView = view
            capture = view
        }
        window.orderBack(nil)
        let until = Date().addingTimeInterval(Theme.snapshotSettle)
        while Date() < until { RunLoop.current.run(until: Date().addingTimeInterval(Theme.snapshotTick)) }
        capture.layoutSubtreeIfNeeded()
        window.displayIfNeeded()
        let size = capture.bounds.size
        guard let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(size.width * Theme.snapshotScale),
                                            pixelsHigh: Int(size.height * Theme.snapshotScale), bitsPerSample: 8,
                                            samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                                            bytesPerRow: 0, bitsPerPixel: 0) else { throw SnapshotError.bitmap }
        bitmap.size = size
        window.appearance?.performAsCurrentDrawingAppearance {
            capture.cacheDisplay(in: capture.bounds, to: bitmap)
        }
        guard let data = bitmap.representation(using: .png, properties: [:]) else { throw SnapshotError.png }
        try data.write(to: path)
        window.orderOut(nil)
        window.close()
    }
}
