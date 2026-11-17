import SwiftUI
import ProcwhoKit

struct Card<Content: View>: View {
    @Environment(\.colorSchemeContrast) private var contrast
    @ViewBuilder var content: Content

    var body: some View {
        content.padding(Theme.large)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Theme.card, in: RoundedRectangle(cornerRadius: Theme.cardRadius))
            .overlay {
                if contrast == .increased {
                    RoundedRectangle(cornerRadius: Theme.cardRadius).stroke(Theme.secondary, lineWidth: Theme.hairline)
                }
            }
    }
}

struct Chip: View {
    let text: String
    var dot: Color? = nil
    @Environment(\.colorSchemeContrast) private var contrast

    var body: some View {
        HStack(spacing: Theme.small) {
            if let dot {
                Circle().fill(dot).frame(width: Theme.small, height: Theme.small).accessibilityHidden(true)
            }
            Text(text)
        }
            .help(Glossary.labelHelp(text))
            .fixedSize(horizontal: true, vertical: false).font(Theme.caption).foregroundStyle(Theme.secondary)
            .lineLimit(1)
    }
}

struct KindIconTile: View {
    let family: KindFamily
    var kind: String? = nil
    var large = false
    var bundlePath: String? = nil

    var body: some View {
        Group {
            if let bundlePath, let icon = AppIconCache.icon(for: bundlePath) {
                Image(nsImage: icon).resizable().scaledToFit()
            } else {
                Image(systemName: kind.map { Theme.symbol(kind: $0) } ?? Theme.symbol(family))
                    .font(large ? Theme.identity : Theme.title).foregroundStyle(Theme.secondary)
            }
        }
        .frame(width: large ? Theme.largeIcon : Theme.icon, height: large ? Theme.largeIcon : Theme.icon)
        .accessibilityHidden(true)
    }
}

@MainActor
private enum AppIconCache {
    static let cache = NSCache<NSString, NSImage>()
    static func icon(for path: String) -> NSImage? {
        guard FileManager.default.fileExists(atPath: path) else { return nil }
        if let image = cache.object(forKey: path as NSString) { return image }
        let image = NSWorkspace.shared.icon(forFile: path)
        cache.setObject(image, forKey: path as NSString)
        return image
    }
}

struct SectionHeader: View {
    let title: String
    var detail: String? = nil

    var body: some View {
        HStack(spacing: Theme.medium) {
            Text(title)
            Spacer(minLength: Theme.small)
            if let detail { Text(detail).monospacedDigit() }
        }
        .font(Theme.section).foregroundStyle(Theme.secondary)
        .padding(.vertical, Theme.medium)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct HoverRow<Content: View>: View {
    var selected = false
    @ViewBuilder var content: (Bool) -> Content
    @State private var hovering = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.colorSchemeContrast) private var contrast

    var body: some View {
        content(hovering).padding(.horizontal, Theme.small)
            .frame(minHeight: Theme.rowHeight)
            .background {
                if hovering || selected {
                    RoundedRectangle(cornerRadius: Theme.radius).fill(Theme.hover)
                }
            }
            .overlay {
                if selected || (hovering && contrast == .increased) {
                    RoundedRectangle(cornerRadius: Theme.radius).stroke(Theme.secondary, lineWidth: Theme.hairline)
                }
            }
            .contentShape(Rectangle())
            .onHover { hovering = $0 }
            .animation(reduceMotion ? nil : Theme.hoverAnimation, value: hovering)
            .accessibilityAddTraits(selected ? [.isSelected] : [])
    }
}

struct SkeletonRows: View {
    var body: some View {
        VStack(spacing: Theme.medium) {
            ForEach(0..<6) { _ in
                HoverRow { _ in
                    HStack(spacing: Theme.medium) {
                        KindIconTile(family: .system)
                        VStack(alignment: .leading, spacing: Theme.small) {
                            Text("Workload memory and ownership").font(Theme.title)
                            Text("Processes and recent activity").font(Theme.caption)
                        }
                        Spacer()
                        Text("1.2 GB").font(Theme.number)
                    }.redacted(reason: .placeholder)
                }
            }
        }
        .accessibilityElement(children: .ignore).accessibilityLabel("Loading workloads")
    }
}

struct EmptyState: View {
    let text: String
    var loading = false
    var good = false

    var body: some View {
        VStack(spacing: Theme.medium) {
            if loading { ProgressView().controlSize(.small) }
            if good { Image(systemName: "checkmark.circle").font(Theme.hero).foregroundStyle(Theme.good) }
            Text(text).font(Theme.title).foregroundStyle(Theme.secondary)
                .multilineTextAlignment(.center).textSelection(.enabled)
        }
        .padding(Theme.generous).frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// Active filters stay visible even when the result list is empty.
struct FilterChip: View {
    let text: String
    let clear: () -> Void
    var body: some View {
        Button(action: clear) {
            HStack(spacing: Theme.small) {
                Text(text)
                Image(systemName: "xmark").accessibilityHidden(true)
            }.font(Theme.caption).foregroundStyle(Theme.secondary)
                .padding(.horizontal, Theme.small).padding(.vertical, Theme.small)
        }.buttonStyle(.plain).help("Clear " + text).accessibilityLabel("Clear " + text)
    }
}
