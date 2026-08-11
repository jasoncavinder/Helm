import AppKit
import Foundation
import os.log
#if canImport(Sparkle)
import Sparkle
#endif

private let externalSparkleLogger = Logger(
    subsystem: "com.jasoncavinder.Helm",
    category: "external_sparkle_update"
)

final class ExternalSparkleUpdateCoordinator {
    static let shared = ExternalSparkleUpdateCoordinator()

    private static let helmBundleIdentifier = "com.jasoncavinder.Helm"

    #if canImport(Sparkle)
    private final class Session {
        let userDriver: SPUStandardUserDriver
        let updater: SPUUpdater

        init(userDriver: SPUStandardUserDriver, updater: SPUUpdater) {
            self.userDriver = userDriver
            self.updater = updater
        }
    }

    private var sessionsByBundlePath: [String: Session] = [:]
    #endif

    private init() {}

    @discardableResult
    func checkForUpdates(bundlePath: String) -> Bool {
        guard let (targetURL, targetBundle) = targetApplication(bundlePath: bundlePath),
              FileManager.default.fileExists(
                atPath: targetURL
                    .appendingPathComponent("Contents/Frameworks/Sparkle.framework")
                    .path
              ) else {
            externalSparkleLogger.error(
                "Rejected external Sparkle update target: \(bundlePath, privacy: .private(mask: .hash))"
            )
            return false
        }

        #if canImport(Sparkle)
        let normalizedPath = targetURL.path
        if let existing = sessionsByBundlePath[normalizedPath] {
            guard existing.updater.canCheckForUpdates else {
                externalSparkleLogger.notice(
                    "External Sparkle updater is already busy for \(targetBundle.bundleIdentifier ?? "unknown", privacy: .public)"
                )
                return false
            }
            existing.updater.checkForUpdates()
            return true
        }

        let userDriver = SPUStandardUserDriver(hostBundle: targetBundle, delegate: nil)
        let updater = SPUUpdater(
            hostBundle: targetBundle,
            applicationBundle: targetBundle,
            userDriver: userDriver,
            delegate: nil
        )
        do {
            try updater.start()
        } catch {
            externalSparkleLogger.error(
                "Failed to start external Sparkle updater for \(targetBundle.bundleIdentifier ?? "unknown", privacy: .public): \(error.localizedDescription, privacy: .public)"
            )
            return false
        }

        sessionsByBundlePath[normalizedPath] = Session(
            userDriver: userDriver,
            updater: updater
        )
        updater.checkForUpdates()
        externalSparkleLogger.info(
            "Started external Sparkle update check for \(targetBundle.bundleIdentifier ?? "unknown", privacy: .public)"
        )
        return true
        #else
        externalSparkleLogger.error("Sparkle framework unavailable for external app update")
        return false
        #endif
    }

    @discardableResult
    func openApplication(bundlePath: String) -> Bool {
        guard let (targetURL, targetBundle) = targetApplication(bundlePath: bundlePath) else {
            externalSparkleLogger.error(
                "Rejected external application launch target: \(bundlePath, privacy: .private(mask: .hash))"
            )
            return false
        }
        guard NSWorkspace.shared.open(targetURL) else {
            externalSparkleLogger.error(
                "Failed to open external application \(targetBundle.bundleIdentifier ?? "unknown", privacy: .public)"
            )
            return false
        }
        return true
    }

    private func targetApplication(bundlePath: String) -> (URL, Bundle)? {
        let targetURL = URL(fileURLWithPath: bundlePath)
            .standardizedFileURL
            .resolvingSymlinksInPath()
        guard isAllowedApplicationURL(targetURL),
              let targetBundle = Bundle(url: targetURL),
              targetBundle.bundleIdentifier != Self.helmBundleIdentifier else {
            return nil
        }
        return (targetURL, targetBundle)
    }

    private func isAllowedApplicationURL(_ url: URL) -> Bool {
        guard url.pathExtension == "app" else { return false }
        let allowedRoots = [
            URL(fileURLWithPath: "/Applications", isDirectory: true),
            FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent("Applications", isDirectory: true)
        ].map { $0.standardizedFileURL.resolvingSymlinksInPath().path + "/" }
        let candidate = url.path + "/"
        return allowedRoots.contains { candidate.hasPrefix($0) }
    }
}
