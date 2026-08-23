import Foundation

struct WholeWorkflowResearchDatasetIssue: Equatable {
    let code: String
    let path: String
    let message: String
}

enum WholeWorkflowResearchDatasetValidator {
    static func validate(_ dataset: WholeWorkflowResearchDataset) -> [WholeWorkflowResearchDatasetIssue] {
        var issues: [WholeWorkflowResearchDatasetIssue] = []
        validateIdentityAndSafety(dataset, into: &issues)
        validateScenarios(dataset, into: &issues)
        validateSnapshot(dataset.snapshot, into: &issues)
        validateFirstRun(dataset.firstRun, into: &issues)
        return issues.sorted {
            ($0.path, $0.code) < ($1.path, $1.code)
        }
    }

    private static func validateIdentityAndSafety(
        _ dataset: WholeWorkflowResearchDataset,
        into issues: inout [WholeWorkflowResearchDatasetIssue]
    ) {
        require(
            dataset.schemaVersion == WholeWorkflowResearchDataset.currentSchemaVersion,
            code: "dataset.schema_version",
            path: "schemaVersion",
            message: "Expected schema version \(WholeWorkflowResearchDataset.currentSchemaVersion).",
            into: &issues
        )
        require(
            dataset.datasetID == WholeWorkflowResearchDataset.currentDatasetID,
            code: "dataset.identifier",
            path: "datasetId",
            message: "Expected the canonical versioned dataset identifier.",
            into: &issues
        )
        require(
            ISO8601DateFormatter().date(from: dataset.generatedAt) != nil,
            code: "dataset.generated_at",
            path: "generatedAt",
            message: "Expected an ISO 8601 timestamp.",
            into: &issues
        )

        let safety = dataset.safety
        require(safety.syntheticOnly, code: "safety.synthetic_only", path: "safety.syntheticOnly", message: "Research data must be synthetic.", into: &issues)
        require(safety.localOnly, code: "safety.local_only", path: "safety.localOnly", message: "Research data must remain local-only.", into: &issues)
        require(!safety.allowsMachineScan, code: "safety.machine_scan", path: "safety.allowsMachineScan", message: "Research data must not permit machine scanning.", into: &issues)
        require(!safety.allowsMutation, code: "safety.mutation", path: "safety.allowsMutation", message: "Research data must not permit real mutation.", into: &issues)
        require(!safety.containsPersonalData, code: "safety.personal_data", path: "safety.containsPersonalData", message: "Research data must not contain personal data.", into: &issues)
    }

    private static func validateScenarios(
        _ dataset: WholeWorkflowResearchDataset,
        into issues: inout [WholeWorkflowResearchDatasetIssue]
    ) {
        let taskNumbers = dataset.scenarios.map(\.taskNumber)
        require(
            Set(taskNumbers) == Set(1 ... 7) && taskNumbers.count == 7,
            code: "scenarios.task_numbers",
            path: "scenarios",
            message: "Expected exactly one scenario for each moderated task from 1 through 7.",
            into: &issues
        )
        requireUnique(dataset.scenarios.map(\.scenarioID), code: "scenarios.duplicate_id", path: "scenarios", into: &issues)

        let allRecordIDs = recordIDList(in: dataset)
        requireUnique(allRecordIDs, code: "records.duplicate_id", path: "dataset", into: &issues)
        let knownRecordIDs = Set(allRecordIDs)
        for (index, scenario) in dataset.scenarios.enumerated() {
            require(
                !scenario.recordIDs.isEmpty,
                code: "scenarios.empty_records",
                path: "scenarios[\(index)].recordIds",
                message: "Each task must reference at least one synthetic record.",
                into: &issues
            )
            for recordID in scenario.recordIDs where !knownRecordIDs.contains(recordID) {
                add(
                    code: "scenarios.unresolved_record",
                    path: "scenarios[\(index)].recordIds",
                    message: "Unknown record reference \(recordID).",
                    into: &issues
                )
            }

            guard let contract = scenarioContract(for: scenario.taskNumber, in: dataset) else {
                continue
            }
            require(
                scenario.scenarioID == contract.scenarioID,
                code: "scenarios.identifier",
                path: "scenarios[\(index)].scenarioId",
                message: "Task \(scenario.taskNumber) must use scenario identifier \(contract.scenarioID).",
                into: &issues
            )
            require(
                scenario.startingSurface == contract.startingSurface,
                code: "scenarios.starting_surface",
                path: "scenarios[\(index)].startingSurface",
                message: "Task \(scenario.taskNumber) must start on \(contract.startingSurface).",
                into: &issues
            )
            require(
                Set(scenario.recordIDs) == contract.recordIDs
                    && scenario.recordIDs.count == contract.recordIDs.count,
                code: "scenarios.record_set",
                path: "scenarios[\(index)].recordIds",
                message: "Task \(scenario.taskNumber) must reference its complete canonical record set.",
                into: &issues
            )
            if scenario.taskNumber == 3 {
                require(
                    scenario.recordIDs
                        == WholeWorkflowResearchTaskThreeContract.orderedScenarioRecordIDs,
                    code: "scenarios.record_order",
                    path: "scenarios[\(index)].recordIds",
                    message: "Task 3 must preserve cached, remote, then install-proposal order.",
                    into: &issues
                )
            }
            if scenario.taskNumber == 1 {
                require(
                    WholeWorkflowResearchTaskOneContract.matchesScenario(scenario),
                    code: "scenarios.record_order",
                    path: "scenarios[\(index)].recordIds",
                    message: "Task 1 must preserve coverage, failed verification, then failed source order.",
                    into: &issues
                )
            }
            if scenario.taskNumber == 4 {
                require(
                    WholeWorkflowResearchTaskFourContract.matchesScenario(scenario),
                    code: "scenarios.record_order",
                    path: "scenarios[\(index)].recordIds",
                    message: "Task 4 must preserve activity records before their ordered recovery actions.",
                    into: &issues
                )
            }
            if scenario.taskNumber == 5 {
                require(
                    WholeWorkflowResearchTaskFiveContract.matchesScenario(scenario),
                    code: "scenarios.record_order",
                    path: "scenarios[\(index)].recordIds",
                    message: "Task 5 must preserve manager, instances, then decision order.",
                    into: &issues
                )
            }
            if scenario.taskNumber == 6 {
                require(
                    WholeWorkflowResearchTaskSixContract.matchesScenario(scenario),
                    code: "scenarios.record_order",
                    path: "scenarios[\(index)].recordIds",
                    message: "Task 6 must preserve setting, failed activity, then diagnostics action order.",
                    into: &issues
                )
            }
        }
    }

    private struct ScenarioContract {
        let scenarioID: String
        let startingSurface: String
        let recordIDs: Set<String>
    }

    private static func scenarioContract(
        for taskNumber: Int,
        in dataset: WholeWorkflowResearchDataset
    ) -> ScenarioContract? {
        let snapshot = dataset.snapshot

        switch taskNumber {
        case 1:
            return ScenarioContract(
                scenarioID: WholeWorkflowResearchTaskOneContract.scenarioID,
                startingSurface: WholeWorkflowResearchTaskOneContract.startingSurface,
                recordIDs: Set(
                    WholeWorkflowResearchTaskOneContract.orderedScenarioRecordIDs
                )
            )
        case 2:
            return ScenarioContract(
                scenarioID: "review-updates",
                startingSurface: "plan",
                recordIDs: Set([snapshot.upgradePlan.id] + snapshot.updates.map(\.id))
            )
        case 3:
            return ScenarioContract(
                scenarioID: WholeWorkflowResearchTaskThreeContract.scenarioID,
                startingSurface: WholeWorkflowResearchTaskThreeContract.startingSurface,
                recordIDs: Set(WholeWorkflowResearchTaskThreeContract.orderedScenarioRecordIDs)
            )
        case 4:
            return ScenarioContract(
                scenarioID: WholeWorkflowResearchTaskFourContract.scenarioID,
                startingSurface: WholeWorkflowResearchTaskFourContract.startingSurface,
                recordIDs: Set(
                    WholeWorkflowResearchTaskFourContract.orderedScenarioRecordIDs
                )
            )
        case 5:
            return ScenarioContract(
                scenarioID: WholeWorkflowResearchTaskFiveContract.scenarioID,
                startingSurface: WholeWorkflowResearchTaskFiveContract.startingSurface,
                recordIDs: Set(WholeWorkflowResearchTaskFiveContract.orderedScenarioRecordIDs)
            )
        case 6:
            return ScenarioContract(
                scenarioID: WholeWorkflowResearchTaskSixContract.scenarioID,
                startingSurface: WholeWorkflowResearchTaskSixContract.startingSurface,
                recordIDs: Set(WholeWorkflowResearchTaskSixContract.orderedScenarioRecordIDs)
            )
        case 7:
            let firstRun = dataset.firstRun
            return ScenarioContract(
                scenarioID: "project-wow-first-run",
                startingSurface: "first_run",
                recordIDs: Set(
                    [
                        firstRun.environmentBrief.briefID,
                        firstRun.setupSession.sessionID,
                        firstRun.plan.planID,
                        firstRun.actionReceipt.receiptID,
                    ].map { $0.uuidString.lowercased() }
                        + firstRun.plan.actions.map { $0.actionID.uuidString.lowercased() }
                )
            )
        default:
            return nil
        }
    }

    private static func validateSnapshot(
        _ snapshot: ResearchWorkflowSnapshot,
        into issues: inout [WholeWorkflowResearchDatasetIssue]
    ) {
        let managerIDs = Set(snapshot.managers.map(\.id))
        requireUnique(snapshot.managers.map(\.id), code: "managers.duplicate_id", path: "snapshot.managers", into: &issues)
        require(snapshot.coverage.state == "partial", code: "coverage.state", path: "snapshot.coverage.state", message: "Task 1 requires partial coverage.", into: &issues)
        require(!snapshot.coverage.cachedManagerIDs.isEmpty, code: "coverage.cached", path: "snapshot.coverage.cachedManagerIds", message: "Task 1 requires cached coverage.", into: &issues)
        require(!snapshot.coverage.failedManagerIDs.isEmpty, code: "coverage.failed", path: "snapshot.coverage.failedManagerIds", message: "Task 1 requires an incomplete source.", into: &issues)
        require(
            WholeWorkflowResearchTaskOneContract.matchesCoverage(snapshot.coverage),
            code: "coverage.canonical_record",
            path: "snapshot.coverage",
            message: "Task 1 coverage must preserve its canonical current, cached, failed, and deferred sources.",
            into: &issues
        )
        require(
            snapshot.managers.contains(
                where: WholeWorkflowResearchTaskOneContract.matchesFailedManager
            ),
            code: "coverage.failed_manager_record",
            path: "snapshot.managers",
            message: "Task 1 must preserve the canonical failed source record.",
            into: &issues
        )

        let coverageIDs = snapshot.coverage.currentManagerIDs
            + snapshot.coverage.cachedManagerIDs
            + snapshot.coverage.failedManagerIDs
            + snapshot.coverage.deferredManagerIDs
        for managerID in coverageIDs where !managerIDs.contains(managerID) {
            add(code: "coverage.unknown_manager", path: "snapshot.coverage", message: "Unknown manager \(managerID).", into: &issues)
        }
        require(
            snapshot.managers
                .flatMap(\.installInstances)
                .allSatisfy { !$0.displayPath.hasPrefix("/Users/") },
            code: "safety.unredacted_path",
            path: "snapshot.managers.installInstances",
            message: "Synthetic paths must use <home> instead of a user-home path.",
            into: &issues
        )

        validateUpdates(snapshot.updates, managerIDs: managerIDs, into: &issues)
        validateUpgradePlan(snapshot.upgradePlan, updates: snapshot.updates, into: &issues)
        validateSearch(snapshot.searchResults, managerIDs: managerIDs, into: &issues)
        validateInstallProposal(snapshot.installProposal, results: snapshot.searchResults, into: &issues)
        validateRecovery(snapshot, managerIDs: managerIDs, into: &issues)
        validateRustup(snapshot.managers, into: &issues)
        validateManagerDecision(snapshot.managerDecision, managerIDs: managerIDs, into: &issues)
        validateSettings(snapshot.settings, into: &issues)
    }

    private static func validateUpdates(
        _ updates: [ResearchUpdateRecord],
        managerIDs: Set<String>,
        into issues: inout [WholeWorkflowResearchDatasetIssue]
    ) {
        require(updates.count == 12, code: "updates.count", path: "snapshot.updates", message: "Task 2 requires exactly 12 updates.", into: &issues)
        requireUnique(updates.map(\.id), code: "updates.duplicate_id", path: "snapshot.updates", into: &issues)
        require(
            Set(updates.map(\.authority)) == ["authoritative", "standard", "guarded"],
            code: "updates.authorities",
            path: "snapshot.updates",
            message: "Task 2 requires authoritative, standard, and guarded updates.",
            into: &issues
        )
        require(updates.filter(\.pinned).count == 1, code: "updates.pinned", path: "snapshot.updates", message: "Task 2 requires exactly one pinned update.", into: &issues)
        require(
            updates.filter(\.pinned).allSatisfy { $0.planSelection == "excluded" },
            code: "updates.pinned_exclusion",
            path: "snapshot.updates",
            message: "Pinned updates must be excluded from the reviewed plan.",
            into: &issues
        )
        require(
            updates.filter { $0.requiresPrivilege && $0.planSelection == "included" }.count == 1,
            code: "updates.privilege",
            path: "snapshot.updates",
            message: "Task 2 requires exactly one selected update that needs authorization.",
            into: &issues
        )
        let osUpdates = updates.filter { $0.managerID == "softwareupdate" }
        require(
            osUpdates.count == 1 && osUpdates.allSatisfy { $0.planSelection == "excluded" },
            code: "updates.os_exclusion",
            path: "snapshot.updates",
            message: "Task 2 requires one excluded operating-system update.",
            into: &issues
        )
        require(
            osUpdates.allSatisfy(\.restartRequired),
            code: "updates.restart_consequence",
            path: "snapshot.updates",
            message: "The excluded OS update must preserve the restart consequence for review.",
            into: &issues
        )
        for update in updates where !managerIDs.contains(update.managerID) {
            add(code: "updates.unknown_manager", path: "snapshot.updates", message: "Unknown manager \(update.managerID).", into: &issues)
        }
    }

    private static func validateUpgradePlan(
        _ plan: ResearchUpgradePlanRecord,
        updates: [ResearchUpdateRecord],
        into issues: inout [WholeWorkflowResearchDatasetIssue]
    ) {
        let updateIDs = Set(updates.map(\.id))
        let selected = Set(plan.selectedUpdateIDs)
        let excluded = Set(plan.excludedUpdateIDs)
        let expectedSelected = Set(updates.filter { $0.planSelection == "included" }.map(\.id))
        let expectedExcluded = Set(updates.filter { $0.planSelection == "excluded" }.map(\.id))

        require(plan.state == "awaiting_confirmation", code: "plan.state", path: "snapshot.upgradePlan.state", message: "Task 2 must stop at final confirmation.", into: &issues)
        require(plan.authorityOrder == ["authoritative", "standard", "guarded"], code: "plan.authority_order", path: "snapshot.upgradePlan.authorityOrder", message: "The plan must preserve Helm's authority order.", into: &issues)
        require(selected == expectedSelected, code: "plan.selected_updates", path: "snapshot.upgradePlan.selectedUpdateIds", message: "Selected plan records must match included updates.", into: &issues)
        require(excluded == expectedExcluded, code: "plan.excluded_updates", path: "snapshot.upgradePlan.excludedUpdateIds", message: "Excluded plan records must match excluded updates.", into: &issues)
        require(selected.isDisjoint(with: excluded), code: "plan.overlapping_updates", path: "snapshot.upgradePlan", message: "An update cannot be selected and excluded.", into: &issues)
        require(selected.union(excluded) == updateIDs, code: "plan.update_coverage", path: "snapshot.upgradePlan", message: "The plan must account for every update.", into: &issues)

        let selectedUpdates = updates.filter { selected.contains($0.id) }
        require(plan.requiresPrivilege == selectedUpdates.contains(where: \.requiresPrivilege), code: "plan.privilege_projection", path: "snapshot.upgradePlan.requiresPrivilege", message: "Privilege risk must derive from selected updates.", into: &issues)
        require(plan.restartRequired == selectedUpdates.contains(where: \.restartRequired), code: "plan.restart_projection", path: "snapshot.upgradePlan.restartRequired", message: "Restart risk must derive from selected updates.", into: &issues)
    }

    private static func validateSearch(
        _ results: [ResearchSearchResultRecord],
        managerIDs: Set<String>,
        into issues: inout [WholeWorkflowResearchDatasetIssue]
    ) {
        requireUnique(results.map(\.id), code: "search.duplicate_id", path: "snapshot.searchResults", into: &issues)
        require(
            results.map(\.id) == WholeWorkflowResearchTaskThreeContract.orderedSearchResultIDs,
            code: "search.canonical_order",
            path: "snapshot.searchResults",
            message: "Task 3 requires exactly the ordered Homebrew and Cargo ripgrep results.",
            into: &issues
        )
        for contract in WholeWorkflowResearchTaskThreeContract.orderedSearchResults {
            let record = results.first { $0.id == contract.id }
            require(
                record.map(contract.matches) == true,
                code: "search.canonical_result",
                path: "snapshot.searchResults[\(contract.id)]",
                message: "Task 3 result \(contract.id) does not match its canonical source contract.",
                into: &issues
            )
        }
        let ripgrep = results.filter { $0.packageName == "ripgrep" }
        require(
            ripgrep.contains { $0.resultOrigin == "local_cache" && $0.recommended },
            code: "search.cached_recommendation",
            path: "snapshot.searchResults",
            message: "Task 3 requires an immediate recommended cached ripgrep result.",
            into: &issues
        )
        require(
            ripgrep.contains { $0.resultOrigin == "remote" && $0.deferredWhenOffline },
            code: "search.remote_alternate",
            path: "snapshot.searchResults",
            message: "Task 3 requires a remote alternate that defers offline.",
            into: &issues
        )
        for result in results where !managerIDs.contains(result.managerID) {
            add(code: "search.unknown_manager", path: "snapshot.searchResults", message: "Unknown manager \(result.managerID).", into: &issues)
        }
    }

    private static func validateInstallProposal(
        _ proposal: ResearchInstallProposalRecord,
        results: [ResearchSearchResultRecord],
        into issues: inout [WholeWorkflowResearchDatasetIssue]
    ) {
        require(
            WholeWorkflowResearchTaskThreeContract.matchesInstallProposalIdentity(proposal),
            code: "install.identity",
            path: "snapshot.installProposal",
            message: "Task 3 requires the canonical Homebrew ripgrep install proposal identity.",
            into: &issues
        )
        let result = results.first { $0.id == proposal.searchResultID }
        require(result != nil, code: "install.unknown_search_result", path: "snapshot.installProposal.searchResultId", message: "The proposal must reference a search result.", into: &issues)
        require(result?.recommended == true, code: "install.recommendation", path: "snapshot.installProposal", message: "Task 3 must confirm the recommended source.", into: &issues)
        require(result?.managerID == proposal.managerID && result?.packageName == proposal.packageName, code: "install.target", path: "snapshot.installProposal", message: "The proposal target must match its search result.", into: &issues)
        require(proposal.state == "awaiting_confirmation", code: "install.state", path: "snapshot.installProposal.state", message: "Task 3 must stop at bounded confirmation.", into: &issues)
        require(proposal.requiresNetwork && !proposal.requiresPrivilege, code: "install.consequences", path: "snapshot.installProposal", message: "The ripgrep proposal must disclose network use without privilege.", into: &issues)
        require(proposal.offlineBehavior == "deferred", code: "install.offline", path: "snapshot.installProposal.offlineBehavior", message: "The offline variant must defer installation.", into: &issues)
    }

    private static func validateRecovery(
        _ snapshot: ResearchWorkflowSnapshot,
        managerIDs: Set<String>,
        into issues: inout [WholeWorkflowResearchDatasetIssue]
    ) {
        requireUnique(snapshot.activities.map(\.id), code: "activity.duplicate_id", path: "snapshot.activities", into: &issues)
        requireUnique(snapshot.activities.map(\.taskID), code: "activity.duplicate_task_id", path: "snapshot.activities", into: &issues)
        require(
            WholeWorkflowResearchTaskFourContract.matchesActivities(snapshot.activities),
            code: "activity.canonical_records",
            path: "snapshot.activities",
            message: "Task 4 requires the exact ordered failed-verification and unstarted-source records.",
            into: &issues
        )
        require(
            snapshot.activities.contains { $0.applyResult == "applied" && $0.verificationResult == "failed" },
            code: "activity.failed_verification",
            path: "snapshot.activities",
            message: "Tasks 1 and 4 require an applied action with failed verification.",
            into: &issues
        )
        require(
            snapshot.activities.contains { !$0.sourceStarted && $0.applyResult == "not_started" },
            code: "activity.unstarted_source",
            path: "snapshot.activities",
            message: "Task 4 requires a source that never started.",
            into: &issues
        )
        for activity in snapshot.activities where !managerIDs.contains(activity.managerID) {
            add(code: "activity.unknown_manager", path: "snapshot.activities", message: "Unknown manager \(activity.managerID).", into: &issues)
        }

        let activityIDs = Set(snapshot.activities.map(\.id))
        requireUnique(snapshot.recoveryActions.map(\.id), code: "recovery.duplicate_id", path: "snapshot.recoveryActions", into: &issues)
        require(
            WholeWorkflowResearchTaskFourContract.matchesRecoveryActions(
                snapshot.recoveryActions
            ),
            code: "recovery.canonical_actions",
            path: "snapshot.recoveryActions",
            message: "Task 4 requires the exact ordered recovery actions and availability contract.",
            into: &issues
        )
        let actions = Set(snapshot.recoveryActions.map(\.action))
        require(
            actions.isSuperset(of: ["retry_verification", "restore", "keep", "copy_diagnostics"]),
            code: "recovery.actions",
            path: "snapshot.recoveryActions",
            message: "Task 4 requires Retry Verification, Restore, Keep, and diagnostics choices.",
            into: &issues
        )
        for action in snapshot.recoveryActions where !activityIDs.contains(action.activityID) {
            add(code: "recovery.unknown_activity", path: "snapshot.recoveryActions", message: "Unknown activity \(action.activityID).", into: &issues)
        }
    }

    private static func validateRustup(
        _ managers: [ResearchManagerRecord],
        into issues: inout [WholeWorkflowResearchDatasetIssue]
    ) {
        require(
            WholeWorkflowResearchTaskFiveContract.matchesManagers(managers),
            code: "managers.canonical_records",
            path: "snapshot.managers",
            message: "Task 5 requires the exact ordered canonical manager records.",
            into: &issues
        )
        guard let rustup = managers.first(where: { $0.id == "rustup" }) else {
            add(code: "rustup.missing", path: "snapshot.managers", message: "Task 5 requires rustup.", into: &issues)
            return
        }
        require(
            WholeWorkflowResearchTaskFiveContract.matchesManager(rustup),
            code: "rustup.canonical_record",
            path: "snapshot.managers[rustup]",
            message: "Task 5 requires the canonical active user and policy-blocked system installations.",
            into: &issues
        )
        require(rustup.installInstances.count == 2, code: "rustup.instances", path: "snapshot.managers[rustup].installInstances", message: "Task 5 requires two rustup installations.", into: &issues)
        require(rustup.installInstances.filter(\.active).count == 1, code: "rustup.active", path: "snapshot.managers[rustup].installInstances", message: "Task 5 requires exactly one active rustup installation.", into: &issues)
        require(
            rustup.installInstances.contains { $0.active && $0.displayPath.hasPrefix("<home>/") && $0.policyState == "manageable" },
            code: "rustup.active_user_instance",
            path: "snapshot.managers[rustup].installInstances",
            message: "Task 5 requires an active manageable user installation.",
            into: &issues
        )
        require(
            rustup.installInstances.contains { !$0.active && $0.displayPath.hasPrefix("/usr/") && $0.policyState == "policy_blocked" },
            code: "rustup.blocked_system_instance",
            path: "snapshot.managers[rustup].installInstances",
            message: "Task 5 requires a policy-blocked system installation.",
            into: &issues
        )
    }

    private static func validateManagerDecision(
        _ decision: ResearchManagerDecisionRecord,
        managerIDs: Set<String>,
        into issues: inout [WholeWorkflowResearchDatasetIssue]
    ) {
        require(managerIDs.contains(decision.managerID), code: "decision.unknown_manager", path: "snapshot.managerDecision.managerId", message: "The decision must reference a manager.", into: &issues)
        require(
            WholeWorkflowResearchTaskFiveContract.matchesDecision(decision),
            code: "decision.canonical_record",
            path: "snapshot.managerDecision",
            message: "Task 5 requires the canonical reversible Keep Multiple acknowledgment.",
            into: &issues
        )
        require(decision.managerID == "rustup" && decision.action == "keep_multiple", code: "decision.keep_multiple", path: "snapshot.managerDecision", message: "Task 5 requires the rustup Keep Multiple decision.", into: &issues)
        require(decision.initialState == "pending" && decision.resultingState == "acknowledged", code: "decision.states", path: "snapshot.managerDecision", message: "The manager decision must capture acknowledgment.", into: &issues)
        require(decision.revisitSurface == "environment", code: "decision.revisit", path: "snapshot.managerDecision.revisitSurface", message: "Task 5 must provide an Environment revisit route.", into: &issues)
    }

    private static func validateSettings(
        _ settings: [ResearchSettingRecord],
        into issues: inout [WholeWorkflowResearchDatasetIssue]
    ) {
        requireUnique(settings.map(\.id), code: "settings.duplicate_id", path: "snapshot.settings", into: &issues)
        require(
            WholeWorkflowResearchTaskSixContract.matchesSettings(settings),
            code: "settings.canonical_record",
            path: "snapshot.settings",
            message: "Task 6 requires the canonical disabled launch-at-login setting.",
            into: &issues
        )
    }

    private static func validateFirstRun(
        _ firstRun: ResearchFirstRunSnapshot,
        into issues: inout [WholeWorkflowResearchDatasetIssue]
    ) {
        let brief = firstRun.environmentBrief
        let session = firstRun.setupSession
        let plan = firstRun.plan
        let receipt = firstRun.actionReceipt
        let summary = firstRun.redactedSummary

        require(brief.schemaVersion == EnvironmentBrief.currentSchemaVersion, code: "first_run.brief_schema", path: "firstRun.environmentBrief.schemaVersion", message: "Environment Brief schema version is not current.", into: &issues)
        require(brief.observationClass == .localOnly, code: "first_run.observation_class", path: "firstRun.environmentBrief.observationClass", message: "First-run observation must remain local-only.", into: &issues)
        require(brief.coverage.failedManagers.count == 1, code: "first_run.partial_failure", path: "firstRun.environmentBrief.coverage.failedManagers", message: "Task 7 requires one partial source failure.", into: &issues)
        require([session.schemaVersion, plan.schemaVersion, receipt.schemaVersion, summary.schemaVersion].allSatisfy { $0 == "1.0.0" }, code: "first_run.schema_versions", path: "firstRun", message: "First-run contracts must use schema version 1.0.0.", into: &issues)

        require(session.briefID == brief.briefID && plan.briefID == brief.briefID, code: "first_run.brief_link", path: "firstRun", message: "Session and plan must reference the Environment Brief.", into: &issues)
        require(session.briefRevision == brief.revision && plan.briefRevision == brief.revision, code: "first_run.brief_revision", path: "firstRun", message: "Session and plan must reference the Environment Brief revision.", into: &issues)
        require(session.sessionID == plan.sessionID && receipt.sessionID == session.sessionID && summary.sessionID == session.sessionID, code: "first_run.session_link", path: "firstRun", message: "First-run records must share one session identity.", into: &issues)
        require(session.planID == plan.planID && receipt.planID == plan.planID, code: "first_run.plan_link", path: "firstRun", message: "Session and receipt must reference the plan.", into: &issues)
        require(session.receiptID == receipt.receiptID, code: "first_run.receipt_link", path: "firstRun", message: "Session must reference the Action Receipt.", into: &issues)
        require(
            !session.consent.networkAllowed
                && session.consent.mutationAllowed
                && !session.consent.privilegeAllowed,
            code: "first_run.simulated_consent",
            path: "firstRun.setupSession.consent",
            message: "The fictional session must allow only the bounded simulated mutation.",
            into: &issues
        )

        require(plan.actions.count == 1, code: "first_run.safe_recommendation", path: "firstRun.plan.actions", message: "Task 7 requires one bounded recommendation.", into: &issues)
        if let action = plan.actions.first {
            require(!action.requiresNetwork && !action.requiresPrivilege, code: "first_run.safe_action", path: "firstRun.plan.actions[0]", message: "The safe recommendation must require neither network nor privilege.", into: &issues)
            require(session.actionStates.contains { $0.actionID == action.actionID && $0.state == "verified" }, code: "first_run.verified_state", path: "firstRun.setupSession.actionStates", message: "The setup session must record verified completion.", into: &issues)
            require(receipt.actionResults.contains { $0.actionID == action.actionID && $0.status == "verified" }, code: "first_run.verified_receipt", path: "firstRun.actionReceipt.actionResults", message: "The Action Receipt must prove verification.", into: &issues)
        }
        require(receipt.status == "verified" && summary.sessionStatus == "verified", code: "first_run.verified_summary", path: "firstRun", message: "Receipt and strict summary must report verified completion.", into: &issues)
        require(summary.redactionClass == "strict", code: "first_run.redaction", path: "firstRun.redactedSummary.redactionClass", message: "The copyable summary must use strict redaction.", into: &issues)
    }

    private static func recordIDList(in dataset: WholeWorkflowResearchDataset) -> [String] {
        let snapshot = dataset.snapshot
        let firstRun = dataset.firstRun
        let UUIDs = [
            firstRun.environmentBrief.briefID,
            firstRun.setupSession.sessionID,
            firstRun.plan.planID,
            firstRun.actionReceipt.receiptID,
        ].map { $0.uuidString.lowercased() }
        let actionIDs = firstRun.plan.actions.map { $0.actionID.uuidString.lowercased() }

        return [snapshot.coverage.id]
            + snapshot.managers.map(\.id)
            + snapshot.managers.flatMap { $0.installInstances.map(\.id) }
            + snapshot.updates.map(\.id)
            + [snapshot.upgradePlan.id]
            + snapshot.searchResults.map(\.id)
            + [snapshot.installProposal.id]
            + snapshot.activities.map(\.id)
            + snapshot.recoveryActions.map(\.id)
            + [snapshot.managerDecision.id]
            + snapshot.settings.map(\.id)
            + UUIDs
            + actionIDs
    }

    private static func requireUnique<T: Hashable>(
        _ values: [T],
        code: String,
        path: String,
        into issues: inout [WholeWorkflowResearchDatasetIssue]
    ) {
        require(values.count == Set(values).count, code: code, path: path, message: "Identifiers must be unique.", into: &issues)
    }

    private static func require(
        _ condition: Bool,
        code: String,
        path: String,
        message: String,
        into issues: inout [WholeWorkflowResearchDatasetIssue]
    ) {
        guard !condition else { return }
        add(code: code, path: path, message: message, into: &issues)
    }

    private static func add(
        code: String,
        path: String,
        message: String,
        into issues: inout [WholeWorkflowResearchDatasetIssue]
    ) {
        issues.append(WholeWorkflowResearchDatasetIssue(code: code, path: path, message: message))
    }
}
