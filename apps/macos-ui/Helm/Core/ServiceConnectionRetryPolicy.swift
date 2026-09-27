import Foundation

struct ServiceStartupSnapshot: Decodable {
    struct Experience: Decodable {
        let schemaVersion: Int
        let experienceId: String
        let acknowledged: Bool
    }

    let schemaVersion: Int
    let experience: Experience
    let onboardingCompleted: Bool
    let acceptedLicenseTermsVersion: String?
    let requiresFirstRunAcknowledgment: Bool
    let safeMode: Bool

    static func decode(_ json: String?, requiringAcknowledgment: Bool) -> Self? {
        guard let data = json?.data(using: .utf8) else { return nil }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let snapshot = try? decoder.decode(Self.self, from: data),
              snapshot.schemaVersion == 1,
              snapshot.experience.schemaVersion == 1,
              snapshot.experience.experienceId == "wayfinder-v0.20",
              snapshot.requiresFirstRunAcknowledgment == requiringAcknowledgment else { return nil }
        return snapshot
    }

    var permitsRuntimeActivation: Bool {
        !requiresFirstRunAcknowledgment || experience.acknowledged
    }
}

enum DeferredOfflineRefreshDisposition: Equatable {
    case none
    case waitForCurrentRefresh
    case resumeNow
}

struct DeferredOfflineRefreshTaskState: Equatable {
    let taskType: String
    let status: String
}

enum DeferredOfflineRefreshPolicy {
    static func disposition(
        networkIsAvailable: Bool,
        refreshRequestedWhileOffline: Bool,
        refreshIsInFlight: Bool
    ) -> DeferredOfflineRefreshDisposition {
        guard refreshRequestedWhileOffline, networkIsAvailable else {
            return .none
        }
        return refreshIsInFlight ? .waitForCurrentRefresh : .resumeNow
    }

    static func refreshIsInFlight(
        presentationIsRefreshing: Bool,
        tasks: [DeferredOfflineRefreshTaskState]
    ) -> Bool {
        presentationIsRefreshing || tasks.contains { task in
            let taskType = task.taskType.lowercased()
            let status = task.status.lowercased()
            return (taskType == "refresh" || taskType == "detection")
                && (status == "queued" || status == "running")
        }
    }
}

struct ServiceConnectionRetryPolicy {
    private(set) var attempt = 0
    private(set) var isReconnectScheduled = false

    mutating func scheduleReconnect() -> TimeInterval? {
        guard !isReconnectScheduled else { return nil }

        let delay = min(2.0 * pow(2.0, Double(attempt)), 60.0)
        attempt += 1
        isReconnectScheduled = true
        return delay
    }

    mutating func beginConnectionAttempt() {
        isReconnectScheduled = false
    }

    mutating func markConnected() {
        attempt = 0
        isReconnectScheduled = false
    }
}
