import Combine
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
            interactiveSurfaceVisible: false,
            updatesReadySuppressedForExecution: false
        )

        XCTAssertNotNil(evaluation.observedFingerprint)
        XCTAssertTrue(evaluation.shouldNotify)
    }

    func testEquivalentSnapshotDoesNotNotifyAgain() {
        let initial = AppUpdateNotificationPolicy.evaluate(
            updateIdentifiers: ["npm|one|1|2", "cargo|two|3|4"],
            previousFingerprint: nil,
            notificationsEnabled: true,
            interactiveSurfaceVisible: false,
            updatesReadySuppressedForExecution: false
        )
        let repeated = AppUpdateNotificationPolicy.evaluate(
            updateIdentifiers: ["cargo|two|3|4", "npm|one|1|2", "npm|one|1|2"],
            previousFingerprint: initial.observedFingerprint,
            notificationsEnabled: true,
            interactiveSurfaceVisible: false,
            updatesReadySuppressedForExecution: false
        )

        XCTAssertEqual(repeated.observedFingerprint, initial.observedFingerprint)
        XCTAssertFalse(repeated.shouldNotify)
    }

    func testVisibleOrDisabledSnapshotsAreRecordedWithoutNotification() {
        let visible = AppUpdateNotificationPolicy.evaluate(
            updateIdentifiers: ["npm|one|1|2"],
            previousFingerprint: nil,
            notificationsEnabled: true,
            interactiveSurfaceVisible: true,
            updatesReadySuppressedForExecution: false
        )
        let disabled = AppUpdateNotificationPolicy.evaluate(
            updateIdentifiers: ["npm|one|1|3"],
            previousFingerprint: visible.observedFingerprint,
            notificationsEnabled: false,
            interactiveSurfaceVisible: false,
            updatesReadySuppressedForExecution: false
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
            interactiveSurfaceVisible: false,
            updatesReadySuppressedForExecution: false
        )

        XCTAssertNil(evaluation.observedFingerprint)
        XCTAssertFalse(evaluation.shouldNotify)
    }

    func testActiveUpdateExecutionConsumesSnapshotWithoutReplayingAfterCompletion() {
        let initial = AppUpdateNotificationPolicy.evaluate(
            updateIdentifiers: ["npm|one|1|2", "cargo|two|3|4"],
            previousFingerprint: nil,
            notificationsEnabled: true,
            interactiveSurfaceVisible: true,
            updatesReadySuppressedForExecution: false
        )
        let duringExecution = AppUpdateNotificationPolicy.evaluate(
            updateIdentifiers: ["cargo|two|3|4"],
            previousFingerprint: initial.observedFingerprint,
            notificationsEnabled: true,
            interactiveSurfaceVisible: false,
            updatesReadySuppressedForExecution: true
        )
        let afterExecution = AppUpdateNotificationPolicy.evaluate(
            updateIdentifiers: ["cargo|two|3|4"],
            previousFingerprint: duringExecution.observedFingerprint,
            notificationsEnabled: true,
            interactiveSurfaceVisible: false,
            updatesReadySuppressedForExecution: false
        )

        XCTAssertNotEqual(duringExecution.observedFingerprint, initial.observedFingerprint)
        XCTAssertFalse(duringExecution.shouldNotify)
        XCTAssertEqual(afterExecution.observedFingerprint, duringExecution.observedFingerprint)
        XCTAssertFalse(afterExecution.shouldNotify)
    }

    func testBackendExecutionWaitsForLatePostTerminalSnapshotBeforeReleasing() {
        var state = AppUpdateNotificationState()
        let initialIdentifiers = ["npm|one|1|2", "cargo|two|3|4", "pipx|three|5|6"]

        XCTAssertTrue(evaluate(&state, identifiers: initialIdentifiers).shouldNotify)
        XCTAssertFalse(
            evaluate(
                &state,
                identifiers: initialIdentifiers,
                observation: .backendExecutionStarted
            ).shouldNotify
        )
        let terminal = evaluate(
            &state,
            identifiers: initialIdentifiers,
            observation: .backendExecutionEnded(awaitingSnapshotRevision: 12)
        )
        XCTAssertTrue(terminal.updatesReadySuppressedForExecution)
        XCTAssertEqual(state.requiredPostExecutionSnapshotRevision, 12)

        let staleResponse = evaluate(
            &state,
            identifiers: ["cargo|two|3|4", "pipx|three|5|6"],
            observation: .postExecutionSnapshotPublished(revision: 11)
        )
        XCTAssertFalse(staleResponse.shouldNotify)
        XCTAssertTrue(staleResponse.updatesReadySuppressedForExecution)
        XCTAssertEqual(state.requiredPostExecutionSnapshotRevision, 12)

        let postExecutionResponse = evaluate(
            &state,
            identifiers: ["pipx|three|5|6"],
            observation: .postExecutionSnapshotPublished(revision: 12)
        )
        XCTAssertFalse(postExecutionResponse.shouldNotify)
        XCTAssertTrue(postExecutionResponse.updatesReadySuppressedForExecution)
        XCTAssertFalse(state.isExecutionSuppressed)

        let genuinelyLaterSnapshot = evaluate(
            &state,
            identifiers: ["pipx|three|5|6", "poetry|four|7|8"]
        )
        XCTAssertTrue(genuinelyLaterSnapshot.shouldNotify)
    }

    func testBackendExecutionConsumesSnapshotBeforeTerminalAndStillWaitsForFreshSnapshot() {
        var state = AppUpdateNotificationState()
        let initialIdentifiers = ["npm|one|1|2", "cargo|two|3|4"]
        let residualIdentifiers = ["cargo|two|3|4"]

        XCTAssertTrue(evaluate(&state, identifiers: initialIdentifiers).shouldNotify)
        _ = evaluate(
            &state,
            identifiers: initialIdentifiers,
            observation: .backendExecutionStarted
        )
        let activeSnapshot = evaluate(
            &state,
            identifiers: residualIdentifiers,
            observation: .postExecutionSnapshotPublished(revision: 20)
        )
        XCTAssertFalse(activeSnapshot.shouldNotify)
        XCTAssertTrue(state.isExecutionSuppressed)

        _ = evaluate(
            &state,
            identifiers: residualIdentifiers,
            observation: .backendExecutionEnded(awaitingSnapshotRevision: 21)
        )
        XCTAssertEqual(state.requiredPostExecutionSnapshotRevision, 21)
        let postTerminalSnapshot = evaluate(
            &state,
            identifiers: [],
            observation: .postExecutionSnapshotPublished(revision: 21)
        )
        XCTAssertFalse(postTerminalSnapshot.shouldNotify)
        XCTAssertNil(postTerminalSnapshot.observedFingerprint)
        XCTAssertFalse(state.isExecutionSuppressed)

        XCTAssertTrue(
            evaluate(
                &state,
                identifiers: residualIdentifiers + ["poetry|three|5|6"]
            ).shouldNotify
        )
    }

    func testHelmOnlyPlanConsumesCurrentSnapshotWithoutSuppressingLaterManualChanges() {
        var state = AppUpdateNotificationState()
        let helmUpdate = ["helm_self_update|helm|0.19.1|0.20.0"]

        XCTAssertTrue(evaluate(&state, identifiers: helmUpdate).shouldNotify)
        let planStart = evaluate(
            &state,
            identifiers: helmUpdate,
            observation: .helmOnlyPlanStarted
        )
        XCTAssertFalse(planStart.shouldNotify)
        XCTAssertTrue(planStart.updatesReadySuppressedForExecution)
        XCTAssertFalse(state.isExecutionSuppressed)

        let unrelatedManualCheckResult = evaluate(
            &state,
            identifiers: ["helm_self_update|helm|0.19.1|0.20.1"]
        )
        XCTAssertTrue(unrelatedManualCheckResult.shouldNotify)
        XCTAssertFalse(unrelatedManualCheckResult.updatesReadySuppressedForExecution)
    }

    func testOutdatedSnapshotRevisionRejectsOlderOutOfOrderResponses() {
        var revisions = OutdatedPackageSnapshotRevisionState()
        let first = revisions.issueRequest()
        let second = revisions.issueRequest()

        XCTAssertTrue(revisions.acceptResponse(revision: second))
        XCTAssertFalse(revisions.acceptResponse(revision: first))
        XCTAssertEqual(revisions.latestAppliedRevision, second)
    }

    func testLocalizedUnknownVersionDoesNotChangeAvailabilityFingerprint() {
        let frenchPackage = PackageItem(
            id: "npm:example",
            name: "example",
            version: "inconnu",
            latestVersion: "2.0.0",
            managerId: "npm",
            manager: "npm"
        )
        let germanPackage = PackageItem(
            id: frenchPackage.id,
            name: frenchPackage.name,
            version: "unbekannt",
            latestVersion: frenchPackage.latestVersion,
            managerId: frenchPackage.managerId,
            manager: frenchPackage.manager
        )
        let frenchSnapshot = AppUpdateNotificationSnapshot(
            packages: [frenchPackage],
            automaticUpdateCount: 1
        )
        let germanSnapshot = AppUpdateNotificationSnapshot(
            packages: [germanPackage],
            automaticUpdateCount: 1
        )
        var state = AppUpdateNotificationState()

        XCTAssertEqual(frenchSnapshot, germanSnapshot)
        XCTAssertTrue(
            evaluate(&state, identifiers: frenchSnapshot.updateIdentifiers).shouldNotify
        )
        XCTAssertFalse(
            evaluate(&state, identifiers: germanSnapshot.updateIdentifiers).shouldNotify
        )
    }

    func testQueuedAcceptedSnapshotsKeepEachRevisionBoundToItsOwnFingerprint() {
        let tracker = AppUpdateNotificationEventTracker()
        var state = AppUpdateNotificationState()
        let initialIdentifiers = ["npm|one|1|2", "cargo|two|3|4"]
        let firstSnapshot = AppUpdateNotificationSnapshot(
            updateIdentifiers: ["cargo|two|3|4"],
            updateCount: 1,
            automaticUpdateCount: 1
        )
        let laterSnapshot = AppUpdateNotificationSnapshot(
            updateIdentifiers: ["cargo|two|3|4", "poetry|three|5|6"],
            updateCount: 2,
            automaticUpdateCount: 2
        )
        let firstRevision = tracker.issueOutdatedPackagesSnapshotRequest()
        let laterRevision = tracker.issueOutdatedPackagesSnapshotRequest()

        _ = evaluate(&state, identifiers: initialIdentifiers)
        _ = evaluate(
            &state,
            identifiers: initialIdentifiers,
            observation: .backendExecutionStarted
        )
        _ = evaluate(
            &state,
            identifiers: initialIdentifiers,
            observation: .backendExecutionEnded(awaitingSnapshotRevision: firstRevision)
        )

        let delivered = expectation(description: "queued snapshot events delivered")
        delivered.expectedFulfillmentCount = 2
        var handledRevisions: [UInt64] = []
        let subscription = tracker.outdatedPackagesSnapshotPublisher
            .receive(on: RunLoop.main)
            .sink { event in
                defer { delivered.fulfill() }
                guard state.isExecutionSuppressed else { return }
                handledRevisions.append(event.revision)
                _ = state.evaluate(
                    updateIdentifiers: event.snapshot.updateIdentifiers,
                    notificationsEnabled: true,
                    interactiveSurfaceVisible: false,
                    observation: .postExecutionSnapshotPublished(revision: event.revision)
                )
            }

        XCTAssertTrue(tracker.acceptOutdatedPackagesSnapshotResponse(revision: firstRevision))
        tracker.publishOutdatedPackagesSnapshot(revision: firstRevision, snapshot: firstSnapshot)
        XCTAssertTrue(tracker.acceptOutdatedPackagesSnapshotResponse(revision: laterRevision))
        tracker.publishOutdatedPackagesSnapshot(revision: laterRevision, snapshot: laterSnapshot)

        wait(for: [delivered], timeout: 1)

        XCTAssertEqual(handledRevisions, [firstRevision])
        XCTAssertFalse(state.isExecutionSuppressed)
        XCTAssertTrue(
            evaluate(
                &state,
                identifiers: laterSnapshot.updateIdentifiers
            ).shouldNotify
        )
        withExtendedLifetime(subscription) {}
    }

    private func evaluate(
        _ state: inout AppUpdateNotificationState,
        identifiers: [String],
        observation: AppUpdateNotificationObservation = .availabilityChanged
    ) -> AppUpdateNotificationEvaluation {
        state.evaluate(
            updateIdentifiers: identifiers,
            notificationsEnabled: true,
            interactiveSurfaceVisible: false,
            observation: observation
        )
    }
}
