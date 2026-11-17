import SwiftUI
import ProcwhoKit

struct ContentView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        @Bindable var model = model
        VStack(spacing: Theme.zero) {
            if model.binaryPath == nil || (model.status == nil && model.statusError != nil) {
                CLIProblemView()
            } else {
                Picker("View", selection: $model.tab) {
                    ForEach(Tab.allCases) { Text($0.rawValue).tag($0) }
                }
                .pickerStyle(.segmented).labelsHidden()
                .padding(.horizontal, Theme.edge).padding(.vertical, Theme.large)
                .onChange(of: model.tab) { _, _ in model.select(nil) }
                ZStack {
                    if let key = model.selectedKey {
                        WorkloadDetailView(key: key)
                            .transition(reduceMotion ? .opacity : .move(edge: .trailing))
                    } else {
                        Group {
                            switch model.tab {
                            case .workloads: WorkloadsView()
                            case .findings: FindingsView()
                            case .worktrees: WorktreesView()
                            }
                        }
                        .transition(reduceMotion ? .opacity : .move(edge: .leading))
                    }
                }
                .clipped()
                .animation(reduceMotion ? nil : Theme.pushAnimation, value: model.selectedKey)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            Divider()
            FooterView()
        }
        .font(Theme.title).foregroundStyle(Theme.primary)
        .background(Theme.background)
        .frame(width: Theme.width, height: Theme.height)
        .background(KeyboardNavigation(model: model))
        .sheet(isPresented: $model.showingGlossary) { GlossaryView() }
        .sheet(isPresented: $model.showingProjectFolders) { ProjectFoldersView() }
        .onAppear { if !model.snapshotMode { model.windowVisible = true } }
        .onDisappear { if !model.snapshotMode { model.windowVisible = false } }
    }
}

struct FooterView: View {
    @Environment(AppModel.self) private var model
    @State private var showingAbout = false
    @Environment(\.openSettings) private var openSettings

    var body: some View {
        @Bindable var model = model
        HStack(spacing: Theme.medium) {
            if model.isRefreshing { ProgressView().controlSize(.mini) }
            TimelineView(.periodic(from: .now, by: 1)) { context in
                Text(model.statusAt.map { "Updated \(Fmt.age(context.date.timeIntervalSince($0))) ago" }
                     ?? (model.isRefreshing ? "Refreshing…" : "No sample yet"))
                    .font(Theme.caption).foregroundStyle(Theme.secondary)
            }
            Spacer()
            Menu {
                Button("Refresh now") { model.refreshNow() }.keyboardShortcut("r")
                Button("Refresh settings…") { model.settingsTab = .general; model.openSettings() }
                Divider()
                Button("Settings…") { model.openSettings() }.keyboardShortcut(",")
                Button("What the labels mean") { model.showingGlossary = true }
                Button("About Procwho") { showingAbout = true }
                Button("Quit") { NSApp.terminate(nil) }.keyboardShortcut("q")
            } label: { Image(systemName: "gearshape") }
                .menuStyle(.borderlessButton).fixedSize().help("Procwho settings")
        }
        .padding(.horizontal, Theme.edge).padding(.vertical, Theme.medium)
        .onAppear {
            model.settingsOpener = {
                NSApp.activate(ignoringOtherApps: true)
                openSettings()
            }
        }
        .sheet(isPresented: $showingAbout) {
            AboutView(sources: model.status?.dataSourceDescriptions ?? [])
        }
    }
}

struct WarningRow: View {
    let message: String
    var critical = false
    let dismiss: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: Theme.medium) {
            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(critical ? Theme.critical : Theme.attention)
            Text(message).font(Theme.caption).foregroundStyle(Theme.secondary)
                .lineLimit(3).textSelection(.enabled).help(message)
            Spacer(minLength: Theme.zero)
            Button(action: dismiss) { Image(systemName: "xmark") }
                .buttonStyle(.plain).help("Dismiss warning").accessibilityLabel("Dismiss warning")
        }.padding(Theme.medium).background(Theme.card, in: RoundedRectangle(cornerRadius: Theme.radius))
    }
}

struct StatusWarning: View {
    @Environment(AppModel.self) private var model
    var body: some View {
        if let text = model.warningText, text != model.dismissedWarning {
            VStack(alignment: .leading, spacing: Theme.medium) {
                WarningRow(message: text, critical: model.statusError != nil) { model.dismissedWarning = text }
                if model.statusError != nil { CLIRepairAdvice() }
            }
        }
    }
}

struct CLIProblemView: View {
    @Environment(AppModel.self) private var model
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Theme.large) {
                Label(model.binaryPath == nil ? "Procwho CLI not found" : "Procwho CLI failed", systemImage: "exclamationmark.triangle")
                    .font(Theme.identity)
                if let error = model.statusError { Text(error).font(Theme.caption).foregroundStyle(Theme.secondary).textSelection(.enabled) }
                Text("Choose a working CLI in Advanced settings.")
                    .font(Theme.title).fixedSize(horizontal: false, vertical: true)
                Text("Paths checked:").font(Theme.caption).foregroundStyle(Theme.secondary)
                ForEach(model.triedPaths, id: \.self) { Text($0).font(Theme.mono).textSelection(.enabled).fixedSize(horizontal: false, vertical: true) }
                Button("Open Advanced settings") { model.settingsTab = .advanced; model.openSettings() }
                if model.binaryPath != nil { Button("Retry") { model.refreshNow() } }
            }.padding(Theme.generous).frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

/// Also shown for failed requests when an older sample remains visible.
struct CLIRepairAdvice: View {
    @Environment(AppModel.self) private var model
    var body: some View {
        VStack(alignment: .leading, spacing: Theme.small) {
            Text("Choose a working CLI in Advanced settings.")
                .font(Theme.caption).foregroundStyle(Theme.secondary)
            Text("CLI: " + (model.binaryPath ?? "Not found"))
                .font(Theme.mono).foregroundStyle(Theme.secondary).textSelection(.enabled)
            Button("Open Advanced settings") { model.settingsTab = .advanced; model.openSettings() }
                .controlSize(.small)
        }.help(Glossary.help("cli"))
    }
}
