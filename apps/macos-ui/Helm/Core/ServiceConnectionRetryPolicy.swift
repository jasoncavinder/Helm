import Foundation

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
