import Foundation

/// Finds the `procwho` binary. Order:
/// 1. `PROCWHO_BIN` environment variable
/// 2. `Contents/Resources/procwho` inside the app bundle
/// 3. `~/.local/bin/procwho`
/// 4. debug builds only: `target/release/procwho` in a repo checkout above the bundle/executable
public enum BinaryLocator {
    public static func candidates(
        environment: [String: String],
        resourceDirectory: URL?,
        homeDirectory: String,
        debugSearchStart: URL?
    ) -> [String] {
        var out: [String] = []
        if let env = environment["PROCWHO_BIN"], !env.isEmpty {
            out.append((env as NSString).expandingTildeInPath)
        }
        if let res = resourceDirectory {
            out.append(res.appendingPathComponent("procwho").path)
        }
        out.append((homeDirectory as NSString).appendingPathComponent(".local/bin/procwho"))
        if let start = debugSearchStart {
            // Walk up a few levels: app/build/Procwho.app or app/.build/debug/ProcwhoBar.
            var dir = start
            for _ in 0..<6 {
                dir = dir.deletingLastPathComponent()
                out.append(dir.appendingPathComponent("target/release/procwho").path)
            }
        }
        var seen = Set<String>()
        return out.filter { seen.insert($0).inserted }
    }

    public static func locate(_ candidates: [String], isExecutable: (String) -> Bool) -> String? {
        candidates.first(where: isExecutable)
    }

    /// Candidates for the running app.
    public static func defaultCandidates() -> [String] {
        #if DEBUG
        let debugStart: URL? = Bundle.main.bundleURL
        #else
        let debugStart: URL? = nil
        #endif
        return candidates(
            environment: ProcessInfo.processInfo.environment,
            resourceDirectory: Bundle.main.bundleURL.pathExtension == "app" ? Bundle.main.resourceURL : nil,
            homeDirectory: NSHomeDirectory(),
            debugSearchStart: debugStart
        )
    }

    public static func locateDefault() -> (path: String?, tried: [String]) {
        let c = defaultCandidates()
        return (locate(c) { FileManager.default.isExecutableFile(atPath: $0) }, c)
    }
}
