import Foundation
import XCTest

final class LibraryResultProvenanceTests: XCTestCase {
    func testRetryUsesFailedTaskBindingAfterPreviewRefresh() {
        for manager in ["cargo", "uv"] {
            let versionKey = manager == "cargo" ? "cargo_candidate_version" : "uv_candidate_version"
            var submittedArguments = [versionKey: "1.0.0"]
            var refreshedArguments = [versionKey: "2.0.0"]
            if manager == "uv" {
                submittedArguments["uv_package_identifier"] = "uv-tool:old-store:tool"
                refreshedArguments["uv_package_identifier"] = "uv-tool:new-store:tool"
            }
            let refreshed = UpgradePreviewPlanner.PlanStep(
                id: "\(manager):tool", orderIndex: 0, managerId: manager,
                authority: "language", action: "upgrade", packageName: "tool",
                reasonLabelKey: "service.task.label.upgrade.package",
                reasonLabelArgs: refreshedArguments, status: "queued"
            )
            let failed = UpgradePlanTaskProjection(
                stepId: refreshed.id, taskId: 1, status: "failed", managerId: manager,
                labelKey: refreshed.reasonLabelKey, labelArgs: submittedArguments
            )
            let retry = ReviewedUpgradeRequestProjection.resolveRetry(
                stepId: refreshed.id, managerId: refreshed.managerId, task: failed
            )
            XCTAssertEqual(retry?.version, "1.0.0")
            XCTAssertEqual(retry?.targetName, submittedArguments["uv_package_identifier"])
            XCTAssertNotEqual(retry?.version, refreshed.reasonLabelArgs[versionKey])
            XCTAssertNil(ReviewedUpgradeRequestProjection.resolveRetry(
                stepId: refreshed.id, managerId: manager, task: nil
            ))
            let legacy = UpgradePlanTaskProjection(
                stepId: refreshed.id, taskId: 2, status: "failed", managerId: manager,
                labelKey: refreshed.reasonLabelKey, labelArgs: nil
            )
            XCTAssertNil(ReviewedUpgradeRequestProjection.resolveRetry(
                stepId: refreshed.id, managerId: manager, task: legacy
            ))
            XCTAssertNil(ReviewedUpgradeRequestProjection.resolveRetry(
                stepId: "\(manager):another-tool", managerId: manager, task: failed
            ))
            XCTAssertNil(ReviewedUpgradeRequestProjection.resolveRetry(
                stepId: refreshed.id, managerId: manager == "cargo" ? "uv" : "cargo", task: failed
            ))
        }
    }

    func testCargoUpgradeUsesCandidateWithoutChangingRemovalVersion() {
        let package = PackageItem(id: "cargo:sd", name: "sd", version: "0.7.6",
                                  latestVersion: "1.0.0", managerId: "cargo", manager: "Cargo")
        XCTAssertEqual(package.upgradeMutationVersion, "1.0.0")
        XCTAssertNil(package.mutationVersion)
    }

    func testReviewedUpgradeRetryRetainsCargoAndUvBindings() {
        XCTAssertEqual(
            ReviewedUpgradeRequestProjection.resolve(
                managerId: "cargo", arguments: ["cargo_candidate_version": "1.0.0"]
            ),
            ReviewedUpgradeRequestProjection(targetName: nil, version: "1.0.0")
        )
        XCTAssertNil(ReviewedUpgradeRequestProjection.resolve(managerId: "cargo", arguments: [:]))
        XCTAssertNil(ReviewedUpgradeRequestProjection.resolve(
            managerId: "cargo", arguments: ["cargo_candidate_version": " "]
        ))
        XCTAssertEqual(
            ReviewedUpgradeRequestProjection.resolve(
                managerId: "uv",
                arguments: ["uv_candidate_version": "2.0", "uv_package_identifier": "uv-tool:store:ruff"]
            ),
            ReviewedUpgradeRequestProjection(targetName: "uv-tool:store:ruff", version: "2.0")
        )
        XCTAssertNil(ReviewedUpgradeRequestProjection.resolve(
            managerId: "uv", arguments: ["uv_candidate_version": "2.0"]
        ))
        XCTAssertEqual(
            ReviewedUpgradeRequestProjection.resolve(managerId: "npm", arguments: [:]),
            ReviewedUpgradeRequestProjection(targetName: nil, version: nil)
        )
    }

    func testUvMutationPreservesStoreIdentityAndUpgradeCandidateWithoutChangingRemovalVersion() {
        let package = PackageItem(id: "uv:ruff", name: "ruff",
                                  packageIdentifier: "uv-tool:store:ruff", version: "1.0",
                                  latestVersion: "2.0", managerId: "uv", manager: "uv")
        XCTAssertEqual(package.mutationPackageName, "ruff")
        XCTAssertEqual(package.mutationTargetPackageName, "uv-tool:store:ruff")
        XCTAssertEqual(package.upgradeMutationVersion, "2.0")
        XCTAssertNil(package.mutationVersion)
    }

    func testValidatesEverySupportedBoundaryCombination() throws {
        let cases: [(String, PackageResultProvenanceBoundary)] = [
            (
                provenanceJSON(origin: "local", discovery: "manager_snapshot", manager: "rustup"),
                .managerSnapshot
            ),
            (
                provenanceJSON(origin: "local_cache", discovery: "catalog_sync", manager: "cargo"),
                .localCacheSearch
            ),
            (
                provenanceJSON(
                    origin: "local_cache",
                    discovery: "manager_search",
                    manager: "cargo",
                    query: "ripgrep"
                ),
                .localCacheSearch
            ),
            (
                provenanceJSON(
                    origin: "remote",
                    discovery: "manager_search",
                    manager: "cargo",
                    query: "ripgrep"
                ),
                .directSearch
            ),
            (
                provenanceJSON(
                    origin: "deferred",
                    discovery: "manager_search",
                    manager: "cargo",
                    query: "ripgrep"
                ),
                .directSearch
            ),
        ]

        for (json, boundary) in cases {
            let provenance = try decode(PackageResultProvenance.self, json)
            let managerId = provenance.sourceManager
            XCTAssertNotNil(provenance.validated(for: managerId, at: boundary))
        }
    }

    func testDecodesAndValidatesVersionedManagerSearchProvenance() throws {
        let provenance = try decode(
            PackageResultProvenance.self,
            provenanceJSON(
                origin: "local_cache",
                discovery: "manager_search",
                manager: "cargo",
                query: "ripgrep"
            )
        )

        let validated = try XCTUnwrap(provenance.validated(for: "cargo", at: .localCacheSearch))
        XCTAssertEqual(validated.origin, .localCache)
        XCTAssertEqual(validated.discoverySource, .managerSearch)
        XCTAssertEqual(validated.originatingQuery, "ripgrep")
    }

    func testRejectsEndpointInvalidProvenance() throws {
        let snapshot = try decode(
            PackageResultProvenance.self,
            provenanceJSON(origin: "local", discovery: "manager_snapshot", manager: "cargo")
        )
        let cached = try decode(
            PackageResultProvenance.self,
            provenanceJSON(
                origin: "local_cache",
                discovery: "manager_search",
                manager: "cargo",
                query: "ripgrep"
            )
        )
        let remote = try decode(
            PackageResultProvenance.self,
            provenanceJSON(
                origin: "remote",
                discovery: "manager_search",
                manager: "cargo",
                query: "ripgrep"
            )
        )

        XCTAssertNil(snapshot.validated(for: "cargo", at: .localCacheSearch))
        XCTAssertNil(cached.validated(for: "cargo", at: .managerSnapshot))
        XCTAssertNil(remote.validated(for: "cargo", at: .localCacheSearch))
    }

    func testRejectsNoncanonicalManagerIdentitiesAndQueries() throws {
        let paddedManager = try decode(
            PackageResultProvenance.self,
            provenanceJSON(origin: "local", discovery: "manager_snapshot", manager: " cargo ")
        )
        let noncanonicalManager = try decode(
            PackageResultProvenance.self,
            provenanceJSON(origin: "local", discovery: "manager_snapshot", manager: "Cargo")
        )
        let mismatchedManager = try decode(
            PackageResultProvenance.self,
            provenanceJSON(origin: "local", discovery: "manager_snapshot", manager: "homebrew_formula")
        )
        let explicitEmptyCatalogQuery = try decode(
            PackageResultProvenance.self,
            provenanceJSON(origin: "local_cache", discovery: "catalog_sync", manager: "cargo", query: "")
        )
        let paddedManagerQuery = try decode(
            PackageResultProvenance.self,
            provenanceJSON(
                origin: "local_cache",
                discovery: "manager_search",
                manager: "cargo",
                query: " ripgrep "
            )
        )

        XCTAssertNil(paddedManager.validated(for: " cargo ", at: .managerSnapshot))
        XCTAssertNil(noncanonicalManager.validated(for: "Cargo", at: .managerSnapshot))
        XCTAssertNil(mismatchedManager.validated(for: "cargo", at: .managerSnapshot))
        XCTAssertNil(explicitEmptyCatalogQuery.validated(for: "cargo", at: .localCacheSearch))
        XCTAssertNil(paddedManagerQuery.validated(for: "cargo", at: .localCacheSearch))
    }

    func testRejectsUnknownSchemaAndEnumValuesWithoutFailingDecode() throws {
        let provenance = try decode(
            PackageResultProvenance.self,
            """
            {
              "schema_version": 2,
              "origin": "future_origin",
              "discovery_source": "future_discovery",
              "source_manager": "cargo"
            }
            """
        )

        XCTAssertEqual(provenance.origin, .unknown)
        XCTAssertEqual(provenance.discoverySource, .unknown)
        XCTAssertNil(provenance.validated(for: "cargo", at: .localCacheSearch))
    }

    func testMalformedNestedProvenanceDoesNotDiscardEnclosingResults() throws {
        let malformedProvenance =
            """
            {
              "schema_version": "future",
              "origin": {"kind": "future"},
              "discovery_source": "manager_search",
              "source_manager": "cargo"
            }
            """

        let results: [EnclosingResult] = try decode(
            [EnclosingResult].self,
            """
            [{
              "name": "ripgrep",
              "provenance": \(malformedProvenance)
            }]
            """
        )

        XCTAssertEqual(results.count, 1)
        XCTAssertEqual(results[0].name, "ripgrep")
        XCTAssertNil(results[0].provenance?.validated(for: "cargo", at: .localCacheSearch))
    }

    func testExactInspectorSelectionWinsBeforePersistedManagerPreference() {
        let selected = PackageInspectorSelectionPolicy.managerId(
            explicitManagerId: "cargo",
            selectedPackageManagerId: "homebrew_formula",
            persistedManagerId: "homebrew_formula",
            candidateManagerIds: ["cargo", "homebrew_formula"],
            fallbackManagerId: "homebrew_formula"
        )

        XCTAssertEqual(selected, "cargo")
    }

    func testExactPackageManagerWinsWhenDeepLinkHasNoExplicitManager() {
        let selected = PackageInspectorSelectionPolicy.managerId(
            explicitManagerId: nil,
            selectedPackageManagerId: "cargo",
            persistedManagerId: "homebrew_formula",
            candidateManagerIds: ["cargo", "homebrew_formula"],
            fallbackManagerId: "homebrew_formula"
        )

        XCTAssertEqual(selected, "cargo")
    }

    func testExactPackageManagerWinsBeforeRecommendationWhenNoPreferenceExists() {
        XCTAssertTrue(
            PackageInspectorSelectionPolicy.hasExactPackageSelection(
                selectedPackageId: "cargo:ripgrep",
                presentedPackageId: "cargo:ripgrep",
                presentedManagerId: "cargo",
                candidateManagerIds: ["cargo", "homebrew_formula"]
            )
        )

        let selected = PackageInspectorSelectionPolicy.managerId(
            explicitManagerId: nil,
            selectedPackageManagerId: "cargo",
            persistedManagerId: nil,
            candidateManagerIds: ["cargo", "homebrew_formula"],
            fallbackManagerId: "homebrew_formula"
        )

        XCTAssertEqual(selected, "cargo")
    }

    func testInspectorSelectionFallsBackWhenExplicitManagerIsNotInFamily() {
        let selected = PackageInspectorSelectionPolicy.managerId(
            explicitManagerId: "npm",
            selectedPackageManagerId: nil,
            persistedManagerId: "cargo",
            candidateManagerIds: ["cargo", "homebrew_formula"],
            fallbackManagerId: "homebrew_formula"
        )

        XCTAssertEqual(selected, "cargo")
    }

    private func provenanceJSON(
        origin: String,
        discovery: String,
        manager: String,
        query: String? = nil
    ) -> String {
        var fields = [
            "\"schema_version\": 1",
            "\"origin\": \"\(origin)\"",
            "\"discovery_source\": \"\(discovery)\"",
            "\"source_manager\": \"\(manager)\"",
        ]
        if let query {
            fields.append("\"originating_query\": \"\(query)\"")
        }
        return "{\(fields.joined(separator: ","))}"
    }

    private func decode<Value: Decodable>(_ type: Value.Type, _ json: String) throws -> Value {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(Value.self, from: Data(json.utf8))
    }

    private struct EnclosingResult: Decodable {
        let name: String
        let provenance: PackageResultProvenance?
    }
}
