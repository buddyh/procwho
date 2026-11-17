import Foundation

public enum CLIError: Error, Sendable, Equatable, CustomStringConvertible {
    case notFound(tried: [String])
    case busy(String)
    case timedOut(seconds: Double)
    case failed(status: Int32, stderr: String)
    case launch(String)
    case cancelled
    case decode(String)

    public var description: String {
        switch self {
        case .notFound(let tried): return "procwho binary not found. Tried: " + tried.joined(separator: ", ")
        case .busy(let k): return "\(k) is already running"
        case .timedOut(let s): return "procwho did not finish within \(Int(s)) s"
        case .failed(let st, let err):
            let msg = err.trimmingCharacters(in: .whitespacesAndNewlines)
            return "procwho exited with status \(st)" + (msg.isEmpty ? "" : ": " + String(msg.prefix(300)))
        case .launch(let m): return "Could not start procwho: \(m)"
        case .cancelled: return "Cancelled"
        case .decode(let m): return "Could not read procwho output: \(m)"
        }
    }
}

/// Runs the `procwho` binary with an argv array (no shell), off the main thread, with a timeout.
/// Two runs with the same `key` never overlap: the second one fails fast with `.busy`.
/// Cancelling the calling task terminates the child process. The only process this type ever
/// terminates is a child it started itself.
public actor CLIRunner {
    public let binary: String
    private var inFlight: Set<String> = []

    public init(binary: String) { self.binary = binary }

    public func isRunning(_ key: String) -> Bool { inFlight.contains(key) }

    public func run(_ arguments: [String], key: String, timeout: Double) async throws -> Data {
        guard inFlight.insert(key).inserted else { throw CLIError.busy(key) }
        defer { inFlight.remove(key) }
        let child = ChildProcess(executable: binary, arguments: arguments, timeout: timeout)
        return try await withTaskCancellationHandler {
            try await child.run()
        } onCancel: {
            child.cancel()
        }
    }

    public func decode<T: Decodable & Sendable>(_ type: T.Type, _ arguments: [String], key: String, timeout: Double) async throws -> T {
        let data = try await run(arguments, key: key, timeout: timeout)
        do {
            return try ProcwhoJSON.decode(type, from: data)
        } catch {
            throw CLIError.decode(String(describing: error))
        }
    }
}

/// One child process. Pipes are drained on background queues so a large JSON output cannot
/// block the child on a full pipe buffer.
final class ChildProcess: @unchecked Sendable {
    private let process = Process()
    private let timeout: Double
    private let lock = NSLock()
    private var timedOut = false
    private var cancelled = false

    init(executable: String, arguments: [String], timeout: Double) {
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = arguments
        process.qualityOfService = .utility
        self.timeout = timeout
    }

    func run() async throws -> Data {
        let out = Pipe()
        let err = Pipe()
        process.standardOutput = out
        process.standardError = err
        process.standardInput = FileHandle.nullDevice

        return try await withCheckedThrowingContinuation { (cont: CheckedContinuation<Data, Error>) in
            let group = DispatchGroup()
            let box = OutputBox()
            group.enter()
            group.enter()
            group.enter()
            process.terminationHandler = { _ in group.leave() }
            do {
                try process.run()
            } catch {
                cont.resume(throwing: CLIError.launch(error.localizedDescription))
                return
            }
            let queue = DispatchQueue.global(qos: .utility)
            queue.async {
                box.setOut(out.fileHandleForReading.readDataToEndOfFile())
                group.leave()
            }
            queue.async {
                box.setErr(err.fileHandleForReading.readDataToEndOfFile())
                group.leave()
            }
            queue.asyncAfter(deadline: .now() + timeout) { [self] in
                lock.lock()
                let running = process.isRunning
                if running { timedOut = true }
                lock.unlock()
                if running { process.terminate() }
            }
            lock.lock()
            let cancelledEarly = cancelled
            lock.unlock()
            if cancelledEarly { process.terminate() }

            group.notify(queue: queue) { [self] in
                lock.lock()
                let (to, cx) = (timedOut, cancelled)
                lock.unlock()
                if cx {
                    cont.resume(throwing: CLIError.cancelled)
                } else if to {
                    cont.resume(throwing: CLIError.timedOut(seconds: timeout))
                } else if process.terminationStatus != 0 {
                    cont.resume(throwing: CLIError.failed(
                        status: process.terminationStatus,
                        stderr: String(decoding: box.err, as: UTF8.self)))
                } else {
                    cont.resume(returning: box.out)
                }
            }
        }
    }

    func cancel() {
        lock.lock()
        cancelled = true
        let running = process.isRunning
        lock.unlock()
        if running { process.terminate() }
    }
}

private final class OutputBox: @unchecked Sendable {
    private let lock = NSLock()
    private var _out = Data()
    private var _err = Data()
    var out: Data { lock.lock(); defer { lock.unlock() }; return _out }
    var err: Data { lock.lock(); defer { lock.unlock() }; return _err }
    func setOut(_ d: Data) { lock.lock(); _out = d; lock.unlock() }
    func setErr(_ d: Data) { lock.lock(); _err = d; lock.unlock() }
}
