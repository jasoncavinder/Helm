import AppKit
import SwiftUI
import XCTest

final class NativeToolbarSearchInteractionTests: XCTestCase {
    func testPauseAndResumeTypingUpdatesQueryWithoutAcceptingResultOrLosingFocus() throws {
        let fixture = try NativeSearchFixture()
        defer { fixture.close() }
        XCTAssertTrue(fixture.field.sendsWholeSearchString)

        fixture.editor.insertText("auth", replacementRange: NSRange(location: 0, length: 0))
        // Longer than AppKit's incremental-search delay: a pause must not act as Return.
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 1.2))

        XCTAssertEqual(fixture.query, "auth")
        XCTAssertEqual(fixture.acceptCount, 0)
        XCTAssertTrue(fixture.window.firstResponder === fixture.editor)
        XCTAssertEqual(fixture.editor.selectedRange(), NSRange(location: 4, length: 0))

        fixture.editor.insertText("orization", replacementRange: fixture.editor.selectedRange())
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 1.2))

        XCTAssertEqual(fixture.query, "authorization")
        XCTAssertEqual(fixture.acceptCount, 0)
        XCTAssertTrue(fixture.window.firstResponder === fixture.editor)
    }

    func testReturnAcceptsResultWithLatestQuery() throws {
        let fixture = try NativeSearchFixture()
        defer { fixture.close() }
        fixture.editor.insertText("authorization", replacementRange: NSRange(location: 0, length: 0))
        fixture.editor.doCommand(by: #selector(NSResponder.insertNewline(_:)))
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.1))

        XCTAssertEqual(fixture.acceptedQuery, "authorization")
        XCTAssertEqual(fixture.acceptCount, 1)
    }

    func testReturnWithoutAcceptedResultKeepsFocus() throws {
        let fixture = try NativeSearchFixture()
        defer { fixture.close() }
        fixture.acceptsResult = false
        fixture.editor.insertText("no-result", replacementRange: NSRange(location: 0, length: 0))
        fixture.editor.doCommand(by: #selector(NSResponder.insertNewline(_:)))
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.1))

        XCTAssertEqual(fixture.acceptCount, 1)
        XCTAssertTrue(fixture.window.firstResponder === fixture.editor)
    }

    func testCancelClearsQueryWithoutAcceptingResult() throws {
        let fixture = try NativeSearchFixture()
        defer { fixture.close() }
        fixture.editor.insertText("authorization", replacementRange: NSRange(location: 0, length: 0))
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.1))
        fixture.field.cancelOperation(nil)
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.1))

        XCTAssertEqual(fixture.query, "")
        XCTAssertEqual(fixture.acceptCount, 0)
        XCTAssertGreaterThan(fixture.cancelCount, 0)
    }
}

final class NativeToolbarSearchCoordinatorTests: XCTestCase {
    func testCancelSupersedesQueuedTypingBeforeDelegateDelivery() {
        var query = ""
        var cancelCount = 0
        let coordinator = ControlCenterToolbarSearchField.Coordinator(
            text: Binding(get: { query }, set: { query = $0 }),
            focusRouter: ControlCenterSearchFocusRouter(),
            onSubmit: { XCTFail("Cancel must not accept a result"); return false },
            onCancel: { cancelCount += 1 }
        )
        let field = NSSearchField()
        field.stringValue = "authorization"
        coordinator.controlTextDidChange(Notification(name: NSControl.textDidChangeNotification, object: field))
        XCTAssertTrue(coordinator.updateGate.hasScheduledControlPublish)

        coordinator.cancelSearch()
        XCTAssertEqual(coordinator.updateGate.displayedValue(modelValue: query), "")
        drainMainQueue()

        XCTAssertEqual(query, "")
        XCTAssertEqual(cancelCount, 1)
    }

    func testNativeClearActionSupersedesQueuedTyping() {
        var query = ""
        var cancelCount = 0
        let coordinator = ControlCenterToolbarSearchField.Coordinator(
            text: Binding(get: { query }, set: { query = $0 }),
            focusRouter: ControlCenterSearchFocusRouter(),
            onSubmit: { XCTFail("Clear must not accept a result"); return false },
            onCancel: { cancelCount += 1 }
        )
        let field = NSSearchField()
        field.stringValue = "authorization"
        coordinator.controlTextDidChange(Notification(name: NSControl.textDidChangeNotification, object: field))
        field.stringValue = ""
        coordinator.submitSearch(field)
        drainMainQueue()

        XCTAssertEqual(query, "")
        XCTAssertEqual(cancelCount, 1)
    }

    func testSubmitSupersedesQueuedTypingWithLatestFieldValue() {
        var query = ""
        var acceptedQueries: [String] = []
        let coordinator = ControlCenterToolbarSearchField.Coordinator(
            text: Binding(get: { query }, set: { query = $0 }),
            focusRouter: ControlCenterSearchFocusRouter(),
            onSubmit: { acceptedQueries.append(query); return false },
            onCancel: { XCTFail("Submit must not cancel") }
        )
        let field = NSSearchField()
        field.stringValue = "auth"
        coordinator.controlTextDidChange(Notification(name: NSControl.textDidChangeNotification, object: field))
        field.stringValue = "authorization"
        coordinator.submitSearch(field)
        drainMainQueue()

        XCTAssertEqual(query, "authorization")
        XCTAssertEqual(acceptedQueries, ["authorization"])
    }

    func testTypingAfterCancelBeforeDeliveryStillPublishes() {
        var query = ""
        let coordinator = ControlCenterToolbarSearchField.Coordinator(
            text: Binding(get: { query }, set: { query = $0 }),
            focusRouter: ControlCenterSearchFocusRouter(),
            onSubmit: { XCTFail("Typing must not accept a result"); return false },
            onCancel: {}
        )
        let field = NSSearchField()
        field.stringValue = "authorization"
        coordinator.controlTextDidChange(Notification(name: NSControl.textDidChangeNotification, object: field))
        coordinator.cancelSearch()
        field.stringValue = "new query"
        coordinator.controlTextDidChange(Notification(name: NSControl.textDidChangeNotification, object: field))
        drainMainQueue()

        XCTAssertEqual(query, "new query")
        XCTAssertFalse(coordinator.updateGate.hasScheduledControlPublish)
    }

    private func drainMainQueue() {
        let delivered = expectation(description: "Queued delegate publication delivered")
        DispatchQueue.main.async { delivered.fulfill() }
        wait(for: [delivered], timeout: 2)
    }
}

private final class NativeSearchFixture {
    var query = ""
    var acceptsResult = true
    var acceptedQuery: String?
    var acceptCount = 0
    var cancelCount = 0
    let window: NSWindow
    private(set) var field: NSSearchField!
    private(set) var editor: NSTextView!

    init() throws {
        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 400, height: 80),
            styleMask: [.titled], backing: .buffered, defer: false
        )
        window.isReleasedWhenClosed = false
        let host = NSHostingView(rootView: ControlCenterToolbarSearchField(
            text: Binding(get: { self.query }, set: { self.query = $0 }),
            placeholder: "Search",
            focusRouter: ControlCenterSearchFocusRouter(),
            onSubmit: {
                self.acceptCount += 1
                self.acceptedQuery = self.query
                return self.acceptsResult
            },
            onCancel: { self.cancelCount += 1 }
        ).frame(width: 320))
        window.contentView = host
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.1))
        field = try XCTUnwrap(findSearchField(in: host))
        XCTAssertTrue(window.makeFirstResponder(field))
        editor = try XCTUnwrap(field.currentEditor() as? NSTextView)
    }

    func close() {
        window.close()
        window.contentView = nil
    }

    private func findSearchField(in view: NSView) -> NSSearchField? {
        if let field = view as? NSSearchField { return field }
        return view.subviews.lazy.compactMap { self.findSearchField(in: $0) }.first
    }
}

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

final class HelmServiceRemoteSearchContractTests: XCTestCase {
    func testProtocolKeepsDescriptionSubmissionAndInteractiveCancellationExplicit() {
        let descriptionSelector = #selector(
            HelmServiceProtocol.triggerPackageDescriptionSearchForManager(
                managerId:query:withReply:
            )
        )
        let cancellationSelector = #selector(
            HelmServiceProtocol.cancelRemoteSearchTask(taskId:withReply:)
        )

        XCTAssertTrue(
            NSStringFromSelector(descriptionSelector)
                .contains("triggerPackageDescriptionSearchForManager")
        )
        XCTAssertTrue(
            NSStringFromSelector(cancellationSelector)
                .contains("cancelRemoteSearchTask")
        )
        XCTAssertNotEqual(
            descriptionSelector,
            #selector(
                HelmServiceProtocol.triggerRemoteSearchForManager(
                    managerId:query:withReply:
                )
            )
        )
        XCTAssertNotEqual(
            cancellationSelector,
            #selector(HelmServiceProtocol.cancelTask(taskId:withReply:))
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
