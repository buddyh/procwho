import AppKit
import SwiftUI

/// A local monitor scopes shortcuts to this panel and leaves text editing keys intact.
struct KeyboardNavigation: NSViewRepresentable {
    let model: AppModel

    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        context.coordinator.install(view: view, model: model)
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {}
    func makeCoordinator() -> Coordinator { Coordinator() }

    static func dismantleNSView(_ nsView: NSView, coordinator: Coordinator) {
        if let monitor = coordinator.monitor { NSEvent.removeMonitor(monitor) }
    }

    @MainActor final class Coordinator {
        var monitor: Any?
        func install(view: NSView, model: AppModel) {
            monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak view, weak model] event in
                guard let view, let model, view.window == NSApp.keyWindow else { return event }
                if [123, 124].contains(event.keyCode), view.window?.firstResponder is NSTextView { return event }
                let handled = PanelKeyboard.handle(keyCode: event.keyCode, characters: event.charactersIgnoringModifiers,
                                                   modifiers: event.modifierFlags, model: model)
                if handled && event.keyCode == 53 { view.window?.makeFirstResponder(nil) }
                return handled ? nil : event
            }
        }
    }
}

/// Shared routing for real local key events and navigation regression tests.
@MainActor
enum PanelKeyboard {
    static func handle(keyCode: UInt16, characters: String?, modifiers: NSEvent.ModifierFlags, model: AppModel) -> Bool {
        let modifiers = modifiers.intersection([.command, .option, .control, .shift])
        if modifiers == .command {
            switch characters {
            case ",": model.openSettings()
            case "1": model.switchTab(.workloads)
            case "2": model.switchTab(.findings)
            case "3": model.switchTab(.worktrees)
            case "f": model.switchTab(.workloads); model.searchFocusRequest += 1
            case "r": model.refreshNow()
            default: return false
            }
            return true
        }
        guard modifiers.isEmpty else { return false }
        switch keyCode {
        case 53: model.escape()
        case 125: model.moveSelection(1)
        case 126: model.moveSelection(-1)
        case 123, 124:
            guard model.tab == .workloads, model.selectedKey == nil,
                  let key = model.highlightedKey, model.visibleWorkloads.contains(where: { $0.key == key }) else { return false }
            if keyCode == 124 { model.expandedWorkloadKeys.insert(key) }
            else { model.expandedWorkloadKeys.remove(key) }
        case 36:
            guard model.tab == .workloads, model.selectedKey == nil,
                  let row = model.visibleWorkloads.first(where: { $0.key == model.highlightedKey }) ?? model.visibleWorkloads.first else { return false }
            model.select(row.key)
        default: return false
        }
        return true
    }
}
