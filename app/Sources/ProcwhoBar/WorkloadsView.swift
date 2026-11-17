import SwiftUI
import ProcwhoKit

struct WorkloadsView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.colorSchemeContrast) private var contrast
    @FocusState private var searchFocused: Bool

    var body: some View {
        @Bindable var model = model
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: Theme.medium, pinnedViews: [.sectionHeaders]) {
                    if model.settings.value.onboardingDismissed != true { FirstLaunchCard() }
                    OverviewHero(status: model.status)
                    StatusWarning()
                    if let status = model.status {
                        CompositionStrip(segments: Presentation.composition(status), selection: $model.familyFilter,
                                         bytes: { $0.bytes }, title: { $0.title }, value: { $0.valueLabel },
                                         color: { Theme.color(KindFamily(rawValue: $0.id) ?? .services, contrast: contrast == .increased) },
                                         highlightedShare: { Presentation.highlightedShare($0, status: status, view: model.settings.value.heroView ?? .associated) })
                            .help(Presentation.stripCaption(Presentation.composition(status)) + ". " + Glossary.footprintSumHelp)
                        attention(status)
                        toolbar
                        if model.familyFilter != nil || !model.search.isEmpty {
                            LegendFlow {
                                if let raw = model.familyFilter {
                                    FilterChip(text: "Family: " + (KindFamily(rawValue: raw)?.title ?? raw)) { model.familyFilter = nil }
                                }
                                if !model.search.isEmpty { FilterChip(text: "Search: " + model.search) { model.search = "" } }
                                Button("Show all") { model.familyFilter = nil; model.search = "" }.buttonStyle(.plain).font(Theme.caption)
                            }
                        }
                        if !model.flatWorkloads { columnCaptions }
                        if model.rankedWorkloads.isEmpty {
                            EmptyState(text: (model.flatWorkloads || model.settings.value.hideBelowMB > 0) ? "No workloads match this filter." : "No workloads in this sample.")
                        } else if model.flatWorkloads {
                            Section {
                                workloadRows(model.rankedWorkloads)
                            } header: {
                                columnCaptions.background(Theme.background)
                            }
                        } else {
                            ForEach(Presentation.rankedFamilies(model.rankedWorkloads, sort: model.sort)) { family in
                                let rows = model.rankedWorkloads.filter { KindFamily(kind: $0.kind) == family }
                                if !rows.isEmpty {
                                    Section {
                                        if !model.collapsedFamilies.contains(family) { workloadRows(rows) }
                                    } header: {
                                        familyHeader(family, rows: rows)
                                    }
                                }
                            }
                        }
                        let hidden = Presentation.hiddenSystemCount(status, hidden: model.settings.value.hideSystemWorkloads != false)
                        if hidden > 0 {
                            HStack(spacing: Theme.small) {
                                Text("\(Fmt.count(hidden)) system \(hidden == 1 ? "workload" : "workloads") hidden ·").foregroundStyle(Theme.secondary)
                                Button("Show") { model.settings.value.hideSystemWorkloads = false }.buttonStyle(.plain)
                            }.font(Theme.caption).padding(.vertical, Theme.medium)
                        }
                    } else if model.statusError == nil {
                        SkeletonRows()
                    } else {
                        Button("Retry") { model.refreshNow() }
                        EmptyState(text: "No sample available.")
                    }
                }
                .padding(.horizontal, Theme.edge).padding(.bottom, Theme.large)
            }
            .onChange(of: model.highlightedKey) { _, key in
                if let key { proxy.scrollTo(key) }
            }
        }
        .onAppear { focusSearchIfRequested() }
        .onChange(of: model.searchFocusRequest) { _, _ in focusSearchIfRequested() }
    }

    private func focusSearchIfRequested() {
        if model.searchFocusRequest > 0 {
            searchFocused = true
            model.searchFocusRequest = 0
        }
    }

    private var toolbar: some View {
        @Bindable var model = model
        return HStack(spacing: Theme.medium) {
            TextField("Search workloads", text: $model.search)
                .textFieldStyle(.roundedBorder).font(Theme.title).focused($searchFocused)
                .accessibilityLabel("Search workloads")
            Menu {
                Toggle("Group by family", isOn: Binding(get: { model.settings.value.groupByFamily == true }, set: { model.settings.value.groupByFamily = $0 }))
                Divider()
                Picker("Sort", selection: $model.sort) {
                    ForEach(WorkloadSort.allCases) { Text($0.title).tag($0) }
                }
            } label: { Text("Sort: \(model.sort.title)").font(Theme.caption) }
                .fixedSize().help(model.sort == .notInRAM ? Theme.notInRamHelp : "Rank workloads")
        }
        .padding(.top, Theme.medium)
    }

    private var columnCaptions: some View {
        HStack(spacing: Theme.medium) {
            Text("\(Fmt.count(model.rankedWorkloads.count)) \(model.rankedWorkloads.count == 1 ? "workload" : "workloads")").frame(maxWidth: .infinity, alignment: .leading)
            Text(model.sort == .notInRAM ? "Not in RAM" : "Memory").help(model.sort == .notInRAM ? Theme.notInRamHelp : Glossary.footprintSumHelp).frame(width: Theme.memoryColumn, alignment: .trailing)
            Text("CPU").frame(width: Theme.cpuColumn, alignment: .trailing).help(Theme.cpuHelp)
        }
        .font(Theme.section).foregroundStyle(Theme.secondary)
        .padding(.horizontal, Theme.small)
    }

    private func familyHeader(_ family: KindFamily, rows: [Workload]) -> some View {
        Button {
            if !model.collapsedFamilies.insert(family).inserted { model.collapsedFamilies.remove(family) }
        } label: {
            HStack(spacing: Theme.small) {
                Image(systemName: model.collapsedFamilies.contains(family) ? "chevron.right" : "chevron.down")
                    .font(Theme.caption).foregroundStyle(Theme.secondary)
                Text("\(family.title)  \(Fmt.count(rows.count))")
                    .frame(maxWidth: .infinity, alignment: .leading)
                if let status = model.status {
                    let totals = Presentation.familyTotals(family, status: status)
                    HStack(spacing: Theme.medium) {
                        Text(model.sort == .notInRAM
                             ? Fmt.estimatedBytes(rows.allSatisfy { $0.notInRamBytes != nil } ? Presentation.sum(rows.compactMap(\.notInRamBytes)) : nil)
                             : Fmt.bytes(totals.memory))
                            .frame(width: Theme.memoryColumn, alignment: .trailing)
                            .help(model.sort == .notInRAM ? "Displayed workloads only. " + Theme.notInRamHelp : Glossary.footprintSumHelp)
                        Text(Fmt.cpu(totals.cpu)).frame(width: Theme.cpuColumn, alignment: .trailing)
                            .help(Theme.cpuHelp)
                    }.monospacedDigit()
                }
            }
            .font(Theme.section).foregroundStyle(Theme.secondary)
            .padding(.horizontal, Theme.small).padding(.vertical, Theme.medium)
            .background(Theme.background)
        }
        .buttonStyle(.plain).help(Glossary.help("family:" + family.rawValue))
        .accessibilityLabel("\(family.title), \(Fmt.count(rows.count)) workloads, \(model.collapsedFamilies.contains(family) ? "collapsed" : "expanded")")
    }

    private func workloadRows(_ rows: [Workload]) -> some View {
        let maximum = model.visibleWorkloads.compactMap { model.sort == .notInRAM ? $0.notInRamBytes : $0.footprintBytes }.max()
        return ForEach(rows) { row in
            let expanded = model.expandedWorkloadKeys.contains(row.key) || Presentation.memberOnlyMatch(row, search: model.search)
            HStack(alignment: .top, spacing: Theme.small) {
                Button {
                    model.highlightedKey = row.key
                    if !model.expandedWorkloadKeys.insert(row.key).inserted { model.expandedWorkloadKeys.remove(row.key) }
                } label: {
                    Image(systemName: "chevron.right")
                        .rotationEffect(.degrees(expanded ? 90 : 0))
                        .font(Theme.caption).foregroundStyle(Theme.secondary)
                        .frame(width: Theme.disclosureWidth, height: Theme.rowHeight)
                        .contentShape(Rectangle())
                }.buttonStyle(.plain).accessibilityLabel(expanded ? "Hide processes" : "Show processes")
                Button {
                    model.highlightedKey = row.key
                    model.select(row.key)
                } label: {
                    HoverRow(selected: model.highlightedKey == row.key) { _ in
                        WorkloadRow(workload: row, share: Presentation.share(model.sort == .notInRAM ? row.notInRamBytes : row.footprintBytes, of: maximum))
                    }
                }.buttonStyle(.plain)
            }.id(row.key)
            if expanded {
                ProcessMembersView(workload: row, sort: model.sort, search: model.search, indent: Theme.memberIndent)
            }
        }
    }

    @ViewBuilder private func attention(_ status: StatusDoc) -> some View {
        let findings = status.findings.filter(\.isReview)
        let machine = Presentation.machineAttention(status)
        let alerts = model.activeAlerts.filter { machine == nil || !["memory-pressure", "swap"].contains($0.id) }
        if machine != nil || !findings.isEmpty || !alerts.isEmpty {
            VStack(alignment: .leading, spacing: Theme.small) {
                Group {
                    if !findings.isEmpty || !alerts.isEmpty { SectionHeader(title: "Needs attention") }
                    if let machine {
                        HStack(alignment: .firstTextBaseline, spacing: Theme.medium) {
                            Image(systemName: "exclamationmark.circle").foregroundStyle(Presentation.machineState(status.system) == .critical ? Theme.critical : Theme.attention)
                            Text(machine).foregroundStyle(Theme.primary).fixedSize(horizontal: false, vertical: true)
                        }.font(Theme.caption).padding(.vertical, Theme.small)
                            .help(Presentation.machineAttentionHelp(status))
                    }
                    ForEach(alerts) { alert in
                        Button {
                            if let key = alert.workloadKey, model.workload(forKey: key) != nil { model.select(key) }
                        } label: {
                            HoverRow { _ in
                                HStack(spacing: Theme.medium) {
                                    Circle().fill(Theme.attention).frame(width: Theme.small, height: Theme.small)
                                    Text(alert.text).font(Theme.title).lineLimit(2).fixedSize(horizontal: false, vertical: true)
                                    Spacer(minLength: Theme.small)
                                    if alert.workloadKey != nil { Image(systemName: "chevron.right").font(Theme.caption).foregroundStyle(Theme.secondary) }
                                }
                            }
                        }.buttonStyle(.plain)
                    }
                    ForEach(Array(findings.prefix(3))) { finding in
                        Button {
                            if let key = finding.workloadKey { model.select(key) }
                            else { model.switchTab(.findings) }
                        } label: {
                            HoverRow { _ in
                                HStack(spacing: Theme.medium) {
                                    Circle().fill(Theme.attention).frame(width: Theme.small, height: Theme.small)
                                    VStack(alignment: .leading, spacing: Theme.small) {
                                        Text(finding.title).font(Theme.title).lineLimit(2).truncationMode(.tail)
                                            .fixedSize(horizontal: false, vertical: true)
                                        Text(Presentation.findingDisplayName(finding, workloads: status.workloads))
                                            .font(Theme.caption).foregroundStyle(Theme.secondary)
                                            .lineLimit(1).truncationMode(.tail)
                                    }
                                    Spacer(minLength: Theme.small)
                                    Image(systemName: "chevron.right").font(Theme.caption).foregroundStyle(Theme.secondary)
                                }
                            }
                        }.buttonStyle(.plain).help(finding.title)
                    }
                    if !findings.isEmpty {
                        Button(status.findings.count == 1 ? "All findings" : "All \(Fmt.count(status.findings.count)) findings") { model.switchTab(.findings) }
                            .font(Theme.caption).buttonStyle(.plain).padding(.top, Theme.small)
                    }
                }
            }
        }
    }
}

struct OverviewHero: View {
    @Environment(AppModel.self) private var model
    let status: StatusDoc?

    var body: some View {
        HStack(alignment: .top, spacing: Theme.large) {
            VStack(alignment: .leading, spacing: Theme.small) {
                let selected = model.settings.value.heroView ?? .associated
                let effective = status.map { selected.effective(for: $0) } ?? selected
                let next = status.map { selected.next(for: $0) } ?? selected.next
                Button { model.settings.value.heroView = next } label: {
                    HStack(spacing: Theme.small) {
                        Text(effective.title).font(Theme.section)
                        Image(systemName: "chevron.up.chevron.down").font(Theme.caption)
                    }.foregroundStyle(Theme.secondary)
                }.buttonStyle(.plain).accessibilityLabel("Show " + next.title)
                Button { model.settings.value.heroView = next } label: {
                    Text(status.map { Fmt.bytes(Presentation.heroMemory($0, view: selected)) } ?? "—")
                        .font(Theme.hero).monospacedDigit().contentTransition(.numericText()).foregroundStyle(Theme.primary)
                }.buttonStyle(.plain).accessibilityLabel("Show " + next.title)
                    .help(Glossary.help(effective == .memoryUsed ? "physical-memory" : "agents-associated"))
                if let status {
                    Text(Presentation.heroCaption(status, view: selected))
                        .font(Theme.caption).foregroundStyle(Theme.secondary).lineLimit(1)
                        .help(Presentation.heroHelp(status, view: selected))
                    if Presentation.agentWorkloadCount(status) == 0 {
                        Link("Supported agents", destination: URL(string: "https://github.com/buddyh/procwho/blob/main/docs/SUPPORTED-AGENTS.md")!)
                            .font(Theme.caption)
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            VStack(spacing: Theme.large) {
                if let pressure = status?.system?.memoryPressure {
                    Gauge(label: "Pressure", value: pressure.capitalized,
                          fraction: Presentation.pressureFraction(pressure), loadState: Presentation.pressureState(pressure))
                        .help(Glossary.help("memory-pressure") + " Load: " + (status?.system?.load1.map { String(format: "%.1f", $0) } ?? "unknown"))
                } else if let load = status?.system?.load1, let cpus = status?.system?.ncpu,
                   let ratio = Presentation.loadRatio(load, cores: cpus) {
                    Gauge(label: "Load", value: String(format: "%.1f", load), fraction: ratio,
                          loadState: Presentation.loadState(load, cores: cpus))
                        .help(Glossary.help("load") + " This Mac has \(Fmt.count(cpus)) cores.")
                }
                if let system = status?.system, let used = system.swapUsedBytes {
                    Gauge(label: "Swap", value: Fmt.bytes(used), fraction: Presentation.swapRatio(used: used, ram: system.memTotalBytes) ?? 0, swap: true, loadState: Presentation.swapState(system))
                        .help(Presentation.swapGaugeHelp(system))
                }
            }.frame(width: Theme.gaugeColumn)
        }.padding(.vertical, Theme.small)
    }
}

struct WorkloadRow: View {
    @Environment(AppModel.self) private var model
    @Environment(\.colorSchemeContrast) private var contrast
    let workload: Workload
    let share: Double

    var body: some View {
        let family = KindFamily(kind: workload.kind)
        HStack(alignment: .top, spacing: Theme.medium) {
            KindIconTile(family: family, kind: workload.kind, bundlePath: workload.bundlePath).padding(.top, Theme.small)
            VStack(alignment: .leading, spacing: Theme.textGap) {
                Text(Presentation.displayName(workload.name)).font(Theme.title).lineLimit(2).truncationMode(.tail)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
                HStack(spacing: Theme.small) {
                    if workload.weakConfidence {
                        Image(systemName: "questionmark.circle").font(Theme.caption).foregroundStyle(Theme.secondary)
                            .help("Owner confidence: " + (workload.confidence ?? "none"))
                    }
                    if let link = workload.agentLink, !workload.isAgent {
                        Image(systemName: "link").font(Theme.caption).foregroundStyle(Theme.secondary)
                            .help("Linked to agents: " + link.via)
                    }
                    Text(subtitle).help(subtitle).font(Theme.caption).foregroundStyle(Theme.secondary).lineLimit(1).truncationMode(.tail)
                }
            }.frame(maxWidth: .infinity, alignment: .leading)
            VStack(alignment: .trailing, spacing: Theme.small) {
                Text(model.sort == .notInRAM ? Fmt.estimatedBytes(workload.notInRamBytes) : Fmt.bytes(workload.footprintBytes)).font(Theme.number)
                ShareBar(share: share, color: Theme.color(family, contrast: contrast == .increased))
            }
            .frame(width: Theme.memoryColumn, alignment: .trailing)
            .help(model.sort == .notInRAM ? Theme.notInRamHelp : (workload.footprintBytes == nil ? "Memory not reported" : Glossary.footprintSumHelp))
            Text(Fmt.cpu(workload.cpuPct)).font(Theme.caption).foregroundStyle(Theme.secondary)
                .frame(width: Theme.cpuColumn, alignment: .trailing)
                .help(Theme.cpuHelp)
        }
        .monospacedDigit().contentTransition(.numericText())
        .padding(.vertical, Theme.small).help(Presentation.displayName(workload.name))
    }

    private var subtitle: String {
        var parts: [String] = []
        if workload.kind == "unknown" { parts.append("Owner unknown") }
        if let count = workload.processes { parts.append("\(Fmt.count(count)) \(count == 1 ? "process" : "processes")") }
        if let age = workload.ageSeconds { parts.append(Fmt.age(age)) }

        let facts = parts.prefix(2).joined(separator: " · ")
        return facts + (model.settings.value.showsSwappedBadge(for: workload, ram: model.installedRAMBytes) ? ", mostly not in RAM" : "")
    }
}

private struct FirstLaunchCard: View {
    @Environment(AppModel.self) private var model
    var body: some View {
        Card {
            HStack(alignment: .top, spacing: Theme.medium) {
                VStack(alignment: .leading, spacing: Theme.small) {
                    Text("Procwho is read-only. It never stops or deletes anything.")
                }.font(Theme.caption).foregroundStyle(Theme.secondary)
                Spacer(minLength: Theme.zero)
                Button { model.settings.value.onboardingDismissed = true } label: { Image(systemName: "xmark") }
                    .buttonStyle(.plain).font(Theme.caption).help("Dismiss introduction")
                    .accessibilityLabel("Dismiss introduction")
            }
        }
    }
}
