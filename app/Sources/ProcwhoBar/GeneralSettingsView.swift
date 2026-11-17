import SwiftUI
import ProcwhoKit

struct GeneralSettingsView: View {
    @Environment(AppModel.self) private var model
    var body: some View {
        @Bindable var store = model.settings
        Form {
            Section {
                Toggle("Launch at login", isOn: Binding(get: { model.services.loginEnabled }, set: { enabled in
                    model.services.setLogin(enabled)
                    store.value.launchAtLogin = model.services.loginEnabled
                }))
                if !["On", "Off"].contains(model.services.loginStatus), model.services.loginError == nil {
                    Text(model.services.loginStatus).font(Theme.caption).foregroundStyle(Theme.secondary)
                }
                if let error = model.services.loginError { Text(error).font(Theme.caption) }
            }
            Section("Refresh") {
                Picker("While panel is open", selection: $store.value.openRefresh) {
                    ForEach(RefreshInterval.allCases) { Text("\($0.rawValue) s").tag($0) }
                }
                Picker("In background", selection: $store.value.backgroundRefresh) {
                    ForEach(BackgroundRefresh.allCases, id: \.self) { Text($0.title).tag($0) }
                }
            }
            Section("Menu bar") {
                Picker("Menu bar shows", selection: $store.value.menuBarMode) {
                    ForEach(MenuBarMode.allCases, id: \.self) { Text($0.title).help(Glossary.labelHelp($0.title)).tag($0) }
                }
                Toggle("Show review count", isOn: $store.value.showReviewCount).toggleStyle(.checkbox)
            }

        }.formStyle(.grouped)
        .onAppear {
            if !model.snapshotMode {
                model.services.refreshLogin()
                store.value.launchAtLogin = model.services.loginEnabled
            }
        }
    }
}
