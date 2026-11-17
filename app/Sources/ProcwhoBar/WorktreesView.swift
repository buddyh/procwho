import SwiftUI
import ProcwhoKit

struct WorktreesView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.colorSchemeContrast) private var contrast
    @State private var search = ""
    @State private var sort: WorktreeSort = .size
    @State private var verdict: String?
    @State private var dismissedError: String?

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.zero) {
            if let doc = model.worktrees {
                if doc.worktrees.isEmpty && (doc.worktreesFound ?? 0) == 0 { emptyScan }
                else { content(doc) }
            } else if model.worktreesScanning {
                VStack(alignment: .leading, spacing: Theme.large) {
                    scanProgress
                    SkeletonRows()
                    Spacer()
                }.padding(.horizontal, Theme.edge)
            } else if let error = model.worktreesError {
                VStack(alignment: .leading, spacing: Theme.large) {
                    Text("Worktree scan failed").font(Theme.identity)
                    if error != dismissedError { WarningRow(message: error, critical: true) { dismissedError = error } }
                    CLIRepairAdvice()
                    Button("Rescan") { model.refreshWorktrees() }
                    Spacer()
                }.padding(.horizontal, Theme.edge)
            } else {
                neverScanned
            }
            if model.worktrees != nil || model.worktreesScanning || model.worktreesError != nil {
                scanStatus.padding(.horizontal, Theme.edge).padding(.vertical, Theme.medium)
            }
        }.onAppear {
            if model.snapshotMode { verdict = model.snapshotWorktreeVerdict; search = model.snapshotWorktreeSearch }
        }
    }

    private var neverScanned: some View {
        VStack(alignment: .leading, spacing: Theme.large) {
            Text("Worktrees have not been scanned").font(Theme.identity)
            Text("Search folders:").font(Theme.title)
            Text(model.worktreeFoldersText).font(Theme.mono).textSelection(.enabled)
            Text("The first scan can take a minute.")
                .font(Theme.caption).foregroundStyle(Theme.secondary)
            Button("Scan worktrees") { model.refreshWorktrees() }.buttonStyle(.borderedProminent)
            Spacer()
        }.padding(Theme.generous)
    }

    private var emptyScan: some View {
        VStack(alignment: .leading, spacing: Theme.large) {
            if let error = model.worktreesError, error != dismissedError {
                WarningRow(message: error, critical: true) { dismissedError = error }
                CLIRepairAdvice()
            }
            Text("No worktrees found").font(Theme.identity)
            Text("Folders searched:").font(Theme.title)
            Text(model.worktreeFoldersText).font(Theme.mono).textSelection(.enabled)
            Text("Choose other folders to search.")
                .font(Theme.caption).foregroundStyle(Theme.secondary)
            Button("Choose folders…") { model.chooseProjectFolders() }
            Spacer()
        }.padding(Theme.generous)
    }

    private func content(_ doc: WorktreesDoc) -> some View {
        let segments = Presentation.composition(doc)
        let rows = Presentation.worktrees(doc.worktrees, search: search, verdict: verdict, sort: sort)
        let maximum = rows.compactMap(\.sizeBytes).max()
        return ScrollView {
            LazyVStack(alignment: .leading, spacing: Theme.medium, pinnedViews: [.sectionHeaders]) {
                HStack(alignment: .center, spacing: Theme.large) {
                    let measured = doc.worktrees.contains { $0.sizeBytes != nil }
                    VStack(alignment: .leading, spacing: Theme.small) {
                        Text(measured ? "Reclaimable" : "Worktrees").font(Theme.section).foregroundStyle(Theme.secondary)
                        Text(measured ? reclaimableLabel(segments) : Fmt.count(doc.worktreesFound ?? doc.worktrees.count))
                            .font(Theme.hero).monospacedDigit().contentTransition(.numericText())
                        Button { verdict = "merged-idle" } label: {
                            Text("\(Fmt.count(doc.mergedIdle?.count ?? 0)) merged and idle")
                                .font(Theme.caption).foregroundStyle(Theme.secondary)
                        }.buttonStyle(.plain).help("Show merged and idle worktrees")
                        if let system = model.status?.system, let free = system.diskFreeBytes, let total = system.diskTotalBytes, total > 0 {
                            Text("\(Fmt.bytes(free)) free of \(Fmt.bytes(total))")
                                .font(Theme.caption).foregroundStyle(Theme.secondary)
                                .help("Capacity of \(model.diskVolumeName), the home volume.")
                        }
                    }.frame(maxWidth: .infinity, alignment: .leading)
                    if doc.worktrees.contains(where: { $0.sizeBytes == nil }) {
                        Button("Measure sizes") { model.refreshWorktrees() }.controlSize(.small)
                            .disabled(model.worktreesScanning)
                    }
                }.padding(.vertical, Theme.small).help(Glossary.help("reclaimable"))
                if let error = model.worktreesError, error != dismissedError {
                    WarningRow(message: error, critical: true) { dismissedError = error }
                    CLIRepairAdvice()
                }
                if doc.worktrees.contains(where: { $0.sizeBytes != nil }) {
                    CompositionStrip(segments: segments, selection: $verdict, bytes: { $0.bytes }, title: { $0.title },
                                     value: { $0.valueLabel }, color: { Theme.verdictColor($0.id, contrast: contrast == .increased) })
                        .help(Glossary.help("apfs"))
                }
                if model.worktreesScanning { scanProgress }
                HStack(spacing: Theme.medium) {
                    TextField("Search repo, branch or path", text: $search).textFieldStyle(.roundedBorder).font(Theme.title)
                        .accessibilityLabel("Search worktrees")
                    Menu {
                        Picker("Sort", selection: $sort) { ForEach(WorktreeSort.allCases) { Text($0.rawValue).tag($0) } }
                    } label: { Text("Sort: \(sort.rawValue)").font(Theme.caption) }.fixedSize()
                }
                if verdict != nil || !search.isEmpty {
                    LegendFlow {
                        if let selected = verdict { FilterChip(text: "Verdict: " + Glossary.verdictTitle(selected)) { verdict = nil } }
                        if !search.isEmpty { FilterChip(text: "Search: " + search) { search = "" } }
                        Button("Show all") { verdict = nil; search = "" }.font(Theme.caption).buttonStyle(.plain)
                    }
                }
                Section {
                    ForEach(rows) { row in
                        WorktreeRow(worktree: row, measuredNow: model.snapshotMode ? (doc.generatedAt ?? 0) : Date().timeIntervalSince1970, share: Presentation.share(row.sizeBytes, of: maximum))
                    }
                    if rows.isEmpty { EmptyState(text: "No worktrees match.") }
                } header: {
                    SectionHeader(title: "\(Fmt.count(rows.count)) worktrees").background(Theme.background)
                }
                if !doc.notes.isEmpty {
                    DisclosureGroup("Scan notes") { BulletList(items: doc.notes).padding(.top, Theme.medium) }
                        .font(Theme.caption).foregroundStyle(Theme.secondary)
                }
            }.padding(.horizontal, Theme.edge).padding(.bottom, Theme.large)
        }
    }

    private var scanProgress: some View {
        VStack(alignment: .leading, spacing: Theme.medium) {
            ProgressView(value: Theme.zero, total: 1).accessibilityLabel("Scanning; progress is unavailable")
            Text("Scanning worktrees…")
                .font(Theme.caption).foregroundStyle(Theme.secondary)
        }
    }

    private var scanStatus: some View {
        HStack(spacing: Theme.medium) {
            TimelineView(.periodic(from: .now, by: 5)) { context in
                Text(model.worktreesAt.map { "Scanned \(Fmt.age(context.date.timeIntervalSince($0))) ago" } ?? "Not scanned")
                    .font(Theme.caption).foregroundStyle(Theme.secondary)
            }
            if let seconds = model.worktrees?.scanSecs { Text(String(format: "in %.0f s", seconds)).font(Theme.caption).foregroundStyle(Theme.secondary) }
            Spacer(minLength: Theme.zero)
            if model.worktreesScanning { ProgressView().controlSize(.mini) }
            Button("Rescan") { model.refreshWorktrees() }.controlSize(.small).disabled(model.worktreesScanning)
        }
    }

    private func reclaimableLabel(_ segments: [CompositionSegment]) -> String {
        guard let segment = segments.first(where: { $0.id == "merged-idle" }) else { return Fmt.bytes(0) }
        guard segment.unknownCount < segment.count else { return "—" }
        return "~" + Fmt.bytes(segment.bytes)
    }

}

private struct WorktreeRow: View {
    @Environment(AppModel.self) private var model
    @Environment(\.colorSchemeContrast) private var contrast
    let worktree: Worktree
    let measuredNow: Double
    let share: Double
    @State private var expanded = false

    var body: some View {
        HoverRow { hovering in
            VStack(alignment: .leading, spacing: Theme.small) {
                HStack(spacing: Theme.medium) {
                    Button { expanded.toggle() } label: {
                        VStack(alignment: .leading, spacing: Theme.small) {
                            HStack(spacing: Theme.small) {
                                Image(systemName: expanded ? "chevron.down" : "chevron.right")
                                    .font(Theme.caption).foregroundStyle(Theme.secondary)
                                Text(worktree.repo ?? "Unknown repository").font(Theme.title).lineLimit(1).truncationMode(.middle)
                            }
                            Text(worktree.branch ?? "Detached HEAD").help(Glossary.help(worktree.branch == nil ? "detached-head" : "branch")).font(Theme.caption).foregroundStyle(Theme.secondary)
                                .lineLimit(1).truncationMode(.middle)
                        }.frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
                    }.buttonStyle(.plain).help(worktree.path)
                    HStack(spacing: Theme.medium) {
                        Button { Actions.revealInFinder(worktree.path) } label: { Image(systemName: "folder") }
                            .disabled(worktree.exists == false).help("Reveal in Finder").accessibilityLabel("Reveal in Finder")
                        Button { Actions.copy(worktree.path) } label: { Image(systemName: "doc.on.doc") }
                            .help("Copy path").accessibilityLabel("Copy path")
                    }
                    .font(Theme.caption).buttonStyle(.borderless).opacity(hovering ? 1 : 0)
                    VStack(alignment: .trailing, spacing: Theme.small) {
                        Text(worktree.sizeBytes.map(Fmt.bytes) ?? "–").font(Theme.number).monospacedDigit()
                        if worktree.sizeBytes != nil { ShareBar(share: share, color: Theme.verdictColor(worktree.verdict, contrast: contrast == .increased)) }
                    }
                    .frame(width: Theme.memoryColumn, alignment: .trailing)
                    .help(worktree.sizeBytes == nil ? "Disk use not measured" : Presentation.sizeAge(worktree, now: measuredNow) + ". " + Glossary.help("apfs"))
                }
                HStack(spacing: Theme.medium) {
                    Chip(text: Glossary.verdictTitle(worktree.verdict), dot: Theme.verdictColor(worktree.verdict, contrast: contrast == .increased)).help(Glossary.verdictHelp(worktree.verdict))
                    if let idle = Presentation.worktreeIdleCaption(worktree) { Text(idle).help(Glossary.help("idle")).font(Theme.caption).foregroundStyle(Theme.secondary) }
                    if !worktree.flags.isEmpty {
                        Text(worktree.flags.joined(separator: ", ")).font(Theme.caption).foregroundStyle(Theme.secondary)
                            .lineLimit(1).help(worktree.flags.map { Glossary.help($0 == "detached" ? "detached-head" : $0) }.joined(separator: " "))
                    }
                }
                if expanded {
                    if worktree.sizeBytes != nil { Text(Presentation.sizeAge(worktree, now: measuredNow)).font(Theme.caption).foregroundStyle(Theme.secondary) }
                    Text(worktree.path).font(Theme.mono).textSelection(.enabled)
                    BulletList(items: worktree.reasons + worktree.notes)
                }
            }.padding(.vertical, Theme.small)
        }.onAppear {
            if model.snapshotMode, model.snapshotWorktreeExpanded == worktree.path { expanded = true }
        }
    }
}
