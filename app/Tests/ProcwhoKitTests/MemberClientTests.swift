import Foundation
import Testing
@testable import ProcwhoKit

/// Exercise compatibility through a real CLI boundary without sampling this Mac.
@Suite struct MemberClientTests {
    private func binary(_ body: String) throws -> URL {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("procwho-client-\(UUID())")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let executable = directory.appendingPathComponent("procwho")
        let script = """
        #!/bin/sh
        printf '%s\\n' "$*" >> "$(dirname "$0")/calls"
        \(body)
        """
        try Data(script.utf8).write(to: executable)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: executable.path)
        return executable
    }

    private func calls(_ binary: URL) throws -> [String] {
        try String(contentsOf: binary.deletingLastPathComponent().appendingPathComponent("calls"), encoding: .utf8)
            .split(separator: "\n").map(String.init)
    }

    @Test func olderCLIRetriesWithoutFlagAndRemembersUnsupportedFlag() async throws {
        let executable = try binary("""
        case "$*" in
          *--members*) printf '%s\\n' "error: unexpected argument '--members' found" >&2; exit 2 ;;
        esac
        printf '%s\\n' '{"workloads":[{"key":"old","name":"Old CLI","kind":"app"}]}'
        """)
        defer { try? FileManager.default.removeItem(at: executable.deletingLastPathComponent()) }
        let client = ProcwhoClient(binary: executable.path)
        let first = try await client.status()
        let second = try await client.status()
        #expect(first.1 == .native)
        #expect(first.0.workloads.first?.members == nil)
        #expect(second.0.workloads.count == 1)
        #expect(try calls(executable) == ["status --json --limit 0 --members", "status --json --limit 0", "status --json --limit 0"])
    }

    @Test func supportedCLIReceivesMembersFlag() async throws {
        let executable = try binary("""
        printf '%s\\n' '{"workloads":[{"key":"new","name":"New CLI","kind":"app","members":[{"pid":10,"name":"Helper","command":"helper"}]}]}'
        """)
        defer { try? FileManager.default.removeItem(at: executable.deletingLastPathComponent()) }
        let result = try await ProcwhoClient(binary: executable.path).status()
        #expect(result.0.workloads.first?.members?.first?.pid == 10)
        #expect(try calls(executable) == ["status --json --limit 0 --members"])
    }

    @Test func unrelatedFailuresDoNotRetry() async throws {
        let executable = try binary("printf '%s\\n' 'permission denied' >&2; exit 2")
        defer { try? FileManager.default.removeItem(at: executable.deletingLastPathComponent()) }
        let client = ProcwhoClient(binary: executable.path)
        await #expect(throws: CLIError.self) { try await client.status() }
        #expect(try calls(executable).count == 1)
    }
}
