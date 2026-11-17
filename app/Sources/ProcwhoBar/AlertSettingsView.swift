import AppKit
import SwiftUI
import ProcwhoKit

struct AlertSettingsView: View {
    @Environment(AppModel.self) private var model
    var body: some View {
        @Bindable var store = model.settings
        Form {
            Section {
                Toggle("Alerts", isOn: Binding(get: { store.value.alertsEnabled }, set: { enabled in
                    store.value.alertsEnabled = enabled
                    if enabled { Task { await model.services.enableAlerts() } }
                }))
                if model.services.notificationDenied {
                    HStack {
                        Text("Notifications are denied").foregroundStyle(Theme.secondary)
                        Spacer()
                        Button("Open System Settings") {
                            NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.Notifications-Settings.extension")!)
                        }
                    }.font(Theme.caption)
                }
                if let error = model.services.notificationError { Text(error).font(Theme.caption) }
            }
            Section("Conditions") {
                automaticCondition("Swap above", value: Binding(get: { store.value.swapGB ?? 2 }, set: { store.value.swapGB = $0 }),
                                   automatic: Binding(get: { store.value.usesAutomaticSwap }, set: { store.value.swapAutomatic = $0 }),
                                   suggested: AutomaticThresholds(installedRAMBytes: model.installedRAMBytes).swapGB, enabled: $store.value.swapAlert).help(Glossary.help("swap"))
                LabeledContent("Memory pressure is critical") {
                    Toggle("Memory pressure is critical", isOn: Binding(get: { store.value.criticalPressureAlert != false }, set: { store.value.criticalPressureAlert = $0 })).labelsHidden().toggleStyle(.switch)
                }
                automaticCondition("Workload memory above", value: $store.value.memoryGB,
                                   automatic: Binding(get: { store.value.usesAutomaticMemory }, set: { store.value.memoryAutomatic = $0 }),
                                   suggested: AutomaticThresholds(installedRAMBytes: model.installedRAMBytes).memoryGB, enabled: $store.value.memoryAlert).help(Glossary.help("memory"))
                automaticCondition("Not in RAM above", value: $store.value.notInRAMGB,
                                   automatic: Binding(get: { store.value.usesAutomaticNotInRAM }, set: { store.value.notInRAMAutomatic = $0 }),
                                   suggested: AutomaticThresholds(installedRAMBytes: model.installedRAMBytes).notInRAMGB, enabled: $store.value.notInRAMAlert).help(Glossary.help("not-in-ram"))
                LabeledContent("New finding that needs review") {
                    Toggle("New finding that needs review", isOn: $store.value.reviewAlert).labelsHidden().toggleStyle(.switch)
                }
            }.disabled(!store.value.alertsEnabled)
            Section {
                LabeledContent("Repeat an alert at most every") { NumberSetting(value: $store.value.cooldownMinutes, unit: "min") }
            }
        }.formStyle(.grouped)
        .task { await model.services.refreshNotificationStatus() }
    }

    private func automaticCondition(_ title: String, value: Binding<Double>, automatic: Binding<Bool>,
                                    suggested: Double, enabled: Binding<Bool>) -> some View {
        HStack(spacing: Theme.medium) {
            AutomaticNumberSetting(title: title, value: value, automatic: automatic, suggested: suggested, unit: "GB")
                .disabled(!enabled.wrappedValue)
            Toggle(title, isOn: enabled).labelsHidden().toggleStyle(.switch)
        }
    }

}
