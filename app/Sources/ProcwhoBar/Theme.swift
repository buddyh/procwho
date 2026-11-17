import SwiftUI
import ProcwhoKit

/// The design system's only visual constants. Keep new tokens in DESIGN.md too.
enum Theme {
    static let dimmedMemoryOpacity = 0.35
    static let minimumScale = 0.8
    static let zero: CGFloat = 0
    static let hairline: CGFloat = 1
    static let shareHeight: CGFloat = 3
    static let shareWidth: CGFloat = 56
    static let textGap: CGFloat = 2
    static let legendLineSpacing: CGFloat = 4
    static let small: CGFloat = 4
    static let medium: CGFloat = 8
    static let large: CGFloat = 12
    static let generous: CGFloat = 16
    static let edge: CGFloat = 14
    static let barRadius: CGFloat = 4
    static let radius: CGFloat = 6
    static let cardRadius: CGFloat = 10
    static let stripHeight: CGFloat = 10
    static let rowHeight: CGFloat = 40
    static let disclosureWidth: CGFloat = 12
    static let memberIndent: CGFloat = 48
    static let memberHeight: CGFloat = 24
    static let icon: CGFloat = 20
    static let largeIcon: CGFloat = 32
    static let memoryColumn: CGFloat = 64
    static let cpuColumn: CGFloat = 44
    static let gaugeColumn: CGFloat = 108
    static let keyColumn: CGFloat = 72
    static let processKeyColumn: CGFloat = 84
    static let width: CGFloat = 440
    static let height: CGFloat = 640
    static let settingsWidth: CGFloat = 520
    static let settingsHeight: CGFloat = 640
    static let aboutHeight: CGFloat = 360
    static let offscreenOffset: CGFloat = 10000
    static let snapshotSettle = 0.15
    static let snapshotTick = 0.01
    static let snapshotScale: CGFloat = 2
    static let hero = Font.system(size: 30, weight: .semibold)
    static let number = Font.system(size: 13, weight: .semibold)
    static let title = Font.system(size: 13)
    static let identity = Font.system(size: 15, weight: .semibold)
    static let caption = Font.system(size: 11)
    static let section = Font.system(size: 11, weight: .semibold)
    static let mono = Font.system(size: 11, design: .monospaced)
    static let primary = Color.primary
    static let secondary = Color.secondary
    static let tertiary = AnyShapeStyle(.tertiary)
    static let hover = AnyShapeStyle(.quaternary)
    static let card = AnyShapeStyle(.quaternary.opacity(0.5))
    static let background = Color(nsColor: .windowBackgroundColor)
    static let attention = Color.orange
    static let critical = Color.red
    static let good = Color.green
    static let clear = Color.clear
    static let hoverAnimation = Animation.easeOut(duration: 0.12)
    static let pushAnimation = Animation.easeOut(duration: 0.22)
    static let notInRamHelp = Glossary.help("not-in-ram")
    static let cpuHelp = Glossary.help("cpu")

    static func color(_ family: KindFamily, contrast: Bool = false) -> Color {
        switch family {
        case .agents: .teal
        case .leftovers: .orange
        case .unknown: .yellow
        case .simulators: .pink
        case .services: .blue
        case .apps: .indigo.opacity(contrast ? 1 : 0.7)
        case .system: .secondary
        }
    }

    static func symbol(_ family: KindFamily) -> String {
        switch family {
        case .agents: "sparkles"
        case .leftovers: "questionmark.app.dashed"
        case .unknown: "questionmark.circle"
        case .simulators: "iphone"
        case .services: "gearshape.2"
        case .apps: "macwindow"
        case .system: "apple.logo"
        }
    }

    static func symbol(kind: String) -> String {
        switch kind {
        case "agent-session": "sparkles"
        case "agent-toolset": "wrench.and.screwdriver"
        case "agent-command": "terminal"
        case "agent-host": "server.rack"
        default: symbol(KindFamily(kind: kind))
        }
    }

    static func verdictColor(_ verdict: String, contrast: Bool = false) -> Color {
        switch verdict {
        case "merged-idle": color(.agents)
        case "in-use": color(.services)
        case "has-work": color(.leftovers)
        case "stale": color(.simulators)
        case "active": color(.apps, contrast: contrast)
        default: secondary
        }
    }
}
