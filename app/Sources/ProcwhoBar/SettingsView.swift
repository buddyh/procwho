import AppKit
import SwiftUI

/// Native settings tabs share the same view in the real window and fixture snapshots.
enum SettingsTab: String, CaseIterable {
    case general = "General", workloads = "Workloads", worktrees = "Worktrees", alerts = "Alerts", advanced = "Advanced"
    var symbol: String {
        switch self {
        case .general: "gearshape"; case .workloads: "memorychip"; case .worktrees: "folder"
        case .alerts: "bell"; case .advanced: "slider.horizontal.3"
        }
    }
}

struct SettingsView: View {
    @Environment(AppModel.self) private var model
    var body: some View {
        @Bindable var model = model
        TabView(selection: $model.settingsTab) {
            ForEach(SettingsTab.allCases, id: \.self) { tab in
                SettingsTabContent(tab: tab)
                    .tabItem { Label(tab.rawValue, systemImage: tab.symbol) }.tag(tab)
            }
        }
        .background(SettingsFocus(enabled: !model.snapshotMode, onWindow: { model.settingsWindow = $0 }))
        .onAppear { if !model.snapshotMode { model.services.refreshLogin() } }
        .frame(width: Theme.settingsWidth, height: Theme.settingsHeight)
        .background(Theme.background)
    }
}

/// SwiftUI's Settings scene supplies native toolbar tabs. Explicit activation gives the
/// settings window keyboard focus even though Procwho is an LSUIElement app.
struct SettingsFocus: NSViewRepresentable {
    let enabled: Bool
    let onWindow: (NSWindow) -> Void
    func makeNSView(context: Context) -> FocusView { FocusView(enabled: enabled, onWindow: onWindow) }
    func updateNSView(_ view: FocusView, context: Context) {}
    final class FocusView: NSView {
        let enabled: Bool
        let onWindow: (NSWindow) -> Void
        init(enabled: Bool, onWindow: @escaping (NSWindow) -> Void) {
            self.enabled = enabled
            self.onWindow = onWindow
            super.init(frame: .zero)
        }
        required init?(coder: NSCoder) { nil }
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            guard enabled, let window else { return }
            onWindow(window)
            DispatchQueue.main.async {
                NSApp.activate(ignoringOtherApps: true)
                window.makeKeyAndOrderFront(nil)
                window.orderFrontRegardless()
            }
        }
    }
}

/// Forms are shared with AppKit's native toolbar tab container during offscreen captures.
struct SettingsTabContent: View {
    let tab: SettingsTab
    var body: some View {
        switch tab {
        case .general: GeneralSettingsView()
        case .workloads: WorkloadSettingsView()
        case .worktrees: WorktreeSettingsView()
        case .alerts: AlertSettingsView()
        case .advanced: AdvancedSettingsView()
        }
    }
}
