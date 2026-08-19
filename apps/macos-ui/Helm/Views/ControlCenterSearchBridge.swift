import Foundation

struct GlobalSearchNavigationDecision {
    let deepLink: WayfinderDeepLink
    let managerFilterID: String?
}

enum GlobalSearchNavigationPolicy {
    static func acceptedResultNavigation(
        packageID: String
    ) -> GlobalSearchNavigationDecision? {
        guard let deepLink = acceptedResultDeepLink(packageID: packageID) else { return nil }
        return GlobalSearchNavigationDecision(
            deepLink: deepLink,
            managerFilterID: nil
        )
    }

    static func acceptedResultDeepLink(packageID: String) -> WayfinderDeepLink? {
        let packageID = packageID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !packageID.isEmpty else { return nil }
        return WayfinderDeepLink(
            destination: .library,
            entityID: packageID,
            focus: .selectedEntity
        )
    }
}

struct ControlCenterGlobalSearchSessionState {
    private(set) var isResultsPresented = false

    mutating func updateQuery(
        _ query: String,
        presentsResults: Bool
    ) {
        let hasQuery = !query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        isResultsPresented = presentsResults && hasQuery
    }

    mutating func synchronize(
        isSearchFieldPresented: Bool,
        supportsGlobalResults: Bool,
        query: String
    ) {
        let hasQuery = !query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        isResultsPresented = isSearchFieldPresented && supportsGlobalResults && hasQuery
    }

    mutating func dismiss() {
        isResultsPresented = false
    }
}

struct ResearchSearchPresentationState {
    private struct Identity: Equatable {
        let normalizedQuery: String
        let isOfflineVariant: Bool
    }

    private(set) var remoteResultsAvailable = false
    private var identity: Identity?
    private var generation = 0

    mutating func update(
        query: String,
        isOfflineVariant: Bool
    ) -> Int? {
        let normalizedQuery = query
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
        let nextIdentity = Identity(
            normalizedQuery: normalizedQuery,
            isOfflineVariant: isOfflineVariant
        )
        guard identity != nextIdentity else { return nil }

        identity = nextIdentity
        generation &+= 1
        let hasQuery = !normalizedQuery.isEmpty
        remoteResultsAvailable = hasQuery && isOfflineVariant
        return hasQuery && !isOfflineVariant ? generation : nil
    }

    mutating func revealRemoteResults(for generation: Int) -> Bool {
        guard self.generation == generation,
              let identity,
              !identity.normalizedQuery.isEmpty,
              !identity.isOfflineVariant,
              !remoteResultsAvailable else {
            return false
        }
        remoteResultsAvailable = true
        return true
    }
}

protocol ControlCenterSearchFocusTarget: AnyObject {
    func requestSearchFocus(completion: @escaping () -> Void)
}

final class ControlCenterSearchFocusRouter {
    private weak var target: ControlCenterSearchFocusTarget?
    private var nextRequestID = 0
    private var pendingRequestID: Int?

    func attach(_ target: ControlCenterSearchFocusTarget) {
        self.target = target
        deliverPendingRequest()
    }

    func detach(_ target: ControlCenterSearchFocusTarget) {
        guard self.target === target else { return }
        self.target = nil
    }

    func requestFocus() {
        nextRequestID += 1
        pendingRequestID = nextRequestID
        deliverPendingRequest()
    }

    private func deliverPendingRequest() {
        guard let requestID = pendingRequestID, let target else { return }
        target.requestSearchFocus { [weak self] in
            guard self?.pendingRequestID == requestID else { return }
            self?.pendingRequestID = nil
        }
    }
}

final class ControlCenterSearchTextUpdateGate {
    private(set) var isApplyingModelValue = false
    private(set) var hasScheduledControlPublish = false
    private var pendingControlValue: String?

    func applyModelValue(_ update: () -> Void) {
        isApplyingModelValue = true
        defer { isApplyingModelValue = false }
        update()
    }

    func shouldPublishControlValue(_ controlValue: String, modelValue: String) -> Bool {
        !isApplyingModelValue && controlValue != modelValue
    }

    func stageControlValue(_ controlValue: String, modelValue: String) -> Bool {
        guard shouldPublishControlValue(controlValue, modelValue: modelValue) else { return false }
        pendingControlValue = controlValue
        guard !hasScheduledControlPublish else { return false }
        hasScheduledControlPublish = true
        return true
    }

    func displayedValue(modelValue: String) -> String {
        pendingControlValue ?? modelValue
    }

    func takePendingControlValue() -> String? {
        hasScheduledControlPublish = false
        defer { pendingControlValue = nil }
        return pendingControlValue
    }
}
