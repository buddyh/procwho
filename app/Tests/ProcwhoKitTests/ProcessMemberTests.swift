import Foundation
import Testing
@testable import ProcwhoKit

@Suite struct ProcessMemberTests {
    @Test(arguments: [("UNKNOWN owner: node", "node"), ("Safari (app)", "Safari"), ("UNKNOWN owner: Preview (app)", "Preview"), ("A (app) helper", "A (app) helper"), ("Known owner: node", "Known owner: node"), ("", "")])
    func displayName(input: String, expected: String) {
        #expect(Presentation.displayName(input) == expected)
    }

    @Test func membersPresentAbsentAndNull() throws {
        let json = #"{"workloads":[{"key":"a","name":"Safari","kind":"app","bundle_path":"/System/Applications/Safari.app","members":[{"pid":42,"ppid":1,"name":"WebKit","command":"WebKit --redacted","footprint_bytes":1024,"cpu_pct":4.4,"age_seconds":90}]},{"key":"b","name":"old","kind":"unknown"},{"key":"c","name":"null","kind":"app","members":null}]}"#
        let rows = try ProcwhoJSON.decode(StatusDoc.self, from: Data(json.utf8)).workloads
        #expect(rows.count == 3)
        #expect(rows[0].bundlePath == "/System/Applications/Safari.app")
        #expect(rows[0].members?.first == ProcessMember(pid: 42, ppid: 1, name: "WebKit", command: "WebKit --redacted", footprintBytes: 1024, cpuPct: 4.4, ageSeconds: 90))
        #expect(rows[1].members == nil)
        #expect(rows[2].members == nil)
        let why = try ProcwhoJSON.decode(WhyDoc.self, from: Data(#"{"workload":{"key":"a","name":"App","kind":"app","members":[]}}"#.utf8))
        #expect(why.workload.members == [])
    }

    private var workload: Workload {
        var row = Workload(key: "browser", name: "Safari", kind: "app")
        row.members = [
            .init(pid: 11, name: "WebKit", command: "web", footprintBytes: 100, cpuPct: 9),
            .init(pid: 22, name: "network", command: "network", footprintBytes: 300, cpuPct: 2),
            .init(pid: 33, name: "unknown", command: "unknown"),
            .init(pid: 44, name: "WebKit", command: "web", footprintBytes: 100, cpuPct: .nan)
        ]
        return row
    }

    @Test func sortsMembersWithMissingLastAndStableTies() {
        #expect(Presentation.members(workload, sort: .memory).map(\.pid) == [22, 11, 44, 33])
        #expect(Presentation.members(workload, sort: .cpu).map(\.pid) == [11, 22, 44, 33])
    }

    @Test func searchMatchesMembersAndKeepsOnlyMatchingChildren() {
        let row = workload
        #expect(Presentation.workloads([row], search: "WEBKIT", family: nil, sort: .memory).count == 1)
        #expect(Presentation.memberOnlyMatch(row, search: "WEBKIT"))
        #expect(Presentation.members(row, sort: .memory, search: "WEBKIT").map(\.pid) == [11, 44])
        #expect(Presentation.workloads([row], search: "22", family: nil, sort: .memory).count == 1)
        #expect(Presentation.members(row, sort: .memory, search: "22").map(\.pid) == [22])
        #expect(!Presentation.memberOnlyMatch(row, search: "Safari"))
        #expect(Presentation.members(row, sort: .memory, search: "Safari").count == 4)
        #expect(Presentation.workloads([row], search: "absent", family: nil, sort: .memory).isEmpty)
        #expect(!Presentation.memberOnlyMatch(row, search: "   "))
    }

    @Test func oldCLIFlagDetectionIsSpecific() {
        #expect(ProcwhoClient.isUnsupportedMembers("error: unexpected argument '--members' found"))
        #expect(ProcwhoClient.isUnsupportedMembers("unknown option --members"))
        #expect(!ProcwhoClient.isUnsupportedMembers("permission denied"))
        #expect(!ProcwhoClient.isUnsupportedMembers("unknown command 'status'"))
    }
}
