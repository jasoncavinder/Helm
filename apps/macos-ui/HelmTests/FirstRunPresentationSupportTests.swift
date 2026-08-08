import XCTest

final class FirstRunPresentationSupportTests: XCTestCase {
    func testFixturesCoverRequiredFirstRunStatesDeterministically() {
        let fixtures = Dictionary(
            uniqueKeysWithValues: EnvironmentBriefFixtureName.allCases.map { name in
                (name, EnvironmentBriefFixtureProvider.fixture(named: name))
            }
        )

        XCTAssertEqual(fixtures.count, 5)
        XCTAssertEqual(fixtures[.firstUseful]?.brief.coverage.currentManagerCount, 1)
        XCTAssertEqual(fixtures[.current]?.brief.coverage.currentManagerCount, 3)
        XCTAssertEqual(fixtures[.partial]?.brief.coverage.failedManagers, ["macports"])
        XCTAssertEqual(fixtures[.partial]?.brief.coverage.deferredManagers, ["rustup"])
        XCTAssertEqual(fixtures[.offline]?.brief.coverage.cachedManagerCount, 2)
        XCTAssertEqual(fixtures[.serviceFailure]?.brief.coverage.failedManagers.count, 3)
        XCTAssertTrue(fixtures.values.allSatisfy { fixture in
            fixture.schemaVersion == EnvironmentBriefPresentationFixture.currentSchemaVersion
                && fixture.brief.observationClass == .localOnly
        })
        XCTAssertEqual(
            EnvironmentBriefFixtureProvider.fixture(named: .partial),
            fixtures[.partial]
        )
        XCTAssertEqual(Set(fixtures.values.map { $0.brief.briefID }).count, 5)
    }

    func testDebugFixtureSelectorRejectsUnknownNames() {
        XCTAssertNil(EnvironmentBriefFixtureProvider.active(environment: [:]))
        XCTAssertNil(
            EnvironmentBriefFixtureProvider.active(
                environment: [EnvironmentBriefFixtureProvider.environmentKey: "unknown"]
            )
        )

        #if DEBUG
        XCTAssertEqual(
            EnvironmentBriefFixtureProvider.active(
                environment: [EnvironmentBriefFixtureProvider.environmentKey: "offline"]
            )?.name,
            .offline
        )
        #endif
    }

    func testRestorationHonorsLegalGateBeforeAvailableBrief() {
        let brief = EnvironmentBriefFixtureProvider.fixture(named: .current).brief
        let saved = FirstRunPresentationState(
            schemaVersion: FirstRunPresentationState.currentSchemaVersion,
            stage: .brief,
            briefID: brief.briefID,
            briefRevision: brief.revision,
            selectedManagerID: "mise"
        )

        let restored = FirstRunPresentationRestorer.restore(
            saved: saved,
            currentBrief: brief,
            requiresLicenseAcceptance: true
        )

        XCTAssertEqual(restored.stage, .legal)
        XCTAssertNil(restored.selectedManagerID)
    }

    func testRestorationUsesDiscoveryUntilBriefIsAvailable() {
        let restored = FirstRunPresentationRestorer.restore(
            saved: nil,
            currentBrief: nil,
            requiresLicenseAcceptance: false
        )

        XCTAssertEqual(restored.stage, .discovering)
        XCTAssertNil(restored.briefID)
        XCTAssertNil(restored.briefRevision)
    }

    func testRestorationRefreshesBriefMetadataAndDropsStaleSelection() {
        let currentBrief = EnvironmentBriefFixtureProvider.fixture(named: .current).brief
        let saved = FirstRunPresentationState(
            schemaVersion: FirstRunPresentationState.currentSchemaVersion,
            stage: .brief,
            briefID: UUID(),
            briefRevision: 99,
            selectedManagerID: "removed-manager"
        )

        let restored = FirstRunPresentationRestorer.restore(
            saved: saved,
            currentBrief: currentBrief,
            requiresLicenseAcceptance: false
        )

        XCTAssertEqual(restored.stage, .brief)
        XCTAssertEqual(restored.briefID, currentBrief.briefID)
        XCTAssertEqual(restored.briefRevision, currentBrief.revision)
        XCTAssertNil(restored.selectedManagerID)
    }

    func testRestorationPreservesSelectionThatExistsInCurrentBrief() {
        let currentBrief = EnvironmentBriefFixtureProvider.fixture(named: .current).brief
        let saved = FirstRunPresentationState(
            schemaVersion: FirstRunPresentationState.currentSchemaVersion,
            stage: .brief,
            briefID: currentBrief.briefID,
            briefRevision: currentBrief.revision,
            selectedManagerID: "mise"
        )

        let restored = FirstRunPresentationRestorer.restore(
            saved: saved,
            currentBrief: currentBrief,
            requiresLicenseAcceptance: false
        )

        XCTAssertEqual(restored.selectedManagerID, "mise")
    }

    func testRestorationDoesNotCarrySelectionForwardFromDiscovery() {
        let currentBrief = EnvironmentBriefFixtureProvider.fixture(named: .current).brief
        let saved = FirstRunPresentationState(
            schemaVersion: FirstRunPresentationState.currentSchemaVersion,
            stage: .discovering,
            briefID: nil,
            briefRevision: nil,
            selectedManagerID: "mise"
        )

        let restored = FirstRunPresentationRestorer.restore(
            saved: saved,
            currentBrief: currentBrief,
            requiresLicenseAcceptance: false
        )

        XCTAssertEqual(restored.stage, .brief)
        XCTAssertNil(restored.selectedManagerID)
    }

    func testStateStoreRoundTripsAndRejectsUnknownSchemaVersion() throws {
        let suiteName = "FirstRunPresentationSupportTests.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let store = FirstRunPresentationStateStore(defaults: defaults)
        let state = FirstRunPresentationState(
            schemaVersion: FirstRunPresentationState.currentSchemaVersion,
            stage: .discovering,
            briefID: nil,
            briefRevision: nil,
            selectedManagerID: nil
        )

        store.save(state)
        XCTAssertEqual(store.load(), state)

        let unsupported = FirstRunPresentationState(
            schemaVersion: "2.0.0",
            stage: .brief,
            briefID: UUID(),
            briefRevision: 1,
            selectedManagerID: nil
        )
        store.save(unsupported)
        XCTAssertNil(store.load())

        store.clear()
        XCTAssertNil(store.load())
    }

    func testPresentationModelRestoresPersistsAndClearsState() throws {
        let suiteName = "FirstRunPresentationModelTests.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let store = FirstRunPresentationStateStore(defaults: defaults)
        let brief = EnvironmentBriefFixtureProvider.fixture(named: .current).brief
        let model = FirstRunPresentationModel(store: store)

        XCTAssertNil(model.state)
        model.synchronize(currentBrief: nil, requiresLicenseAcceptance: false)
        XCTAssertEqual(model.state?.stage, .discovering)
        XCTAssertEqual(store.load(), model.state)

        model.synchronize(currentBrief: brief, requiresLicenseAcceptance: false)
        XCTAssertEqual(model.state?.stage, .brief)
        XCTAssertEqual(model.state?.briefID, brief.briefID)
        XCTAssertEqual(store.load(), model.state)

        model.clear()
        XCTAssertNil(model.state)
        XCTAssertNil(store.load())
    }
}
