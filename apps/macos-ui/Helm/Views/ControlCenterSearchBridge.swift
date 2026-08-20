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

struct RemoteSearchSessionToken: Equatable {
    let generation: UInt64
    let query: String
}

struct RemoteSearchQueryTransition: Equatable {
    let token: RemoteSearchSessionToken?
    let taskIDsToCancel: Set<Int64>
    let didChange: Bool
}

enum RemoteSearchSubmissionResolution: Equatable {
    case tracked
    case cancelStaleTask(Int64)
    case currentFailure
    case staleFailure
}

struct RemoteSearchSessionState: Equatable {
    private(set) var token: RemoteSearchSessionToken?
    private(set) var activeTaskIDs: Set<Int64> = []
    private(set) var pendingSubmissionCount = 0

    private var generation: UInt64 = 0
    private var queryIdentity = ""
    private var hasSubmittedCurrentQuery = false

    var isSearching: Bool {
        token != nil && (pendingSubmissionCount > 0 || !activeTaskIDs.isEmpty)
    }

    mutating func updateQuery(_ query: String) -> RemoteSearchQueryTransition {
        let normalizedQuery = query.trimmingCharacters(in: .whitespacesAndNewlines)
        let nextIdentity = normalizedQuery.lowercased()
        guard nextIdentity != queryIdentity else {
            return RemoteSearchQueryTransition(
                token: token,
                taskIDsToCancel: [],
                didChange: false
            )
        }

        let taskIDsToCancel = activeTaskIDs
        generation &+= 1
        queryIdentity = nextIdentity
        token = normalizedQuery.isEmpty
            ? nil
            : RemoteSearchSessionToken(generation: generation, query: normalizedQuery)
        activeTaskIDs = []
        pendingSubmissionCount = 0
        hasSubmittedCurrentQuery = false

        return RemoteSearchQueryTransition(
            token: token,
            taskIDsToCancel: taskIDsToCancel,
            didChange: true
        )
    }

    mutating func beginSubmissions(
        for token: RemoteSearchSessionToken,
        count: Int
    ) -> Bool {
        guard count > 0,
              self.token == token,
              !hasSubmittedCurrentQuery else {
            return false
        }

        hasSubmittedCurrentQuery = true
        pendingSubmissionCount = count
        return true
    }

    mutating func resolveSubmission(
        for token: RemoteSearchSessionToken,
        taskID: Int64
    ) -> RemoteSearchSubmissionResolution {
        guard self.token == token else {
            return taskID >= 0 ? .cancelStaleTask(taskID) : .staleFailure
        }

        if pendingSubmissionCount > 0 {
            pendingSubmissionCount -= 1
        }
        guard taskID >= 0 else { return .currentFailure }

        activeTaskIDs.insert(taskID)
        return .tracked
    }

    mutating func finish(taskIDs: Set<Int64>) {
        activeTaskIDs.subtract(taskIDs)
    }

    @discardableResult
    mutating func reset() -> Set<Int64> {
        let taskIDsToCancel = activeTaskIDs
        generation &+= 1
        queryIdentity = ""
        token = nil
        activeTaskIDs = []
        pendingSubmissionCount = 0
        hasSubmittedCurrentQuery = false
        return taskIDsToCancel
    }
}

struct LibraryPackageFocusRequest: Equatable {
    let id: Int
    let packageID: String
}

struct LibraryPackageFocusRequestState: Equatable {
    private(set) var pendingRequest: LibraryPackageFocusRequest?
    private(set) var lastCompletedRequestID: Int?
    private var nextRequestID = 0

    @discardableResult
    mutating func request(packageID: String) -> LibraryPackageFocusRequest? {
        let packageID = packageID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !packageID.isEmpty else { return nil }

        nextRequestID &+= 1
        let request = LibraryPackageFocusRequest(
            id: nextRequestID,
            packageID: packageID
        )
        pendingRequest = request
        return request
    }

    @discardableResult
    mutating func complete(
        _ request: LibraryPackageFocusRequest,
        focusSucceeded: Bool
    ) -> Bool {
        guard focusSucceeded, pendingRequest == request else { return false }
        pendingRequest = nil
        lastCompletedRequestID = request.id
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
