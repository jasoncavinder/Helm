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
