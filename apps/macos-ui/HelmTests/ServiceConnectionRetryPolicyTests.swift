import XCTest

final class ServiceConnectionRetryPolicyTests: XCTestCase {
    func testOnlyOneReconnectCanBeScheduledPerAttempt() {
        var policy = ServiceConnectionRetryPolicy()

        XCTAssertEqual(policy.scheduleReconnect(), 2)
        XCTAssertNil(policy.scheduleReconnect())
        XCTAssertEqual(policy.attempt, 1)
    }

    func testReconnectDelayBacksOffAndCapsAtOneMinute() {
        var policy = ServiceConnectionRetryPolicy()
        let expectedDelays: [TimeInterval] = [2, 4, 8, 16, 32, 60, 60]

        for expectedDelay in expectedDelays {
            XCTAssertEqual(policy.scheduleReconnect(), expectedDelay)
            policy.beginConnectionAttempt()
        }
    }

    func testVerifiedConnectionResetsBackoff() {
        var policy = ServiceConnectionRetryPolicy()

        XCTAssertEqual(policy.scheduleReconnect(), 2)
        policy.beginConnectionAttempt()
        XCTAssertEqual(policy.scheduleReconnect(), 4)

        policy.markConnected()

        XCTAssertEqual(policy.attempt, 0)
        XCTAssertFalse(policy.isReconnectScheduled)
        XCTAssertEqual(policy.scheduleReconnect(), 2)
    }

    func testDeferredOfflineRefreshPolicyWaitsForCurrentRefreshBeforeResuming() {
        XCTAssertEqual(
            DeferredOfflineRefreshPolicy.disposition(
                networkIsAvailable: true,
                refreshRequestedWhileOffline: true,
                refreshIsInFlight: true
            ),
            .waitForCurrentRefresh
        )
        XCTAssertEqual(
            DeferredOfflineRefreshPolicy.disposition(
                networkIsAvailable: true,
                refreshRequestedWhileOffline: true,
                refreshIsInFlight: false
            ),
            .resumeNow
        )
        XCTAssertEqual(
            DeferredOfflineRefreshPolicy.disposition(
                networkIsAvailable: false,
                refreshRequestedWhileOffline: true,
                refreshIsInFlight: false
            ),
            .none
        )
        XCTAssertEqual(
            DeferredOfflineRefreshPolicy.disposition(
                networkIsAvailable: true,
                refreshRequestedWhileOffline: false,
                refreshIsInFlight: false
            ),
            .none
        )
    }

    func testDeferredOfflineRefreshUsesCoreTaskTruthAfterPresentationTimeout() {
        let runningRefresh = DeferredOfflineRefreshTaskState(
            taskType: "refresh",
            status: "running"
        )
        let completedRefresh = DeferredOfflineRefreshTaskState(
            taskType: "refresh",
            status: "completed"
        )

        XCTAssertTrue(
            DeferredOfflineRefreshPolicy.refreshIsInFlight(
                presentationIsRefreshing: false,
                tasks: [runningRefresh]
            ),
            "a service refresh must remain authoritative after the UI safety timeout"
        )
        XCTAssertFalse(
            DeferredOfflineRefreshPolicy.refreshIsInFlight(
                presentationIsRefreshing: false,
                tasks: [completedRefresh]
            )
        )
    }
}
