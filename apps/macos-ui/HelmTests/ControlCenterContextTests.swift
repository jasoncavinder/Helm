import Combine
import Foundation
import SwiftUI
import XCTest

final class ControlCenterContextTests: XCTestCase {
    func testLocaleChangeInvalidatesMountedActivityAndInspectorHosts() {
        let localizationChanges = PassthroughSubject<Void, Never>()
        let context = ControlCenterContextBase(
            localizationChanges: localizationChanges.eraseToAnyPublisher()
        )
        XCTAssertEqual(context.localeRevision, 0)
        let tracker = ControlCenterLocaleRenderTracker()
        let hostingView = NSHostingView(
            rootView: HStack {
                ControlCenterLocaleProbe(host: .activity, tracker: tracker)
                ControlCenterLocaleProbe(host: .inspector, tracker: tracker)
            }
            .environmentObject(context)
        )
        hostingView.frame = NSRect(x: 0, y: 0, width: 320, height: 80)

        XCTAssertTrue(
            waitUntil {
                hostingView.layoutSubtreeIfNeeded()
                return tracker.didRenderBothHosts
            },
            "Expected both Control Center locale-invalidating hosts to mount"
        )
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.05))
        hostingView.layoutSubtreeIfNeeded()

        let activityRenderCount = tracker.renderCount(for: .activity)
        let inspectorRenderCount = tracker.renderCount(for: .inspector)
        var forwardedChangeCount = 0
        let observation = context.objectWillChange.sink {
            forwardedChangeCount += 1
        }

        localizationChanges.send(())

        XCTAssertEqual(forwardedChangeCount, 1)
        XCTAssertEqual(context.localeRevision, 1)
        XCTAssertTrue(
            waitUntil {
                hostingView.layoutSubtreeIfNeeded()
                return tracker.renderCount(for: .activity) > activityRenderCount
                    && tracker.renderCount(for: .inspector) > inspectorRenderCount
            },
            "Expected a locale change to invalidate both mounted hosts"
        )
        withExtendedLifetime(observation) {}
        withExtendedLifetime(hostingView) {}
    }

    func testLocalePulseAndSubclassPublishedStateShareObjectWillChange() {
        let localizationChanges = PassthroughSubject<Void, Never>()
        let context = ControlCenterLocaleStateProbe(
            localizationChanges: localizationChanges.eraseToAnyPublisher()
        )
        var publishedChangeCount = 0
        let observation = context.objectWillChange.sink {
            publishedChangeCount += 1
        }

        localizationChanges.send(())

        XCTAssertEqual(publishedChangeCount, 1)

        context.selectedPackageId = "package-1"

        XCTAssertEqual(publishedChangeCount, 2)
        withExtendedLifetime(observation) {}
    }

    func testLocaleRefreshHostPreservesMountedChildStateAndLifecycle() {
        let localizationChanges = PassthroughSubject<Void, Never>()
        let presentReview = PassthroughSubject<Void, Never>()
        let context = ControlCenterContextBase(
            localizationChanges: localizationChanges.eraseToAnyPublisher()
        )
        let tracker = ControlCenterLocaleStateTracker()
        let hostingView = NSHostingView(
            rootView: ControlCenterLocaleStatePreservationHost(
                context: context,
                presentReview: presentReview.eraseToAnyPublisher(),
                tracker: tracker
            )
        )
        hostingView.frame = NSRect(x: 0, y: 0, width: 320, height: 80)

        XCTAssertTrue(
            waitUntil {
                hostingView.layoutSubtreeIfNeeded()
                return tracker.latestRevision == 0 && tracker.appearCount == 1
            },
            "Expected the locale refresh host to mount its stateful child"
        )
        let initialLifecycleID = tracker.latestLifecycleID
        let initialStateToken = tracker.latestStateToken

        presentReview.send(())

        XCTAssertTrue(
            waitUntil {
                hostingView.layoutSubtreeIfNeeded()
                return tracker.reviewPresented
            },
            "Expected representative child presentation state to become active"
        )

        localizationChanges.send(())

        XCTAssertTrue(
            waitUntil {
                hostingView.layoutSubtreeIfNeeded()
                return tracker.latestRevision == 1
                    && tracker.latestRenderedText == "locale-revision-1"
            },
            "Expected the mounted child to render the new locale revision"
        )
        XCTAssertEqual(tracker.latestLifecycleID, initialLifecycleID)
        XCTAssertEqual(tracker.latestStateToken, initialStateToken)
        XCTAssertTrue(tracker.reviewPresented)
        XCTAssertEqual(tracker.appearCount, 1)
        XCTAssertEqual(tracker.disappearCount, 0)
        withExtendedLifetime(hostingView) {}
    }

    func testRetainedTaskDescriptionResolvesAtPresentationTime() {
        let presentation = TaskDescriptionPresentation(
            rawDescription: "raw persisted fallback",
            labelKey: "service.task.label.install.package",
            labelArgs: ["manager": "Homebrew", "package": "ripgrep"]
        )
        var presentationLocale = "en"
        var resolutionCount = 0
        let resolve: (String, [String: String]) -> String? = { key, arguments in
            resolutionCount += 1
            return "\(presentationLocale):\(key):\(arguments["package"] ?? "")"
        }

        XCTAssertEqual(
            presentation.resolve(using: resolve),
            "en:service.task.label.install.package:ripgrep"
        )
        presentationLocale = "de"
        XCTAssertEqual(
            presentation.resolve(using: resolve),
            "de:service.task.label.install.package:ripgrep"
        )
        XCTAssertEqual(resolutionCount, 2)

        let fallbackPresentation = TaskDescriptionPresentation(
            rawDescription: "raw fallback",
            labelKey: nil,
            labelArgs: nil
        )
        XCTAssertEqual(fallbackPresentation.resolve(using: resolve), "raw fallback")
        XCTAssertEqual(resolutionCount, 2)
    }

    func testMissingTaskDescriptionKeyUsesRawFallback() {
        let presentation = TaskDescriptionPresentation(
            rawDescription: "backend-provided task description",
            labelKey: "service.task.label.missing",
            labelArgs: ["package": "ripgrep"]
        )

        XCTAssertEqual(
            presentation.resolve(using: { _, _ in nil }),
            "backend-provided task description"
        )
    }

    func testMissingTaskDescriptionKeyUsesLiveFallbackWhenAvailable() {
        let presentation = TaskDescriptionPresentation(
            rawDescription: "stale generic fallback",
            labelKey: "service.task.label.missing",
            labelArgs: nil,
            fallbackLocalization: .genericTask(
                taskType: "refresh",
                managerID: "homebrew_formula"
            )
        )

        let resolved = presentation.resolve(
            using: { key, _ in
                key == "app.tasks.fallback.description" ? "live generic fallback" : nil
            }
        )

        XCTAssertEqual(resolved, "live generic fallback")
    }

    func testNilLabelProductionTaskFallbackReResolvesAcrossLocales() {
        let presentation = TaskDescriptionPresentation(
            rawDescription: "stale production fallback",
            labelKey: nil,
            labelArgs: nil,
            fallbackLocalization: .productionTask(
                taskType: "refresh",
                managerID: "homebrew_formula",
                override: nil
            )
        )

        XCTAssertEqual(resolveTaskDescription(presentation, locale: "en"),
                       "en|app.tasks.fallback.description|manager=en-homebrew_formula,task_type=en-refresh")
        XCTAssertEqual(resolveTaskDescription(presentation, locale: "de"),
                       "de|app.tasks.fallback.description|manager=de-homebrew_formula,task_type=de-refresh")
    }

    func testLegacyCatalogSyncUsesCompleteLiveDescriptor() {
        let presentation = TaskDescriptionPresentation(
            rawDescription: "stale catalog label",
            labelKey: nil,
            labelArgs: nil,
            fallbackLocalization: .productionTask(
                taskType: "catalog_sync",
                managerID: "homebrew_formula",
                override: nil
            )
        )

        XCTAssertEqual(
            resolveTaskDescription(presentation, locale: "de"),
            "de|service.task.label.search.manager|manager=de-homebrew_formula"
        )
    }

    func testQueuedManagerPlaceholderFallbackReResolvesAcrossLocales() throws {
        let localization = try XCTUnwrap(
            TaskDescriptionLocalization.managerAction(
                taskType: "manager_setup",
                managerID: "mise"
            )
        )
        let presentation = TaskDescriptionPresentation(
            rawDescription: "stale queued placeholder",
            labelKey: nil,
            labelArgs: nil,
            fallbackLocalization: localization
        )

        XCTAssertEqual(resolveTaskDescription(presentation, locale: "en"),
                       "en|service.task.label.setup.manager|manager=en-mise")
        XCTAssertEqual(resolveTaskDescription(presentation, locale: "de"),
                       "de|service.task.label.setup.manager|manager=de-mise")
    }

    func testLocalManagerFailureFallbackReResolvesAcrossLocales() throws {
        let localization = try XCTUnwrap(
            TaskDescriptionLocalization.managerAction(
                taskType: "install",
                managerID: "npm"
            )
        )
        let presentation = TaskDescriptionPresentation(
            rawDescription: "stale local failure",
            labelKey: nil,
            labelArgs: nil,
            fallbackLocalization: localization
        )

        XCTAssertEqual(resolveTaskDescription(presentation, locale: "en"),
                       "en|app.tasks.fallback.description|manager=en-npm,task_type=en-install")
        XCTAssertEqual(resolveTaskDescription(presentation, locale: "de"),
                       "de|app.tasks.fallback.description|manager=de-npm,task_type=de-install")
    }

    func testRetainedUnknownPackageVersionUsesCurrentLocalePresentation() {
        let retainedVersion = "inconnu"

        XCTAssertEqual(
            PackageVersionPresentation.currentVersionText(
                storedVersion: retainedVersion,
                localizedUnknown: "inconnu"
            ),
            "inconnu"
        )
        XCTAssertEqual(
            PackageVersionPresentation.currentVersionText(
                storedVersion: retainedVersion,
                localizedUnknown: "unbekannt"
            ),
            "unbekannt"
        )
    }

    func testGlobalSearchUnknownPackageVersionUsesCurrentLocalePresentation() {
        let package = PackageItem(
            id: "npm:example",
            name: "example",
            version: "inconnu",
            managerId: "npm",
            manager: "npm"
        )

        XCTAssertEqual(
            PackageVersionPresentation.currentVersionText(
                for: package,
                localizedUnknown: "unbekannt"
            ),
            "unbekannt"
        )
    }

    func testLocalizedUnknownPackageVersionIsNeverUsedAsMutationSelector() {
        for placeholder in ["unknown", "unbekannt", "desconocida", "inconnu",
                            "ismeretlen", "不明", "desconhecido"] {
            XCTAssertNil(
                PackageMutationVersionPolicy.versionSelector(storedVersion: placeholder),
                "Expected \(placeholder) to remain presentation-only"
            )
        }
        XCTAssertEqual(
            PackageMutationVersionPolicy.versionSelector(storedVersion: " 1.2.3 "),
            "1.2.3"
        )
    }

    func testReviewedPlanConfirmationPreservesTheReviewedSelection() {
        var presentation = UpgradeSheetPresentationState()
        let selectedSteps = [
            planStep(id: "update-mise-node", orderIndex: 1, managerID: "mise"),
            planStep(id: "update-mas-pages", orderIndex: 2, managerID: "mas"),
        ]
        let automaticStepIDs: Set<String> = ["update-mise-node", "update-mas-pages"]
        let backendSteps = selectedSteps
        let riskSummary = UpgradePreviewPlanner.RiskSummary(
            requiresElevatedPrivileges: true,
            mayRequireReboot: false
        )

        presentation.presentReviewedPlan(
            in: .controlCenter,
            managerScopeID: "mise",
            packageFilter: "node",
            selectedSteps: selectedSteps,
            selectedBackendSteps: backendSteps,
            automaticallyRunStepIDs: automaticStepIDs,
            riskSummary: riskSummary
        )

        XCTAssertTrue(presentation.isPresented)
        guard let request = presentation.reviewedPlanRequest else {
            XCTFail("Expected a reviewed Plan confirmation request")
            return
        }
        XCTAssertEqual(request.managerScopeID, "mise")
        XCTAssertEqual(request.packageFilter, "node")
        XCTAssertEqual(request.selectedSteps, selectedSteps)
        XCTAssertEqual(request.selectedBackendSteps, backendSteps)
        XCTAssertEqual(request.selectedStepIDs, Set(selectedSteps.map(\.id)))
        XCTAssertEqual(request.automaticallyRunStepIDs, automaticStepIDs)
        XCTAssertEqual(request.riskSummary, riskSummary)

        presentation.dismiss()
        XCTAssertFalse(presentation.isPresented)
        XCTAssertEqual(presentation.reviewedPlanRequest, request)
    }

    func testConfirmationRequestIsConsumedOnlyOnceAcrossPlanRemounts() {
        var state = UpgradePlanConfirmationRequestState()
        state.requestUpgradeAll()

        guard let request = state.pendingRequest else {
            XCTFail("Expected a pending confirmation request")
            return
        }
        var previewState = UpgradePlanPreviewRevisionState()
        let previewRequest = previewState.issue(includePinned: false, allowOsUpdates: false)
        XCTAssertTrue(state.requirePreview(previewRequest, for: request))
        guard let boundRequest = state.pendingRequest else {
            XCTFail("Expected a preview-bound confirmation request")
            return
        }
        XCTAssertEqual(boundRequest.requiredPreviewRevision, previewRequest.revision)
        XCTAssertTrue(state.complete(boundRequest, presentationSucceeded: true))
        XCTAssertNil(state.pendingRequest)
        XCTAssertEqual(state.lastConsumedRequestID, boundRequest.id)

        // A newly mounted Plan reads the same persistent state and finds no request to replay.
        XCTAssertNil(state.pendingRequest)
        XCTAssertFalse(state.complete(boundRequest, presentationSucceeded: true))
    }

    func testNotificationConfirmationRequestEnforcesNoPinnedOrOsUpdates() {
        var state = UpgradePlanConfirmationRequestState()
        state.requestUpgradeAll()

        guard let request = state.pendingRequest else {
            XCTFail("Expected a pending confirmation request")
            return
        }
        XCTAssertFalse(request.includePinned)
        XCTAssertFalse(request.allowOsUpdates)
        XCTAssertTrue(request.includes(managerID: "brew", isPinned: false))
        XCTAssertFalse(request.includes(managerID: "brew", isPinned: true))
        XCTAssertFalse(request.includes(managerID: "softwareupdate", isPinned: false))
    }

    func testConfirmationRequestRemainsPendingForEmptyOrUnavailablePresentation() {
        var state = UpgradePlanConfirmationRequestState()
        state.requestUpgradeAll()

        guard let request = state.pendingRequest else {
            XCTFail("Expected a pending confirmation request")
            return
        }
        let emptyPlanCanPresent = ReviewedUpgradePlanPresentationPolicy.canPresent(
            automaticallyRunStepIDs: [],
            executionAvailable: true
        )
        XCTAssertFalse(emptyPlanCanPresent)
        XCTAssertFalse(state.complete(request, presentationSucceeded: emptyPlanCanPresent))
        XCTAssertEqual(state.pendingRequest, request)

        let unavailablePlanCanPresent = ReviewedUpgradePlanPresentationPolicy.canPresent(
            automaticallyRunStepIDs: ["update-brew-ripgrep"],
            executionAvailable: false
        )
        XCTAssertFalse(unavailablePlanCanPresent)
        XCTAssertFalse(state.complete(request, presentationSucceeded: unavailablePlanCanPresent))
        XCTAssertEqual(state.pendingRequest, request)
        XCTAssertNil(state.lastConsumedRequestID)
    }

    func testUpgradePlanPreviewIgnoresOutOfOrderResponses() {
        var confirmationState = UpgradePlanConfirmationRequestState()
        confirmationState.requestUpgradeAll()
        guard let confirmationRequest = confirmationState.pendingRequest else {
            XCTFail("Expected a pending confirmation request")
            return
        }

        var previewState = UpgradePlanPreviewRevisionState()
        let first = previewState.issue(includePinned: false, allowOsUpdates: false)
        XCTAssertTrue(confirmationState.requirePreview(first, for: confirmationRequest))
        let second = previewState.issue(includePinned: false, allowOsUpdates: false)

        XCTAssertFalse(previewState.apply(first))
        XCTAssertNil(previewState.latestAppliedRequest)
        guard let firstBoundRequest = confirmationState.pendingRequest else {
            XCTFail("Expected a preview-bound confirmation request")
            return
        }
        XCTAssertFalse(
            UpgradePlanConfirmationPreviewPolicy.isReady(
                firstBoundRequest,
                previewState: previewState
            )
        )

        XCTAssertTrue(confirmationState.requirePreview(second, for: firstBoundRequest))
        XCTAssertTrue(previewState.apply(second))
        guard let secondBoundRequest = confirmationState.pendingRequest else {
            XCTFail("Expected a rebound confirmation request")
            return
        }
        XCTAssertTrue(
            UpgradePlanConfirmationPreviewPolicy.isReady(
                secondBoundRequest,
                previewState: previewState
            )
        )
    }

    func testUnappliedPreviewCanBeReboundAfterServiceRecovery() {
        var confirmationState = UpgradePlanConfirmationRequestState()
        confirmationState.requestUpgradeAll()
        guard let request = confirmationState.pendingRequest else {
            XCTFail("Expected a pending confirmation request")
            return
        }

        var previewState = UpgradePlanPreviewRevisionState()
        let unavailablePreview = previewState.issue(includePinned: false, allowOsUpdates: false)
        XCTAssertTrue(confirmationState.requirePreview(unavailablePreview, for: request))
        XCTAssertNil(previewState.latestAppliedRequest)

        let recoveredPreview = previewState.issue(includePinned: false, allowOsUpdates: false)
        guard let pendingRequest = confirmationState.pendingRequest else {
            XCTFail("Expected the request to remain pending")
            return
        }
        XCTAssertTrue(confirmationState.requirePreview(recoveredPreview, for: pendingRequest))
        XCTAssertTrue(previewState.apply(recoveredPreview))
        guard let reboundRequest = confirmationState.pendingRequest else {
            XCTFail("Expected the request to remain pending until presentation")
            return
        }
        XCTAssertEqual(reboundRequest.requiredPreviewRevision, recoveredPreview.revision)
        XCTAssertTrue(
            UpgradePlanConfirmationPreviewPolicy.isReady(
                reboundRequest,
                previewState: previewState
            )
        )
    }

    func testReviewedBackendStepEncodesWithCoreFFIKeys() throws {
        let step = planStep(id: "update-mise-node", orderIndex: 1, managerID: "mise")

        let data = try JSONEncoder().encode([step])
        let payload = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [[String: Any]])
        let encodedStep = try XCTUnwrap(payload.first)

        XCTAssertEqual(encodedStep["stepId"] as? String, step.id)
        XCTAssertEqual(encodedStep["managerId"] as? String, step.managerID)
        XCTAssertNil(encodedStep["id"])
        XCTAssertNil(encodedStep["managerID"])
    }

    func testReviewedPlanValidationAcceptsAnUnchangedSnapshot() {
        let request = reviewedRequest()

        XCTAssertTrue(
            ReviewedUpgradePlanValidation.isCurrent(
                request: request,
                currentSelectedSteps: request.selectedSteps,
                currentAutomaticallyRunStepIDs: request.automaticallyRunStepIDs,
                currentRiskSummary: request.riskSummary
            )
        )
    }

    func testReviewedPlanValidationRejectsChangedStepsEligibilityAndRisk() {
        let request = reviewedRequest()
        var reorderedSteps = request.selectedSteps
        reorderedSteps.swapAt(0, 1)

        XCTAssertFalse(
            ReviewedUpgradePlanValidation.isCurrent(
                request: request,
                currentSelectedSteps: reorderedSteps,
                currentAutomaticallyRunStepIDs: request.automaticallyRunStepIDs,
                currentRiskSummary: request.riskSummary
            )
        )
        XCTAssertFalse(
            ReviewedUpgradePlanValidation.isCurrent(
                request: request,
                currentSelectedSteps: request.selectedSteps,
                currentAutomaticallyRunStepIDs: ["update-mise-node"],
                currentRiskSummary: request.riskSummary
            )
        )
        XCTAssertFalse(
            ReviewedUpgradePlanValidation.isCurrent(
                request: request,
                currentSelectedSteps: request.selectedSteps,
                currentAutomaticallyRunStepIDs: request.automaticallyRunStepIDs,
                currentRiskSummary: .init(
                    requiresElevatedPrivileges: false,
                    mayRequireReboot: true
                )
            )
        )
    }

    func testPreviewDismissalStateIsSharedAcrossPresentationChecks() {
        var session = EnvironmentBriefFirstRunSession()

        XCTAssertTrue(
            session.shouldPresent(
                mode: .preview,
                hasCompletedOnboarding: true
            )
        )

        session.dismissPreview()

        XCTAssertFalse(
            session.shouldPresent(
                mode: .preview,
                hasCompletedOnboarding: true
            )
        )
    }

    func testPreviewDismissalDoesNotAffectEnabledFirstRunRoute() {
        var session = EnvironmentBriefFirstRunSession()
        session.dismissPreview()

        XCTAssertTrue(
            session.shouldPresent(
                mode: .enabled,
                hasCompletedOnboarding: false
            )
        )
        XCTAssertFalse(
            session.shouldPresent(
                mode: .enabled,
                hasCompletedOnboarding: true
            )
        )
    }

    private func planStep(
        id: String,
        orderIndex: UInt64,
        managerID: String
    ) -> ReviewedUpgradePlanStep {
        ReviewedUpgradePlanStep(
            id: id,
            orderIndex: orderIndex,
            managerID: managerID,
            authority: "standard",
            action: "upgrade",
            packageName: id,
            reasonLabelKey: "service.task.label.upgrade.package",
            reasonLabelArgs: ["package": id],
            status: "queued"
        )
    }

    private func reviewedRequest() -> ReviewedUpgradePlanRequest {
        let selectedSteps = [
            planStep(id: "update-mise-node", orderIndex: 1, managerID: "mise"),
            planStep(id: "update-mas-pages", orderIndex: 2, managerID: "mas"),
        ]
        return ReviewedUpgradePlanRequest(
            managerScopeID: "",
            packageFilter: "",
            selectedSteps: selectedSteps,
            selectedBackendSteps: selectedSteps,
            automaticallyRunStepIDs: ["update-mise-node", "update-mas-pages"],
            riskSummary: .init(
                requiresElevatedPrivileges: true,
                mayRequireReboot: false
            )
        )
    }

    private func waitUntil(
        timeout: TimeInterval = 1,
        condition: () -> Bool
    ) -> Bool {
        let deadline = Date(timeIntervalSinceNow: timeout)
        while !condition(), Date() < deadline {
            RunLoop.main.run(until: min(deadline, Date(timeIntervalSinceNow: 0.01)))
        }
        return condition()
    }

    private func resolveTaskDescription(
        _ presentation: TaskDescriptionPresentation,
        locale: String
    ) -> String {
        presentation.resolve(
            using: { key, arguments in
                let renderedArguments = arguments
                    .sorted(by: { $0.key < $1.key })
                    .map { "\($0.key)=\($0.value)" }
                    .joined(separator: ",")
                return "\(locale)|\(key)|\(renderedArguments)"
            },
            argumentResolver: { argument in
                "\(locale)-\(argument.rawValue)"
            }
        )
    }
}

private enum ControlCenterLocaleHost: Hashable {
    case activity
    case inspector
}

private final class ControlCenterLocaleRenderTracker {
    private var counts: [ControlCenterLocaleHost: Int] = [:]

    var didRenderBothHosts: Bool {
        renderCount(for: .activity) > 0 && renderCount(for: .inspector) > 0
    }

    func record(_ host: ControlCenterLocaleHost) {
        counts[host, default: 0] += 1
    }

    func renderCount(for host: ControlCenterLocaleHost) -> Int {
        counts[host, default: 0]
    }
}

private final class ControlCenterLocaleStateProbe: ControlCenterContextBase {
    @Published var selectedPackageId: String?
}

private final class ControlCenterLocaleStateTracker {
    private(set) var latestRevision: Int?
    private(set) var latestRenderedText: String?
    private(set) var latestLifecycleID: UUID?
    private(set) var latestStateToken: UUID?
    private(set) var reviewPresented = false
    private(set) var appearCount = 0
    private(set) var disappearCount = 0

    func recordRender(
        revision: Int,
        renderedText: String,
        lifecycleID: UUID,
        stateToken: UUID,
        reviewPresented: Bool
    ) {
        latestRevision = revision
        latestRenderedText = renderedText
        latestLifecycleID = lifecycleID
        latestStateToken = stateToken
        self.reviewPresented = reviewPresented
    }

    func recordAppearance() {
        appearCount += 1
    }

    func recordDisappearance() {
        disappearCount += 1
    }
}

private final class ControlCenterLocaleLifecycleProbe: ObservableObject {
    let id = UUID()
}

private struct ControlCenterLocaleStatePreservationHost: View {
    @ObservedObject var context: ControlCenterContextBase
    let presentReview: AnyPublisher<Void, Never>
    let tracker: ControlCenterLocaleStateTracker

    var body: some View {
        ControlCenterLocaleRefreshHost(revision: context.localeRevision) {
            ControlCenterLocaleStatefulChildProbe(
                presentReview: presentReview,
                tracker: tracker
            )
        }
    }
}

private struct ControlCenterLocaleStatefulChildProbe: View {
    @Environment(\.controlCenterLocaleRevision) private var localeRevision
    @StateObject private var lifecycle = ControlCenterLocaleLifecycleProbe()
    @State private var stateToken = UUID()
    @State private var reviewPresented = false
    let presentReview: AnyPublisher<Void, Never>
    let tracker: ControlCenterLocaleStateTracker

    var body: some View {
        let renderedText = "locale-revision-\(localeRevision)"
        tracker.recordRender(
            revision: localeRevision,
            renderedText: renderedText,
            lifecycleID: lifecycle.id,
            stateToken: stateToken,
            reviewPresented: reviewPresented
        )
        return Text(renderedText)
            .overlay {
                if reviewPresented {
                    Text("review-presented")
                        .accessibilityHidden(true)
                }
            }
            .onReceive(presentReview) {
                reviewPresented = true
            }
            .onAppear {
                tracker.recordAppearance()
            }
            .onDisappear {
                tracker.recordDisappearance()
            }
    }
}

private struct ControlCenterLocaleProbe: View {
    @EnvironmentObject private var context: ControlCenterContextBase
    let host: ControlCenterLocaleHost
    let tracker: ControlCenterLocaleRenderTracker

    var body: some View {
        tracker.record(host)
        return Text(context.selectedTaskId ?? host.label)
    }
}

private extension ControlCenterLocaleHost {
    var label: String {
        switch self {
        case .activity:
            "Activity"
        case .inspector:
            "Inspector"
        }
    }
}
