import SwiftUI
import ProcwhoKit

struct GlossaryView: View {
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        VStack(alignment: .leading, spacing: Theme.large) {
            HStack {
                Text("What the labels mean").font(Theme.identity)
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
            }
            Text("Labels describe evidence.")
                .font(Theme.caption).foregroundStyle(Theme.secondary)
            ScrollView {
                VStack(alignment: .leading, spacing: Theme.large) {
                    ForEach(Glossary.sections, id: \.self) { section in
                        SectionHeader(title: section)
                        ForEach(Glossary.entries.filter { $0.section == section }) { entry in
                            VStack(alignment: .leading, spacing: Theme.small) {
                                Text(entry.title).font(Theme.title)
                                Text(entry.text).font(Theme.caption).foregroundStyle(Theme.secondary)
                                    .fixedSize(horizontal: false, vertical: true).textSelection(.enabled)
                            }
                        }
                    }
                }
            }
        }.padding(Theme.generous).frame(width: Theme.width, height: Theme.height)
            .background(Theme.background)
    }
}

struct GlossaryButton: View {
    @Environment(AppModel.self) private var model
    var body: some View {
        Button { model.showingGlossary = true } label: { Image(systemName: "questionmark.circle") }
            .buttonStyle(.plain).font(Theme.caption).foregroundStyle(Theme.secondary)
            .help("What the labels mean").accessibilityLabel("What the labels mean")
    }
}
