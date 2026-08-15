import Foundation

enum DeferredOfflineRefreshDisposition: Equatable {
    case none
    case waitForCurrentRefresh
    case resumeNow
}

enum DeferredOfflineRefreshPolicy {
    static func disposition(
        networkIsAvailable: Bool,
        refreshRequestedWhileOffline: Bool,
        isRefreshing: Bool
    ) -> DeferredOfflineRefreshDisposition {
        guard refreshRequestedWhileOffline, networkIsAvailable else {
            return .none
        }
        return isRefreshing ? .waitForCurrentRefresh : .resumeNow
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
