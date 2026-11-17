import AppKit
import SwiftUI
import ProcwhoKit

/// The same first-scan choice is available later from Worktrees settings.
struct ProjectFoldersView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var selected = Set<String>()
    @State private var added: [String] = []
    @State private var initialized = false

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.large) {
            Text("Project folders").font(Theme.identity)
            Text("Choose folders to scan for Git worktrees.")
                .font(Theme.title).fixedSize(horizontal: false, vertical: true)
            ScrollView {
                VStack(alignment: .leading, spacing: Theme.large) {
                    if model.folderSuggestions == nil {
                        HStack { ProgressView().controlSize(.small); Text("Checking project folders…") }
                    }
                    ForEach(model.folderSuggestions?.candidates.filter(\.exists) ?? []) { folder in
                        folderRow(folder)
                    }
                    ForEach(added, id: \.self) { path in
                        Toggle(isOn: selection(path)) { Text(display(path)).font(Theme.mono) }
                            .toggleStyle(.checkbox)
                    }
                    if let error = model.folderSuggestionError { Text(error).font(Theme.caption).foregroundStyle(Theme.secondary) }
                }.frame(maxWidth: .infinity, alignment: .leading)
            }
            Button("Add folder…", systemImage: "plus") { addFolders() }.disabled(model.folderSuggestions == nil || model.folderSuggestionsProbing)
            Text("Included automatically: " + automaticText)
                .font(Theme.caption).foregroundStyle(Theme.secondary).fixedSize(horizontal: false, vertical: true)
            HStack {
                Spacer()
                Button("Not now") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Scan") {
                    var settings = model.settings.value
                    settings.searchRoots = selected.sorted()
                    settings.worktreeFoldersChosen = true
                    model.settings.value = settings
                    dismiss()
                    if !model.worktreesScanning { model.refreshWorktrees() }
                }.buttonStyle(.borderedProminent).keyboardShortcut(.defaultAction)
                    .disabled(model.folderSuggestions == nil || model.folderSuggestionsProbing)
            }
        }
        .padding(Theme.generous).font(Theme.title).foregroundStyle(Theme.primary)
        .frame(width: Theme.width, height: Theme.height).background(Theme.background)
        .task {
            await model.loadFolderSuggestions()
            if !initialized {
                let existing = model.settings.value.searchRoots
                if model.settings.value.worktreeFoldersChosen == true || !existing.isEmpty { selected = Set(existing) }
                else { selected = Set(model.folderSuggestions?.candidates.filter { !$0.protected && ($0.repoCount ?? 0) > 0 }.map(\.path) ?? []) }
                let candidates = Set(model.folderSuggestions?.candidates.filter(\.exists).map(\.path) ?? [])
                added = existing.filter { !candidates.contains($0) }
                if model.snapshotFoldersDocumentsSelected { selected.insert("/Users/user/Documents") }
                initialized = true
            }
        }
    }

    private func folderRow(_ folder: ProjectFolders.Candidate) -> some View {
        Toggle(isOn: selection(folder.path)) {
            VStack(alignment: .leading, spacing: Theme.small) {
                HStack {
                    Text(display(folder.path)).font(Theme.mono)
                    Spacer(minLength: Theme.medium)
                    if !folder.protected {
                        Text(folder.repoCount.map { "\(Fmt.count($0)) \($0 == 1 ? "repository" : "repositories")" } ?? (model.folderSuggestionsProbing || model.snapshotFoldersProbing ? "Checking…" : "Check timed out"))
                            .font(Theme.caption).foregroundStyle(Theme.secondary)
                    }
                }
                if folder.protected { Text("May require macOS permission").font(Theme.caption).foregroundStyle(Theme.secondary) }
            }
        }.toggleStyle(.checkbox).disabled(model.folderSuggestionsProbing)
    }

    private var automaticText: String {
        let folders = model.folderSuggestions?.automaticFolders.map(display) ?? []
        return (folders + ["repositories with running agent sessions"]).joined(separator: ", ")
    }
    private func display(_ path: String) -> String {
        path.replacingOccurrences(of: model.snapshotMode ? "/Users/user" : NSHomeDirectory(), with: "~")
    }
    private func selection(_ path: String) -> Binding<Bool> {
        Binding(get: { selected.contains(path) }, set: { if $0 { selected.insert(path) } else { selected.remove(path) } })
    }
    private func addFolders() {
        guard !model.snapshotMode else { return }
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true; panel.canChooseFiles = false; panel.allowsMultipleSelection = true
        if panel.runModal() == .OK {
            for url in panel.urls {
                if !(model.folderSuggestions?.candidates.contains { $0.path == url.path } ?? false), !added.contains(url.path) { added.append(url.path) }
                selected.insert(url.path)
            }
        }
    }
}
