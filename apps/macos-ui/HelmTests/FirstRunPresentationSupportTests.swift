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

    func testFirstRunRouteConfigurationIsDebugOnlyAndSupportsPreview() {
        XCTAssertEqual(
            EnvironmentBriefFirstRunConfiguration.mode(environment: [:]),
            .disabled
        )

        #if DEBUG
        XCTAssertEqual(
            EnvironmentBriefFirstRunConfiguration.mode(
                environment: [EnvironmentBriefFirstRunConfiguration.environmentKey: "true"]
            ),
            .enabled
        )
        XCTAssertEqual(
            EnvironmentBriefFirstRunConfiguration.mode(
                environment: [EnvironmentBriefFirstRunConfiguration.environmentKey: "preview"]
            ),
            .preview
        )
        XCTAssertEqual(
            EnvironmentBriefFirstRunConfiguration.previewAppearance(
                environment: [EnvironmentBriefFirstRunConfiguration.appearanceEnvironmentKey: "dark"]
            ),
            .dark
        )
        #endif

        XCTAssertEqual(
            EnvironmentBriefFirstRunConfiguration.previewAppearance(environment: [:]),
            .system
        )

        XCTAssertTrue(
            EnvironmentBriefFirstRunConfiguration.shouldPresent(
                mode: .enabled,
                hasCompletedOnboarding: false,
                dismissedPreview: false
            )
        )
        XCTAssertFalse(
            EnvironmentBriefFirstRunConfiguration.shouldPresent(
                mode: .enabled,
                hasCompletedOnboarding: true,
                dismissedPreview: false
            )
        )
        XCTAssertTrue(
            EnvironmentBriefFirstRunConfiguration.shouldPresent(
                mode: .preview,
                hasCompletedOnboarding: true,
                dismissedPreview: false
            )
        )
        XCTAssertFalse(
            EnvironmentBriefFirstRunConfiguration.shouldPresent(
                mode: .preview,
                hasCompletedOnboarding: true,
                dismissedPreview: true
            )
        )
        XCTAssertFalse(
            EnvironmentBriefFirstRunConfiguration.allowsAutomaticRefresh(
                mode: .preview,
                fixtureActive: true
            )
        )
        XCTAssertTrue(
            EnvironmentBriefFirstRunConfiguration.allowsAutomaticRefresh(
                mode: .preview,
                fixtureActive: false
            )
        )
        XCTAssertTrue(
            EnvironmentBriefFirstRunConfiguration.allowsAutomaticRefresh(
                mode: .enabled,
                fixtureActive: true
            )
        )
    }

    func testFixtureSummariesCoverAllRenderedBriefStates() {
        let summaries = Dictionary(
            uniqueKeysWithValues: EnvironmentBriefFixtureName.allCases.map { fixtureName in
                let brief = EnvironmentBriefFixtureProvider.fixture(named: fixtureName).brief
                return (fixtureName, EnvironmentBriefPresentationSummary.make(from: brief))
            }
        )

        XCTAssertEqual(summaries[.firstUseful]?.kind, .mapping)
        XCTAssertEqual(
            summaries[.firstUseful]?.completionFraction ?? 0,
            1.0 / 3.0,
            accuracy: 0.001
        )
        XCTAssertEqual(summaries[.current]?.kind, .current)
        XCTAssertEqual(summaries[.current]?.readyManagerCount, 3)
        XCTAssertEqual(summaries[.partial]?.kind, .partial)
        XCTAssertEqual(summaries[.partial]?.mappedManagerCount, 2)
        XCTAssertEqual(summaries[.partial]?.completionFraction, 0.5)
        XCTAssertEqual(summaries[.offline]?.kind, .cached)
        XCTAssertEqual(summaries[.serviceFailure]?.kind, .serviceFailure)
        XCTAssertEqual(summaries[.serviceFailure]?.attentionCount, 3)
        XCTAssertEqual(summaries[.serviceFailure]?.completionFraction, 0)
    }

    func testManagementReadinessUsesExclusiveTruthfulBuckets() {
        let brief = EnvironmentBriefProjector.project(
            EnvironmentBriefProjectionInput(
                system: EnvironmentBriefSystem(
                    osVersion: "26.6.0",
                    architecture: .arm64,
                    activeShell: "zsh",
                    distributionChannel: "developer_id",
                    updateAuthority: "sparkle"
                ),
                intendedManagerIDs: ["homebrew_formula", "mise", "rubygems", "macports"],
                observations: [
                    EnvironmentBriefManagerObservation(
                        manager: "homebrew_formula",
                        detected: true,
                        eligibility: .eligible,
                        managementState: .ready,
                        activeInstallationMethod: nil,
                        provenance: .homebrew,
                        freshness: .current
                    ),
                    EnvironmentBriefManagerObservation(
                        manager: "mise",
                        detected: true,
                        eligibility: .eligible,
                        managementState: .setupRequired,
                        activeInstallationMethod: nil,
                        provenance: .mise,
                        freshness: .current
                    ),
                    EnvironmentBriefManagerObservation(
                        manager: "rubygems",
                        detected: true,
                        eligibility: .ineligible,
                        managementState: .detectedUnmanageable,
                        activeInstallationMethod: nil,
                        provenance: .system,
                        freshness: .current
                    ),
                ],
                failedManagerIDs: ["homebrew_formula", "macports"],
                cancelledManagerIDs: [],
                deferredManagerIDs: [],
                observationClass: .localOnly
            )
        )

        XCTAssertEqual(
            EnvironmentBriefManagementReadiness.make(from: brief),
            EnvironmentBriefManagementReadiness(
                readyCount: 0,
                attentionCount: 3,
                observedOnlyCount: 1
            )
        )
    }

    func testSummaryDoesNotDoubleCountManagersWithObservationAndFailure() {
        let input = EnvironmentBriefProjectionInput(
            system: EnvironmentBriefSystem(
                osVersion: "26.6.0",
                architecture: .arm64,
                activeShell: "zsh",
                distributionChannel: "developer_id",
                updateAuthority: "sparkle"
            ),
            intendedManagerIDs: ["homebrew_formula", "rustup"],
            observations: [
                EnvironmentBriefManagerObservation(
                    manager: "homebrew_formula",
                    detected: true,
                    eligibility: .eligible,
                    managementState: .ready,
                    activeInstallationMethod: nil,
                    provenance: .homebrew,
                    freshness: .cached
                )
            ],
            failedManagerIDs: ["homebrew_formula", "rustup"],
            cancelledManagerIDs: [],
            deferredManagerIDs: [],
            observationClass: .localOnly
        )

        let summary = EnvironmentBriefPresentationSummary.make(
            from: EnvironmentBriefProjector.project(input)
        )

        XCTAssertEqual(summary.kind, .partial)
        XCTAssertEqual(summary.mappedManagerCount, 1)
        XCTAssertEqual(summary.attentionCount, 2)
        XCTAssertEqual(summary.completionFraction, 0.5)
    }

    func testSummaryIgnoresUndetectedManagersWhenComputingMappedProgress() {
        let brief = EnvironmentBriefProjector.project(
            EnvironmentBriefProjectionInput(
                system: EnvironmentBriefSystem(
                    osVersion: "26.6.0",
                    architecture: .arm64,
                    activeShell: "zsh",
                    distributionChannel: "developer_id",
                    updateAuthority: "sparkle"
                ),
                intendedManagerIDs: ["homebrew_formula", "mise"],
                observations: [
                    EnvironmentBriefManagerObservation(
                        manager: "homebrew_formula",
                        detected: true,
                        eligibility: .eligible,
                        managementState: .ready,
                        activeInstallationMethod: nil,
                        provenance: .homebrew,
                        freshness: .current
                    ),
                    EnvironmentBriefManagerObservation(
                        manager: "mise",
                        detected: false,
                        eligibility: .unknown,
                        managementState: .ready,
                        activeInstallationMethod: nil,
                        provenance: .mise,
                        freshness: .current
                    ),
                ],
                failedManagerIDs: [],
                cancelledManagerIDs: [],
                deferredManagerIDs: [],
                observationClass: .localOnly
            )
        )

        let summary = EnvironmentBriefPresentationSummary.make(from: brief)

        XCTAssertEqual(brief.coverage.currentManagerCount, 1)
        XCTAssertEqual(summary.kind, .mapping)
        XCTAssertEqual(summary.mappedManagerCount, 1)
        XCTAssertEqual(summary.attentionCount, 0)
        XCTAssertEqual(summary.completionFraction, 0.5)
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
