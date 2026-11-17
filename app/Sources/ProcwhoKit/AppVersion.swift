import Foundation

public enum AppVersion {
    /// Version string from the bundle's Info.plist (written by scripts/bundle.sh from app/VERSION).
    /// "dev" when running outside a bundle.
    public static var current: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "dev"
    }
}
