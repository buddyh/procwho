import Foundation

extension Presentation {
    /// Only positive, measured segments belong in a strip's compact legend.
    public static func legendSegments(_ segments: [CompositionSegment]) -> [CompositionSegment] {
        segments.filter { $0.bytes > 0 && ($0.count == 0 || $0.unknownCount < $0.count) }
    }

}
