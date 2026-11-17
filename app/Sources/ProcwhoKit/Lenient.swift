import Foundation

/// A list that decodes leniently: a missing key or `null` gives `[]`, and an element that
/// fails to decode is skipped instead of failing the whole document. The app must survive
/// schema growth in the CLI's JSON.
@propertyWrapper
public struct LossyList<Element: Decodable & Sendable>: Decodable, Sendable {
    public var wrappedValue: [Element]

    public init(wrappedValue: [Element]) { self.wrappedValue = wrappedValue }
    public init() { self.wrappedValue = [] }

    public init(from decoder: Decoder) throws {
        var out: [Element] = []
        guard var c = try? decoder.unkeyedContainer() else {
            wrappedValue = []
            return
        }
        while !c.isAtEnd {
            if let e = try? c.decode(Element.self) {
                out.append(e)
            } else {
                _ = try? c.decode(Skip.self)
            }
        }
        wrappedValue = out
    }

    private struct Skip: Decodable {}
}

extension LossyList: Equatable where Element: Equatable {}

extension KeyedDecodingContainer {
    public func decode<T>(_ type: LossyList<T>.Type, forKey key: Key) throws -> LossyList<T> {
        try decodeIfPresent(type, forKey: key) ?? LossyList(wrappedValue: [])
    }
}

/// One value of the `sources` object: `true`/`false` for a source that worked or failed,
/// or a string describing its state ("Local service service metadata 12s old").
public enum SourceState: Decodable, Sendable, Equatable {
    case ok(Bool)
    case note(String)
    case notes([String])
    case other

    public init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if let b = try? c.decode(Bool.self) {
            self = .ok(b)
        } else if let s = try? c.decode(String.self) {
            self = .note(s)
        } else if let notes = try? c.decode([String].self) {
            self = .notes(notes)
        } else {
            self = .other
        }
    }
}

public enum ProcwhoJSON {
    public static func decoder() -> JSONDecoder {
        let d = JSONDecoder()
        d.keyDecodingStrategy = .convertFromSnakeCase
        return d
    }

    public static func decode<T: Decodable>(_ type: T.Type, from data: Data) throws -> T {
        try decoder().decode(type, from: data)
    }
}
