import AppKit
import Observation
import ServiceManagement
import UserNotifications

/// System actions are kept outside the preferences store and never run in snapshot mode.
@MainActor @Observable
final class SettingsServices {
    var loginEnabled = false
    var loginStatus = "Off"
    var loginError: String?
    var notificationStatus = "Notifications: not requested yet"
    var notificationNotRequested = true
    var notificationDenied = false
    var notificationError: String?
    private let snapshotMode: Bool
    /// Notifications need a real app bundle and are never touched in snapshot mode.
    private var notificationsAvailable: Bool {
        !snapshotMode && Bundle.main.bundleURL.pathExtension == "app"
    }

    init(snapshotMode: Bool) {
        self.snapshotMode = snapshotMode
        if !snapshotMode { refreshLogin() }
    }

    func refreshLogin() {
        guard !snapshotMode else { return }
        let status = SMAppService.mainApp.status
        loginEnabled = status == .enabled || status == .requiresApproval
        switch status {
        case .enabled: loginStatus = "On"
        case .requiresApproval: loginStatus = "Approval required in System Settings → General → Login Items."
        case .notRegistered: loginStatus = "Off"
        case .notFound: loginStatus = "App not found. Move Procwho to Applications."
        @unknown default: loginStatus = "Unknown status"
        }
    }

    func setLogin(_ enabled: Bool) {
        guard !snapshotMode else { return }
        loginError = nil
        do {
            if enabled { try SMAppService.mainApp.register() }
            else { try SMAppService.mainApp.unregister() }
        } catch { loginError = error.localizedDescription }
        refreshLogin()
    }

    func refreshNotificationStatus() async {
        guard notificationsAvailable else { return }
        let permission = await Self.currentPermission()
        notificationDenied = permission == .denied
        notificationNotRequested = permission == .notDetermined
        switch permission {
        case .notDetermined: notificationStatus = "Notifications: not requested yet"
        case .denied: notificationStatus = "Notifications: denied in System Settings"
        case .authorized, .provisional, .ephemeral: notificationStatus = "Notifications: allowed"
        case .unknown: notificationStatus = "Notifications: unknown permission"
        }
    }

    enum Permission: Sendable {
        case notDetermined, denied, authorized, provisional, ephemeral, unknown

        nonisolated init(_ authorizationStatus: UNAuthorizationStatus) {
            switch authorizationStatus {
            case .notDetermined: self = .notDetermined
            case .denied: self = .denied
            case .authorized: self = .authorized
            case .provisional: self = .provisional
            case .ephemeral: self = .ephemeral
            @unknown default: self = .unknown
            }
        }
    }

    // The SDK's notification center is not Sendable. Each helper below is nonisolated and
    // gets the center itself, so nothing main-actor-isolated is sent across isolation and
    // only our own Sendable values come back (needed for Swift 6.1 with the macOS 15 SDK).

    nonisolated static func currentPermission() async -> Permission {
        await withCheckedContinuation { continuation in
            UNUserNotificationCenter.current().getNotificationSettings { settings in
                continuation.resume(returning: Permission(settings.authorizationStatus))
            }
        }
    }

    private nonisolated static func requestAuthorization() async throws {
        _ = try await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound])
    }

    private nonisolated static func deliver(body: String) async throws {
        let content = UNMutableNotificationContent()
        content.title = "Procwho"
        content.body = body
        let request = UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil)
        try await UNUserNotificationCenter.current().add(request)
    }

    /// Call only from the user's explicit Request permission action.
    func enableAlerts() async {
        guard notificationsAvailable else {
            if !snapshotMode { notificationError = "Use the bundled Procwho app to enable notifications." }
            return
        }
        do { try await Self.requestAuthorization() }
        catch { notificationError = error.localizedDescription }
        await refreshNotificationStatus()
    }

    func post(_ alerts: [ProcwhoKit.WorkloadAlert]) async {
        guard notificationsAvailable else { return }
        let permission = await Self.currentPermission()
        guard permission == .authorized || permission == .provisional else { return }
        for alert in alerts {
            do { try await Self.deliver(body: alert.text) }
            catch { notificationError = "Could not deliver a Procwho notification." }
        }
    }
}

/// Notification clicks activate the app; foreground delivery also shows the banner.
final class NotificationDelegate: NSObject, UNUserNotificationCenterDelegate, @unchecked Sendable {
    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse) async {
        await MainActor.run { NSApp.activate(ignoringOtherApps: true) }
    }
    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter,
        willPresent notification: UNNotification) async -> UNNotificationPresentationOptions { [.banner, .list] }
}

import ProcwhoKit
