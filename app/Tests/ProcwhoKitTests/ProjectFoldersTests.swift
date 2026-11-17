import Foundation
import Testing
@testable import ProcwhoKit

struct ProjectFoldersTests {
    @Test func emptyFolderSelectionDoesNotRestoreDefaultRoots() {
        var s = AppSettings(); s.worktreeFoldersChosen = true
        #expect(s.worktreeArguments.contains("--no-default-roots"))
        #expect(!s.worktreeArguments.contains("--root"))
    }
    @Test func decodesProtectedCandidatesWithoutCounts() throws {
        let data = Data(#"{"candidates":[{"path":"/Users/user/Documents","exists":true,"protected":true,"repo_count":null}],"automatic_folders":["/Users/user/.cursor/worktrees"]}"#.utf8)
        let result = try ProcwhoJSON.decode(ProjectFolders.self, from: data)
        #expect(result.candidates[0].protected)
        #expect(result.candidates[0].repoCount == nil)
        #expect(result.automaticFolders.count == 1)
    }
}
