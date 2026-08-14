import AppKit
import Foundation
import os.log

private let externalSparkleLogger = Logger(
    subsystem: "com.jasoncavinder.Helm",
    category: "external_sparkle_application"
)

final class ExternalSparkleApplicationCoordinator {
    static let shared = ExternalSparkleApplicationCoordinator()

    private static let helmBundleIdentifier = "com.jasoncavinder.Helm"

    private init() {}

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
