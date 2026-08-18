enum UpgradeSheetHost: Equatable {
    case controlCenter
}

struct ReviewedUpgradePlanStep: Equatable, Identifiable {
    let id: String
    let orderIndex: UInt64
    let managerID: String
    let authority: String
    let action: String
    let packageName: String
    let reasonLabelKey: String
    let reasonLabelArgs: [String: String]
    let status: String
}

struct ReviewedUpgradePlanRequest: Equatable {
    let managerScopeID: String
    let packageFilter: String
    let selectedSteps: [ReviewedUpgradePlanStep]
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

struct UpgradePlanConfirmationRequestState: Equatable {
    private(set) var token = 0

    mutating func requestUpgradeAll() {
        token &+= 1
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
        automaticallyRunStepIDs: Set<String>,
        riskSummary: UpgradePreviewPlanner.RiskSummary
    ) {
        self.host = host
        reviewedPlanRequest = ReviewedUpgradePlanRequest(
            managerScopeID: managerScopeID,
            packageFilter: packageFilter,
            selectedSteps: selectedSteps,
            automaticallyRunStepIDs: automaticallyRunStepIDs,
            riskSummary: riskSummary
        )
        isPresented = true
    }

    mutating func dismiss() {
        isPresented = false
    }
}
