import Foundation
import Testing
@testable import ProcwhoKit

@Suite struct Formatting {
    @Test func compactBytes() {
        #expect(Fmt.bytesCompact(nil) == "-")
        #expect(Fmt.bytesCompact(0) == "0K")
        #expect(Fmt.bytesCompact(64 * 1024) == "64K")
        #expect(Fmt.bytesCompact(812 * 1024 * 1024) == "812M")
        #expect(Fmt.bytesCompact(Int64(23.4 * 1024 * 1024 * 1024)) == "23.4G")
        #expect(Fmt.bytesCompact(150 * 1024 * 1024 * 1024) == "150G")
        #expect(Fmt.bytesCompact(-5) == "0K")
    }

    @Test func readableBytes() {
        #expect(Fmt.bytes(nil) == "—")
        #expect(Fmt.bytes(512) == "<1 MB")
        #expect(Fmt.bytes(310 * 1024 * 1024) == "310 MB")
        #expect(Fmt.bytes(Int64(1.25 * 1024 * 1024 * 1024)) == "1.2 GB")
    }

    @Test(arguments: [
        (Int64(1_073_741_824), "1 GB", "1G"),
        (1_610_612_736, "1.5 GB", "1.5G"),
        (106_837_311_488, "99.5 GB", "99.5G"),
        (107_374_182_400, "100 GB", "100G"),
        (129_385_889_792, "120 GB", "120G"),
        (1_073_741_824_000, "1000 GB", "1000G"),
        (1_099_511_627_775, "1024 GB", "1024G"),
        (1_099_511_627_776, "1 TB", "1T"),
        (1_649_267_441_664, "1.5 TB", "1.5T"),
        (109_401_406_963_712, "99.5 TB", "99.5T"),
        (109_951_162_777_600, "100 TB", "100T"),
        (110_171_065_103_155, "100 TB", "100T")
    ])
    func sizePrecisionAndTerabyteBoundaries(bytes: Int64, readable: String, compact: String) {
        #expect(Fmt.bytes(bytes) == readable)
        #expect(Fmt.bytesCompact(bytes) == compact)
        #expect(Fmt.estimatedBytes(bytes) == "~" + readable)
    }

    @Test func ages() {
        #expect(Fmt.age(nil) == "-")
        #expect(Fmt.age(45) == "45s")
        #expect(Fmt.age(12 * 60 + 5) == "12m")
        #expect(Fmt.age(3 * 3600) == "3h")
        #expect(Fmt.age(3 * 3600 + 12 * 60) == "3h 12m")
        #expect(Fmt.age(4 * 86400 + 3 * 3600 + 59) == "4d 3h")
        #expect(Fmt.age(-3) == "0s")
        #expect(Fmt.age(.infinity) == "-")
    }

    @Test func cpu() {
        #expect(Fmt.cpu(nil) == "-")
        #expect(Fmt.cpu(0.44) == "0.4%")
        #expect(Fmt.cpu(12.5) == "12%")
        #expect(Fmt.cpu(402) == "402%")
    }
}

@Suite struct BinaryLookup {
    let home = "/Users/user"

    @Test func orderIsEnvBundleLocalThenDebugRepo() {
        let c = BinaryLocator.candidates(
            environment: ["PROCWHO_BIN": "/opt/procwho"],
            resourceDirectory: URL(fileURLWithPath: "/Apps/Procwho.app/Contents/Resources"),
            homeDirectory: home,
            debugSearchStart: URL(fileURLWithPath: "/Users/user/repos/procwho/app/build/Procwho.app"))
        #expect(c[0] == "/opt/procwho")
        #expect(c[1] == "/Apps/Procwho.app/Contents/Resources/procwho")
        #expect(c[2] == "/Users/user/.local/bin/procwho")
        #expect(c.contains("/Users/user/repos/procwho/target/release/procwho"))
        #expect(c.firstIndex(of: "/Users/user/repos/procwho/target/release/procwho")! > 2)
    }

    @Test func emptyEnvAndNoBundleAreSkipped() {
        let c = BinaryLocator.candidates(environment: ["PROCWHO_BIN": ""], resourceDirectory: nil,
                                         homeDirectory: home, debugSearchStart: nil)
        #expect(c == ["/Users/user/.local/bin/procwho"])
    }

    @Test func locatePicksFirstExecutable() {
        let c = ["/a", "/b", "/c"]
        #expect(BinaryLocator.locate(c) { $0 == "/b" || $0 == "/c" } == "/b")
        #expect(BinaryLocator.locate(c) { _ in false } == nil)
    }
}

@Suite struct Runner {
    @Test func returnsStdout() async throws {
        let r = CLIRunner(binary: "/bin/echo")
        let data = try await r.run(["hello"], key: "t", timeout: 5)
        #expect(String(decoding: data, as: UTF8.self) == "hello\n")
    }

    @Test func nonZeroExitIsAnError() async {
        let r = CLIRunner(binary: "/usr/bin/false")
        await #expect(throws: CLIError.self) { try await r.run([], key: "t", timeout: 5) }
    }

    @Test func timeoutTerminatesChild() async {
        let r = CLIRunner(binary: "/bin/sleep")
        let start = Date()
        await #expect(throws: CLIError.timedOut(seconds: 0.3)) { try await r.run(["5"], key: "t", timeout: 0.3) }
        #expect(Date().timeIntervalSince(start) < 3)
    }

    @Test func sameKeyNeverOverlaps() async throws {
        let r = CLIRunner(binary: "/bin/sleep")
        let first = Task { try await r.run(["0.5"], key: "status", timeout: 5) }
        try await Task.sleep(for: .milliseconds(100))
        await #expect(throws: CLIError.busy("status")) { try await r.run(["0.1"], key: "status", timeout: 5) }
        // A different key may run alongside.
        _ = try await r.run(["0.1"], key: "why", timeout: 5)
        _ = try await first.value
    }

    @Test func cancellationStopsChild() async {
        let r = CLIRunner(binary: "/bin/sleep")
        let start = Date()
        let t = Task { try await r.run(["5"], key: "t", timeout: 10) }
        try? await Task.sleep(for: .milliseconds(200))
        t.cancel()
        _ = try? await t.value
        #expect(Date().timeIntervalSince(start) < 3)
    }

    @Test func kindOrder() {
        #expect(KindOrder.sorted(["app", "zzz", "agent-host", "agent-session", "system"])
                == ["agent-session", "agent-host", "app", "system", "zzz"])
    }
}

struct FirstRunPresentationTests {
    @Test func noAgentsUsesFootprintAndPlainCaption() {
        var status = StatusDoc(totals: Totals(footprintBytes: 5 * 1_073_741_824),
                               workloads: [Workload(key: "app", name: "Editor", kind: "app", footprintBytes: 1024)])
        let measured = Presentation.heroMemory(status)
        #expect(measured == status.system?.memUsedBytes)
        #expect(Presentation.heroCaption(status).contains("Pressure unknown"))
        status.byKind = [KindSummary(kind: "agent-session", workloads: 1, processes: 2, footprintBytes: 42, cpuPct: 0)]
        #expect(Presentation.heroMemory(status, view: .agents) == 42)
        #expect(Presentation.agentWorkloadCount(status) == 1)
        #expect(Presentation.heroMemory(StatusDoc()) == nil)
    }

    @Test func resolvesCustomRootsAndAgentContainers() throws {
        let config = try ProcwhoJSON.decode(WorktreeSearchConfiguration.self, from: Data(#"{"values":{"worktree_roots":["/Users/user/code"]},"worktree_scan":{"containers":[{"path":"/Users/user/.codex/worktrees","depth":3}]}}"#.utf8))
        #expect(config.folders(roots: []) == ["/Users/user/code", "/Users/user/.codex/worktrees"])
        #expect(config.folders(roots: ["/Users/user/projects"]) == ["/Users/user/projects", "/Users/user/.codex/worktrees"])
    }
}
