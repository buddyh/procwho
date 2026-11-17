import Foundation
import Testing
@testable import ProcwhoKit

@Suite struct ReviewPresentationTests {
    @Test func designFixturesCoverReviewStates() throws {
        let status = try ProcwhoJSON.decode(StatusDoc.self, from: fixture("design-review-status"))
        #expect(Set(status.workloads.filter(\.isAgent).map(\.kind)).count == 4)
        #expect(Presentation.heroCaption(status, view: .agents) == "1 session · 48 GB RAM")
        #expect(Presentation.legendSegments(Presentation.composition(status)).count == 5)
        #expect(Presentation.findingDisplayName(status.findings[0], workloads: status.workloads).hasPrefix("procwho · Codex command"))
        #expect(Presentation.findingDisplayName(status.findings[1], workloads: status.workloads) == "First reason explains the missing workload.")
        #expect(status.sourceFailureNotes.isEmpty)
        let worktrees = try ProcwhoJSON.decode(WorktreesDoc.self, from: fixture("design-review-worktrees"))
        #expect(Presentation.legendSegments(Presentation.composition(worktrees)).count == 5)
        #expect(Presentation.composition(worktrees).reduce(0) { $0 + $1.unknownCount } == 2)
    }

    @Test func countsAndInstalledRAM() {
        #expect(Fmt.count(1091) == "1,091")
        #expect(Fmt.count(0) == "0")
        #expect(Fmt.count(Int64(1_234_567)) == "1,234,567")
        #expect(Fmt.installedRAM(48 * 1024 * 1024 * 1024) == "48 GB")
        let doc = StatusDoc(system: SystemInfo(memTotalBytes: 48 * 1024 * 1024 * 1024),
                            totals: Totals(processes: 1091),
                            byKind: [KindSummary(kind: "agent-command", workloads: 35, processes: nil, footprintBytes: nil, cpuPct: nil)])
        #expect(Presentation.heroCaption(doc, view: .agents) == "0 sessions · 48 GB RAM")
        #expect(Presentation.agentWorkloadCount(StatusDoc(workloads: [Workload(key: "a", name: "A", kind: "agent-host")])) == 1)
    }

    @Test func sessionAndHelperGrammar() {
        let session = Workload(key: "s", name: "S", kind: "agent-session")
        let helper = Workload(key: "h", name: "H", kind: "agent-toolset")
        #expect(Presentation.agentCountsCaption(StatusDoc(workloads: [session, helper])) == "1 agent session · 1 helper group")
        #expect(Presentation.agentCountsCaption(StatusDoc(workloads: [helper])) == "1 helper group")
        #expect(Presentation.agentCountsCaption(StatusDoc(workloads: [session, session])) == "2 agent sessions")
        #expect(Presentation.agentCountsCaption(StatusDoc()) == "")
    }

    @Test func legendsOmitZeroAndUnmeasuredValues() {
        let segments = [CompositionSegment(id: "measured", title: "Measured", bytes: 1024, count: 2, unknownCount: 1),
                        CompositionSegment(id: "zero", title: "Zero", bytes: 0, count: 1),
                        CompositionSegment(id: "unknown", title: "Unknown", bytes: 1024, count: 1, unknownCount: 1)]
        #expect(Presentation.legendSegments(segments).map(\.id) == ["measured"])
        #expect(Presentation.legendSegments([]).isEmpty)
    }

    @Test func machinePressureUsesCurrentThresholdsAndLargestFamilies() {
        var status = StatusDoc(system: SystemInfo(ncpu: 8, memTotalBytes: 100, swapUsedBytes: 49, swapTotalBytes: 100, load1: 8),
                               byKind: [KindSummary(kind: "app", workloads: 1, processes: 1, footprintBytes: 10, cpuPct: 0), KindSummary(kind: "agent-session", workloads: 1, processes: 1, footprintBytes: 20, cpuPct: 0)])
        #expect(Presentation.machineAttention(status) == nil)
        status.system?.swapUsedBytes = 51
        #expect(Presentation.machineAttention(status)?.hasPrefix("Swap is high: <1 MB") == true)
        status.system?.load1 = 17
        #expect(Presentation.machineAttention(status)?.contains("Load is high: 17.0") == true)
        status.system?.swapTotalBytes = 0
        #expect(Presentation.machineAttention(status)?.hasPrefix("Load is high") == true)
    }

    @Test func swapTextShowsUsedOnly() {
        #expect(Presentation.swapUsageText(used: 8_804_682_957, allocated: 10_737_418_240) == "8.2 GB")
        #expect(Presentation.swapUsageText(used: 0, allocated: 0) == "0 MB")
        #expect(Presentation.swapUsageText(used: 1_048_576, allocated: 1_073_741_824) == "1 MB")
    }

    @Test func loadRatiosAndStrictThresholds() {
        #expect(Presentation.loadRatio(4, cores: 8) == 0.5)
        #expect(Presentation.loadRatio(24, cores: 8) == 1)
        #expect(Presentation.loadState(8, cores: 8) == .normal)
        #expect(Presentation.loadState(8.1, cores: 8) == .attention)
        #expect(Presentation.loadState(16, cores: 8) == .attention)
        #expect(Presentation.loadState(16.1, cores: 8) == .critical)
        for load: Double? in [nil, .nan, .infinity, -1] {
            #expect(Presentation.loadRatio(load, cores: 8) == nil)
            #expect(Presentation.loadState(load, cores: 8) == .normal)
        }
        #expect(Presentation.loadRatio(1, cores: 0) == nil)
        #expect(Presentation.loadRatio(1, cores: nil) == nil)
    }

    @Test func findingIdentityUsesNameThenFirstReasonLine() throws {
        let finding = try ProcwhoJSON.decode(Finding.self, from: Data(#"{"title":"Needs review","workload_key":"toolset:1:2","reasons":["First reason\nSecond line"]}"#.utf8))
        #expect(Presentation.findingDisplayName(finding, workloads: [Workload(key: "toolset:1:2", name: "Build tools", kind: "agent-toolset")]) == "Build tools")
        #expect(Presentation.findingDisplayName(finding, workloads: []) == "First reason")
        var noReason = finding; noReason.reasons = []
        #expect(Presentation.findingDisplayName(noReason, workloads: []) == "Needs review")
    }

    @Test func totalsPreferFullSummaryAndPreserveUnknowns() {
        let row = Workload(key: "a", name: "A", kind: "agent-session", footprintBytes: 5, cpuPct: 2)
        let doc = StatusDoc(byKind: [KindSummary(kind: "agent-session", workloads: 1000, processes: nil, footprintBytes: 20, cpuPct: 9),
                                    KindSummary(kind: "agent-command", workloads: 5, processes: nil, footprintBytes: 30, cpuPct: 11)], workloads: [row])
        #expect(Presentation.familyTotals(.agents, status: doc).memory == 50)
        #expect(Presentation.familyTotals(.agents, status: doc).cpu == 20)
        #expect(Presentation.familyTotals(.agents, status: StatusDoc(workloads: [row])).memory == 5)
        #expect(Presentation.familyTotals(.services, status: doc).memory == nil)
        #expect(Presentation.familyTotals(.services, status: doc).cpu == nil)
    }

    @Test func optionalMissingToolsAreInformationalAndNamesAreFriendly() throws {
        let data = Data(#"{"sources":{"lsof_ports":false,"tmux":false,"launchctl":false,"notes":["tmux sessions: optional tool not installed"]}}"#.utf8)
        let doc = try ProcwhoJSON.decode(StatusDoc.self, from: data)
        #expect(doc.failedSources == ["launchd jobs", "listening ports"])
        #expect(doc.sourceFailureNotes.isEmpty)
        #expect(doc.sourceNotes == ["tmux sessions: optional tool not installed"])
        #expect(!doc.dataSourceDescriptions.joined().contains("lsof_ports"))
    }

    @Test func staleNotesAreInformationAndFailuresRemainVisible() {
        let doc = StatusDoc(sources: ["stale": .note("Local service service metadata 282s old"), "missing": .ok(false),
                                     "broken": .note("Source failed"), "offline": .note("Source unavailable"), "ok": .ok(true)])
        #expect(doc.failedSources == ["missing"])
        #expect(doc.sourceFailureNotes == ["Source failed", "Source unavailable"])
        #expect(doc.dataSourceDescriptions.contains("Stale: Local service service metadata 282s old"))
        #expect(doc.dataSourceDescriptions.contains("Ok: Available"))
        #expect(doc.dataSourceDescriptions.contains("Missing: Unavailable"))
    }
}
