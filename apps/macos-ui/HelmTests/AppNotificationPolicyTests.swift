import XCTest

final class AppNotificationPolicyTests: XCTestCase {
    func testNotificationPreferenceDefaultsOnAndPreservesStoredChoice() {
        XCTAssertTrue(AppNotificationPreference.resolvedEnabled(storedValue: nil))
        XCTAssertTrue(AppNotificationPreference.resolvedEnabled(storedValue: NSNumber(value: true)))
        XCTAssertFalse(AppNotificationPreference.resolvedEnabled(storedValue: NSNumber(value: false)))
    }

    func testNewHiddenUpdateSnapshotRequestsNotification() {
        let evaluation = AppUpdateNotificationPolicy.evaluate(
            updateIdentifiers: ["npm|one|1|2", "cargo|two|3|4"],
            previousFingerprint: nil,
            notificationsEnabled: true,
            interactiveSurfaceVisible: false
        )

        XCTAssertNotNil(evaluation.observedFingerprint)
        XCTAssertTrue(evaluation.shouldNotify)
    }

    func testEquivalentSnapshotDoesNotNotifyAgain() {
        let initial = AppUpdateNotificationPolicy.evaluate(
            updateIdentifiers: ["npm|one|1|2", "cargo|two|3|4"],
            previousFingerprint: nil,
            notificationsEnabled: true,
            interactiveSurfaceVisible: false
        )
        let repeated = AppUpdateNotificationPolicy.evaluate(
            updateIdentifiers: ["cargo|two|3|4", "npm|one|1|2", "npm|one|1|2"],
            previousFingerprint: initial.observedFingerprint,
            notificationsEnabled: true,
            interactiveSurfaceVisible: false
        )

        XCTAssertEqual(repeated.observedFingerprint, initial.observedFingerprint)
        XCTAssertFalse(repeated.shouldNotify)
    }

    func testVisibleOrDisabledSnapshotsAreRecordedWithoutNotification() {
        let visible = AppUpdateNotificationPolicy.evaluate(
            updateIdentifiers: ["npm|one|1|2"],
            previousFingerprint: nil,
            notificationsEnabled: true,
            interactiveSurfaceVisible: true
        )
        let disabled = AppUpdateNotificationPolicy.evaluate(
            updateIdentifiers: ["npm|one|1|3"],
            previousFingerprint: visible.observedFingerprint,
            notificationsEnabled: false,
            interactiveSurfaceVisible: false
        )

        XCTAssertNotNil(visible.observedFingerprint)
        XCTAssertFalse(visible.shouldNotify)
        XCTAssertNotEqual(disabled.observedFingerprint, visible.observedFingerprint)
        XCTAssertFalse(disabled.shouldNotify)
    }

    func testEmptySnapshotClearsObservedFingerprint() {
        let evaluation = AppUpdateNotificationPolicy.evaluate(
            updateIdentifiers: [],
            previousFingerprint: "existing",
            notificationsEnabled: true,
            interactiveSurfaceVisible: false
        )

        XCTAssertNil(evaluation.observedFingerprint)
        XCTAssertFalse(evaluation.shouldNotify)
    }
}
