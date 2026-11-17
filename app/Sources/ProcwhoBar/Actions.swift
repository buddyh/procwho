import AppKit

/// The only side effects the UI may cause: clipboard writes and showing files in Finder.
/// Nothing here stops, deletes or runs anything.
@MainActor
enum Actions {
    static func copy(_ text: String) {
        let pb = NSPasteboard.general
        pb.clearContents()
        pb.setString(text, forType: .string)
    }

    /// Selects the item in a Finder window (the `open -R <path>` equivalent).
    static func revealInFinder(_ path: String) {
        let url = URL(fileURLWithPath: path)
        guard FileManager.default.fileExists(atPath: url.path) else { return }
        NSWorkspace.shared.activateFileViewerSelecting([url])
    }

    static func exists(_ path: String?) -> Bool {
        guard let path else { return false }
        return FileManager.default.fileExists(atPath: path)
    }
}
