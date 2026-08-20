import XCTest

final class GlobalSearchNavigationPolicyTests: XCTestCase {
    func testAcceptedResultTargetsTheSelectedLibraryEntity() throws {
        let deepLink = try XCTUnwrap(
            GlobalSearchNavigationPolicy.acceptedResultDeepLink(
                packageID: " search-ripgrep-homebrew "
            )
        )

        XCTAssertEqual(deepLink.destination, .library)
        XCTAssertEqual(deepLink.entityID, "search-ripgrep-homebrew")
        XCTAssertEqual(deepLink.focus, .selectedEntity)
    }

    func testEmptyResultIdentifierCannotNavigate() {
        XCTAssertNil(
            GlobalSearchNavigationPolicy.acceptedResultDeepLink(packageID: "  ")
        )
    }

    func testAcceptedResultNavigationClearsAStaleManagerFilter() throws {
        let decision = try XCTUnwrap(
            GlobalSearchNavigationPolicy.acceptedResultNavigation(
                packageID: "search-ripgrep-homebrew"
            )
        )

        XCTAssertEqual(decision.deepLink.destination, .library)
        XCTAssertEqual(decision.deepLink.entityID, "search-ripgrep-homebrew")
        XCTAssertNil(decision.managerFilterID)
    }
}

final class GlobalSearchSessionStateTests: XCTestCase {
    func testAcceptedOrDismissedQueryDoesNotReplayWithoutANewPresentation() {
        var state = ControlCenterGlobalSearchSessionState()
        state.updateQuery("ripgrep", presentsResults: true)
        XCTAssertTrue(state.isResultsPresented)

        state.dismiss()
        XCTAssertFalse(state.isResultsPresented)

        state.synchronize(
            isSearchFieldPresented: false,
            supportsGlobalResults: true,
            query: "ripgrep"
        )
        XCTAssertFalse(state.isResultsPresented)

        state.synchronize(
            isSearchFieldPresented: true,
            supportsGlobalResults: true,
            query: "ripgrep"
        )
        XCTAssertTrue(state.isResultsPresented)
    }

    func testLibraryQueryDoesNotCreateAGlobalResultsSession() {
        var state = ControlCenterGlobalSearchSessionState()
        state.updateQuery("ripgrep", presentsResults: false)
        XCTAssertFalse(state.isResultsPresented)

        state.synchronize(
            isSearchFieldPresented: true,
            supportsGlobalResults: false,
            query: "ripgrep"
        )
        XCTAssertFalse(state.isResultsPresented)
    }
}

final class ResearchSearchPresentationStateTests: XCTestCase {
    func testSameNormalizedQueryPreservesCompletedRemoteReveal() throws {
        var state = ResearchSearchPresentationState()
        let generation = try XCTUnwrap(
            state.update(query: "ripgrep", isOfflineVariant: false)
        )
        XCTAssertFalse(state.remoteResultsAvailable)
        XCTAssertTrue(state.revealRemoteResults(for: generation))
        XCTAssertTrue(state.remoteResultsAvailable)

        XCTAssertNil(
            state.update(query: "  RIPGREP  ", isOfflineVariant: false)
        )
        XCTAssertTrue(state.remoteResultsAvailable)
    }

    func testStaleRevealCannotPublishForANewerQuery() throws {
        var state = ResearchSearchPresentationState()
        let firstGeneration = try XCTUnwrap(
            state.update(query: "ripgrep", isOfflineVariant: false)
        )
        let secondGeneration = try XCTUnwrap(
            state.update(query: "cargo", isOfflineVariant: false)
        )

        XCTAssertFalse(state.revealRemoteResults(for: firstGeneration))
        XCTAssertFalse(state.remoteResultsAvailable)
        XCTAssertTrue(state.revealRemoteResults(for: secondGeneration))
        XCTAssertTrue(state.remoteResultsAvailable)
    }

    func testOfflineQueryExposesDeferredRemoteResultsWithoutSchedulingReveal() {
        var state = ResearchSearchPresentationState()

        XCTAssertNil(
            state.update(query: "ripgrep", isOfflineVariant: true)
        )
        XCTAssertTrue(state.remoteResultsAvailable)

        XCTAssertNil(state.update(query: "", isOfflineVariant: true))
        XCTAssertFalse(state.remoteResultsAvailable)
    }
}

final class RemoteSearchSessionStateTests: XCTestCase {
    func testQueryStartsIdleUntilItsSubmissionsBegin() throws {
        var state = RemoteSearchSessionState()

        let transition = state.updateQuery("  Ripgrep  ")
        let token = try XCTUnwrap(transition.token)

        XCTAssertTrue(transition.didChange)
        XCTAssertEqual(token.query, "Ripgrep")
        XCTAssertTrue(transition.taskIDsToCancel.isEmpty)
        XCTAssertFalse(state.isSearching)

        XCTAssertTrue(state.beginSubmissions(for: token, count: 2))
        XCTAssertTrue(state.isSearching)
        XCTAssertEqual(state.pendingSubmissionCount, 2)
    }

    func testSameNormalizedQueryPreservesTheCurrentSession() throws {
        var state = RemoteSearchSessionState()
        let firstToken = try XCTUnwrap(state.updateQuery("Ripgrep").token)
        XCTAssertTrue(state.beginSubmissions(for: firstToken, count: 1))
        XCTAssertEqual(state.resolveSubmission(for: firstToken, taskID: 41), .tracked)

        let transition = state.updateQuery("  RIPGREP  ")

        XCTAssertFalse(transition.didChange)
        XCTAssertEqual(transition.token, firstToken)
        XCTAssertTrue(transition.taskIDsToCancel.isEmpty)
        XCTAssertEqual(state.activeTaskIDs, [41])
        XCTAssertTrue(state.isSearching)
    }

    func testNewQueryCancelsOnlyTasksOwnedByThePreviousSession() throws {
        var state = RemoteSearchSessionState()
        let firstToken = try XCTUnwrap(state.updateQuery("ripgrep").token)
        XCTAssertTrue(state.beginSubmissions(for: firstToken, count: 2))
        XCTAssertEqual(state.resolveSubmission(for: firstToken, taskID: 41), .tracked)
        XCTAssertEqual(state.resolveSubmission(for: firstToken, taskID: 42), .tracked)

        let transition = state.updateQuery("cargo")

        XCTAssertEqual(transition.taskIDsToCancel, [41, 42])
        XCTAssertTrue(state.activeTaskIDs.isEmpty)
        XCTAssertEqual(state.pendingSubmissionCount, 0)
        XCTAssertFalse(state.isSearching)
    }

    func testLateSubmissionIsCancelledInsteadOfJoiningTheReplacementSession() throws {
        var state = RemoteSearchSessionState()
        let staleToken = try XCTUnwrap(state.updateQuery("ripgrep").token)
        XCTAssertTrue(state.beginSubmissions(for: staleToken, count: 1))
        let currentToken = try XCTUnwrap(state.updateQuery("cargo").token)
        XCTAssertTrue(state.beginSubmissions(for: currentToken, count: 1))

        XCTAssertEqual(
            state.resolveSubmission(for: staleToken, taskID: 41),
            .cancelStaleTask(41)
        )
        XCTAssertTrue(state.activeTaskIDs.isEmpty)
        XCTAssertEqual(state.pendingSubmissionCount, 1)

        XCTAssertEqual(state.resolveSubmission(for: currentToken, taskID: 42), .tracked)
        XCTAssertEqual(state.activeTaskIDs, [42])
    }

    func testStaleFailureDoesNotAffectTheCurrentSession() throws {
        var state = RemoteSearchSessionState()
        let staleToken = try XCTUnwrap(state.updateQuery("ripgrep").token)
        XCTAssertTrue(state.beginSubmissions(for: staleToken, count: 1))
        let currentToken = try XCTUnwrap(state.updateQuery("cargo").token)
        XCTAssertTrue(state.beginSubmissions(for: currentToken, count: 1))

        XCTAssertEqual(
            state.resolveSubmission(for: staleToken, taskID: -1),
            .staleFailure
        )
        XCTAssertEqual(state.pendingSubmissionCount, 1)
        XCTAssertTrue(state.isSearching)
    }

    func testCurrentFailuresFinishPendingSubmissionsWithoutInventingTasks() throws {
        var state = RemoteSearchSessionState()
        let token = try XCTUnwrap(state.updateQuery("ripgrep").token)
        XCTAssertTrue(state.beginSubmissions(for: token, count: 2))

        XCTAssertEqual(state.resolveSubmission(for: token, taskID: -1), .currentFailure)
        XCTAssertTrue(state.isSearching)
        XCTAssertEqual(state.resolveSubmission(for: token, taskID: -1), .currentFailure)
        XCTAssertFalse(state.isSearching)
        XCTAssertTrue(state.activeTaskIDs.isEmpty)
    }

    func testTerminalTaskSnapshotsCanOnlyFinishExplicitlyOwnedTasks() throws {
        var state = RemoteSearchSessionState()
        let token = try XCTUnwrap(state.updateQuery("ripgrep").token)
        XCTAssertTrue(state.beginSubmissions(for: token, count: 2))
        XCTAssertEqual(state.resolveSubmission(for: token, taskID: 41), .tracked)
        XCTAssertEqual(state.resolveSubmission(for: token, taskID: 42), .tracked)

        state.reconcileTaskSnapshot(
            visibleTaskIDs: [41, 42, 99],
            terminalTaskIDs: [42, 99]
        )

        XCTAssertEqual(state.activeTaskIDs, [41])
        XCTAssertTrue(state.isSearching)
        state.reconcileTaskSnapshot(
            visibleTaskIDs: [41],
            terminalTaskIDs: [41]
        )
        XCTAssertFalse(state.isSearching)
    }

    func testOneMissingSnapshotCannotRetireAJustReturnedTask() throws {
        var state = RemoteSearchSessionState()
        let token = try XCTUnwrap(state.updateQuery("ripgrep").token)
        XCTAssertTrue(state.beginSubmissions(for: token, count: 1))
        XCTAssertEqual(state.resolveSubmission(for: token, taskID: 41), .tracked)

        // This can be an older listTasks request that began before task 41 was submitted.
        state.reconcileTaskSnapshot(visibleTaskIDs: [], terminalTaskIDs: [])

        XCTAssertEqual(state.activeTaskIDs, [41])
        XCTAssertTrue(state.isSearching)
    }

    func testConsecutiveMissingSnapshotsRetireAnOwnedTask() throws {
        var state = RemoteSearchSessionState()
        let token = try XCTUnwrap(state.updateQuery("ripgrep").token)
        XCTAssertTrue(state.beginSubmissions(for: token, count: 1))
        XCTAssertEqual(state.resolveSubmission(for: token, taskID: 41), .tracked)

        state.reconcileTaskSnapshot(visibleTaskIDs: [], terminalTaskIDs: [])
        state.reconcileTaskSnapshot(visibleTaskIDs: [], terminalTaskIDs: [])

        XCTAssertTrue(state.activeTaskIDs.isEmpty)
        XCTAssertFalse(state.isSearching)
    }

    func testVisibleTaskResetsMissingSnapshotGrace() throws {
        var state = RemoteSearchSessionState()
        let token = try XCTUnwrap(state.updateQuery("ripgrep").token)
        XCTAssertTrue(state.beginSubmissions(for: token, count: 1))
        XCTAssertEqual(state.resolveSubmission(for: token, taskID: 41), .tracked)

        state.reconcileTaskSnapshot(visibleTaskIDs: [], terminalTaskIDs: [])
        state.reconcileTaskSnapshot(visibleTaskIDs: [41], terminalTaskIDs: [])
        state.reconcileTaskSnapshot(visibleTaskIDs: [], terminalTaskIDs: [])

        XCTAssertEqual(state.activeTaskIDs, [41])
        XCTAssertTrue(state.isSearching)
    }

    func testResetInvalidatesPendingCallbacksAndReturnsOwnedTasks() throws {
        var state = RemoteSearchSessionState()
        let token = try XCTUnwrap(state.updateQuery("ripgrep").token)
        XCTAssertTrue(state.beginSubmissions(for: token, count: 2))
        XCTAssertEqual(state.resolveSubmission(for: token, taskID: 41), .tracked)

        XCTAssertEqual(state.reset(), [41])
        XCTAssertFalse(state.isSearching)
        XCTAssertEqual(
            state.resolveSubmission(for: token, taskID: 42),
            .cancelStaleTask(42)
        )
    }
}

final class LibraryPackageFocusRequestStateTests: XCTestCase {
    func testRequestWaitsForSuccessfulFocusAndIsConsumedOnlyOnce() throws {
        var state = LibraryPackageFocusRequestState()
        let request = try XCTUnwrap(state.request(packageID: " search-ripgrep-homebrew "))

        XCTAssertEqual(request.packageID, "search-ripgrep-homebrew")
        XCTAssertEqual(state.pendingRequest, request)
        XCTAssertFalse(state.complete(request, focusSucceeded: false))
        XCTAssertEqual(state.pendingRequest, request)

        XCTAssertTrue(state.complete(request, focusSucceeded: true))
        XCTAssertNil(state.pendingRequest)
        XCTAssertEqual(state.lastCompletedRequestID, request.id)
        XCTAssertFalse(state.complete(request, focusSucceeded: true))
    }

    func testStaleCompletionCannotConsumeAReplacementRequest() throws {
        var state = LibraryPackageFocusRequestState()
        let firstRequest = try XCTUnwrap(state.request(packageID: "first"))
        let replacementRequest = try XCTUnwrap(state.request(packageID: "replacement"))

        XCTAssertFalse(state.complete(firstRequest, focusSucceeded: true))
        XCTAssertEqual(state.pendingRequest, replacementRequest)
        XCTAssertTrue(state.complete(replacementRequest, focusSucceeded: true))
    }

    func testEmptyPackageIdentifierDoesNotIssueARequest() {
        var state = LibraryPackageFocusRequestState()

        XCTAssertNil(state.request(packageID: "  \n "))
        XCTAssertNil(state.pendingRequest)
    }
}

final class ControlCenterSearchFocusRouterTests: XCTestCase {
    func testRequestBeforeAttachmentIsDeliveredWhenTargetAttaches() {
        let router = ControlCenterSearchFocusRouter()
        let target = SearchFocusTargetSpy()

        router.requestFocus()
        XCTAssertEqual(target.requestCount, 0)

        router.attach(target)
        XCTAssertEqual(target.requestCount, 1)

        target.completeFocusRequest()
        router.detach(target)
        router.attach(target)
        XCTAssertEqual(target.requestCount, 1)
    }

    func testUnfulfilledRequestMovesToReplacementTarget() {
        let router = ControlCenterSearchFocusRouter()
        let firstTarget = SearchFocusTargetSpy()
        let replacementTarget = SearchFocusTargetSpy()

        router.attach(firstTarget)
        router.requestFocus()
        router.detach(firstTarget)
        router.attach(replacementTarget)

        XCTAssertEqual(firstTarget.requestCount, 1)
        XCTAssertEqual(replacementTarget.requestCount, 1)
    }
}

final class ControlCenterSearchTextUpdateGateTests: XCTestCase {
    func testModelUpdateCannotPublishBackThroughControlDelegate() {
        let gate = ControlCenterSearchTextUpdateGate()
        var shouldPublishDuringUpdate = true

        gate.applyModelValue {
            shouldPublishDuringUpdate = gate.shouldPublishControlValue(
                "model value",
                modelValue: "old value"
            )
        }

        XCTAssertFalse(shouldPublishDuringUpdate)
        XCTAssertTrue(
            gate.shouldPublishControlValue("user value", modelValue: "model value")
        )
        XCTAssertFalse(
            gate.shouldPublishControlValue("model value", modelValue: "model value")
        )
    }

    func testControlUpdatesCoalesceWithoutBeingOverwrittenByStaleModel() {
        let gate = ControlCenterSearchTextUpdateGate()

        XCTAssertTrue(gate.stageControlValue("h", modelValue: ""))
        XCTAssertFalse(gate.stageControlValue("he", modelValue: ""))
        XCTAssertEqual(gate.displayedValue(modelValue: ""), "he")
        XCTAssertEqual(gate.takePendingControlValue(), "he")
        XCTAssertFalse(gate.hasScheduledControlPublish)
        XCTAssertEqual(gate.displayedValue(modelValue: "he"), "he")
    }
}

private final class SearchFocusTargetSpy: ControlCenterSearchFocusTarget {
    private(set) var requestCount = 0
    private var completion: (() -> Void)?

    func requestSearchFocus(completion: @escaping () -> Void) {
        requestCount += 1
        self.completion = completion
    }

    func completeFocusRequest() {
        completion?()
        completion = nil
    }
}
