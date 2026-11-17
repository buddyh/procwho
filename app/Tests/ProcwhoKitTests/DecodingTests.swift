import Foundation
import Testing
@testable import ProcwhoKit

func fixture(_ name: String) throws -> Data {
    let url = try #require(Bundle.module.url(forResource: name, withExtension: "json", subdirectory: "Fixtures"))
    return try Data(contentsOf: url)
}

@Suite struct StatusDecoding {
    @Test func specFixtureDecodes() throws {
        let doc = try ProcwhoJSON.decode(StatusDoc.self, from: fixture("status"))
        #expect(doc.schemaVersion == 3)
        #expect(doc.system?.ncpu == 12)
        #expect(doc.system?.load1 == 3.2)
        #expect(doc.totals?.unreadableProcesses == 216)
        #expect(doc.byKind.count == 3)
        // The row without a key is skipped, not fatal.
        #expect(doc.workloads.map(\.key) == ["session:4201:1001", "detached:node"])
        let w = doc.workloads[0]
        #expect(w.accounting == "exclusive")
        #expect(w.rootPid == 8689)
        #expect(w.projectPath == "/Users/user/repos/web-app")
        #expect(w.ports == ["3001"])
        #expect(w.ageSeconds == 3600)
        #expect(!w.weakConfidence)
        #expect(doc.workloads[1].weakConfidence)
        // A numeric port is dropped rather than failing the document.
        #expect(doc.workloads[1].ports.isEmpty)
        #expect(doc.findings.count == 2)
        #expect(doc.findings[0].workloadKey == "session:4201:1001")
        #expect(doc.findings[0].notProven == ["..."])
        #expect(doc.reviewFindingCount == 1)
        #expect(doc.failedSources == ["listening ports"])
        #expect(doc.sourceNotes == ["Local service service metadata 12s old"])
    }

    @Test func agentFootprintUsesAgentKindsOnly() throws {
        let doc = try ProcwhoJSON.decode(StatusDoc.self, from: fixture("status"))
        #expect(doc.agentFootprintBytes == 25_125_558_681 + 1_073_741_824)
        #expect(Fmt.bytesCompact(doc.agentFootprintBytes) == "24.4G")
    }

    @Test func realStatusDecodes() throws {
        let doc = try ProcwhoJSON.decode(StatusDoc.self, from: fixture("status-real"))
        #expect(doc.schemaVersion == 3)
        #expect(doc.workloads.count == 3)
        #expect(doc.workloads.allSatisfy { $0.rootPid != nil })
        #expect(!doc.byKind.isEmpty)
        #expect(doc.system?.memTotalBytes ?? 0 > 0)
        #expect(doc.sources?["launchctl"] == .ok(true))
    }

    @Test func minimalDocumentDecodes() throws {
        let doc = try ProcwhoJSON.decode(StatusDoc.self, from: Data("{}".utf8))
        #expect(doc.workloads.isEmpty)
        #expect(doc.findings.isEmpty)
        #expect(doc.agentFootprintBytes == 0)
    }
}

@Suite struct FallbackDecoding {
    @Test func realTopDecodes() throws {
        let top = try ProcwhoJSON.decode(TopDoc.self, from: fixture("top"))
        #expect(top.workloads.count == 3)
        #expect(top.processes ?? 0 > 0)
        #expect(top.workloads.contains { !$0.evidence.isEmpty })
        #expect(top.workloads.contains { !$0.ifStopped.isEmpty })
    }

    @Test func realFindingsDecodeWithWorkloadKey() throws {
        let list = try ProcwhoJSON.decode(FindingsDoc.self, from: fixture("findings")).findings
        #expect(list.count == 3)
        #expect(list.filter(\.isReview).count == 2)
        // `findings --json` names the key `workload`.
        #expect(list[0].workloadKey != nil)
        #expect(!list[0].notProven.isEmpty)
    }

    @Test func composeGroupsByKind() throws {
        let top = try ProcwhoJSON.decode(TopDoc.self, from: fixture("top"))
        let findings = try ProcwhoJSON.decode(FindingsDoc.self, from: fixture("findings")).findings
        let doc = StatusDoc.compose(top: top, findings: findings, ncpu: 8, memTotal: 1 << 34)
        #expect(doc.system?.ncpu == 8)
        #expect(doc.byKind.reduce(0) { $0 + ($1.workloads ?? 0) } == top.workloads.count)
        let sum = top.workloads.reduce(Int64(0)) { $0 + ($1.footprintBytes ?? 0) }
        #expect(doc.byKind.reduce(Int64(0)) { $0 + ($1.footprintBytes ?? 0) } == sum)
        #expect(doc.reviewFindingCount == 2)
    }
}

@Suite struct DetailDecoding {
    @Test func realWhyDecodes() throws {
        let why = try ProcwhoJSON.decode(WhyDoc.self, from: fixture("why"))
        #expect(why.workload.kind == "agent-session")
        #expect(why.workload.cpuPct == nil)
        #expect(why.workload.cpuSampleState == "not_sampled")
        #expect(why.process?.pid ?? 0 > 0)
        #expect(why.process?.ancestry.isEmpty == false)
        #expect(why.reveal.count == 2)
        #expect(why.reveal[0].openablePath?.hasPrefix("/Users/user/") == true)
        #expect(why.reveal[1].openablePath == nil)
        #expect(why.readOnly != nil)
    }

    @Test func inclusiveTotalsObject() throws {
        let json = #"{"workload":{"key":"k","name":"n","kind":"app"},"totals_inclusive":{"accounting":"inclusive","nested_workloads":2,"footprint_bytes":1024,"processes":7},"history":{"samples":3,"span_min":2.5,"footprint_first_bytes":1,"footprint_last_bytes":2,"avg_cpu_pct":null}}"#
        let why = try ProcwhoJSON.decode(WhyDoc.self, from: Data(json.utf8))
        #expect(why.inclusive?.children == 2)
        #expect(why.inclusive?.footprintBytes == 1024)
        #expect(why.inclusive?.processes == 7)
        #expect(why.history?.samples == 3)
        #expect(why.history?.avgCpuPct == nil)
        #expect(why.reveal.isEmpty)
    }

    @Test func revealOpenablePathRules() {
        func action(_ argv: [String]) -> RevealAction {
            try! ProcwhoJSON.decode(RevealAction.self, from: JSONSerialization.data(withJSONObject: ["label": "x", "argv": argv]))
        }
        #expect(action(["open", "/tmp/a"]).openablePath == "/tmp/a")
        #expect(action(["open", "-R", "/tmp/a"]).openablePath == "/tmp/a")
        #expect(action(["open", "-a", "Terminal", "/tmp/a"]).openablePath == nil)
        #expect(action(["example-tool", "search", "/tmp/a"]).openablePath == nil)
        #expect(action(["open", "relative"]).openablePath == nil)
    }

    @Test func realWorktreesDecode() throws {
        let doc = try ProcwhoJSON.decode(WorktreesDoc.self, from: fixture("worktrees"))
        #expect(doc.schemaVersion == 3)
        #expect(!doc.totals.isEmpty)
        #expect(!doc.worktrees.isEmpty)
        #expect(!doc.notes.isEmpty)
        // --no-size: sizes are null, never 0.
        #expect(doc.worktrees.allSatisfy { $0.sizeBytes == nil })
        #expect(doc.worktrees.allSatisfy { !$0.reasons.isEmpty })
        #expect(doc.mergedIdle == nil || doc.mergedIdle?.verdict == "merged-idle")
    }
}
