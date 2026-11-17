import SwiftUI
import ProcwhoKit

struct WorkloadSettingsView: View {
    @Environment(AppModel.self) private var model
    var body: some View {
        @Bindable var store = model.settings
        Form {
            Section {
                Picker("Default sort", selection: $store.value.defaultSort) {
                    ForEach(WorkloadSort.allCases) { Text($0.title).help(Glossary.labelHelp($0.title)).tag($0) }
                }
                Toggle("Group by family", isOn: Binding(get: { store.value.groupByFamily == true }, set: { store.value.groupByFamily = $0 }))
                Toggle("Hide system workloads", isOn: Binding(get: { store.value.hideSystemWorkloads != false }, set: { store.value.hideSystemWorkloads = $0 }))
                LabeledContent("Hide workloads below") {
                    NumberSetting(value: $store.value.hideBelowMB, unit: "MB")
                }
            }
            Section {
                DisclosureGroup("Groups collapsed by default") {
                    ForEach(KindFamily.allCases) { family in
                        Toggle(family.title, isOn: Binding(get: { store.value.collapsedFamilies.contains(family) }, set: { enabled in
                            if enabled { store.value.collapsedFamilies.insert(family) }
                            else { store.value.collapsedFamilies.remove(family) }
                        })).toggleStyle(.checkbox).help(Glossary.help("family:" + family.rawValue))
                    }
                }
            }
            Section("Not in RAM") {
                Toggle("Show “mostly not in RAM”", isOn: $store.value.showSwappedBadge).help(Glossary.help("mostly-not-in-ram"))
                LabeledContent("Minimum share") { NumberSetting(value: $store.value.swappedSharePercent, unit: "%") }
                AutomaticNumberSetting(title: "Minimum size", value: $store.value.swappedMinimumMB,
                                       automatic: Binding(get: { store.value.usesAutomaticSwapped }, set: { store.value.swappedAutomatic = $0 }),
                                       suggested: AutomaticThresholds(installedRAMBytes: model.installedRAMBytes).swappedMinimumMB, unit: "MB")
            }
        }.formStyle(.grouped)
    }
}

struct NumberSetting: View {
    @Binding var value: Double
    let unit: String
    var body: some View {
        HStack(spacing: Theme.small) {
            TextField("", value: $value, format: .number).labelsHidden().multilineTextAlignment(.trailing)
                .frame(width: Theme.keyColumn).accessibilityLabel(unit)
            Text(unit).foregroundStyle(Theme.secondary).help(Glossary.help(unit.lowercased()))
        }.fixedSize()
    }
}

/// A number stays automatic until the user selects Custom and edits it.
struct AutomaticNumberSetting: View {
    let title: String
    @Binding var value: Double
    @Binding var automatic: Bool
    let suggested: Double
    let unit: String
    @State private var editing = false

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.small) {
            Text(title)
            HStack(spacing: Theme.medium) {
                if automatic && !editing {
                    Text("Automatic: \(suggested.formatted(.number)) \(unit)")
                        .font(Theme.caption).foregroundStyle(Theme.secondary)
                    Spacer(minLength: Theme.zero)
                    Button("Custom…") { editing = true }
                } else {
                    NumberSetting(value: Binding(get: { automatic ? suggested : value }, set: { value = $0; automatic = false }), unit: unit).accessibilityLabel(title)
                    Spacer(minLength: Theme.zero)
                    Button("Automatic") { automatic = true; editing = false }
                }
            }
        }
    }
}
