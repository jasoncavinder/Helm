import XCTest

final class EnvironmentBriefProjectionTests: XCTestCase {
    func testRevisionChangesOnlyWhenObservedStateChanges() {
        let generatedAt = Date(timeIntervalSince1970: 1_775_260_800)
        let briefID = UUID(uuidString: "550e8400-e29b-41d4-a716-446655440000")!
        let input = environmentBriefInput(
            intendedManagerIDs: ["mise", "homebrew_formula"],
            observations: [
                managerObservation("mise"),
                managerObservation("homebrew_formula", provenance: .homebrew)
            ]
        )

        let first = EnvironmentBriefProjector.project(
            input,
            generatedAt: generatedAt,
            briefID: briefID
        )
        let unchanged = EnvironmentBriefProjector.project(
            input,
            replacing: first,
            generatedAt: generatedAt.addingTimeInterval(60)
        )
        let changed = EnvironmentBriefProjector.project(
            environmentBriefInput(
                intendedManagerIDs: ["mise", "homebrew_formula"],
                observations: [
                    managerObservation("mise", managementState: .setupRequired),
                    managerObservation("homebrew_formula", provenance: .homebrew)
                ]
            ),
            replacing: unchanged,
            generatedAt: generatedAt.addingTimeInterval(120)
        )

        XCTAssertEqual(first.revision, 1)
        XCTAssertEqual(unchanged, first)
        XCTAssertEqual(changed.briefID, briefID)
        XCTAssertEqual(changed.revision, 2)
        XCTAssertNotEqual(changed.generatedAt, first.generatedAt)
    }

    func testCurrentPartialOfflineAndFailureCoverage() {
        let current = EnvironmentBriefProjector.project(
            environmentBriefInput(
                intendedManagerIDs: ["homebrew_formula", "mise"],
                observations: [
                    managerObservation("homebrew_formula", provenance: .homebrew),
                    managerObservation("mise")
                ]
            )
        )
        XCTAssertEqual(current.coverage.currentManagerCount, 2)
        XCTAssertEqual(current.coverage.cachedManagerCount, 0)

        let partial = EnvironmentBriefProjector.project(
            environmentBriefInput(
                intendedManagerIDs: ["homebrew_formula", "mise", "macports"],
                observations: [managerObservation("homebrew_formula", provenance: .homebrew)],
                failedManagerIDs: ["macports"]
            )
        )
        XCTAssertEqual(partial.coverage.intendedManagerCount, 3)
        XCTAssertEqual(partial.coverage.currentManagerCount, 1)
        XCTAssertEqual(partial.coverage.failedManagers, ["macports"])

        let offline = EnvironmentBriefProjector.project(
            environmentBriefInput(
                intendedManagerIDs: ["homebrew_formula"],
                observations: [
                    managerObservation(
                        "homebrew_formula",
                        provenance: .homebrew,
                        freshness: .cached
                    )
                ]
            )
        )
        XCTAssertEqual(offline.coverage.currentManagerCount, 0)
        XCTAssertEqual(offline.coverage.cachedManagerCount, 1)
        XCTAssertEqual(offline.observationClass, .localOnly)

        let serviceFailure = EnvironmentBriefProjector.project(
            environmentBriefInput(
                intendedManagerIDs: ["homebrew_formula", "mise"],
                observations: [],
                failedManagerIDs: ["mise", "homebrew_formula", "mise"]
            )
        )
        XCTAssertEqual(serviceFailure.coverage.currentManagerCount, 0)
        XCTAssertEqual(
            serviceFailure.coverage.failedManagers,
            ["homebrew_formula", "mise"]
        )
    }

    func testCanonicalizesDetectedAndUndetectedManagerRecords() {
        let brief = EnvironmentBriefProjector.project(
            environmentBriefInput(
                intendedManagerIDs: ["mise", "mise", "npm"],
                observations: [
                    EnvironmentBriefManagerObservation(
                        manager: "npm",
                        detected: false,
                        eligibility: .unknown,
                        managementState: .ready,
                        activeInstallationMethod: nil,
                        provenance: .sourceBuild,
                        freshness: .current
                    ),
                    managerObservation("mise", provenance: nil)
                ]
            )
        )

        XCTAssertEqual(brief.coverage.intendedManagerCount, 2)
        XCTAssertEqual(brief.coverage.currentManagerCount, 1)
        XCTAssertEqual(brief.discoveredManagers.map(\.manager), ["mise", "npm"])
        XCTAssertEqual(brief.discoveredManagers[0].provenance, .unknown)
        XCTAssertEqual(brief.discoveredManagers[1].managementState, .notInstalled)
        XCTAssertEqual(brief.discoveredManagers[1].freshness, .unknown)
        XCTAssertNil(brief.discoveredManagers[1].provenance)
    }

    func testEncodesSchemaContractWithoutExecutionFields() throws {
        let briefID = UUID(uuidString: "550e8400-e29b-41d4-a716-446655440000")!
        let brief = EnvironmentBriefProjector.project(
            environmentBriefInput(
                intendedManagerIDs: ["homebrew_formula", "mise", "macports"],
                observations: [
                    managerObservation("homebrew_formula", provenance: .homebrew),
                    managerObservation("mise", managementState: .setupRequired)
                ],
                failedManagerIDs: ["macports"]
            ),
            generatedAt: Date(timeIntervalSince1970: 1_775_260_800),
            briefID: briefID
        )
        let data = try JSONEncoder().encode(brief)
        let payload = try XCTUnwrap(
            JSONSerialization.jsonObject(with: data) as? [String: Any]
        )

        XCTAssertEqual(payload["schemaVersion"] as? String, "1.0.0")
        XCTAssertEqual(UUID(uuidString: payload["briefId"] as? String ?? ""), briefID)
        XCTAssertEqual(payload["revision"] as? Int, 1)
        XCTAssertNotNil(ISO8601DateFormatter().date(from: payload["generatedAt"] as? String ?? ""))
        XCTAssertEqual(payload["observationClass"] as? String, "local_only")
        XCTAssertNil(payload["command"])
        XCTAssertNil(payload["actionStates"])
        XCTAssertNil(payload["consent"])

        let managers = try XCTUnwrap(payload["discoveredManagers"] as? [[String: Any]])
        XCTAssertEqual(managers.map { $0["manager"] as? String }, ["homebrew_formula", "mise"])
        XCTAssertEqual(managers[1]["managementState"] as? String, "setup_required")
        XCTAssertEqual(managers[1]["provenance"] as? String, "mise")
    }

    private func environmentBriefInput(
        intendedManagerIDs: [String],
        observations: [EnvironmentBriefManagerObservation],
        failedManagerIDs: [String] = [],
        cancelledManagerIDs: [String] = [],
        deferredManagerIDs: [String] = []
    ) -> EnvironmentBriefProjectionInput {
        EnvironmentBriefProjectionInput(
            system: EnvironmentBriefSystem(
                osVersion: "14.5.0",
                architecture: .arm64,
                activeShell: "zsh",
                distributionChannel: "developer_id",
                updateAuthority: "sparkle"
            ),
            intendedManagerIDs: intendedManagerIDs,
            observations: observations,
            failedManagerIDs: failedManagerIDs,
            cancelledManagerIDs: cancelledManagerIDs,
            deferredManagerIDs: deferredManagerIDs,
            observationClass: .localOnly
        )
    }

    private func managerObservation(
        _ manager: String,
        managementState: EnvironmentBriefManagementState = .ready,
        provenance: EnvironmentBriefProvenance? = .mise,
        freshness: EnvironmentBriefFreshness = .current
    ) -> EnvironmentBriefManagerObservation {
        EnvironmentBriefManagerObservation(
            manager: manager,
            detected: true,
            eligibility: .eligible,
            managementState: managementState,
            activeInstallationMethod: nil,
            provenance: provenance,
            freshness: freshness
        )
    }
}
