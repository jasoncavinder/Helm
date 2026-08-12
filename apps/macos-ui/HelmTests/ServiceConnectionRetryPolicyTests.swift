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
}
