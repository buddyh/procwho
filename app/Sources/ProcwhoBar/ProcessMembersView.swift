import SwiftUI
import ProcwhoKit

/// Both the outline and detail use the same unbounded, lazy process rows.
struct ProcessMembersView: View {
    let workload: Workload
    let sort: WorkloadSort
    var search = ""
    var indent: CGFloat = Theme.zero

    var body: some View {
        if workload.members == nil {
            Text("Update the procwho CLI to list processes.")
                .font(Theme.caption).foregroundStyle(Theme.secondary)
                .padding(.leading, indent).padding(.vertical, Theme.small)
        } else {
            LazyVStack(alignment: .leading, spacing: Theme.zero) {
                ForEach(Presentation.members(workload, sort: sort, search: search)) { member in
                    ProcessMemberRow(member: member).padding(.leading, indent)
                }
            }
        }
    }
}

struct ProcessMemberRow: View {
    let member: ProcessMember

    var body: some View {
        HStack(spacing: Theme.medium) {
            HStack(spacing: Theme.small) {
                Text(member.name).font(Theme.title).lineLimit(1).truncationMode(.middle)
                Text(String(member.pid)).font(Theme.mono).foregroundStyle(Theme.secondary)
            }.frame(maxWidth: .infinity, alignment: .leading)
            Text(Fmt.bytes(member.footprintBytes)).font(Theme.title).monospacedDigit()
                .frame(width: Theme.memoryColumn, alignment: .trailing)
            Text(Fmt.cpu(member.cpuPct)).font(Theme.caption).foregroundStyle(Theme.secondary).monospacedDigit()
                .frame(width: Theme.cpuColumn, alignment: .trailing)
        }
        .padding(.horizontal, Theme.small).frame(minHeight: Theme.memberHeight)
        .contentShape(Rectangle()).help(member.command)
        .contextMenu {
            Button("Copy PID") { Actions.copy(String(member.pid)) }
            Button("Copy Command") { Actions.copy(member.command) }
        }
        .accessibilityElement(children: .combine)
    }
}
