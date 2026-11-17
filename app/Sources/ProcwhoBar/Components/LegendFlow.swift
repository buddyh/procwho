import SwiftUI

/// Size legend buttons at their intrinsic width and wrap them with consistent gaps.
struct LegendFlow: Layout {
    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        arrange(subviews, width: proposal.width).size
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let positions = arrange(subviews, width: bounds.width).positions
        for (view, position) in zip(subviews, positions) {
            view.place(at: CGPoint(x: bounds.minX + position.x, y: bounds.minY + position.y),
                       anchor: .topLeading, proposal: .unspecified)
        }
    }

    private func arrange(_ subviews: Subviews, width: CGFloat?) -> (size: CGSize, positions: [CGPoint]) {
        let limit = width ?? .infinity
        var x = Theme.zero, y = Theme.zero, height = Theme.zero, usedWidth = Theme.zero
        var positions: [CGPoint] = []
        for view in subviews {
            let size = view.sizeThatFits(.unspecified)
            if x > Theme.zero && x + size.width > limit {
                x = Theme.zero
                y += height + Theme.legendLineSpacing
                height = Theme.zero
            }
            positions.append(CGPoint(x: x, y: y))
            usedWidth = max(usedWidth, x + size.width)
            x += size.width + Theme.large
            height = max(height, size.height)
        }
        return (CGSize(width: width ?? usedWidth, height: y + height), positions)
    }
}
