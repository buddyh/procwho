import Foundation

/// Valid owner-rule fields plus ignored snapshot metadata. Copying never writes the registry.
public struct NamingRule: Decodable, Sendable {
    public var name: String
    public var label: String?
    public var exe: String?
    public var cwd: String?
    public var argv: String?
    public var owner: String?
    public var impact: String?
    public var _matchesNow: Int?
    public var _note: String?

    public var copyJSON: String? {
        var fields: [String: Any] = ["name": name]
        for (key, value) in [("label", label), ("exe", exe), ("cwd", cwd), ("argv", argv), ("owner", owner), ("impact", impact), ("_note", _note)] {
            if let value { fields[key] = value }
        }
        if let count = _matchesNow { fields["_matches_now"] = count }
        guard let data = try? JSONSerialization.data(withJSONObject: fields, options: [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]) else { return nil }
        return String(data: data, encoding: .utf8)
    }
}
