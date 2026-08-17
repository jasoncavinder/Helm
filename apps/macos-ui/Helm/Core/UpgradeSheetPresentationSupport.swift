enum UpgradeSheetHost: Equatable {
    case controlCenter
}

struct ReviewedUpgradePlanRequest: Equatable {
    let managerScopeID: String
    let packageFilter: String
    let selectedStepIDs: Set<String>
}

enum UpgradeSheetIntent: Equatable {
    case upgradeAll
    case reviewedPlan(ReviewedUpgradePlanRequest)
}

struct UpgradeSheetPresentationState: Equatable {
    private(set) var isPresented = false
    private(set) var host: UpgradeSheetHost = .controlCenter
    private(set) var intent: UpgradeSheetIntent = .upgradeAll

    mutating func presentUpgradeAll(in host: UpgradeSheetHost) {
        self.host = host
        intent = .upgradeAll
        isPresented = true
    }

    mutating func presentReviewedPlan(
        in host: UpgradeSheetHost,
        managerScopeID: String,
        packageFilter: String,
        selectedStepIDs: Set<String>
    ) {
        self.host = host
        intent = .reviewedPlan(
            ReviewedUpgradePlanRequest(
                managerScopeID: managerScopeID,
                packageFilter: packageFilter,
                selectedStepIDs: selectedStepIDs
            )
        )
        isPresented = true
    }

    mutating func dismiss() {
        isPresented = false
    }
}
