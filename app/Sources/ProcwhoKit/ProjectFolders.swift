import Foundation

/// Bounded CLI suggestions. Protected candidates deliberately have no probe count.
public struct ProjectFolders: Decodable, Sendable {
    public struct Candidate: Decodable, Identifiable, Sendable {
        public var path: String
        public var exists: Bool
        public var `protected`: Bool
        public var repoCount: Int?
        public var id: String { path }
        public init(path: String, exists: Bool = true, protected: Bool = false, repoCount: Int? = nil) {
            self.path = path; self.exists = exists; self.protected = protected; self.repoCount = repoCount
        }
    }
    public var candidates: [Candidate]
    public var automaticFolders: [String]
    public init(candidates: [Candidate] = [], automaticFolders: [String] = []) {
        self.candidates = candidates; self.automaticFolders = automaticFolders
    }
}
