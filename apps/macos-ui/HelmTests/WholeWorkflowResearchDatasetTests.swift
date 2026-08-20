import XCTest

final class WholeWorkflowResearchDatasetTests: XCTestCase {
    func testCanonicalDatasetCoversTheSevenTaskProtocol() throws {
        let dataset = try WholeWorkflowResearchDatasetLoader.load(from: fixtureURL)

        XCTAssertEqual(dataset.datasetID, WholeWorkflowResearchDataset.currentDatasetID)
        XCTAssertEqual(dataset.scenarios.map(\.taskNumber), Array(1 ... 7))
        XCTAssertEqual(dataset.snapshot.updates.count, 12)
        XCTAssertEqual(Set(dataset.snapshot.updates.map(\.authority)), ["authoritative", "standard", "guarded"])
        XCTAssertEqual(dataset.snapshot.updates.filter(\.pinned).count, 1)
        XCTAssertEqual(
            dataset.snapshot.updates.filter {
                $0.requiresPrivilege && $0.planSelection == "included"
            }.count,
            1
        )
        XCTAssertTrue(
            dataset.snapshot.updates.contains {
                $0.managerID == "softwareupdate" && $0.planSelection == "excluded"
            }
        )
        XCTAssertEqual(dataset.snapshot.upgradePlan.state, "awaiting_confirmation")
        XCTAssertEqual(dataset.snapshot.upgradePlan.selectedUpdateIDs.count, 10)
        XCTAssertEqual(dataset.snapshot.upgradePlan.excludedUpdateIDs.count, 2)
        XCTAssertEqual(dataset.snapshot.installProposal.searchResultID, "search-ripgrep-homebrew")
        XCTAssertTrue(WholeWorkflowResearchDatasetValidator.validate(dataset).isEmpty)
    }

    func testCanonicalDatasetLinksRecoveryProvenanceAndFirstRunEvidence() throws {
        let dataset = try WholeWorkflowResearchDatasetLoader.load(from: fixtureURL)
        let rustup = try XCTUnwrap(dataset.snapshot.managers.first { $0.id == "rustup" })
        let firstRunAction = try XCTUnwrap(dataset.firstRun.plan.actions.first)

        XCTAssertEqual(rustup.installInstances.count, 2)
        XCTAssertEqual(rustup.installInstances.first(where: \.active)?.id, "rustup-user")
        XCTAssertEqual(dataset.snapshot.managerDecision.action, "keep_multiple")
        XCTAssertEqual(dataset.snapshot.managerDecision.revisitSurface, "environment")
        XCTAssertTrue(
            dataset.snapshot.activities.contains {
                $0.applyResult == "applied" && $0.verificationResult == "failed"
            }
        )
        XCTAssertTrue(
            dataset.snapshot.activities.contains {
                !$0.sourceStarted && $0.applyResult == "not_started"
            }
        )
        XCTAssertFalse(firstRunAction.requiresNetwork)
        XCTAssertFalse(firstRunAction.requiresPrivilege)
        XCTAssertEqual(dataset.firstRun.actionReceipt.status, "verified")
        XCTAssertEqual(dataset.firstRun.redactedSummary.redactionClass, "strict")
    }

    func testLoaderRejectsUnsafeDataset() throws {
        let original = try String(contentsOf: fixtureURL, encoding: .utf8)
        let modified = try replacingFirst(
            "\"syntheticOnly\": true",
            with: "\"syntheticOnly\": false",
            in: original
        )
        let dataset = try WholeWorkflowResearchDatasetLoader.decode(Data(modified.utf8))
        let issues = WholeWorkflowResearchDatasetValidator.validate(dataset)

        XCTAssertTrue(issues.contains { $0.code == "safety.synthetic_only" })
    }

    func testLoaderRejectsUnresolvedScenarioRecord() throws {
        let original = try String(contentsOf: fixtureURL, encoding: .utf8)
        let modified = try replacingFirst(
            "\"search-ripgrep-cargo\"",
            with: "\"search-ripgrep-missing\"",
            in: original
        )
        let dataset = try WholeWorkflowResearchDatasetLoader.decode(Data(modified.utf8))
        let issues = WholeWorkflowResearchDatasetValidator.validate(dataset)

        XCTAssertTrue(issues.contains { $0.code == "scenarios.unresolved_record" })
    }

    func testValidatorRejectsScenarioContractDrift() throws {
        let original = try String(contentsOf: fixtureURL, encoding: .utf8)
        let mutations = [
            (
                target: "\"scenarioId\": \"ambient-health\"",
                replacement: "\"scenarioId\": \"wrong-scenario\"",
                issueCode: "scenarios.identifier"
            ),
            (
                target: "\"startingSurface\": \"popover\"",
                replacement: "\"startingSurface\": \"dashboard\"",
                issueCode: "scenarios.starting_surface"
            ),
            (
                target: "\"plan-non-os-updates\",",
                replacement: "\"mise\",",
                issueCode: "scenarios.record_set"
            ),
        ]

        for mutation in mutations {
            let modified = try replacingFirst(
                mutation.target,
                with: mutation.replacement,
                in: original
            )
            let dataset = try WholeWorkflowResearchDatasetLoader.decode(Data(modified.utf8))
            let issues = WholeWorkflowResearchDatasetValidator.validate(dataset)

            XCTAssertTrue(
                issues.contains { $0.code == mutation.issueCode },
                "Expected \(mutation.issueCode) for \(mutation.target)"
            )
        }
    }

    func testDatasetPathSelectionIsDebugOnlyAndRequiresAbsolutePath() {
        XCTAssertNil(WholeWorkflowResearchDatasetProvider.selectedURL(environment: [:]))
        XCTAssertNil(
            WholeWorkflowResearchDatasetProvider.selectedURL(
                environment: [WholeWorkflowResearchDatasetProvider.environmentKey: "fixture.json"]
            )
        )

        #if DEBUG
        XCTAssertEqual(
            WholeWorkflowResearchDatasetProvider.selectedURL(
                environment: [WholeWorkflowResearchDatasetProvider.environmentKey: fixtureURL.path]
            ),
            fixtureURL
        )
        #endif
    }

    func testTaskTwoProjectsThroughTheProductionPlanContract() throws {
        let dataset = try WholeWorkflowResearchDatasetLoader.load(from: fixtureURL)
        let projection = try XCTUnwrap(WholeWorkflowResearchPlanProjector.project(dataset))

        XCTAssertEqual(projection.planID, "plan-non-os-updates")
        XCTAssertEqual(projection.state, "awaiting_confirmation")
        XCTAssertEqual(projection.steps.count, 12)
        XCTAssertEqual(projection.initialSelectedStepIDs.count, 10)
        XCTAssertEqual(
            projection.excludedStepIDs,
            ["update-npm-typescript", "update-macos"]
        )
        XCTAssertEqual(
            projection.steps.map(\.authority),
            [
                "authoritative", "authoritative",
                "standard", "standard", "standard", "standard", "standard",
                "guarded", "guarded", "guarded", "guarded", "guarded",
            ]
        )
        XCTAssertEqual(
            projection.steps.filter { $0.status == "not_included" }.map(\.id),
            ["update-npm-typescript", "update-macos"]
        )
        XCTAssertFalse(projection.isSelectable(stepID: "update-npm-typescript"))
        XCTAssertFalse(projection.isSelectable(stepID: "update-macos"))
        XCTAssertTrue(projection.isSelectable(stepID: "update-mise-node"))
    }

    func testTaskTwoRiskSummaryTracksOnlySelectedSyntheticRecords() throws {
        let dataset = try WholeWorkflowResearchDatasetLoader.load(from: fixtureURL)
        let projection = try XCTUnwrap(WholeWorkflowResearchPlanProjector.project(dataset))

        XCTAssertEqual(
            projection.riskSummary(selectedStepIDs: projection.initialSelectedStepIDs),
            .init(requiresElevatedPrivileges: true, mayRequireReboot: false)
        )

        let withoutAppStore = projection.initialSelectedStepIDs.subtracting(["update-mas-pages"])
        XCTAssertEqual(
            projection.riskSummary(selectedStepIDs: withoutAppStore),
            .init(requiresElevatedPrivileges: false, mayRequireReboot: false)
        )

        XCTAssertEqual(
            projection.riskSummary(
                selectedStepIDs: projection.initialSelectedStepIDs.union(["update-macos"])
            ),
            .init(requiresElevatedPrivileges: true, mayRequireReboot: true)
        )
    }

    func testTaskThreeProjectsSearchResultsAndBoundedConfirmation() throws {
        let dataset = try WholeWorkflowResearchDatasetLoader.load(from: fixtureURL)
        let projection = try XCTUnwrap(WholeWorkflowResearchLibraryProjector.project(dataset))

        XCTAssertEqual(projection.scenarioID, "find-and-install-ripgrep")
        XCTAssertEqual(projection.query, "ripgrep")
        XCTAssertEqual(
            projection.results.map(\.id),
            ["search-ripgrep-homebrew", "search-ripgrep-cargo"]
        )
        XCTAssertEqual(projection.results.map(\.origin), [.localCache, .remote])
        XCTAssertEqual(projection.results.filter(\.recommended).map(\.managerID), ["homebrew_formula"])

        let cachedOnly = projection.visibleResults(
            matching: "ripgrep",
            includeRemoteResults: false
        )
        XCTAssertEqual(cachedOnly.map(\.id), ["search-ripgrep-homebrew"])
        XCTAssertEqual(projection.resultState(for: cachedOnly[0]), .cached)

        let enriched = projection.visibleResults(
            matching: "ripgrep",
            includeRemoteResults: true
        )
        XCTAssertEqual(enriched.map(\.id), ["search-ripgrep-homebrew", "search-ripgrep-cargo"])
        XCTAssertEqual(projection.resultState(for: enriched[1]), .remote)

        let confirmation = try XCTUnwrap(
            projection.installConfirmation(forPackageID: "search-ripgrep-homebrew")
        )
        XCTAssertEqual(confirmation.id, "install-ripgrep-homebrew")
        XCTAssertEqual(confirmation.managerID, "homebrew_formula")
        XCTAssertTrue(confirmation.requiresNetwork)
        XCTAssertFalse(confirmation.requiresPrivilege)
        XCTAssertFalse(confirmation.isDeferred)
        XCTAssertNil(
            projection.installConfirmation(forPackageID: "search-ripgrep-cargo")
        )
    }

    func testTaskThreeOfflineVariantKeepsCachedResultAndMarksRemoteWorkDeferred() throws {
        let dataset = try WholeWorkflowResearchDatasetLoader.load(from: fixtureURL)
        let projection = try XCTUnwrap(
            WholeWorkflowResearchLibraryProjector.project(dataset, isOfflineVariant: true)
        )

        let results = projection.visibleResults(
            matching: "ripgrep",
            includeRemoteResults: false
        )
        XCTAssertEqual(results.map(\.id), ["search-ripgrep-homebrew", "search-ripgrep-cargo"])
        XCTAssertEqual(projection.resultState(for: results[0]), .cached)
        XCTAssertEqual(projection.resultState(for: results[1]), .deferred)
        XCTAssertTrue(
            try XCTUnwrap(
                projection.installConfirmation(forPackageID: "search-ripgrep-homebrew")
            ).isDeferred
        )
    }

    func testTaskThreeProjectionFailsClosedWhenScenarioDoesNotStartInLibrary() throws {
        let source = try String(contentsOf: fixtureURL, encoding: .utf8)
        let modified = try replacingFirst(
            "\"startingSurface\": \"library\"",
            with: "\"startingSurface\": \"activity\"",
            in: source
        )
        let dataset = try WholeWorkflowResearchDatasetLoader.decode(Data(modified.utf8))

        XCTAssertNil(WholeWorkflowResearchLibraryProjector.project(dataset))
    }

    func testTaskThreeRejectsCanonicalScenarioOrderDrift() throws {
        let source = try String(contentsOf: fixtureURL, encoding: .utf8)
        let modified = try replacingFirst(
            "\"search-ripgrep-homebrew\",\n        \"search-ripgrep-cargo\"",
            with: "\"search-ripgrep-cargo\",\n        \"search-ripgrep-homebrew\"",
            in: source
        )
        let dataset = try WholeWorkflowResearchDatasetLoader.decode(Data(modified.utf8))
        let issues = WholeWorkflowResearchDatasetValidator.validate(dataset)

        XCTAssertTrue(issues.contains { $0.code == "scenarios.record_order" })
        XCTAssertNil(WholeWorkflowResearchLibraryProjector.project(dataset))
    }

    func testTaskThreeRejectsSwappedCanonicalSourceManagers() throws {
        let source = try String(contentsOf: fixtureURL, encoding: .utf8)
        var modified = try replacingFirst(
            "\"id\": \"search-ripgrep-homebrew\",\n        \"managerId\": \"homebrew_formula\"",
            with: "\"id\": \"search-ripgrep-homebrew\",\n        \"managerId\": \"cargo\"",
            in: source
        )
        modified = try replacingFirst(
            "\"id\": \"search-ripgrep-cargo\",\n        \"managerId\": \"cargo\"",
            with: "\"id\": \"search-ripgrep-cargo\",\n        \"managerId\": \"homebrew_formula\"",
            in: modified
        )
        modified = try replacingFirst(
            "\"searchResultId\": \"search-ripgrep-homebrew\",\n      \"managerId\": \"homebrew_formula\"",
            with: "\"searchResultId\": \"search-ripgrep-homebrew\",\n      \"managerId\": \"cargo\"",
            in: modified
        )
        let dataset = try WholeWorkflowResearchDatasetLoader.decode(Data(modified.utf8))
        let issues = WholeWorkflowResearchDatasetValidator.validate(dataset)

        XCTAssertTrue(issues.contains { $0.code == "search.canonical_result" })
        XCTAssertNil(WholeWorkflowResearchLibraryProjector.project(dataset))
    }

    func testTaskThreeRejectsCanonicalRecommendationReasonDrift() throws {
        let source = try String(contentsOf: fixtureURL, encoding: .utf8)
        let modified = try replacingFirst(
            "\"recommendationReasonKey\": \"research.search.recommendation.existing_authority\"",
            with: "\"recommendationReasonKey\": \"research.search.recommendation.alternate_source\"",
            in: source
        )
        let dataset = try WholeWorkflowResearchDatasetLoader.decode(Data(modified.utf8))
        let issues = WholeWorkflowResearchDatasetValidator.validate(dataset)

        XCTAssertTrue(issues.contains { $0.code == "search.canonical_result" })
        XCTAssertNil(WholeWorkflowResearchLibraryProjector.project(dataset))
    }

    func testTaskThreeRejectsCanonicalInstallProposalIdentityDrift() throws {
        let source = try String(contentsOf: fixtureURL, encoding: .utf8)
        let modified = try replacingFirst(
            "\"id\": \"install-ripgrep-homebrew\",\n      \"searchResultId\": \"search-ripgrep-homebrew\"",
            with: "\"id\": \"install-ripgrep-drifted\",\n      \"searchResultId\": \"search-ripgrep-homebrew\"",
            in: source
        )
        let dataset = try WholeWorkflowResearchDatasetLoader.decode(Data(modified.utf8))
        let issues = WholeWorkflowResearchDatasetValidator.validate(dataset)

        XCTAssertTrue(issues.contains { $0.code == "install.identity" })
        XCTAssertNil(WholeWorkflowResearchLibraryProjector.project(dataset))
    }

    func testTaskThreeProjectionFailsClosedForDuplicateSearchResultIdentifiers() throws {
        let source = try String(contentsOf: fixtureURL, encoding: .utf8)
        let duplicateRecord = """
              {
                "id": "search-ripgrep-homebrew",
                "managerId": "cargo",
                "packageName": "ripgrep-copy",
                "version": "14.1.1",
                "resultOrigin": "remote",
                "recommended": false,
                "recommendationReasonKey": "research.search.recommendation.alternate_source",
                "deferredWhenOffline": true
              },
        """
        let modified = try replacingFirst(
            "    \"searchResults\": [\n",
            with: "    \"searchResults\": [\n\(duplicateRecord)",
            in: source
        )
        let dataset = try WholeWorkflowResearchDatasetLoader.decode(Data(modified.utf8))

        XCTAssertNil(WholeWorkflowResearchLibraryProjector.project(dataset))
    }

    func testTaskThreeProjectionFailsClosedWhenProposalRequirementsDrift() throws {
        let source = try String(contentsOf: fixtureURL, encoding: .utf8)
        let modified = try replacingFirst(
            "\"state\": \"awaiting_confirmation\",\n      \"requiresNetwork\": true,\n      \"requiresPrivilege\": false",
            with: "\"state\": \"awaiting_confirmation\",\n      \"requiresNetwork\": true,\n      \"requiresPrivilege\": true",
            in: source
        )
        let dataset = try WholeWorkflowResearchDatasetLoader.decode(Data(modified.utf8))

        XCTAssertNil(WholeWorkflowResearchLibraryProjector.project(dataset))
    }

    func testTaskFourProjectsAppliedUnverifiedAndUnstartedActivity() throws {
        let dataset = try WholeWorkflowResearchDatasetLoader.load(from: fixtureURL)
        let projection = try XCTUnwrap(
            WholeWorkflowResearchActivityProjector.project(dataset)
        )

        XCTAssertEqual(projection.scenarioID, "recover-from-failure")
        XCTAssertEqual(
            projection.activities.map(\.id),
            ["activity-npm-verification", "activity-mas-not-started"]
        )

        let failed = try XCTUnwrap(
            projection.activity(withSelectionID: "7001")
        )
        XCTAssertEqual(failed.managerID, "npm")
        XCTAssertEqual(failed.packageName, "prettier")
        XCTAssertEqual(failed.state, .failedVerification)
        XCTAssertEqual(failed.applyResult, .applied)
        XCTAssertEqual(failed.verificationResult, .failed)
        XCTAssertTrue(failed.sourceStarted)
        XCTAssertFalse(failed.rollbackEligible)

        let unstarted = try XCTUnwrap(
            projection.activity(withSelectionID: "7002")
        )
        XCTAssertEqual(unstarted.managerID, "mas")
        XCTAssertEqual(unstarted.packageName, "Pages")
        XCTAssertEqual(unstarted.state, .deferred)
        XCTAssertEqual(unstarted.applyResult, .notStarted)
        XCTAssertEqual(unstarted.verificationResult, .notRun)
        XCTAssertFalse(unstarted.sourceStarted)
        XCTAssertFalse(unstarted.rollbackEligible)
    }

    func testTaskFourProjectsExactRecoveryAvailabilityAndRedactedDiagnostics() throws {
        let dataset = try WholeWorkflowResearchDatasetLoader.load(from: fixtureURL)
        let projection = try XCTUnwrap(
            WholeWorkflowResearchActivityProjector.project(dataset)
        )
        let activity = try XCTUnwrap(
            projection.activity(withSelectionID: "7001")
        )
        let actions = projection.recoveryActions(for: activity)

        XCTAssertEqual(
            actions.map(\.kind),
            [.retryVerification, .restore, .keep, .copyDiagnostics]
        )
        XCTAssertEqual(actions.map(\.allowed), [true, false, true, true])
        XCTAssertEqual(
            projection.recoveryActions(for: projection.activities[1]),
            []
        )

        let diagnostics = projection.redactedDiagnostics(for: activity)
        XCTAssertEqual(
            diagnostics.components(separatedBy: "\n"),
            [
                "dataset_id=v0.20-whole-workflow-v1",
                "scenario_id=recover-from-failure",
                "activity_id=activity-npm-verification",
                "task_id=7001",
                "manager_id=npm",
                "state=failed_verification",
                "apply_result=applied",
                "verification_result=failed",
                "source_started=true",
                "rollback_eligible=false",
                "redaction_class=strict",
            ]
        )
    }

    func testTaskFourProjectionFailsClosedForPersonalDataSafetyDrift() throws {
        let source = try String(contentsOf: fixtureURL, encoding: .utf8)
        let modified = try replacingFirst(
            "\"containsPersonalData\": false",
            with: "\"containsPersonalData\": true",
            in: source
        )
        let dataset = try WholeWorkflowResearchDatasetLoader.decode(Data(modified.utf8))

        XCTAssertTrue(
            WholeWorkflowResearchDatasetValidator.validate(dataset).contains {
                $0.code == "safety.personal_data"
            }
        )
        XCTAssertNil(WholeWorkflowResearchActivityProjector.project(dataset))
    }

    func testTaskFourProjectionFailsClosedForNoncanonicalDatasetIdentity() throws {
        let source = try String(contentsOf: fixtureURL, encoding: .utf8)
        let mutations = [
            (
                target: "\"schemaVersion\": \"1.0.0\"",
                replacement: "\"schemaVersion\": \"2.0.0\"",
                issueCode: "dataset.schema_version"
            ),
            (
                target: "\"datasetId\": \"v0.20-whole-workflow-v1\"",
                replacement: "\"datasetId\": \"v0.20-whole-workflow-drifted\"",
                issueCode: "dataset.identifier"
            ),
        ]

        for mutation in mutations {
            let modified = try replacingFirst(
                mutation.target,
                with: mutation.replacement,
                in: source
            )
            let dataset = try WholeWorkflowResearchDatasetLoader.decode(Data(modified.utf8))

            XCTAssertTrue(
                WholeWorkflowResearchDatasetValidator.validate(dataset).contains {
                    $0.code == mutation.issueCode
                },
                "Expected \(mutation.issueCode) for \(mutation.target)"
            )
            XCTAssertNil(WholeWorkflowResearchActivityProjector.project(dataset))
        }
    }

    func testTaskFourProjectionFailsClosedForScenarioAndRecoveryDrift() throws {
        let source = try String(contentsOf: fixtureURL, encoding: .utf8)
        let wrongSurface = try replacingFirst(
            "\"scenarioId\": \"recover-from-failure\",\n      \"startingSurface\": \"activity\"",
            with: "\"scenarioId\": \"recover-from-failure\",\n      \"startingSurface\": \"dashboard\"",
            in: source
        )
        let wrongSurfaceDataset = try WholeWorkflowResearchDatasetLoader.decode(
            Data(wrongSurface.utf8)
        )
        XCTAssertNil(
            WholeWorkflowResearchActivityProjector.project(wrongSurfaceDataset)
        )

        let wrongAction = try replacingFirst(
            "\"action\": \"restore\",\n        \"allowed\": false",
            with: "\"action\": \"restore\",\n        \"allowed\": true",
            in: source
        )
        let wrongActionDataset = try WholeWorkflowResearchDatasetLoader.decode(
            Data(wrongAction.utf8)
        )
        let issues = WholeWorkflowResearchDatasetValidator.validate(wrongActionDataset)
        XCTAssertTrue(issues.contains { $0.code == "recovery.canonical_actions" })
        XCTAssertNil(
            WholeWorkflowResearchActivityProjector.project(wrongActionDataset)
        )

        let wrongOrder = try replacingFirst(
            "\"activity-npm-verification\",\n        \"activity-mas-not-started\"",
            with: "\"activity-mas-not-started\",\n        \"activity-npm-verification\"",
            in: source
        )
        let wrongOrderDataset = try WholeWorkflowResearchDatasetLoader.decode(
            Data(wrongOrder.utf8)
        )
        let wrongOrderIssues = WholeWorkflowResearchDatasetValidator.validate(
            wrongOrderDataset
        )
        XCTAssertTrue(
            wrongOrderIssues.contains { $0.code == "scenarios.record_order" }
        )
        XCTAssertNil(
            WholeWorkflowResearchActivityProjector.project(wrongOrderDataset)
        )
    }

    func testTaskFourProjectionFailsClosedForActivityStateDrift() throws {
        let source = try String(contentsOf: fixtureURL, encoding: .utf8)
        let modified = try replacingFirst(
            "\"state\": \"failed_verification\"",
            with: "\"state\": \"failed\"",
            in: source
        )
        let dataset = try WholeWorkflowResearchDatasetLoader.decode(Data(modified.utf8))
        let issues = WholeWorkflowResearchDatasetValidator.validate(dataset)

        XCTAssertTrue(issues.contains { $0.code == "activity.canonical_records" })
        XCTAssertNil(WholeWorkflowResearchActivityProjector.project(dataset))
    }

    func testWholeWorkflowSelectionFailsClosedForLiveOperations() throws {
        let environment = [WholeWorkflowResearchDatasetProvider.environmentKey: fixtureURL.path]

        #if DEBUG
        XCTAssertTrue(WholeWorkflowResearchDatasetProvider.isSelected(environment: environment))
        XCTAssertNotNil(WholeWorkflowResearchDatasetProvider.active(environment: environment))
        XCTAssertNotNil(
            WholeWorkflowResearchDatasetProvider.activePlanProjection(environment: environment)
        )
        XCTAssertNotNil(
            WholeWorkflowResearchDatasetProvider.activeLibraryProjection(environment: environment)
        )
        XCTAssertNotNil(
            WholeWorkflowResearchDatasetProvider.activeActivityProjection(environment: environment)
        )
        XCTAssertFalse(
            WholeWorkflowResearchDatasetProvider.isOfflineVariantSelected(environment: environment)
        )
        XCTAssertTrue(
            WholeWorkflowResearchDatasetProvider.isOfflineVariantSelected(
                environment: environment.merging(
                    [WholeWorkflowResearchDatasetProvider.offlineEnvironmentKey: "true"],
                    uniquingKeysWith: { _, new in new }
                )
            )
        )
        XCTAssertTrue(ResearchFixtureSafetyPolicy.blocksLiveOperations(environment: environment))

        let popoverEnvironment = [
            WayfinderPopoverFixtureProvider.environmentKey: "healthy"
        ]
        XCTAssertFalse(
            WholeWorkflowResearchDatasetProvider.isSelected(environment: popoverEnvironment)
        )
        XCTAssertTrue(
            ResearchFixtureSafetyPolicy.blocksLiveOperations(environment: popoverEnvironment)
        )

        let missingEnvironment = [
            WholeWorkflowResearchDatasetProvider.environmentKey: "/tmp/missing-helm-research.json"
        ]
        XCTAssertTrue(WholeWorkflowResearchDatasetProvider.isSelected(environment: missingEnvironment))
        XCTAssertNil(WholeWorkflowResearchDatasetProvider.active(environment: missingEnvironment))
        XCTAssertTrue(
            ResearchFixtureSafetyPolicy.blocksLiveOperations(environment: missingEnvironment)
        )
        #else
        XCTAssertFalse(WholeWorkflowResearchDatasetProvider.isSelected(environment: environment))
        XCTAssertFalse(ResearchFixtureSafetyPolicy.blocksLiveOperations(environment: environment))
        #endif
    }

    private var fixtureURL: URL {
        var root = URL(fileURLWithPath: #filePath)
        for _ in 0 ..< 4 {
            root.deleteLastPathComponent()
        }
        return root.appendingPathComponent(
            "docs/validation/fixtures/v0.20-whole-workflow-v1.json"
        )
    }

    private func replacingFirst(
        _ target: String,
        with replacement: String,
        in source: String
    ) throws -> String {
        let range = try XCTUnwrap(source.range(of: target))
        var result = source
        result.replaceSubrange(range, with: replacement)
        return result
    }
}
