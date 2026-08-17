import Foundation

struct WholeWorkflowResearchDataset: Codable, Equatable {
    static let currentSchemaVersion = "1.0.0"
    static let currentDatasetID = "v0.20-whole-workflow-v1"

    let schemaVersion: String
    let datasetID: String
    let generatedAt: String
    let safety: ResearchDatasetSafety
    let scenarios: [ResearchScenario]
    let snapshot: ResearchWorkflowSnapshot
    let firstRun: ResearchFirstRunSnapshot

    private enum CodingKeys: String, CodingKey {
        case schemaVersion
        case datasetID = "datasetId"
        case generatedAt
        case safety
        case scenarios
        case snapshot
        case firstRun
    }
}

struct ResearchDatasetSafety: Codable, Equatable {
    let syntheticOnly: Bool
    let localOnly: Bool
    let allowsMachineScan: Bool
    let allowsMutation: Bool
    let containsPersonalData: Bool
}

struct ResearchScenario: Codable, Equatable {
    let taskNumber: Int
    let scenarioID: String
    let startingSurface: String
    let recordIDs: [String]

    private enum CodingKeys: String, CodingKey {
        case taskNumber
        case scenarioID = "scenarioId"
        case startingSurface
        case recordIDs = "recordIds"
    }
}

struct ResearchWorkflowSnapshot: Codable, Equatable {
    let coverage: ResearchCoverageRecord
    let managers: [ResearchManagerRecord]
    let updates: [ResearchUpdateRecord]
    let upgradePlan: ResearchUpgradePlanRecord
    let searchResults: [ResearchSearchResultRecord]
    let installProposal: ResearchInstallProposalRecord
    let activities: [ResearchActivityRecord]
    let recoveryActions: [ResearchRecoveryActionRecord]
    let managerDecision: ResearchManagerDecisionRecord
    let settings: [ResearchSettingRecord]
}

struct ResearchCoverageRecord: Codable, Equatable {
    let id: String
    let state: String
    let currentManagerIDs: [String]
    let cachedManagerIDs: [String]
    let failedManagerIDs: [String]
    let deferredManagerIDs: [String]

    private enum CodingKeys: String, CodingKey {
        case id
        case state
        case currentManagerIDs = "currentManagerIds"
        case cachedManagerIDs = "cachedManagerIds"
        case failedManagerIDs = "failedManagerIds"
        case deferredManagerIDs = "deferredManagerIds"
    }
}

struct ResearchManagerRecord: Codable, Equatable {
    let id: String
    let authority: String
    let detected: Bool
    let enabled: Bool
    let freshness: String
    let sourceState: String
    let installInstances: [ResearchInstallInstanceRecord]
    let findingCode: String?
}

struct ResearchInstallInstanceRecord: Codable, Equatable {
    let id: String
    let displayPath: String
    let provenance: String
    let active: Bool
    let policyState: String
}

struct ResearchUpdateRecord: Codable, Equatable {
    let id: String
    let managerID: String
    let packageName: String
    let installedVersion: String
    let candidateVersion: String
    let authority: String
    let pinned: Bool
    let requiresPrivilege: Bool
    let restartRequired: Bool
    let planSelection: String
    let exclusionReason: String?

    private enum CodingKeys: String, CodingKey {
        case id
        case managerID = "managerId"
        case packageName
        case installedVersion
        case candidateVersion
        case authority
        case pinned
        case requiresPrivilege
        case restartRequired
        case planSelection
        case exclusionReason
    }
}

struct ResearchSearchResultRecord: Codable, Equatable {
    let id: String
    let managerID: String
    let packageName: String
    let version: String
    let resultOrigin: String
    let recommended: Bool
    let recommendationReasonKey: String
    let deferredWhenOffline: Bool

    private enum CodingKeys: String, CodingKey {
        case id
        case managerID = "managerId"
        case packageName
        case version
        case resultOrigin
        case recommended
        case recommendationReasonKey
        case deferredWhenOffline
    }
}

struct ResearchUpgradePlanRecord: Codable, Equatable {
    let id: String
    let state: String
    let authorityOrder: [String]
    let selectedUpdateIDs: [String]
    let excludedUpdateIDs: [String]
    let requiresPrivilege: Bool
    let restartRequired: Bool

    private enum CodingKeys: String, CodingKey {
        case id
        case state
        case authorityOrder
        case selectedUpdateIDs = "selectedUpdateIds"
        case excludedUpdateIDs = "excludedUpdateIds"
        case requiresPrivilege
        case restartRequired
    }
}

struct ResearchInstallProposalRecord: Codable, Equatable {
    let id: String
    let searchResultID: String
    let managerID: String
    let packageName: String
    let state: String
    let requiresNetwork: Bool
    let requiresPrivilege: Bool
    let offlineBehavior: String

    private enum CodingKeys: String, CodingKey {
        case id
        case searchResultID = "searchResultId"
        case managerID = "managerId"
        case packageName
        case state
        case requiresNetwork
        case requiresPrivilege
        case offlineBehavior
    }
}

struct ResearchActivityRecord: Codable, Equatable {
    let id: String
    let taskID: UInt64
    let managerID: String
    let updateID: String?
    let state: String
    let applyResult: String
    let verificationResult: String
    let sourceStarted: Bool
    let beforeKey: String
    let afterKey: String
    let rollbackEligible: Bool
    let recoveryLimitsKey: String

    private enum CodingKeys: String, CodingKey {
        case id
        case taskID = "taskId"
        case managerID = "managerId"
        case updateID = "updateId"
        case state
        case applyResult
        case verificationResult
        case sourceStarted
        case beforeKey
        case afterKey
        case rollbackEligible
        case recoveryLimitsKey
    }
}

struct ResearchRecoveryActionRecord: Codable, Equatable {
    let id: String
    let activityID: String
    let action: String
    let allowed: Bool
    let reasonKey: String

    private enum CodingKeys: String, CodingKey {
        case id
        case activityID = "activityId"
        case action
        case allowed
        case reasonKey
    }
}

struct ResearchManagerDecisionRecord: Codable, Equatable {
    let id: String
    let managerID: String
    let action: String
    let initialState: String
    let resultingState: String
    let revisitSurface: String

    private enum CodingKeys: String, CodingKey {
        case id
        case managerID = "managerId"
        case action
        case initialState
        case resultingState
        case revisitSurface
    }
}

struct ResearchSettingRecord: Codable, Equatable {
    let id: String
    let key: String
    let booleanValue: Bool
}

struct ResearchFirstRunSnapshot: Codable, Equatable {
    let environmentBrief: EnvironmentBrief
    let setupSession: ResearchSetupSession
    let plan: ResearchFirstRunPlan
    let actionReceipt: ResearchActionReceipt
    let redactedSummary: ResearchRedactedSummary
}

struct ResearchSetupSession: Codable, Equatable {
    let schemaVersion: String
    let sessionID: UUID
    let briefID: UUID
    let briefRevision: UInt64
    let planID: UUID
    let receiptID: UUID
    let startedAt: String
    let updatedAt: String
    let state: String
    let consent: ResearchSetupConsent
    let coverage: ResearchSetupCoverage
    let actionStates: [ResearchSetupActionState]
    let resume: ResearchSetupResume
    let offlineBehavior: String

    private enum CodingKeys: String, CodingKey {
        case schemaVersion
        case sessionID = "sessionId"
        case briefID = "briefId"
        case briefRevision
        case planID = "planId"
        case receiptID = "receiptId"
        case startedAt
        case updatedAt
        case state
        case consent
        case coverage
        case actionStates
        case resume
        case offlineBehavior
    }
}

struct ResearchSetupConsent: Codable, Equatable {
    let networkAllowed: Bool
    let mutationAllowed: Bool
    let privilegeAllowed: Bool
}

struct ResearchSetupCoverage: Codable, Equatable {
    let completeManagers: [String]
    let cachedManagers: [String]
    let failedManagers: [String]
    let cancelledManagers: [String]
    let deferredManagers: [String]
}

struct ResearchSetupActionState: Codable, Equatable {
    let actionID: UUID
    let state: String
    let updatedAt: String

    private enum CodingKeys: String, CodingKey {
        case actionID = "actionId"
        case state
        case updatedAt
    }
}

struct ResearchSetupResume: Codable, Equatable {
    let state: String
    let lastDurableStage: String?
    let requiresRevalidation: Bool
}

struct ResearchFirstRunPlan: Codable, Equatable {
    let schemaVersion: String
    let planID: UUID
    let sessionID: UUID
    let briefID: UUID
    let briefRevision: UInt64
    let policyRevision: String
    let createdAt: String
    let state: String
    let actions: [ResearchPlannedAction]

    private enum CodingKeys: String, CodingKey {
        case schemaVersion
        case planID = "planId"
        case sessionID = "sessionId"
        case briefID = "briefId"
        case briefRevision
        case policyRevision
        case createdAt
        case state
        case actions
    }
}

struct ResearchPlannedAction: Codable, Equatable {
    let actionID: UUID
    let typedActionID: String
    let sequence: Int
    let dependsOn: [UUID]
    let target: ResearchActionTarget
    let mutationClass: String
    let requiresNetwork: Bool
    let requiresPrivilege: Bool
    let impactKey: String
    let impactArgs: [String: String]
    let preVerificationStatus: String
    let verificationMethodID: String
    let expectedStateKey: String
    let retryBehavior: String
    let rollbackEligible: Bool
    let recoveryLimitsKey: String

    private enum CodingKeys: String, CodingKey {
        case actionID = "actionId"
        case typedActionID = "typedActionId"
        case sequence
        case dependsOn
        case target
        case mutationClass
        case requiresNetwork
        case requiresPrivilege
        case impactKey
        case impactArgs
        case preVerificationStatus
        case verificationMethodID = "verificationMethodId"
        case expectedStateKey
        case retryBehavior
        case rollbackEligible
        case recoveryLimitsKey
    }
}

struct ResearchActionTarget: Codable, Equatable {
    let kind: String
    let identifier: String
}

struct ResearchActionReceipt: Codable, Equatable {
    let schemaVersion: String
    let receiptID: UUID
    let sessionID: UUID
    let planID: UUID
    let createdAt: String
    let status: String
    let summaryKey: String
    let summaryArgs: [String: String]
    let actionResults: [ResearchActionResult]
    let unchangedState: [ResearchLocalizedFact]

    private enum CodingKeys: String, CodingKey {
        case schemaVersion
        case receiptID = "receiptId"
        case sessionID = "sessionId"
        case planID = "planId"
        case createdAt
        case status
        case summaryKey
        case summaryArgs
        case actionResults
        case unchangedState
    }
}

struct ResearchActionResult: Codable, Equatable {
    let actionID: UUID
    let typedActionID: String
    let status: String
    let applyResult: String
    let verificationResult: String
    let before: ResearchLocalizedFact
    let after: ResearchLocalizedFact
    let authority: String?
    let rollbackEligible: Bool
    let recoveryLimits: ResearchLocalizedFact

    private enum CodingKeys: String, CodingKey {
        case actionID = "actionId"
        case typedActionID = "typedActionId"
        case status
        case applyResult
        case verificationResult
        case before
        case after
        case authority
        case rollbackEligible
        case recoveryLimits
    }
}

struct ResearchLocalizedFact: Codable, Equatable {
    let localizationKey: String
    let localizationArgs: [String: String]
}

struct ResearchRedactedSummary: Codable, Equatable {
    let schemaVersion: String
    let sessionID: UUID
    let redactionClass: String
    let sessionStatus: String
    let metrics: ResearchSummaryMetrics
    let environment: ResearchSummaryEnvironment

    private enum CodingKeys: String, CodingKey {
        case schemaVersion
        case sessionID = "sessionId"
        case redactionClass
        case sessionStatus
        case metrics
        case environment
    }
}

struct ResearchSummaryMetrics: Codable, Equatable {
    let totalActions: Int
    let verifiedActions: Int
    let noChangeActions: Int
    let failedVerificationActions: Int
    let failedActions: Int
    let cancelledOrInterruptedActions: Int
    let deferredActions: Int
    let managedActions: Int
    let durationSeconds: Int?
}

struct ResearchSummaryEnvironment: Codable, Equatable {
    let osFamily: String
    let architecture: String
}

enum WholeWorkflowResearchDatasetLoadError: LocalizedError, Equatable {
    case invalid([WholeWorkflowResearchDatasetIssue])

    var errorDescription: String? {
        switch self {
        case let .invalid(issues):
            return issues.map { "\($0.path): \($0.message)" }.joined(separator: "; ")
        }
    }
}

enum WholeWorkflowResearchDatasetLoader {
    static func decode(_ data: Data) throws -> WholeWorkflowResearchDataset {
        try JSONDecoder().decode(WholeWorkflowResearchDataset.self, from: data)
    }

    static func load(from url: URL) throws -> WholeWorkflowResearchDataset {
        let dataset = try decode(Data(contentsOf: url))
        let issues = WholeWorkflowResearchDatasetValidator.validate(dataset)
        guard issues.isEmpty else {
            throw WholeWorkflowResearchDatasetLoadError.invalid(issues)
        }
        return dataset
    }
}

enum WholeWorkflowResearchDatasetProvider {
    static let environmentKey = "HELM_WAYFINDER_RESEARCH_DATASET"

    static func selectedURL(environment: [String: String] = ProcessInfo.processInfo.environment) -> URL? {
        #if DEBUG
        guard let path = environment[environmentKey]?.trimmingCharacters(in: .whitespacesAndNewlines),
              path.hasPrefix("/") else {
            return nil
        }
        return URL(fileURLWithPath: path)
        #else
        return nil
        #endif
    }
}
