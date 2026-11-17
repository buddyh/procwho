import Foundation

/// Binary units match Activity Monitor. Rounding is to the nearest step, ties upward.
public struct AutomaticThresholds: Equatable, Sendable {
    public let swapGB: Double
    public let memoryGB: Double
    public let notInRAMGB: Double
    public let swappedMinimumMB: Double

    public init(installedRAMBytes: UInt64) {
        let gb = Double(installedRAMBytes) / 1_073_741_824
        swapGB = max(2, (gb / 4 * 2).rounded() / 2)
        memoryGB = max(2, (gb / 6 * 2).rounded() / 2)
        notInRAMGB = max(1, (gb / 12 * 2).rounded() / 2)
        swappedMinimumMB = max(100, (gb * 1024 / 160 / 50).rounded() * 50)
    }
}
