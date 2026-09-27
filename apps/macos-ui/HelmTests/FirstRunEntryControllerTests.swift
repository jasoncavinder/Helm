import XCTest

final class FirstRunEntryControllerTests: XCTestCase {
    private final class Service {
        static let managerIDs: Set<String> = ["homebrew_formula", "homebrew_cask", "cargo"]
        var acknowledged = false
        var terms: String?
        var onboardingCompleted = false
        var failSave = false
        var failReadback = false
        var failActivation = false
        var invalidObservation = false
        var events: [String] = []
        var heldObservation: ((String?) -> Void)?
        var holdObservation = false

        var snapshot: String {
            let payload: [String: Any] = [
                "schema_version": 1,
                "experience": ["schema_version": 1, "experience_id": "wayfinder-v0.20", "acknowledged": acknowledged],
                "onboarding_completed": onboardingCompleted,
                "accepted_license_terms_version": terms as Any? ?? NSNull(),
                "requires_first_run_acknowledgment": true,
                "safe_mode": true
            ]
            return encode(payload)
        }

        var evidence: String {
            let rows: [[String: Any]] = Self.managerIDs.sorted().map { manager in
                ["manager_id": manager, "configured_enabled": NSNull(),
                 "selected_executable_path": NSNull(), "candidate_scan_status": "complete",
                 "inspected_path_count": 1, "candidate_paths": ["/tmp/candidate/\(manager)"],
                 "cached_detection": NSNull()]
            }
            return encode([
                "schema_version": 1, "experience_id": "wayfinder-v0.20", "managers": rows
            ])
        }

        private func encode(_ object: Any) -> String {
            guard let data = try? JSONSerialization.data(withJSONObject: object),
                  let json = String(data: data, encoding: .utf8) else {
                XCTFail("Invalid test fixture")
                return "{}"
            }
            return json
        }

        var client: FirstRunEntryController.Client {
            .init(prepare: { reply in
                self.events.append("prepare")
                reply(self.snapshot)
            }, acceptTerms: { terms, reply in
                self.events.append("terms")
                if !self.failSave && !self.failReadback { self.terms = terms }
                reply(!self.failSave)
            }, observe: { reply in
                self.events.append("observe")
                if self.holdObservation { self.heldObservation = reply } else {
                    reply(self.invalidObservation ? "{}" : self.evidence)
                }
            }, acknowledge: { experience, reply in
                XCTAssertEqual(experience, "wayfinder-v0.20")
                self.events.append("acknowledge")
                if !self.failSave && !self.failReadback { self.acknowledged = true }
                reply(!self.failSave)
            }, activate: { reply in
                self.events.append("activate")
                reply(!self.failActivation)
            })
        }
    }

    private func awaitPhase(_ expected: FirstRunEntryController.Phase, _ entry: FirstRunEntryController,
                            file: StaticString = #filePath, line: UInt = #line) {
        let deadline = Date().addingTimeInterval(2)
        while entry.phase != expected && Date() < deadline {
            RunLoop.main.run(until: Date().addingTimeInterval(0.005))
        }
        XCTAssertEqual(entry.phase, expected, file: file, line: line)
    }

    func testFreshEntryRequiresTermsThenObservationThenExplicitDurableCompletion() {
        let service = Service()
        let entry = FirstRunEntryController(expectedManagerIDs: Service.managerIDs)
        var activated = false
        XCTAssertTrue(entry.isPresenting)
        entry.begin(client: service.client, requiresLegalAcceptance: true, localAcceptedTerms: nil) { activated = true }
        awaitPhase(.legal, entry)
        XCTAssertEqual(service.events, ["prepare"])
        entry.continueToHelm()
        XCTAssertFalse(service.acknowledged)
        entry.acceptTerms()
        awaitPhase(.brief, entry)
        XCTAssertEqual(service.events, ["prepare", "terms", "prepare", "observe"])
        XCTAssertFalse(activated)
        XCTAssertFalse(service.onboardingCompleted)
        entry.continueToHelm()
        entry.continueToHelm()
        awaitPhase(.active, entry)
        XCTAssertEqual(service.events.suffix(3), ["acknowledge", "prepare", "activate"])
        XCTAssertTrue(activated)
        XCTAssertFalse(entry.isPresenting)
        XCTAssertFalse(service.onboardingCompleted)
        XCTAssertTrue(entry.snapshot?.safeMode == true)
    }

    func testLegacyCompletionStillEntersBriefWithoutReacceptingSavedTerms() {
        let service = Service()
        service.onboardingCompleted = true
        service.terms = AppUpdateConfiguration.currentLicenseTermsVersion
        let entry = FirstRunEntryController(expectedManagerIDs: Service.managerIDs)
        entry.begin(client: service.client, requiresLegalAcceptance: true, localAcceptedTerms: nil) {}
        awaitPhase(.brief, entry)
        XCTAssertEqual(service.events, ["prepare", "observe"])
        XCTAssertFalse(service.acknowledged)
    }

    func testExistingGUIAcceptanceMigratesWithoutGrantingExperienceAcknowledgment() {
        let service = Service()
        let entry = FirstRunEntryController(expectedManagerIDs: Service.managerIDs)
        entry.begin(client: service.client, requiresLegalAcceptance: true,
                    localAcceptedTerms: AppUpdateConfiguration.currentLicenseTermsVersion) {}
        awaitPhase(.brief, entry)
        XCTAssertEqual(service.events, ["prepare", "terms", "prepare", "observe"])
        XCTAssertFalse(service.acknowledged)
    }

    func testAlreadyAcknowledgedRCStablePatchAndRelaunchSkipObservation() {
        let service = Service()
        service.acknowledged = true
        service.terms = AppUpdateConfiguration.currentLicenseTermsVersion
        for _ in 0..<3 {
            let entry = FirstRunEntryController(expectedManagerIDs: Service.managerIDs)
            entry.begin(client: service.client, requiresLegalAcceptance: true, localAcceptedTerms: nil) {}
            awaitPhase(.active, entry)
        }
        XCTAssertEqual(service.events, ["prepare", "activate", "prepare", "activate", "prepare", "activate"])
    }

    func testFailedOrUnverifiedTermsSaveNeverObservesOrActivates() {
        for failedReply in [true, false] {
            let service = Service()
            service.failSave = failedReply
            service.failReadback = !failedReply
            let entry = FirstRunEntryController(expectedManagerIDs: Service.managerIDs)
            entry.begin(client: service.client, requiresLegalAcceptance: true, localAcceptedTerms: nil) {}
            awaitPhase(.legal, entry)
            entry.acceptTerms()
            awaitPhase(.failed, entry)
            XCTAssertFalse(service.events.contains("observe"))
            XCTAssertFalse(service.events.contains("activate"))
        }
    }

    func testFailedOrUnverifiedAcknowledgmentNeverActivates() {
        for failedReply in [true, false] {
            let service = Service()
            let entry = FirstRunEntryController(expectedManagerIDs: Service.managerIDs)
            entry.begin(client: service.client, requiresLegalAcceptance: false, localAcceptedTerms: nil) {}
            awaitPhase(.brief, entry)
            service.failSave = failedReply
            service.failReadback = !failedReply
            entry.continueToHelm()
            awaitPhase(.failed, entry)
            XCTAssertFalse(service.events.contains("activate"))
            XCTAssertFalse(entry.canContinue)
        }
    }

    func testMalformedObservationCannotMasqueradeAsAnEmptyEnvironment() {
        let service = Service()
        service.invalidObservation = true
        let entry = FirstRunEntryController(expectedManagerIDs: Service.managerIDs)
        entry.begin(client: service.client, requiresLegalAcceptance: false, localAcceptedTerms: nil) {}
        awaitPhase(.failed, entry)
        XCTAssertNil(entry.observation)
        entry.continueToHelm()
        XCTAssertFalse(service.events.contains("acknowledge"))
    }

    func testLegalStateIsRevalidatedAfterAcknowledgmentBeforeActivation() {
        let service = Service()
        service.terms = AppUpdateConfiguration.currentLicenseTermsVersion
        let entry = FirstRunEntryController(expectedManagerIDs: Service.managerIDs)
        entry.begin(client: service.client, requiresLegalAcceptance: true, localAcceptedTerms: nil) {}
        awaitPhase(.brief, entry)
        service.terms = nil
        entry.continueToHelm()
        awaitPhase(.legal, entry)
        XCTAssertFalse(service.events.contains("activate"))
        entry.acceptTerms()
        awaitPhase(.active, entry)
    }

    func testDisconnectedLateObservationCannotAdvanceReplacementSession() {
        let old = Service()
        old.holdObservation = true
        let entry = FirstRunEntryController(expectedManagerIDs: Service.managerIDs)
        entry.begin(client: old.client, requiresLegalAcceptance: false, localAcceptedTerms: nil) {}
        awaitPhase(.observing, entry)
        entry.disconnect()
        let next = Service()
        entry.begin(client: next.client, requiresLegalAcceptance: true, localAcceptedTerms: nil) {}
        old.heldObservation?(old.evidence)
        awaitPhase(.legal, entry)
        XCTAssertNil(entry.observation)
        XCTAssertEqual(next.events, ["prepare"])
    }

    func testDismissalWithoutExplicitContinueDoesNotAcknowledge() {
        let service = Service()
        let entry = FirstRunEntryController(expectedManagerIDs: Service.managerIDs)
        entry.begin(client: service.client, requiresLegalAcceptance: false, localAcceptedTerms: nil) {}
        awaitPhase(.brief, entry)
        entry.disconnect()
        XCTAssertFalse(service.acknowledged)
        entry.begin(client: service.client, requiresLegalAcceptance: false, localAcceptedTerms: nil) {}
        awaitPhase(.brief, entry)
        XCTAssertEqual(service.events, ["prepare", "observe", "prepare", "observe"])
    }

    func testFailedActivationDoesNotReportCompletionAndCanRetryDurableAcknowledgment() {
        let service = Service()
        service.failActivation = true
        let entry = FirstRunEntryController(expectedManagerIDs: Service.managerIDs)
        var activated = false
        entry.begin(client: service.client, requiresLegalAcceptance: false, localAcceptedTerms: nil) { activated = true }
        awaitPhase(.brief, entry)
        entry.continueToHelm()
        awaitPhase(.failed, entry)
        XCTAssertFalse(activated)
        XCTAssertTrue(service.acknowledged)
        service.failActivation = false
        entry.begin(client: service.client, requiresLegalAcceptance: false, localAcceptedTerms: nil) { activated = true }
        awaitPhase(.active, entry)
        XCTAssertTrue(activated)
        XCTAssertEqual(service.events.filter { $0 == "acknowledge" }.count, 1)
    }

    func testEvidenceDecoderRequiresCompleteUniqueKnownManagerIdentities() throws {
        let valid = Service().evidence
        let evidence = try XCTUnwrap(FirstRunLocalEvidence.decode(valid, expectedManagerIDs: Service.managerIDs))
        XCTAssertEqual(evidence.managers.count, Service.managerIDs.count)
        XCTAssertTrue(evidence.managers.allSatisfy { $0.cachedDetection == nil })
        for invalid in [nil, "{}", valid.replacingOccurrences(of: "wayfinder-v0.20", with: "unknown"),
                        valid.replacingOccurrences(of: "homebrew_formula", with: "homebrew_cask"),
                        valid.replacingOccurrences(of: "complete", with: "future"),
                        valid.replacingOccurrences(of: "\"inspected_path_count\":1", with: "\"inspected_path_count\":0")] {
            XCTAssertNil(FirstRunLocalEvidence.decode(invalid, expectedManagerIDs: Service.managerIDs))
        }
    }
}
