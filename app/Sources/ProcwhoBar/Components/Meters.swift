import SwiftUI
import ProcwhoKit

struct ShareBar: View {
    let share: Double
    let color: Color
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        GeometryReader { geometry in
            ZStack(alignment: .leading) {
                RoundedRectangle(cornerRadius: Theme.barRadius).fill(Theme.hover)
                RoundedRectangle(cornerRadius: Theme.barRadius).fill(color)
                    .frame(width: geometry.size.width * min(1, max(0, share)))
            }
        }
        .frame(width: Theme.shareWidth, height: Theme.shareHeight)
        .animation(reduceMotion ? nil : Theme.hoverAnimation, value: share)
        .accessibilityHidden(true)
    }
}

struct Gauge: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let label: String
    let value: String
    let fraction: Double
    var swap = false
    var loadState: LoadState = .normal

    private var color: Color {
        if loadState == .critical { return Theme.critical }
        if loadState == .attention { return Theme.attention }
        return Theme.secondary
    }

    var body: some View {
        VStack(spacing: Theme.small) {
            HStack {
                Text(label)
                Spacer(minLength: Theme.small)
                if !swap { Text(value).foregroundStyle(color).monospacedDigit().contentTransition(.numericText()) }
            }.font(Theme.caption).foregroundStyle(Theme.secondary)
            if swap { Text(value).font(Theme.caption).foregroundStyle(Theme.secondary).monospacedDigit().frame(maxWidth: .infinity, alignment: .leading) }
            GeometryReader { proxy in
                ZStack(alignment: .leading) {
                    Capsule().fill(Theme.card)
                    Capsule().fill(color).frame(width: proxy.size.width * min(1, max(0, fraction)))
                }
            }.frame(height: Theme.small)
        }
        .animation(reduceMotion ? nil : Theme.hoverAnimation, value: fraction)
        .accessibilityElement(children: .combine)
    }
}

/// Generic segment input keeps the same strip and legend on memory and disk screens.
struct CompositionStrip: View {
    let segments: [CompositionSegment]
    @Binding var selection: String?
    let bytes: (CompositionSegment) -> Int64
    let title: (CompositionSegment) -> String
    let value: (CompositionSegment) -> String
    let color: (CompositionSegment) -> Color
    var highlightedShare: (CompositionSegment) -> Double = { _ in 1 }
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var measured: [CompositionSegment] { Presentation.legendSegments(segments) }
    private var total: Double { measured.reduce(0) { $0 + Double(bytes($1)) } }

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.medium) {
            GeometryReader { proxy in
                let gaps = CGFloat(max(0, measured.count - 1)) * Theme.hairline
                HStack(spacing: Theme.hairline) {
                    ForEach(measured) { segment in
                        Button { toggle(segment.id) } label: {
                            GeometryReader { part in
                                HStack(spacing: Theme.zero) {
                                    Rectangle().fill(color(segment)).frame(width: part.size.width * min(1, max(0, highlightedShare(segment))))
                                    Rectangle().fill(color(segment).opacity(Theme.dimmedMemoryOpacity))
                                }
                            }
                        }
                        .buttonStyle(.plain)
                        .frame(width: max(Theme.zero, proxy.size.width - gaps) * Double(bytes(segment)) / max(1, total))
                        .help("\(title(segment)): \(value(segment)). " + segmentHelp(segment))
                        .accessibilityLabel("Filter \(title(segment)), \(value(segment))")
                    }
                }
            }
            .frame(height: Theme.stripHeight)
            .background(Theme.card)
            .clipShape(RoundedRectangle(cornerRadius: Theme.barRadius))
            .animation(reduceMotion ? nil : Theme.hoverAnimation, value: total)
            LegendFlow {
                ForEach(measured) { segment in
                    Button { toggle(segment.id) } label: {
                        HStack(spacing: Theme.small) {
                            Circle().fill(color(segment)).frame(width: Theme.small, height: Theme.small)
                            Text("\(title(segment)) \(value(segment))")
                                .font(Theme.caption).foregroundStyle(Theme.secondary).monospacedDigit().lineLimit(1)
                        }
                        .fixedSize()
                        .background {
                            if selection == segment.id {
                                RoundedRectangle(cornerRadius: Theme.radius).fill(Theme.hover)
                            }
                        }
                    }
                    .buttonStyle(.plain)
                    .help("\(title(segment)): \(value(segment)). " + segmentHelp(segment))
                    .accessibilityAddTraits(selection == segment.id ? [.isSelected] : [])
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func segmentHelp(_ segment: CompositionSegment) -> String {
        KindFamily(rawValue: segment.id) != nil ? Glossary.help("family:" + segment.id) : Glossary.verdictHelp(segment.id)
    }

    private func toggle(_ id: String) { selection = selection == id ? nil : id }
}
