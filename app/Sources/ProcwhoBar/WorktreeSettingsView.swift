import AppKit
import SwiftUI
import ProcwhoKit

struct WorktreeSettingsView: View {
    @Environment(AppModel.self) private var model
    var body: some View {
        @Bindable var store = model.settings
        Form {
            Section("Search folders") {
                Text(model.worktreeFoldersText).font(Theme.caption).foregroundStyle(Theme.secondary)
                Button("Choose folders…") { model.chooseProjectFolders() }
            }
            Section("Scan") {
                Stepper("Stale after \(store.value.staleDays) days", value: $store.value.staleDays, in: 1...3650)
                Toggle("Check GitHub for merged pull requests", isOn: $store.value.checkPullRequests).help(Glossary.help("pr"))
                Picker("Rescan automatically", selection: $store.value.worktreeRescan) {
                    ForEach(WorktreeRescan.allCases, id: \.self) { Text($0.title).tag($0) }
                }
            }
        }.formStyle(.grouped)
        .sheet(isPresented: Binding(get: { model.showingProjectFolders }, set: { model.showingProjectFolders = $0 })) { ProjectFoldersView() }
    }
}
