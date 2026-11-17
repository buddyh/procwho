import SwiftUI
import ProcwhoKit

struct FindingsView: View {
    @Environment(AppModel.self) private var model
    @State private var informationalExpanded = false

    var body: some View {
        if let status = model.status {
            if status.findings.isEmpty {
                VStack(spacing: Theme.zero) {
                    StatusWarning().padding(.horizontal, Theme.edge)
                    Text("Nothing needs review.").font(Theme.title).foregroundStyle(Theme.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading).padding(Theme.edge)
                    Spacer()
                }
            } else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: Theme.large, pinnedViews: [.sectionHeaders]) {
                        StatusWarning()
                        let reviews = status.findings.filter(\.isReview)
                        if reviews.isEmpty {
                            Text("Nothing needs review.").foregroundStyle(Theme.secondary)
                        }
                        if !reviews.isEmpty {
                            Section {
                                ForEach(reviews) { finding in
                                    FindingContent(finding: finding)
                                }
                            } header: { SectionHeader(title: "Needs review", detail: "\(Fmt.count(reviews.count))").background(Theme.background) }
                        }
                        let informational = status.findings.filter { !$0.isReview }
                        if !informational.isEmpty {
                            Section {
                                if informationalExpanded {
                                    ForEach(informational) { FindingContent(finding: $0) }
                                }
                            } header: {
                                Button { informationalExpanded.toggle() } label: {
                                    HStack(spacing: Theme.small) {
                                        Image(systemName: informationalExpanded ? "chevron.down" : "chevron.right")
                                            .font(Theme.caption).foregroundStyle(Theme.secondary)
                                        SectionHeader(title: "Informational", detail: "\(Fmt.count(informational.count))")
                                    }.background(Theme.background)
                                }.buttonStyle(.plain)
                            }
                        }
                    }.padding(.horizontal, Theme.edge).padding(.bottom, Theme.large)
                }
            }
        } else if model.statusError == nil {
            ScrollView { SkeletonRows().padding(.horizontal, Theme.edge) }
        } else {
            VStack {
                StatusWarning().padding(.horizontal, Theme.edge)
                EmptyState(text: "No sample available.")
                Button("Retry") { model.refreshNow() }.padding(Theme.large)
            }
        }
    }
}

private struct FindingContent: View {
    @Environment(AppModel.self) private var model
    let finding: Finding

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.medium) {
            HStack(alignment: .firstTextBaseline, spacing: Theme.medium) {
                Image(systemName: finding.isReview ? "circle.fill" : "info.circle")
                    .font(Theme.caption).foregroundStyle(finding.isReview ? Theme.attention : Theme.secondary)
                Text(finding.title).font(Theme.title).textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
            }
            BulletList(items: finding.reasons)
            ForEach(Array(finding.notProven.enumerated()), id: \.offset) { _, line in
                Text("Not proven: \(line)").font(Theme.caption).foregroundStyle(Theme.secondary)
                    .textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
            }
            HStack(spacing: Theme.medium) {
                if let basis = finding.basis { Chip(text: basis.prefix(1).capitalized + basis.dropFirst()) }
                Spacer(minLength: Theme.zero)
                if let key = finding.workloadKey {
                    Button("Show workload") { model.select(key) }.controlSize(.small)
                }
            }
            if let key = finding.workloadKey, let workload = model.workload(forKey: key) {
                Text(Presentation.displayName(workload.name)).font(Theme.caption).foregroundStyle(Theme.secondary).lineLimit(1).truncationMode(.middle)
            }
        }
    }
}
