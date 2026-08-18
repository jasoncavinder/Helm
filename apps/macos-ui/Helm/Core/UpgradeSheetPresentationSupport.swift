enum UpgradeSheetHost: Equatable {
    case controlCenter
}

struct ReviewedUpgradePlanStep: Codable, Equatable, Identifiable {
    let id: String
    let orderIndex: UInt64
    let managerID: String
    let authority: String
    let action: String
    let packageName: String
    let reasonLabelKey: String
    let reasonLabelArgs: [String: String]
    let status: String

    private enum CodingKeys: String, CodingKey {
        case id = "stepId"
        case orderIndex
        case managerID = "managerId"
        case authority
        case action
        case packageName
        case reasonLabelKey
        case reasonLabelArgs
        case status
    }
}

struct ReviewedUpgradePlanRequest: Equatable {
    let managerScopeID: String
    let packageFilter: String
    let selectedSteps: [ReviewedUpgradePlanStep]
    let selectedBackendSteps: [ReviewedUpgradePlanStep]
    let automaticallyRunStepIDs: Set<String>
    let riskSummary: UpgradePreviewPlanner.RiskSummary

    var selectedStepIDs: Set<String> {
        Set(selectedSteps.map(\.id))
    }
}

enum ReviewedUpgradePlanValidation {
    static func isCurrent(
        request: ReviewedUpgradePlanRequest,
        currentSelectedSteps: [ReviewedUpgradePlanStep],
        currentAutomaticallyRunStepIDs: Set<String>,
        currentRiskSummary: UpgradePreviewPlanner.RiskSummary
    ) -> Bool {
        currentSelectedSteps == request.selectedSteps
            && currentAutomaticallyRunStepIDs == request.automaticallyRunStepIDs
            && currentRiskSummary == request.riskSummary
    }
}

enum ReviewedUpgradePlanPresentationPolicy {
    static func canPresent(
        automaticallyRunStepIDs: Set<String>,
        executionAvailable: Bool
    ) -> Bool {
        !automaticallyRunStepIDs.isEmpty && executionAvailable
    }
}

struct UpgradePlanConfirmationRequest: Equatable, Identifiable {
    let id: Int
    let includePinned: Bool
    let allowOsUpdates: Bool
    var requiredPreviewRevision: Int?

    func includes(managerID: String, isPinned: Bool) -> Bool {
        (includePinned || !isPinned)
            && (allowOsUpdates || managerID != "softwareupdate")
    }
}

struct UpgradePlanPreviewRequest: Equatable {
    let revision: Int
    let includePinned: Bool
    let allowOsUpdates: Bool

    func matches(_ confirmationRequest: UpgradePlanConfirmationRequest) -> Bool {
        includePinned == confirmationRequest.includePinned
            && allowOsUpdates == confirmationRequest.allowOsUpdates
    }
}

struct UpgradePlanPreviewRevisionState: Equatable {
    private(set) var latestIssuedRequest: UpgradePlanPreviewRequest?
    private(set) var latestAppliedRequest: UpgradePlanPreviewRequest?
    private var nextRevision = 0

    mutating func issue(
        includePinned: Bool,
        allowOsUpdates: Bool
    ) -> UpgradePlanPreviewRequest {
        nextRevision &+= 1
        let request = UpgradePlanPreviewRequest(
            revision: nextRevision,
            includePinned: includePinned,
            allowOsUpdates: allowOsUpdates
        )
        latestIssuedRequest = request
        return request
    }

    @discardableResult
    mutating func apply(_ request: UpgradePlanPreviewRequest) -> Bool {
        guard latestIssuedRequest == request else { return false }
        latestAppliedRequest = request
        return true
    }
}

enum UpgradePlanConfirmationPreviewPolicy {
    static func isReady(
        _ confirmationRequest: UpgradePlanConfirmationRequest,
        previewState: UpgradePlanPreviewRevisionState
    ) -> Bool {
        guard let requiredRevision = confirmationRequest.requiredPreviewRevision,
              let latestIssuedRequest = previewState.latestIssuedRequest,
              latestIssuedRequest.revision == requiredRevision,
              latestIssuedRequest.matches(confirmationRequest) else {
            return false
        }
        return previewState.latestAppliedRequest == latestIssuedRequest
    }
}

struct UpgradePlanConfirmationRequestState: Equatable {
    private(set) var pendingRequest: UpgradePlanConfirmationRequest?
    private(set) var lastConsumedRequestID: Int?
    private var nextRequestID = 0

    mutating func requestUpgradeAll() {
        nextRequestID &+= 1
        pendingRequest = UpgradePlanConfirmationRequest(
            id: nextRequestID,
            includePinned: false,
            allowOsUpdates: false,
            requiredPreviewRevision: nil
        )
    }

    @discardableResult
    mutating func requirePreview(
        _ previewRequest: UpgradePlanPreviewRequest,
        for request: UpgradePlanConfirmationRequest
    ) -> Bool {
        guard pendingRequest?.id == request.id,
              previewRequest.matches(request) else {
            return false
        }
        pendingRequest?.requiredPreviewRevision = previewRequest.revision
        return true
    }

    @discardableResult
    mutating func complete(
        _ request: UpgradePlanConfirmationRequest,
        presentationSucceeded: Bool
    ) -> Bool {
        guard presentationSucceeded, pendingRequest == request else { return false }
        pendingRequest = nil
        lastConsumedRequestID = request.id
        return true
    }
}

struct UpgradeSheetPresentationState: Equatable {
    private(set) var isPresented = false
    private(set) var host: UpgradeSheetHost = .controlCenter
    private(set) var reviewedPlanRequest: ReviewedUpgradePlanRequest?

    mutating func presentReviewedPlan(
        in host: UpgradeSheetHost,
        managerScopeID: String,
        packageFilter: String,
        selectedSteps: [ReviewedUpgradePlanStep],
        selectedBackendSteps: [ReviewedUpgradePlanStep],
        automaticallyRunStepIDs: Set<String>,
        riskSummary: UpgradePreviewPlanner.RiskSummary
    ) {
        self.host = host
        reviewedPlanRequest = ReviewedUpgradePlanRequest(
            managerScopeID: managerScopeID,
            packageFilter: packageFilter,
            selectedSteps: selectedSteps,
            selectedBackendSteps: selectedBackendSteps,
            automaticallyRunStepIDs: automaticallyRunStepIDs,
            riskSummary: riskSummary
        )
        isPresented = true
    }

    mutating func dismiss() {
        isPresented = false
    }
}
