import Foundation

extension Presentation {
    /// A finding's opaque key is navigation data, never its display identity.
    public static func findingDisplayName(_ finding: Finding, workloads: [Workload]) -> String {
        if let key = finding.workloadKey, let row = workloads.first(where: { $0.key == key }) {
            return displayName(row.name)
        }
        return finding.reasons.first?.components(separatedBy: .newlines).first ?? finding.title
    }

}
