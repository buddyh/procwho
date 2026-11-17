import Foundation
import Testing
@testable import ProcwhoKit

struct GlossaryTests {
    @Test func coversEveryDisplayKindFamilyVerdictAndConfidence() {
        for kind in KindOrder.order + ["terminal", "declared"] { #expect(Glossary.entry("kind:" + kind) != nil) }
        for family in KindFamily.allCases { #expect(Glossary.entry("family:" + family.rawValue) != nil) }
        for verdict in Glossary.verdicts { #expect(Glossary.entry("verdict:" + verdict) != nil) }
        for level in Glossary.confidenceLevels { #expect(Glossary.entry("confidence:" + level) != nil) }
        for id in ["agents-associated", "linked-to-agents", "physical-memory", "memory-pressure"] { #expect(Glossary.entry(id) != nil) }
        #expect(Set(Glossary.entries.map(\.id)).count == Glossary.entries.count)
        #expect(Glossary.kindHelp("custom-group") == Glossary.help("family:services"))
    }

    @Test func definitionsAreOneShortSentence() {
        for entry in Glossary.entries {
            #expect(entry.text.split(separator: " ").count <= 20)
            #expect(entry.text.filter { $0 == "." }.count == 1)
            #expect(!entry.title.hasPrefix("~"))
        }
    }

    @Test func documentationCoversTermsAndRustVerdicts() throws {
        let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        // CLI documentation retains fuller definitions; the app uses short definitions.
        let documentation = try String(contentsOf: root.appendingPathComponent("docs/GLOSSARY.md"), encoding: .utf8)
        for entry in Glossary.entries { #expect(documentation.contains("`" + entry.id + "`")) }
        let rust = try String(contentsOf: root.appendingPathComponent("crates/procwho-wt/src/verdict.rs"), encoding: .utf8)
        let pattern = #"Verdict::[A-Za-z]+ => "([a-z-]+)""#
        let matches = try NSRegularExpression(pattern: pattern).matches(in: rust, range: NSRange(rust.startIndex..., in: rust))
        let values = matches.compactMap { Range($0.range(at: 1), in: rust).map { String(rust[$0]) } }
        #expect(Set(values) == Set(Glossary.verdicts))
    }
}
