import AppKit
import SwiftUI
import ProcwhoKit

struct AdvancedSettingsView: View {
    @Environment(AppModel.self) private var model
    @State private var resetting = false
    @State private var copied = false
    var body: some View {
        Form {
            Section("procwho CLI") {
                LabeledContent("Resolved path") {
                    Text(model.binaryPath ?? "Not found").font(Theme.mono).textSelection(.enabled)
                        .lineLimit(3).truncationMode(.middle)
                }
                LabeledContent("Source", value: model.binarySource)
                HStack {
                    Button("Choose CLI…") { chooseCLI() }
                    Button("Use bundled") { model.useBundledCLI() }
                }
            }.help(Glossary.help("cli"))
            Section {
                Button(copied ? "Diagnostics copied" : "Copy diagnostics") {
                    Task { Actions.copy(await model.diagnostics()); copied = true }
                }.help("Versions, CLI path, and last error.")
                Button("Reset all settings…") { resetting = true }
                if let error = model.services.loginError { Text(error).font(Theme.caption) }
            }
            Section("Data sources") {
                ForEach(model.status?.dataSourceDescriptions ?? ["No source information available."], id: \.self) {
                    Text($0).font(Theme.caption).foregroundStyle(Theme.secondary).textSelection(.enabled)
                }
            }
        }.formStyle(.grouped)
        .confirmationDialog("Reset all settings?", isPresented: $resetting) {
            Button("Reset settings") { model.resetSettings() }
        } message: { Text("Restore defaults, turn off alerts, and disable launch at login?") }
    }
    private func chooseCLI() {
        guard !model.snapshotMode else { return }
        let panel = NSOpenPanel()
        panel.canChooseDirectories = false
        panel.canChooseFiles = true
        if panel.runModal() == .OK, let url = panel.url {
            model.settings.value.cliOverride = url.path
        }
    }
}
