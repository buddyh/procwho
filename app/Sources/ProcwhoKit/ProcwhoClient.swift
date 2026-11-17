import Foundation

/// Typed calls to the procwho CLI. Prefers `status --json`; when the binary has no `status`
/// command it composes the same document from `top --json` and `findings --json` and reports
/// that it did so. The `status` probe is retried every 10 minutes, so a newer binary dropped
/// in place is picked up without restarting the app.
public actor ProcwhoClient {
    public enum StatusMode: String, Sendable { case native, fallback }

    public static let statusTimeout = 10.0
    public static let whyTimeout = 10.0
    public static let worktreesTimeout = 120.0
    static let probeInterval = 600.0

    public let runner: CLIRunner
    private var fallbackSince: Date?
    private var membersUnsupportedSince: Date?

    public init(binary: String) { runner = CLIRunner(binary: binary) }

    public var binary: String { runner.binary }

    public func status() async throws -> (StatusDoc, StatusMode) {
        if let since = fallbackSince, Date().timeIntervalSince(since) < Self.probeInterval {
            return (try await composedStatus(), .fallback)
        }
        do {
            let base = ["status", "--json", "--limit", "0"]
            let retryMembers = membersUnsupportedSince.map { Date().timeIntervalSince($0) >= Self.probeInterval } ?? true
            let doc: StatusDoc
            do {
                doc = try await runner.decode(StatusDoc.self, base + (retryMembers ? ["--members"] : []), key: "status", timeout: Self.statusTimeout)
                if retryMembers { membersUnsupportedSince = nil }
            } catch CLIError.failed(_, let stderr) where retryMembers && Self.isUnsupportedMembers(stderr) {
                membersUnsupportedSince = Date()
                doc = try await runner.decode(StatusDoc.self, base, key: "status", timeout: Self.statusTimeout)
            }
            fallbackSince = nil
            return (doc, .native)
        } catch CLIError.failed(_, let stderr) where Self.isUnknownCommand(stderr) {
            fallbackSince = Date()
            return (try await composedStatus(), .fallback)
        }
    }

    private func composedStatus() async throws -> StatusDoc {
        // Sequential: both commands sample the process table; running them together would
        // double the momentary load for no gain.
        let top = try await runner.decode(TopDoc.self, ["top", "--json", "-n", "10000"], key: "status", timeout: Self.statusTimeout)
        let findings = try await runner.decode(FindingsDoc.self, ["findings", "--json"], key: "status", timeout: Self.statusTimeout)
        return StatusDoc.compose(top: top, findings: findings.findings)
    }

    public func why(_ target: String) async throws -> WhyDoc {
        try await runner.decode(WhyDoc.self, ["why", target, "--json"], key: "why", timeout: Self.whyTimeout)
    }

    public func worktrees(settings: AppSettings = AppSettings()) async throws -> WorktreesDoc {
        try await runner.decode(WorktreesDoc.self, settings.validated().worktreeArguments, key: "worktrees", timeout: Self.worktreesTimeout)
    }

    static func isUnsupportedMembers(_ stderr: String) -> Bool {
        let message = stderr.lowercased()
        return message.contains("--members") && ["unexpected", "unknown", "unrecognized", "unsupported"].contains { message.contains($0) }
    }

    static func isUnknownCommand(_ stderr: String) -> Bool {
        stderr.contains("unknown command 'status'") || stderr.contains("unknown command \"status\"")
    }
}
