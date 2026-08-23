import Foundation

extension WholeWorkflowResearchDatasetProvider {
    static func activeSettingsDiagnosticsProjection(
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> ResearchSettingsDiagnosticsProjection? {
        guard let dataset = active(environment: environment) else { return nil }
        return ResearchSettingsDiagnosticsProjector.project(dataset)
    }

    static func settingsDiagnosticsRuntimeState(
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> ResearchSettingsDiagnosticsRuntimeState {
        if isSelected(environment: environment) {
            guard let projection = activeSettingsDiagnosticsProjection(
                environment: environment
            ) else {
                return .unavailable
            }
            return .ready(projection)
        }
        if WayfinderPopoverFixtureProvider.isActive(environment: environment) {
            return .safetyBlocked
        }
        return .inactive
    }
}

enum ResearchSettingsDiagnosticsRuntimeState: Equatable {
    case inactive
    case ready(ResearchSettingsDiagnosticsProjection)
    case unavailable
    case safetyBlocked
}

enum ResearchLaunchAtLoginMutationTarget: Equatable {
    case production
    case fixture(ResearchSettingsDiagnosticsProjection)
    case none
}

enum ResearchSettingsDiagnosticsLocalization {
    static let datasetUnavailable = "app.settings.research.dataset_unavailable"
    static let liveChangesUnavailable = "app.settings.research.live_changes_unavailable"
}

enum ResearchFixtureLiveOperationGate {
    static func perform<Result>(
        liveOperationsBlocked: Bool,
        operation: () throws -> Result
    ) rethrows -> Result? {
        guard !liveOperationsBlocked else { return nil }
        return try operation()
    }
}

struct ResearchLaunchAtLoginPresentation: Equatable {
    let booleanValue: Bool?
    let mutationTarget: ResearchLaunchAtLoginMutationTarget
    let explanationKey: String?

    var isInteractive: Bool {
        mutationTarget != .none
    }

    static func project(
        runtimeState: ResearchSettingsDiagnosticsRuntimeState,
        productionValue: Bool,
        fixtureValue: Bool?
    ) -> ResearchLaunchAtLoginPresentation {
        switch runtimeState {
        case .inactive:
            return ResearchLaunchAtLoginPresentation(
                booleanValue: productionValue,
                mutationTarget: .production,
                explanationKey: nil
            )
        case let .ready(projection):
            return ResearchLaunchAtLoginPresentation(
                booleanValue: fixtureValue ?? projection.launchAtLoginSetting.booleanValue,
                mutationTarget: .fixture(projection),
                explanationKey: nil
            )
        case .unavailable:
            return ResearchLaunchAtLoginPresentation(
                booleanValue: nil,
                mutationTarget: .none,
                explanationKey: ResearchSettingsDiagnosticsLocalization.datasetUnavailable
            )
        case .safetyBlocked:
            return ResearchLaunchAtLoginPresentation(
                booleanValue: nil,
                mutationTarget: .none,
                explanationKey: ResearchSettingsDiagnosticsLocalization.liveChangesUnavailable
            )
        }
    }
}

struct ResearchSettingsDiagnosticsProjection: Equatable {
    let datasetID: String
    let scenarioID: String
    let launchAtLoginSetting: ResearchSettingRecord
    let failedActivitySelectionID: String
    let copyDiagnosticsActionID: String

    var failedActivityDeepLink: WayfinderDeepLink {
        WayfinderDeepLink(
            destination: .activity,
            entityID: failedActivitySelectionID,
            focus: .selectedEntity
        )
    }

    fileprivate var launchAtLoginSessionKey: String {
        "\(datasetID):\(launchAtLoginSetting.id)"
    }
}

struct ResearchSettingsSession: Equatable {
    private var launchAtLoginValuesBySettingID: [String: Bool] = [:]

    func launchAtLoginValue(
        for projection: ResearchSettingsDiagnosticsProjection
    ) -> Bool {
        launchAtLoginValuesBySettingID[projection.launchAtLoginSessionKey]
            ?? projection.launchAtLoginSetting.booleanValue
    }

    @discardableResult
    mutating func setLaunchAtLogin(
        _ enabled: Bool,
        for projection: ResearchSettingsDiagnosticsProjection
    ) -> Bool {
        let key = projection.launchAtLoginSessionKey
        guard launchAtLoginValue(for: projection) != enabled else { return false }
        launchAtLoginValuesBySettingID[key] = enabled
        return true
    }
}

enum WholeWorkflowResearchTaskSixContract {
    static let scenarioID = "settings-and-diagnostics"
    static let startingSurface = "settings"
    static let launchAtLoginSettingID = "setting-launch-at-login"
    static let failedActivityID = WholeWorkflowResearchTaskFourContract.failedActivityID
    static let copyDiagnosticsActionID = "recovery-copy-diagnostics"
    static let orderedScenarioRecordIDs = [
        launchAtLoginSettingID,
        failedActivityID,
        copyDiagnosticsActionID,
    ]

    static let launchAtLoginSetting = ResearchSettingRecord(
        id: launchAtLoginSettingID,
        key: "launch_at_login",
        booleanValue: false
    )

    static func matchesScenario(_ scenario: ResearchScenario) -> Bool {
        scenario.taskNumber == 6
            && scenario.scenarioID == scenarioID
            && scenario.startingSurface == startingSurface
            && scenario.recordIDs == orderedScenarioRecordIDs
    }

    static func matchesSettings(_ settings: [ResearchSettingRecord]) -> Bool {
        settings == [launchAtLoginSetting]
    }

    static func matchesCopyDiagnosticsAction(
        _ action: ResearchRecoveryActionRecord
    ) -> Bool {
        action.id == copyDiagnosticsActionID
            && action.activityID == failedActivityID
            && action.action == WholeWorkflowResearchRecoveryActionKind.copyDiagnostics.rawValue
            && action.allowed
            && action.reasonKey == "research.recovery.copy_redacted_diagnostics"
    }
}

enum ResearchSettingsDiagnosticsProjector {
    static func project(
        _ dataset: WholeWorkflowResearchDataset
    ) -> ResearchSettingsDiagnosticsProjection? {
        guard WholeWorkflowResearchDatasetContract.matchesCurrentIdentityAndSafety(dataset),
              let scenario = dataset.scenarios.first(where: { $0.taskNumber == 6 }),
              WholeWorkflowResearchTaskSixContract.matchesScenario(scenario),
              WholeWorkflowResearchTaskSixContract.matchesSettings(dataset.snapshot.settings),
              let failedActivity = dataset.snapshot.activities.first(where: {
                  $0.id == WholeWorkflowResearchTaskSixContract.failedActivityID
              }),
              WholeWorkflowResearchTaskFourContract.matchesFailedActivity(failedActivity),
              let copyDiagnosticsAction = dataset.snapshot.recoveryActions.first(where: {
                  $0.id == WholeWorkflowResearchTaskSixContract.copyDiagnosticsActionID
              }),
              WholeWorkflowResearchTaskSixContract.matchesCopyDiagnosticsAction(
                  copyDiagnosticsAction
              ) else {
            return nil
        }

        return ResearchSettingsDiagnosticsProjection(
            datasetID: dataset.datasetID,
            scenarioID: scenario.scenarioID,
            launchAtLoginSetting: dataset.snapshot.settings[0],
            failedActivitySelectionID: String(failedActivity.taskID),
            copyDiagnosticsActionID: copyDiagnosticsAction.id
        )
    }
}
