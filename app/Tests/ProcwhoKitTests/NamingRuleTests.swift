import Foundation
import Testing
@testable import ProcwhoKit

struct NamingRuleTests {
    @Test func copyPreservesValidRuleAndCount() throws {
        let rule = try ProcwhoJSON.decode(NamingRule.self, from: Data(#"{"name":"...","cwd":"^/Users/user/project(?:/|$)","_matches_now":3}"#.utf8))
        let copied = try #require(rule.copyJSON)
        let fields = try #require(JSONSerialization.jsonObject(with: Data(copied.utf8)) as? [String: Any])
        #expect(fields["_matches_now"] as? Int == 3)
        #expect(fields["cwd"] as? String == "^/Users/user/project(?:/|$)")
        #expect(fields["name"] as? String == "...")
    }
}
