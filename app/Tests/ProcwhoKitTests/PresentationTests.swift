import Foundation
import Testing
@testable import ProcwhoKit

@Suite struct PresentationTests {
    @Test func familyMapping() {
        #expect(KindFamily(kind: "agent-host") == .agents)
        #expect(KindFamily(kind: "terminal") == .apps)
        #expect(KindFamily(kind: "unknown") == .unknown)
        #expect(KindFamily(kind: "detached") == .leftovers)
        #expect(KindFamily(kind: "custom") == .services)
        #expect(KindFamily.allCases.map(\.title) == ["Agents", "Leftovers", "Unknown owner", "Simulators", "Services", "Apps", "System"])
    }

    @Test func nonresidentMemoryAndBadge() {
        var workload = Workload(key: "test", name: "Test", kind: "app", footprintBytes: 500 * 1024 * 1024)
        #expect(workload.notInRamBytes == nil)
        #expect(!AppSettings().showsSwappedBadge(for: workload, ram: 48 * 1_073_741_824))
        workload.rssBytes = 150 * 1024 * 1024
        #expect(workload.notInRamBytes == Int64(350 * 1024 * 1024))
        #expect(AppSettings().showsSwappedBadge(for: workload, ram: 48 * 1_073_741_824))
        workload.rssBytes = 600 * 1024 * 1024
        #expect(workload.notInRamBytes == 0)
        #expect(!AppSettings().showsSwappedBadge(for: workload, ram: 48 * 1_073_741_824))
    }
}

@Suite struct RankingAndCompositionTests {
    private func row(_ key: String, memory: Int64?, rss: Int64?, cpu: Double?, age: Double?) -> Workload {
        var w = Workload(key: key, name: key, kind: "agent-session", footprintBytes: memory, cpuPct: cpu)
        w.rssBytes = rss
        w.ageSeconds = age
        return w
    }

    @Test func allSortsAndMissingValues() {
        let a = row("a", memory: 1000, rss: 900, cpu: 1, age: 100)
        let b = row("b", memory: 800, rss: 0, cpu: 9, age: 10)
        let c = row("c", memory: nil, rss: nil, cpu: .nan, age: nil)
        #expect(WorkloadSort.memory.sorted([c,b,a]).map(\.key) == ["a","b","c"])
        #expect(WorkloadSort.notInRAM.sorted([c,a,b]).map(\.key) == ["b","a","c"])
        #expect(WorkloadSort.cpu.sorted([c,a,b]).map(\.key) == ["b","a","c"])
        #expect(WorkloadSort.age.sorted([b,c,a]).map(\.key) == ["a","b","c"])
        #expect(WorkloadSort.memory.sorted([b,b,a]).first?.key == "a")
    }

    @Test func stripTotalsUseMeasuredSegmentsNotMachineCapacity() {
        let parts = [CompositionSegment(id: "a", title: "A", bytes: 1_073_741_824, count: 1), CompositionSegment(id: "b", title: "B", bytes: 2_147_483_648, count: 1)]
        #expect(Presentation.stripCaption(parts) == "Share of attributed memory · 3 GB")
        #expect(Presentation.stripCaption(parts, disk: true) == "Share of measured disk · 3 GB")
        #expect(Presentation.stripCaption([]).hasSuffix("0 MB"))
    }

    @Test func sharesAreFiniteAndClamped() {
        #expect(Presentation.share(nil, of: 10) == 0)
        #expect(Presentation.share(5, of: nil) == 0)
        #expect(Presentation.share(5, of: 0) == 0)
        #expect(Presentation.share(-5, of: 10) == 0)
        #expect(Presentation.share(20, of: 10) == 1)
        #expect(Presentation.share(5, of: 10) == 0.5)
        #expect(Presentation.sum([.max, 1, -1]) == .max)
    }

    @Test func kindSummariesWinOverSampledRows() {
        let summaries = [KindSummary(kind: "system", workloads: 2, processes: 5, footprintBytes: 100, cpuPct: 0),
                         KindSummary(kind: "agent-session", workloads: 3, processes: 8, footprintBytes: 200, cpuPct: 0),
                         KindSummary(kind: "agent-toolset", workloads: 4, processes: 9, footprintBytes: 300, cpuPct: 0)]
        let doc = StatusDoc(byKind: summaries, workloads: [row("a", memory: 10, rss: nil, cpu: nil, age: nil)])
        let parts = Presentation.composition(doc)
        #expect(parts.map(\.id) == ["agents", "system"])
        #expect(parts.first?.bytes == 500)
        #expect(parts.first?.count == 7)
        #expect(Presentation.composition(StatusDoc(workloads: doc.workloads)).first?.bytes == 10)
    }

    @Test func unknownDiskAndVerdictOrdering() throws {
        let data = Data(#"{"totals":[{"verdict":"unknown","count":2,"size_bytes":0,"size_unknown":2},{"verdict":"merged-idle","count":3,"size_bytes":1024,"size_unknown":1}]}"#.utf8)
        let doc = try ProcwhoJSON.decoder().decode(WorktreesDoc.self, from: data)
        let parts = Presentation.composition(doc)
        #expect(parts.map(\.id) == ["merged-idle", "unknown"])
        #expect(parts[0].valueLabel == "<1 MB + 1 unknown")
        #expect(parts[1].valueLabel == "Not measured")
    }

    @Test func globalRankAndGroupTotalsIncludeEveryFamily() {
        var agent = row("a", memory: 100, rss: 0, cpu: 1, age: 1)
        var app = row("b", memory: 200, rss: 190, cpu: 2, age: 2); app.kind = "app"
        var service = row("c", memory: 150, rss: 0, cpu: 3, age: 3); service.kind = "service"
        var other = service; other.key = "d"
        let rows = [agent,app,service,other]
        #expect(Presentation.workloads(rows, search: "", family: nil, sort: .memory).map(\.key) == ["b","c","d","a"])
        #expect(Presentation.rankedFamilies(rows, sort: .memory) == [.services,.apps,.agents])
        agent.cpuPct = 100
        #expect(Presentation.rankedFamilies([agent,app,service], sort: .cpu) == [.agents,.services,.apps])
        #expect(Presentation.rankedFamilies([agent,app,service], sort: .notInRAM) == [.services,.agents,.apps])
        #expect(Presentation.rankedFamilies([agent,app,service], sort: .age) == [.services,.apps,.agents])
        let system = Workload(key: "s", name: "System", kind: "system")
        let opaque = Workload(key: "o", name: "Opaque", kind: "system-opaque")
        #expect(Presentation.hiddenSystemCount(StatusDoc(workloads: [system,opaque,agent]), hidden: true) == 2)
        #expect(Presentation.hiddenSystemCount(StatusDoc(workloads: [system]), hidden: false) == 0)
        #expect(Presentation.workloads([system,agent], search: "", family: nil, sort: .memory, hideSystem: true).map(\.key) == ["a"])
    }

    @Test func filtersAndTies() {
        let a = row("a", memory: 10, rss: nil, cpu: 0, age: 0)
        var b = a; b.key = "b"; b.kind = "system"
        #expect(Presentation.workloads([b,a], search: " A ", family: .agents, sort: .memory).map(\.key) == ["a"])
        #expect(Presentation.workloads([b], search: "", family: .agents, sort: .memory).isEmpty)
        #expect(WorkloadSort.memory.sorted([b,a]).map(\.key) == ["a","b"])
    }

    @Test func binaryUnitBoundaries() {
        #expect(Fmt.bytes(-1) == "0 MB")
        #expect(Fmt.bytes(0) == "0 MB")
        #expect(Fmt.bytes(1) == "<1 MB")
        #expect(Fmt.bytes(512 * 1024 - 1) == "<1 MB")
        #expect(Fmt.bytes(512 * 1024) == "<1 MB")
        #expect(Fmt.bytes(1024 * 1024 - 1) == "<1 MB")
        #expect(Fmt.bytes(1024 * 1024) == "1 MB")
        #expect(Fmt.bytes(1024 * 1024 * 1024) == "1 GB")
        #expect(Fmt.bytes(1024 * 1024 * 1024 * 1024) == "1 TB")
    }
}

@Suite struct PresentationBoundaryTests {
    @Test func refreshIntervalAllowlist() {
        #expect(RefreshInterval.allCases.map(\.rawValue) == [5, 10, 30, 60])
        #expect(RefreshInterval(rawValue: 0) == nil)
        #expect(RefreshInterval(rawValue: 7) == nil)
        #expect((RefreshInterval(rawValue: 0) ?? .ten).rawValue == 10)
    }

    @Test func swappedBadgeRequiresBothLimits() {
        let mb: Int64 = 1024 * 1024
        var row = Workload(key: "x", name: "x", kind: "app", footprintBytes: 1000 * mb)
        row.rssBytes = 300 * mb
        #expect(AppSettings().showsSwappedBadge(for: row, ram: 48 * 1_073_741_824))
        row.rssBytes = 300 * mb + 1
        #expect(!AppSettings().showsSwappedBadge(for: row, ram: 48 * 1_073_741_824))
        row.footprintBytes = 300 * mb
        row.rssBytes = 0
        #expect(AppSettings().showsSwappedBadge(for: row, ram: 48 * 1_073_741_824))
        row.footprintBytes = 300 * mb - 1
        #expect(!AppSettings().showsSwappedBadge(for: row, ram: 48 * 1_073_741_824))
    }
}

@Suite struct SizeProvenanceTests {
    @Test func cachedAgeAndRangeAreDistinctFromScanTime() throws {
        let doc = try ProcwhoJSON.decode(WorktreesDoc.self, from: Data(#"{"generated_at":1000,"worktrees":[{"path":"/a","verdict":"active","size_bytes":10,"size_measured_unix":100},{"path":"/b","verdict":"active","size_bytes":20,"size_measured_unix":900}]}"#.utf8))
        #expect(Presentation.sizeAge(doc.worktrees[0], now: 1000) == "measured 15m ago")
        #expect(Presentation.sizeRangeCaption(doc, now: 1000) == "sizes measured between 15m and 1m ago")
        #expect(Presentation.sizeRangeCaption(doc, now: 1000) != nil)
    }
}

@Suite struct WorktreeNavigationTests {
    @Test func idleCaptionSuppressesLiveUseAndKeepsRecentActivityBrief() throws {
        var row = try ProcwhoJSON.decode(Worktree.self, from: Data(#"{"path":"/Users/user/code/example","verdict":"in-use","idle_secs":0}"#.utf8))
        #expect(Presentation.worktreeIdleCaption(row) == nil)
        row.idleSecs = 3600
        #expect(Presentation.worktreeIdleCaption(row) == nil)
        row.verdict = "active"
        row.inUse = true
        #expect(Presentation.worktreeIdleCaption(row) == nil)
        row.inUse = false
        row.idleSecs = 0
        #expect(Presentation.worktreeIdleCaption(row) == "just now")
        row.idleSecs = 59.9
        #expect(Presentation.worktreeIdleCaption(row) == "just now")
        row.idleSecs = 60
        #expect(Presentation.worktreeIdleCaption(row) == "Idle 1m")
        row.idleSecs = 3600
        #expect(Presentation.worktreeIdleCaption(row) == "Idle 1h")
        row.idleSecs = nil
        #expect(Presentation.worktreeIdleCaption(row) == nil)
        row.idleSecs = .infinity
        #expect(Presentation.worktreeIdleCaption(row) == nil)
    }

    @Test func searchSortAndVerdictCompose() throws {
        let doc = try ProcwhoJSON.decode(WorktreesDoc.self, from: Data(#"{"worktrees":[{"path":"/code/z","repo":"zebra","branch":"feature","verdict":"merged-idle","size_bytes":20,"idle_secs":1},{"path":"/code/a","repo":"alpha","branch":"main","verdict":"active","size_bytes":10,"idle_secs":5},{"path":"/code/u","repo":"unknown","verdict":"active","size_bytes":null,"notes":["size: permission denied"]}]}"#.utf8))
        #expect(Presentation.worktrees(doc.worktrees, search: "FEATURE", verdict: "merged-idle", sort: .size).map(\.path) == ["/code/z"])
        #expect(Presentation.worktrees(doc.worktrees, search: "/code/a", verdict: nil, sort: .repo).map(\.path) == ["/code/a"])
        #expect(Presentation.worktrees(doc.worktrees, search: "", verdict: nil, sort: .idle).map(\.path) == ["/code/a","/code/z","/code/u"])
        #expect(Presentation.worktrees(doc.worktrees, search: "", verdict: nil, sort: .repo).first?.repo == "alpha")
        #expect(Presentation.worktrees(doc.worktrees, search: "alpha", verdict: "merged-idle", sort: .size).isEmpty)
        #expect(Presentation.sizingFailure(doc.worktrees[2]) == "Sizing failed: permission denied")
    }
}
