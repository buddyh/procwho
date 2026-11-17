import SwiftUI
import ProcwhoKit

/// Uses status identity while the richer explanation is loading; never runs reveal commands.
struct WorkloadDetailView: View {
    @Environment(AppModel.self) private var model
    let key: String
    @State private var ruleCopied = false

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.zero) {
            HStack {
                Button { model.select(nil) } label: { Label(model.tab.rawValue, systemImage: "chevron.left") }
                    .buttonStyle(.borderless)
                Spacer()
                if model.whyLoading { ProgressView().controlSize(.small) }
            }
            .padding(.horizontal, Theme.edge).padding(.bottom, Theme.medium)
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: Theme.large) {
                        if let w = model.why?.workload ?? model.workload(forKey: key) {
                            identity(w)
                            stats(w)
                            if let error = model.whyError { detailError(error) }
                            if !w.evidence.isEmpty {
                                DetailSection(title: "How it was identified") { BulletList(items: w.evidence, symbol: "checkmark") }
                            }
                            if w.weakConfidence {
                                HStack(alignment: .top, spacing: Theme.medium) {
                                    Image(systemName: "exclamationmark.triangle").foregroundStyle(Theme.attention)
                                    VStack(alignment: .leading, spacing: Theme.small) {
                                        Text("The owner is not certain (confidence: \(w.confidence ?? "unknown")).")
                                            .font(Theme.caption).fixedSize(horizontal: false, vertical: true)
                                        Button("What the labels mean") { model.showingGlossary = true }
                                            .buttonStyle(.plain).font(Theme.caption)
                                    }
                                }.padding(Theme.medium).background(Theme.card, in: RoundedRectangle(cornerRadius: Theme.radius))
                            }
                            if !w.ifStopped.isEmpty {
                                DetailSection(title: "If it stopped") {
                                    BulletList(items: w.ifStopped)
                                }
                            }
                            Section {
                                let statusRow = model.workload(forKey: key)
                                let memberRow = w.members != nil ? w : (statusRow ?? w)
                                ProcessMembersView(workload: memberRow, sort: model.sort)
                            } header: { SectionHeader(title: "Processes").id("processes") }
                            if let why = model.why {
                                if let history = why.history {
                                    DetailSection(title: "History") {
                                        Text(historyText(history)).font(Theme.caption).foregroundStyle(Theme.secondary).lineLimit(1).help(historyText(history))
                                    }
                                }
                                if let process = why.process { processCard(process) }
                                actions(why).id("detail-actions")
                            }
                        } else if model.whyLoading {
                            SkeletonRows()
                        } else if let error = model.whyError {
                            detailError(error)
                        } else {
                            EmptyState(text: "This workload is no longer in the current sample.")
                        }
                    }
                    .padding(.horizontal, Theme.edge).padding(.bottom, Theme.large)
                }
                .onAppear {
                    if let section = model.snapshotDetailSection {
                        DispatchQueue.main.async { proxy.scrollTo(section, anchor: .top) }
                    }
                }
            }
        }
    }

    private func detailError(_ message: String) -> some View {
        VStack(alignment: .leading, spacing: Theme.medium) {
            Label { Text(message).foregroundStyle(Theme.secondary) }
                icon: { Image(systemName: "exclamationmark.triangle").foregroundStyle(Theme.critical) }
                .font(Theme.caption).textSelection(.enabled)
            CLIRepairAdvice()
            Button("Retry explanation") { model.select(key) }.controlSize(.small)
        }
    }

    private func identity(_ w: Workload) -> some View {
        HStack(alignment: .top, spacing: Theme.large) {
            KindIconTile(family: KindFamily(kind: w.kind), kind: w.kind, large: true, bundlePath: w.bundlePath ?? model.workload(forKey: key)?.bundlePath)
            VStack(alignment: .leading, spacing: Theme.medium) {
                Text(Presentation.displayName(w.name)).font(Theme.identity).textSelection(.enabled).lineLimit(3)
                    .fixedSize(horizontal: false, vertical: true)
                Text(KindOrder.title(w.kind)).font(Theme.caption).foregroundStyle(Theme.secondary)
                    .help(Glossary.kindHelp(w.kind))
            }
        }
    }

    private func stats(_ w: Workload) -> some View {
        GroupBox {
            VStack(spacing: Theme.zero) {
                measurement("Memory", value: Fmt.bytes(w.footprintBytes), help: Glossary.footprintSumHelp)
                if let bytes = w.notInRamBytes, bytes > 0 {
                    Divider()
                    measurement("Not in RAM", value: Fmt.estimatedBytes(bytes), help: Theme.notInRamHelp)
                }
                Divider()
                let sampled = model.status?.workloads.first { $0.key == key }
                if sampled?.cpuPct != nil, let at = model.statusAt {
                    TimelineView(.periodic(from: .now, by: 5)) { context in
                        measurement("CPU", value: Fmt.cpu(sampled?.cpuPct),
                                    help: "Sampled \(Fmt.age(context.date.timeIntervalSince(at))) ago")
                    }
                } else { measurement("CPU", value: "Not sampled", help: Theme.cpuHelp) }
                Divider()
                measurement("Processes", value: w.processes.map(Fmt.count) ?? "—")
                Divider()
                measurement("Age", value: (w.ageSeconds ?? model.workload(forKey: key)?.ageSeconds).map { Fmt.age($0) } ?? "—")
                if !w.ports.isEmpty {
                    Divider()
                    measurement("Ports", value: w.ports.joined(separator: ", "))
                }
            }
        }
    }

    private func measurement(_ label: String, value: String, help: String? = nil) -> some View {
        HStack(spacing: Theme.medium) {
            Text(label).foregroundStyle(Theme.secondary)
            Spacer(minLength: Theme.medium)
            Text(value).monospacedDigit().multilineTextAlignment(.trailing)
        }.font(Theme.title).padding(.vertical, Theme.small)
            .help(help ?? Glossary.labelHelp(label))
    }

    private func processCard(_ p: ProcessDetail) -> some View {
        DetailSection(title: "Root process") {
            GroupBox {
                VStack(alignment: .leading, spacing: Theme.medium) {
                    CopyValueRow(label: "PID", value: String(p.pid))
                    if let started = p.started { CopyValueRow(label: "Started", value: started) }
                    if let cwd = p.cwd { CopyValueRow(label: "Working folder", value: cwd) }
                    if let command = p.command { CopyValueRow(label: "Command", value: command) }
                    if !p.ancestry.isEmpty {
                        CopyValueRow(label: "Parents", value: p.ancestry.map { "\($0.pid) \($0.command ?? "")" }.joined(separator: "\n"))
                    } else if let ppid = p.ppid { CopyValueRow(label: "Parent", value: String(ppid)) }
                    if let parent = p.originalParent { CopyValueRow(label: "Original", value: parent) }
                }
            }
        }
    }

    private func actions(_ why: WhyDoc) -> some View {
        VStack(alignment: .leading, spacing: Theme.medium) {
            if ruleCopied || model.snapshotRuleCopied {
                Text("Copied. Paste it into ~/.config/procwho/registry.json.")
                    .font(Theme.caption).foregroundStyle(Theme.secondary).fixedSize(horizontal: false, vertical: true)
                Link("How naming works", destination: URL(string: "https://github.com/buddyh/procwho/blob/main/docs/CONFIGURATION.md#owner-rules")!)
                    .font(Theme.caption)
            }
            ForEach(why.reveal) { reveal in
                CopyValueRow(label: reveal.label.prefix(1).capitalized + reveal.label.dropFirst(), value: reveal.displayCommand)
                if let note = reveal.note { Text(note).font(Theme.caption).foregroundStyle(Theme.secondary) }
            }
            HStack(spacing: Theme.small) {
                namingButton(why)
                if let path = why.process?.cwd ?? why.reveal.compactMap(\.openablePath).first {
                    Button("Reveal in Finder") { Actions.revealInFinder(path) }
                }
                if let pid = why.process?.pid { Button("Copy PID") { Actions.copy(String(pid)) } }
                if let command = why.process?.command { Button("Copy Command") { Actions.copy(command) } }
            }.font(Theme.caption).controlSize(.small)
        }
    }

    private func namingButton(_ why: WhyDoc) -> some View {
        Button("Copy naming rule") {
            guard let text = why.suggestedRule?.copyJSON else { return }
            Actions.copy(text)
            ruleCopied = true
        }.disabled(why.suggestedRule?.copyJSON == nil)
            .help("Copy a rule for registry.json.")
    }

    private func historyText(_ h: WhyHistory) -> String {
        var parts: [String] = []
        if let samples = h.samples { parts.append("\(Fmt.count(samples)) samples") }
        if let minutes = h.spanMin { parts.append("over \(Fmt.age(minutes * 60))") }
        if let first = h.footprintFirstBytes, let last = h.footprintLastBytes {
            parts.append("\(Fmt.bytes(first)) → \(Fmt.bytes(last))")
        }
        if let cpu = h.avgCpuPct { parts.append("average CPU \(Fmt.cpu(cpu))") }
        return parts.prefix(2).joined(separator: " · ")
    }
}

struct DetailSection<Content: View>: View {
    let title: String
    @ViewBuilder var content: Content
    var body: some View {
        VStack(alignment: .leading, spacing: Theme.medium) {
            SectionHeader(title: title)
            content
        }
    }
}

struct BulletList: View {
    let items: [String]
    var symbol = "minus"
    var body: some View {
        VStack(alignment: .leading, spacing: Theme.medium) {
            ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                HStack(alignment: .firstTextBaseline, spacing: Theme.medium) {
                    Image(systemName: symbol).foregroundStyle(Theme.secondary).accessibilityHidden(true)
                    Text(item).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
                }.font(Theme.caption)
            }
        }
    }
}

struct CopyValueRow: View {
    let label: String
    let value: String
    @State private var hovering = false
    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: Theme.medium) {
            Text(label).font(Theme.caption).foregroundStyle(Theme.secondary).help(Glossary.labelHelp(label))
                .frame(width: Theme.processKeyColumn, alignment: .leading)
            Text(value).font(Theme.mono).textSelection(.enabled).lineLimit(6)
                .frame(maxWidth: .infinity, alignment: .leading).help(value)
            Button { Actions.copy(value) } label: { Image(systemName: "doc.on.doc").font(Theme.caption) }
                .buttonStyle(.borderless).opacity(hovering ? 1 : 0)
                .help("Copy \(label)").accessibilityLabel("Copy \(label)")
        }.onHover { hovering = $0 }
    }
}
