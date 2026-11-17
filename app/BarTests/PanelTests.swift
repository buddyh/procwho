import AppKit
import Foundation
import Testing
import ProcwhoKit
@testable import ProcwhoBar

@Suite @MainActor struct PanelTests {
    private func model() -> AppModel {
        let model = AppModel(snapshotMode: true)
        let rows = [Workload(key: "system", name: "System", kind: "system", footprintBytes: 1000),
                    Workload(key: "agent", name: "Agent", kind: "agent-session", footprintBytes: 500),
                    Workload(key: "service", name: "Service", kind: "service", footprintBytes: 100)]
        model.seedSnapshot(status: StatusDoc(workloads: rows), why: nil, worktrees: nil, tab: .workloads)
        return model
    }

    @Test func arrowsSkipCollapsedFamiliesAndRespectVisibleRanking() {
        let model = model()
        #expect(PanelKeyboard.handle(keyCode: 125, characters: nil, modifiers: [.function, .numericPad], model: model))
        #expect(model.highlightedKey == "agent")
        model.moveSelection(1)
        #expect(model.highlightedKey == "service")
        model.moveSelection(1)
        #expect(model.highlightedKey == "service")
        model.moveSelection(-1)
        #expect(model.highlightedKey == "agent")
        model.search = "system"
        #expect(model.rankedWorkloads.isEmpty)
        model.settings.value.hideSystemWorkloads = false
        model.moveSelection(1)
        #expect(model.highlightedKey == "system")
    }

    @Test func flatRankingAndGroupedOrderingUseSameMetric() {
        let model = model()
        #expect(model.flatWorkloads)
        model.settings.value.hideSystemWorkloads = false
        #expect(model.visibleWorkloads.map(\.key) == ["system", "agent", "service"])
        model.settings.value.groupByFamily = true
        #expect(!model.flatWorkloads)
        model.collapsedFamilies = []
        #expect(model.visibleWorkloads.map(\.key) == ["system", "agent", "service"])
    }

    @Test func returnAndEscapePreserveOriginatingTab() {
        let model = model()
        #expect(PanelKeyboard.handle(keyCode: 36, characters: "\r", modifiers: [], model: model))
        #expect(model.selectedKey == "agent")
        #expect(PanelKeyboard.handle(keyCode: 53, characters: nil, modifiers: [], model: model))
        #expect(model.selectedKey == nil)
        model.tab = .findings
        model.select("agent")
        model.escape()
        #expect(model.tab == .findings)
        #expect(model.selectedKey == nil)
        model.search = "query"; model.familyFilter = "agents"
        model.escape()
        #expect(model.search.isEmpty)
        #expect(model.familyFilter == "agents")
        model.escape()
        #expect(model.familyFilter == nil)
    }

    @Test func disclosureKeysPreserveExpansionAcrossRefreshAndNavigation() {
        let model = model()
        model.highlightedKey = "agent"
        #expect(PanelKeyboard.handle(keyCode: 124, characters: nil, modifiers: [], model: model))
        #expect(model.expandedWorkloadKeys == ["agent"])
        model.select("agent"); model.escape()
        #expect(model.expandedWorkloadKeys == ["agent"])
        model.seedSnapshot(status: model.status, why: nil, worktrees: nil, tab: .workloads)
        #expect(model.expandedWorkloadKeys == ["agent"])
        #expect(PanelKeyboard.handle(keyCode: 123, characters: nil, modifiers: [], model: model))
        #expect(model.expandedWorkloadKeys.isEmpty)
        model.highlightedKey = "system"
        #expect(!PanelKeyboard.handle(keyCode: 124, characters: nil, modifiers: [], model: model))
    }

    @Test func commandTabsSearchAndUnhandledEditingKeys() {
        let model = model()
        #expect(PanelKeyboard.handle(keyCode: 0, characters: "2", modifiers: .command, model: model))
        #expect(model.tab == .findings)
        #expect(PanelKeyboard.handle(keyCode: 0, characters: "3", modifiers: .command, model: model))
        #expect(model.tab == .worktrees)
        #expect(PanelKeyboard.handle(keyCode: 0, characters: "f", modifiers: .command, model: model))
        #expect(model.tab == .workloads)
        #expect(model.searchFocusRequest == 1)
        #expect(PanelKeyboard.handle(keyCode: 0, characters: "1", modifiers: .command, model: model))
        #expect(!PanelKeyboard.handle(keyCode: 0, characters: "x", modifiers: .command, model: model))
        #expect(!PanelKeyboard.handle(keyCode: 0, characters: "a", modifiers: [], model: model))
        #expect(!PanelKeyboard.handle(keyCode: 125, characters: nil, modifiers: .shift, model: model))
    }

    @Test func returnNeverOpensSelectionHiddenByFilter() {
        let model = model()
        model.highlightedKey = "agent"
        model.familyFilter = "services"
        #expect(PanelKeyboard.handle(keyCode: 36, characters: "\r", modifiers: [], model: model))
        #expect(model.selectedKey == "service")
    }

    @Test func snapshotModeNeverStartsRefreshesOrChangesPreferences() {
        let model = model()
        let previous = UserDefaults.standard.object(forKey: "refreshInterval") as? Int
        model.refreshInterval = .sixty
        model.start()
        model.refreshNow()
        model.refreshWorktrees()
        #expect(!model.isRefreshing)
        #expect(!model.worktreesScanning)
        #expect(UserDefaults.standard.object(forKey: "refreshInterval") as? Int == previous)
        #expect(model.collapsedFamilies == [.system])
    }

    @Test func menuLabelUsesBinaryGBUnits() {
        let model = AppModel(snapshotMode: true)
        let row = Workload(key: "agent", name: "Agent", kind: "agent-session", footprintBytes: 2 * 1024 * 1024 * 1024)
        model.seedSnapshot(status: StatusDoc(workloads: [row]), why: nil, worktrees: nil, tab: .workloads)
        #expect(model.labelText == "2 GB")
        #expect(model.reviewCount == 0)
    }

    @Test func defaultMenuFallsBackToPhysicalUseAndWorktreesWaitForUser() {
        let model = AppModel(snapshotMode: true)
        model.seedSnapshot(status: StatusDoc(system: SystemInfo(memTotalBytes: 8 * 1_073_741_824, memUsedBytes: 2 * 1_073_741_824),
                                             totals: Totals(footprintBytes: 3 * 1_073_741_824)), why: nil, worktrees: nil, tab: .worktrees)
        #expect(model.labelText == "2 GB")
        #expect(model.tooltipText.contains("Memory used"))
        #expect(!model.worktreesScanning)
    }

    @Test func staleSourceMetadataDoesNotRaiseAnAttentionRow() {
        let model = AppModel(snapshotMode: true)
        model.seedSnapshot(status: StatusDoc(sources: ["dev-service": .note("Local service service metadata 282s old")]),
                           why: nil, worktrees: nil, tab: .workloads)
        #expect(model.warningText == nil)
        model.seedSnapshot(status: StatusDoc(sources: ["lsof": .ok(false)]),
                           why: nil, worktrees: nil, tab: .workloads)
        #expect(model.warningText == "Sources unavailable: lsof")
        model.seedSnapshot(status: nil, why: nil, worktrees: nil, tab: .workloads, error: "Sample failed")
        #expect(model.warningText == "Sample failed")
    }

    @Test func snapshotMachineValuesComeFromInput() throws {
        let model = AppModel(snapshotMode: true)
        let status = try ProcwhoJSON.decode(StatusDoc.self, from: Data(#"{"system":{"mem_total_bytes":8589934592},"worktree_roots":["/Users/user/custom projects"]}"#.utf8))
        model.settings.value.searchRoots = status.worktreeRoots ?? []
        model.seedSnapshot(status: status, why: nil, worktrees: nil, tab: .workloads)
        #expect(model.installedRAMBytes == 8_589_934_592)
        #expect(model.worktreeFoldersText == "~/custom projects")
        #expect(AutomaticThresholds(installedRAMBytes: model.installedRAMBytes).memoryGB == 2)
    }

    @Test func snapshotOptionsValidateAndPreservePaths() throws {
        let options = try SnapshotRenderer.Options(arguments: ["ProcwhoBar", "--snapshot", "/tmp/procwho snapshots", "--status", "/tmp/status.json", "--why", "/tmp/why.json", "--worktrees", "/tmp/trees.json"])
        #expect(options.directory.path == "/tmp/procwho snapshots")
        #expect(options.status?.lastPathComponent == "status.json")
        #expect(options.why?.lastPathComponent == "why.json")
        #expect(options.worktrees?.lastPathComponent == "trees.json")
        #expect(try SnapshotRenderer.Options(arguments: ["ProcwhoBar", "--snapshot", "/tmp/out", "--live"]).live)
        let defaults = try SnapshotRenderer.Options(arguments: ["ProcwhoBar", "--snapshot", "/tmp/out"])
        #expect(defaults.status?.lastPathComponent == "screenshot-status.json")
        for args in [["ProcwhoBar"], ["ProcwhoBar", "--snapshot", "/tmp/out", "--live", "--status", "/tmp/status.json"]] {
            #expect(throws: SnapshotRenderer.SnapshotError.self) { try SnapshotRenderer.Options(arguments: args) }
        }
        #expect(throws: SnapshotRenderer.SnapshotError.self) { try SnapshotRenderer.Options(arguments: ["ProcwhoBar", "--snapshot"]) }
        #expect(throws: SnapshotRenderer.SnapshotError.self) { try SnapshotRenderer.Options(arguments: ["ProcwhoBar", "--snapshot", "/tmp/out", "--unknown", "x"]) }
    }
}
