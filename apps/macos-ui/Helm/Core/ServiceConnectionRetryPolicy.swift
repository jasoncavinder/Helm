import Foundation

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
