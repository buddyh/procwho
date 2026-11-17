import Foundation

/// Text formatting shared by the menu bar label and the window. Memory uses binary units, as
/// the CLI does (1G = 1024^3 bytes).
public enum Fmt {
    static let kib = 1024.0
    static let mib = kib * 1024
    static let gib = mib * 1024
    static let tib = gib * 1024

    /// Group human-facing counts consistently, including in SwiftUI interpolations.
    public static func count<T: BinaryInteger>(_ value: T) -> String {
        IntegerFormatStyle<T>(locale: Locale(identifier: "en_US")).format(value)
    }

    /// Installed physical RAM uses whole binary GB, unlike measured footprints.
    public static func installedRAM(_ bytes: Int64) -> String {
        let gb = (Double(max(0, bytes)) / gib).rounded()
        return gb.formatted(.number.precision(.fractionLength(0)).locale(Locale(identifier: "en_US"))) + " GB"
    }

    /// Compact form for the menu bar: "23.4G", "812M", "64K".
    public static func bytesCompact(_ bytes: Int64?) -> String {
        guard let bytes else { return "-" }
        let b = Double(max(0, bytes))
        if b >= tib { return sizeNumber(b / tib) + "T" }
        if b >= gib { return sizeNumber(b / gib) + "G" }
        if b >= mib { return String(format: "%.0fM", b / mib) }
        return String(format: "%.0fK", b / kib)
    }

    /// Binary MB/GB/TB: whole MB, up to one decimal GB/TB, whole values from 100.
    public static func bytes(_ bytes: Int64?) -> String {
        guard let bytes else { return "—" }
        let b = Double(max(0, bytes))
        if b >= tib { return sizeNumber(b / tib) + " TB" }
        if b >= gib { return sizeNumber(b / gib) + " GB" }
        if b > 0 && b < mib { return "<1 MB" }
        return String(format: "%.0f MB", b / mib)
    }

    private static func sizeNumber(_ value: Double) -> String {
        let text = String(format: value >= 100 ? "%.0f" : "%.1f", value)
        return text.hasSuffix(".0") ? String(text.dropLast(2)) : text
    }

    /// Mark estimates without turning missing measurements into numeric zeroes.
    public static func estimatedBytes(_ bytes: Int64?) -> String {
        bytes.map { "~" + self.bytes($0) } ?? "—"
    }

    /// Duration: "45s", "12m", "3h 12m", "4d 3h".
    public static func age(_ seconds: Double?) -> String {
        guard let seconds, seconds.isFinite else { return "-" }
        let s = Int(max(0, seconds))
        if s < 60 { return "\(s)s" }
        if s < 3600 { return "\(s / 60)m" }
        if s < 86400 {
            let m = (s % 3600) / 60
            return m == 0 ? "\(s / 3600)h" : "\(s / 3600)h \(m)m"
        }
        let h = (s % 86400) / 3600
        return h == 0 ? "\(s / 86400)d" : "\(s / 86400)d \(h)h"
    }

    /// CPU percent of one core: "0.4%", "12%", "402%".
    public static func cpu(_ pct: Double?) -> String {
        guard let pct, pct.isFinite else { return "-" }
        if pct < 10 { return String(format: "%.1f%%", pct) }
        return String(format: "%.0f%%", pct)
    }
}
