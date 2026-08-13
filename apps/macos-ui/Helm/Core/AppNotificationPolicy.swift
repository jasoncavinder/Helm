import Foundation

enum AppNotificationPreference {
    static func resolvedEnabled(storedValue: Any?) -> Bool {
        (storedValue as? NSNumber)?.boolValue ?? true
    }
}

struct AppUpdateNotificationEvaluation: Equatable {
    let observedFingerprint: String?
    let shouldNotify: Bool
}

enum AppUpdateNotificationPolicy {
    static func evaluate(
        updateIdentifiers: [String],
        previousFingerprint: String?,
        notificationsEnabled: Bool,
        interactiveSurfaceVisible: Bool
    ) -> AppUpdateNotificationEvaluation {
        let fingerprint = normalizedFingerprint(updateIdentifiers)
        guard fingerprint != previousFingerprint else {
            return AppUpdateNotificationEvaluation(
                observedFingerprint: previousFingerprint,
                shouldNotify: false
            )
        }

        return AppUpdateNotificationEvaluation(
            observedFingerprint: fingerprint,
            shouldNotify: fingerprint != nil
                && notificationsEnabled
                && !interactiveSurfaceVisible
        )
    }

    private static func normalizedFingerprint(_ identifiers: [String]) -> String? {
        let normalized = Set(
            identifiers.compactMap { identifier -> String? in
                let trimmed = identifier.trimmingCharacters(in: .whitespacesAndNewlines)
                return trimmed.isEmpty ? nil : trimmed
            }
        )
        .sorted()

        return normalized.isEmpty ? nil : normalized.joined(separator: "\u{1F}")
    }
}
