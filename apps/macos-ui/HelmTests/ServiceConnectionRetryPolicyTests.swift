import XCTest

final class ServiceConnectionRetryPolicyTests: XCTestCase {
    func testBackgroundChecksRequireBothStartupAndConnectivityInEitherOrder() {
        for networkFirst in [true, false] {
            var gate = AppUpdateExecutionGate()
            XCTAssertFalse(gate.allowsChecks)
            gate.deferCheck(.information)
            gate.networkAvailable = networkFirst
            gate.runtimeAvailable = !networkFirst
            XCTAssertNil(gate.takeReadyCheck())
            XCTAssertEqual(gate.pendingCheckKind, .information)
            gate.networkAvailable = true
            gate.runtimeAvailable = true
            XCTAssertTrue(gate.allowsChecks)
            XCTAssertEqual(gate.takeReadyCheck(), .information)
            XCTAssertNil(gate.takeReadyCheck())
        }
    }

    func testReconnectDefersChecksWithoutDiscardingManualIntent() {
        var gate = AppUpdateExecutionGate(networkAvailable: true, runtimeAvailable: true)
        gate.runtimeAvailable = false
        gate.deferCheck(.userInitiated)
        gate.deferCheck(.information)
        XCTAssertNil(gate.takeReadyCheck())
        gate.runtimeAvailable = true
        XCTAssertEqual(gate.takeReadyCheck(), .userInitiated)
        XCTAssertNil(gate.takeReadyCheck())
    }

    private func startupJSON(required: Bool = true, acknowledged: Bool = false) -> String {
        """
        {"schema_version":1,"experience":{"schema_version":1,"experience_id":"wayfinder-v0.20","acknowledged":\(acknowledged)},
        "requires_first_run_acknowledgment":\(required),"onboarding_completed":true,
        "accepted_license_terms_version":"existing-terms","safe_mode":false}
        """
    }

    func testLegacyCompletionDoesNotAcknowledgeNewExperience() throws {
        let snapshot = try XCTUnwrap(ServiceStartupSnapshot.decode(startupJSON(), requiringAcknowledgment: true))
        XCTAssertTrue(snapshot.onboardingCompleted)
        XCTAssertEqual(snapshot.acceptedLicenseTermsVersion, "existing-terms")
        XCTAssertFalse(snapshot.permitsRuntimeActivation)
    }

    func testAcknowledgmentAndLegacyCompatibilityAllowActivation() throws {
        XCTAssertTrue(try XCTUnwrap(ServiceStartupSnapshot.decode(
            startupJSON(acknowledged: true), requiringAcknowledgment: true
        )).permitsRuntimeActivation)
        XCTAssertTrue(try XCTUnwrap(ServiceStartupSnapshot.decode(
            startupJSON(required: false), requiringAcknowledgment: false
        )).permitsRuntimeActivation)
    }

    func testMalformedOrIncompatibleStartupReplyFailsClosed() {
        for json in [nil, "", "{}", startupJSON(required: false),
                     startupJSON().replacingOccurrences(of: "wayfinder-v0.20", with: "unknown"),
                     startupJSON().replacingOccurrences(of: "\"schema_version\":1", with: "\"schema_version\":2"),
                     startupJSON().replacingOccurrences(of: "\"acknowledged\":false", with: "\"acknowledged\":null")] {
            XCTAssertNil(ServiceStartupSnapshot.decode(json, requiringAcknowledgment: true))
        }
    }

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
