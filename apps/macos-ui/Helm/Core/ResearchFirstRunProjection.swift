import Foundation

extension WholeWorkflowResearchDatasetProvider {
    static func isFirstRunPreviewSelected(
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> Bool {
        isSelected(environment: environment)
            && EnvironmentBriefFirstRunConfiguration.mode(environment: environment) == .preview
    }

    static func activeFirstRunProjection(
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> ResearchFirstRunProjection? {
        guard let dataset = active(environment: environment) else { return nil }
        return ResearchFirstRunProjector.project(dataset)
    }

    static func firstRunRuntimeState(
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> ResearchFirstRunRuntimeState {
        guard isFirstRunPreviewSelected(environment: environment) else { return .inactive }
        guard let projection = activeFirstRunProjection(environment: environment) else {
            return .unavailable
        }
        return .ready(projection)
    }
}

enum ResearchFirstRunRuntimeState: Equatable {
    case inactive
    case ready(ResearchFirstRunProjection)
    case unavailable
}

enum ResearchFirstRunStage: String, Hashable {
    case environmentBrief
    case planReview
    case verifiedProgress
    case actionReceipt
}

struct ResearchFirstRunSession: Equatable {
    private(set) var stage: ResearchFirstRunStage = .environmentBrief

    @discardableResult
    mutating func reviewPlan() -> Bool {
        transition(from: [.environmentBrief, .actionReceipt], to: .planReview)
    }

    @discardableResult
    mutating func returnToBrief() -> Bool {
        transition(from: [.planReview], to: .environmentBrief)
    }

    @discardableResult
    mutating func applyReviewedPlan() -> Bool {
        transition(from: [.planReview], to: .verifiedProgress)
    }

    @discardableResult
    mutating func viewActionReceipt() -> Bool {
        transition(from: [.verifiedProgress], to: .actionReceipt)
    }

    private mutating func transition(
        from allowedStages: Set<ResearchFirstRunStage>,
        to nextStage: ResearchFirstRunStage
    ) -> Bool {
        guard allowedStages.contains(stage) else { return false }
        stage = nextStage
        return true
    }
}

enum FirstRunCompletionPolicy {
    static func shouldPersistOnboardingCompletion(
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> Bool {
        !WholeWorkflowResearchDatasetProvider.isFirstRunPreviewSelected(
            environment: environment
        )
    }
}

struct ResearchFirstRunProjection: Equatable {
    let datasetID: String
    let scenarioID: String
    let environmentBrief: EnvironmentBrief
    let setupSession: ResearchSetupSession
    let plan: ResearchFirstRunPlan
    let actionReceipt: ResearchActionReceipt
    let redactedSummary: ResearchRedactedSummary
    let recommendation: ResearchPlannedAction
    let receiptResult: ResearchActionResult

    var expectedStateFact: ResearchLocalizedFact {
        ResearchLocalizedFact(
            localizationKey: recommendation.expectedStateKey,
            localizationArgs: recommendation.impactArgs
        )
    }

    var summaryProjection: ResearchFirstRunSummaryProjection {
        ResearchFirstRunSummaryProjection(
            status: redactedSummary.sessionStatus,
            totalActions: redactedSummary.metrics.totalActions,
            verifiedActions: redactedSummary.metrics.verifiedActions,
            networkUsed: setupSession.consent.networkAllowed,
            administratorAuthorizationUsed: setupSession.consent.privilegeAllowed,
            osFamily: redactedSummary.environment.osFamily,
            architecture: redactedSummary.environment.architecture
        )
    }
}

struct ResearchFirstRunSummaryProjection: Equatable {
    let status: String
    let totalActions: Int
    let verifiedActions: Int
    let networkUsed: Bool
    let administratorAuthorizationUsed: Bool
    let osFamily: String
    let architecture: String
}

enum WholeWorkflowResearchTaskSevenContract {
    static let scenarioID = "project-wow-first-run"
    static let startingSurface = "first_run"
    static let briefID = UUID(uuidString: "11111111-1111-4111-8111-111111111111")!
    static let sessionID = UUID(uuidString: "22222222-2222-4222-8222-222222222222")!
    static let planID = UUID(uuidString: "33333333-3333-4333-8333-333333333333")!
    static let actionID = UUID(uuidString: "44444444-4444-4444-8444-444444444444")!
    static let receiptID = UUID(uuidString: "55555555-5555-4555-8555-555555555555")!
    static let orderedScenarioRecordIDs = [
        briefID.uuidString.lowercased(),
        sessionID.uuidString.lowercased(),
        planID.uuidString.lowercased(),
        actionID.uuidString.lowercased(),
        receiptID.uuidString.lowercased(),
    ]

    static func matchesScenario(_ scenario: ResearchScenario) -> Bool {
        scenario.taskNumber == 7
            && scenario.scenarioID == scenarioID
            && scenario.startingSurface == startingSurface
            && scenario.recordIDs == orderedScenarioRecordIDs
    }

    static func matchesSnapshot(_ snapshot: ResearchFirstRunSnapshot) -> Bool {
        snapshot == canonicalSnapshot
    }

    private static let canonicalSnapshot = ResearchFirstRunSnapshot(
        environmentBrief: EnvironmentBrief(
            schemaVersion: EnvironmentBrief.currentSchemaVersion,
            briefID: briefID,
            revision: 1,
            generatedAt: "2026-08-16T18:01:00Z",
            system: EnvironmentBriefSystem(
                osVersion: "26.6.1",
                architecture: .arm64,
                activeShell: "zsh",
                distributionChannel: "developer_id",
                updateAuthority: "sparkle"
            ),
            discoveredManagers: [
                EnvironmentBriefManagerObservation(
                    manager: "homebrew_formula",
                    detected: true,
                    eligibility: .eligible,
                    managementState: .ready,
                    activeInstallationMethod: "homebrew",
                    provenance: .homebrew,
                    freshness: .current
                ),
                EnvironmentBriefManagerObservation(
                    manager: "mise",
                    detected: true,
                    eligibility: .eligible,
                    managementState: .setupRequired,
                    activeInstallationMethod: "script_installer",
                    provenance: .mise,
                    freshness: .current
                ),
                EnvironmentBriefManagerObservation(
                    manager: "macports",
                    detected: false,
                    eligibility: .unknown,
                    managementState: .unknown,
                    activeInstallationMethod: nil,
                    provenance: nil,
                    freshness: .unknown
                ),
            ],
            coverage: EnvironmentBriefCoverage(
                intendedManagerCount: 3,
                currentManagerCount: 2,
                cachedManagerCount: 0,
                failedManagers: ["macports"],
                cancelledManagers: [],
                deferredManagers: []
            ),
            observationClass: .localOnly
        ),
        setupSession: ResearchSetupSession(
            schemaVersion: "1.0.0",
            sessionID: sessionID,
            briefID: briefID,
            briefRevision: 1,
            planID: planID,
            receiptID: receiptID,
            startedAt: "2026-08-16T18:01:00Z",
            updatedAt: "2026-08-16T18:02:00Z",
            state: "partially_completed",
            consent: ResearchSetupConsent(
                networkAllowed: false,
                mutationAllowed: true,
                privilegeAllowed: false
            ),
            coverage: ResearchSetupCoverage(
                completeManagers: ["homebrew_formula", "mise"],
                cachedManagers: [],
                failedManagers: ["macports"],
                cancelledManagers: [],
                deferredManagers: []
            ),
            actionStates: [
                ResearchSetupActionState(
                    actionID: actionID,
                    state: "verified",
                    updatedAt: "2026-08-16T18:01:50Z"
                ),
            ],
            resume: ResearchSetupResume(
                state: "not_applicable",
                lastDurableStage: "receipt_recorded",
                requiresRevalidation: false
            ),
            offlineBehavior: "opportunistic"
        ),
        plan: ResearchFirstRunPlan(
            schemaVersion: "1.0.0",
            planID: planID,
            sessionID: sessionID,
            briefID: briefID,
            briefRevision: 1,
            policyRevision: "research-v1",
            createdAt: "2026-08-16T18:01:15Z",
            state: "approved",
            actions: [canonicalAction]
        ),
        actionReceipt: ResearchActionReceipt(
            schemaVersion: "1.0.0",
            receiptID: receiptID,
            sessionID: sessionID,
            planID: planID,
            createdAt: "2026-08-16T18:02:00Z",
            status: "verified",
            summaryKey: "research.first_run.receipt.verified",
            summaryArgs: ["count": "1"],
            actionResults: [
                ResearchActionResult(
                    actionID: actionID,
                    typedActionID: "manager.setup.shell_defaults",
                    status: "verified",
                    applyResult: "applied",
                    verificationResult: "passed",
                    before: ResearchLocalizedFact(
                        localizationKey: "research.first_run.before.shell_hook_missing",
                        localizationArgs: ["manager": "mise"]
                    ),
                    after: ResearchLocalizedFact(
                        localizationKey: "research.first_run.after.shell_hook_verified",
                        localizationArgs: ["manager": "mise"]
                    ),
                    authority: "authoritative",
                    rollbackEligible: true,
                    recoveryLimits: ResearchLocalizedFact(
                        localizationKey: "research.first_run.recovery.remove_managed_hook",
                        localizationArgs: ["manager": "mise"]
                    )
                ),
            ],
            unchangedState: [
                ResearchLocalizedFact(
                    localizationKey: "research.first_run.unchanged.network_not_used",
                    localizationArgs: [:]
                ),
                ResearchLocalizedFact(
                    localizationKey: "research.first_run.unchanged.packages_not_changed",
                    localizationArgs: [:]
                ),
                ResearchLocalizedFact(
                    localizationKey: "research.first_run.unchanged.failed_source_not_modified",
                    localizationArgs: ["manager": "macports"]
                ),
            ]
        ),
        redactedSummary: ResearchRedactedSummary(
            schemaVersion: "1.0.0",
            sessionID: sessionID,
            redactionClass: "strict",
            sessionStatus: "verified",
            metrics: ResearchSummaryMetrics(
                totalActions: 1,
                verifiedActions: 1,
                noChangeActions: 0,
                failedVerificationActions: 0,
                failedActions: 0,
                cancelledOrInterruptedActions: 0,
                deferredActions: 0,
                managedActions: 1,
                durationSeconds: 60
            ),
            environment: ResearchSummaryEnvironment(
                osFamily: "macOS",
                architecture: "arm64"
            )
        )
    )

    private static let canonicalAction = ResearchPlannedAction(
        actionID: actionID,
        typedActionID: "manager.setup.shell_defaults",
        sequence: 0,
        dependsOn: [],
        target: ResearchActionTarget(kind: "manager", identifier: "mise"),
        mutationClass: "user_environment",
        requiresNetwork: false,
        requiresPrivilege: false,
        impactKey: "research.first_run.mise_shell_setup.impact",
        impactArgs: ["manager": "mise"],
        preVerificationStatus: "applicable",
        verificationMethodID: "manager.setup.verify_shell_defaults",
        expectedStateKey: "research.first_run.mise_shell_setup.expected",
        retryBehavior: "revalidate_then_retry",
        rollbackEligible: true,
        recoveryLimitsKey: "research.first_run.mise_shell_setup.recovery_limits"
    )
}

enum ResearchFirstRunProjector {
    static func project(
        _ dataset: WholeWorkflowResearchDataset
    ) -> ResearchFirstRunProjection? {
        guard WholeWorkflowResearchDatasetContract.matchesCurrentIdentityAndSafety(dataset),
              let scenario = dataset.scenarios.first(where: { $0.taskNumber == 7 }),
              WholeWorkflowResearchTaskSevenContract.matchesScenario(scenario),
              WholeWorkflowResearchTaskSevenContract.matchesSnapshot(dataset.firstRun),
              let recommendation = dataset.firstRun.plan.actions.first,
              let receiptResult = dataset.firstRun.actionReceipt.actionResults.first else {
            return nil
        }

        return ResearchFirstRunProjection(
            datasetID: dataset.datasetID,
            scenarioID: scenario.scenarioID,
            environmentBrief: dataset.firstRun.environmentBrief,
            setupSession: dataset.firstRun.setupSession,
            plan: dataset.firstRun.plan,
            actionReceipt: dataset.firstRun.actionReceipt,
            redactedSummary: dataset.firstRun.redactedSummary,
            recommendation: recommendation,
            receiptResult: receiptResult
        )
    }
}
