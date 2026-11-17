import AppKit
import SwiftUI
import ProcwhoKit

struct ProcwhoBarApp: App {
    // The model is created and started here, not in a view, because MenuBarExtra creates its
    // content lazily: the label must refresh before the window is first opened.
    @State private var model: AppModel

    init() {
        let m = AppModel()
        m.start()
        _model = State(initialValue: m)
    }

    var body: some Scene {
        MenuBarExtra {
            ContentView()
                .environment(model)
        } label: {
            MenuBarLabel(model: model)
        }
        .menuBarExtraStyle(.window)
        Settings {
            SettingsView().environment(model)
        }
        .windowResizability(.contentSize)
    }
}

struct MenuBarLabel: View {
    let model: AppModel

    var body: some View {
        let review = model.reviewCount
        let text = review > 0
            ? Text("\(Image(systemName: "memorychip")) \(model.labelText) \(Image(systemName: "exclamationmark.circle"))\(Fmt.count(review))")
            : Text("\(Image(systemName: "memorychip")) \(model.labelText)")
        text
            .accessibilityLabel("procwho: \(model.settings.value.menuBarMode.rawValue) \(model.labelText)" + (review > 0 ? ", \(Fmt.count(review)) findings to review" : ""))
            .help(model.tooltipText)
    }
}

/// Select snapshot mode before SwiftUI installs the menu bar scene or starts polling.
@main
@MainActor
enum ProcwhoEntry {
    static func main() async {
        if CommandLine.arguments.contains("--snapshot") {
            do {
                try await SnapshotRenderer.run(SnapshotRenderer.Options(arguments: CommandLine.arguments))
                exit(EXIT_SUCCESS)
            } catch {
                FileHandle.standardError.write(Data("Snapshot failed: \(error)\n".utf8))
                exit(EXIT_FAILURE)
            }
        }
        ProcwhoBarApp.main()
    }
}
