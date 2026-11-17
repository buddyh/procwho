import SwiftUI
import ProcwhoKit

struct AboutView: View {
    let sources: [String]
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.large) {
            Text("Procwho").font(Theme.identity)
            Text("Version \(AppVersion.current)").font(Theme.caption).foregroundStyle(Theme.secondary)
            Text("Read-only. Procwho never stops or deletes anything.").font(Theme.title)
            Divider()
            SectionHeader(title: "Data sources")
            ScrollView {
                VStack(alignment: .leading, spacing: Theme.medium) {
                    if sources.isEmpty { Text("No source information available.") }
                    ForEach(sources, id: \.self) { Text($0).textSelection(.enabled) }
                }
                .font(Theme.caption).foregroundStyle(Theme.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            HStack {
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
            }
        }
        .padding(Theme.generous).frame(width: Theme.width, height: Theme.aboutHeight)
        .foregroundStyle(Theme.primary).background(Theme.background)
    }
}
