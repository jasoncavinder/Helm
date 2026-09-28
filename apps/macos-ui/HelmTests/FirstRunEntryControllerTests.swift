import XCTest

final class FirstRunEntryControllerTests: XCTestCase {
    func testReplyValidationAndDeliveryOccurOnMainQueue() {
        let delivered = expectation(description: "main-queue reply")
        DispatchQueue.global().async {
            FirstRunReplyDelivery.deliver("evidence", isCurrent: {
                XCTAssertTrue(Thread.isMainThread)
                return true
            }, reply: { value in
                XCTAssertTrue(Thread.isMainThread)
                XCTAssertEqual(value, "evidence")
                delivered.fulfill()
            })
        }
        wait(for: [delivered], timeout: 2)
    }

    func testConnectionChangedBeforeQueuedReplyCannotAdvanceFirstRun() {
        XCTAssertTrue(Thread.isMainThread)
        let service = Service()
        let snapshot = service.snapshot
        let entry = FirstRunEntryController(expectedManagerIDs: Service.managerIDs)
        let queued = DispatchSemaphore(value: 0)
        let validated = expectation(description: "validation after invalidation")
        var generation = 1
        var connected = true
        var client = service.client
        client = .init(prepare: { reply in
            DispatchQueue.global().async {
                FirstRunReplyDelivery.deliver(snapshot, isCurrent: {
                    XCTAssertTrue(Thread.isMainThread)
                    validated.fulfill()
                    return generation == 1 && connected
                }, reply: { value in
                    XCTFail("A stale service reply reached the first-run controller")
                    reply(value)
                })
                queued.signal()
            }
        }, acceptTerms: client.acceptTerms, observe: client.observe,
        acknowledge: client.acknowledge, activate: client.activate)
        entry.begin(client: client, requiresLegalAcceptance: true, localAcceptedTerms: nil) {
            XCTFail("A stale reply activated the runtime")
        }
        // Hold the main queue until the background transport has queued delivery.
        // Invalidation wins before any connection state is read by that reply.
        XCTAssertEqual(queued.wait(timeout: .now() + 2), .success)
        generation = 2
        connected = false
        entry.disconnect()
        wait(for: [validated], timeout: 2)
        XCTAssertEqual(entry.phase, .failed)
        XCTAssertTrue(service.events.isEmpty)
    }

    func testCurrentBackgroundReplyAdvancesControllerWithoutInlinePublishing() {
        let service = Service()
        let snapshot = service.snapshot
        let entry = FirstRunEntryController(expectedManagerIDs: Service.managerIDs)
        var client = service.client
        client = .init(prepare: { reply in
            DispatchQueue.global().async {
                FirstRunReplyDelivery.deliver(snapshot, isCurrent: { true }, reply: reply)
            }
        }, acceptTerms: client.acceptTerms, observe: client.observe,
        acknowledge: client.acknowledge, activate: client.activate)
        entry.begin(client: client, requiresLegalAcceptance: true, localAcceptedTerms: nil) {
            XCTFail("No acknowledgment was given")
        }
        XCTAssertEqual(entry.phase, .preparing)
        awaitPhase(.legal, entry)
        XCTAssertFalse(service.acknowledged)
    }

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
        var offerRepair = true
        var receipts: [[String: Any]] = []
        var invalidRepairReadback = false
        var omitAppliedReceipt = false
        var nilApplyReply = false
        var mismatchedApplyReply = false
        var duplicateApplyReply = false
        var holdApplyReply = false
        var heldApplyReply: ((String?) -> Void)?
        var repairVerification = "verified"

        static let fingerprint = String(repeating: "a", count: 64)
        static let repairToken = fingerprint + ":a:123:1"
        var plan: [String: Any] {
            ["fingerprint": Self.fingerprint, "policy_revision": 1,
             "action_id": "manager.clear_selected_executable_override",
             "mutation_class": "helm_preference", "requires_network": false,
             "requires_privilege": false, "rollback_eligible": false,
             "verification_method_id": "manager.detect_bound_executable",
             "before": ["manager": "mise", "enabled": true, "selected_executable_path": "/old/mise"],
             "executable": ["path": "/new/mise"]]
        }

        var receipt: [String: Any] {
            ["receipt_id": 1, "plan_fingerprint": Self.fingerprint,
             "action_id": "manager.clear_selected_executable_override",
             "previous_path": "/old/mise", "checked_path": "/new/mise",
             "applied": true, "verification": repairVerification,
             "observed_version": repairVerification == "verified" ? "2026.9.13" : NSNull(),
             "reason": repairVerification == "failed" ? "version_check_failed" : NSNull()]
        }

        var review: String {
            encode(["schema_version": 1, "plan": offerRepair ? plan as Any : NSNull(),
                    "review_token": offerRepair ? Self.repairToken as Any : NSNull(), "receipts": receipts])
        }

        var applyReply: String { encode(["schema_version": 1, "receipt": receipt]) }

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
            }, reviewRepair: { reply in
                self.events.append("review")
                reply(self.invalidRepairReadback ? nil : self.review)
            }, applyRepair: { token, reply in
                self.events.append("apply")
                XCTAssertEqual(token, Self.repairToken)
                self.offerRepair = false
                if !self.omitAppliedReceipt { self.receipts.append(self.receipt) }
                if self.holdApplyReply { self.heldApplyReply = reply; return }
                var result: String? = self.nilApplyReply ? nil : self.applyReply
                if self.mismatchedApplyReply {
                    var conflicting = self.receipt
                    conflicting["checked_path"] = "/other/mise"
                    result = self.encode(["schema_version": 1, "receipt": conflicting])
                }
                reply(result)
                if self.duplicateApplyReply { reply(result) }
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

    private func repairEntry(_ service: Service) -> FirstRunEntryController {
        service.terms = AppUpdateConfiguration.currentLicenseTermsVersion
        let entry = FirstRunEntryController(expectedManagerIDs: Service.managerIDs)
        entry.begin(client: service.client, requiresLegalAcceptance: true, localAcceptedTerms: nil) {}
        awaitPhase(.brief, entry)
        return entry
    }

    func testRepairRequiresLegalStateReviewAndExplicitConfirmation() {
        let service = Service()
        let entry = FirstRunEntryController(expectedManagerIDs: Service.managerIDs)
        entry.begin(client: service.client, requiresLegalAcceptance: true, localAcceptedTerms: nil) {}
        awaitPhase(.legal, entry)
        entry.reviewRepair()
        entry.confirmRepair()
        XCTAssertEqual(service.events, ["prepare"])
        entry.acceptTerms()
        awaitPhase(.brief, entry)
        entry.confirmRepair()
        XCTAssertFalse(service.events.contains("apply"))
        entry.reviewRepair()
        awaitPhase(.reviewRepair, entry)
        XCTAssertFalse(entry.canContinue)
        XCTAssertFalse(service.events.contains("apply"))
        entry.confirmRepair()
        entry.confirmRepair()
        awaitPhase(.repairReceipt, entry)
        XCTAssertEqual(service.events.suffix(3), ["review", "apply", "review"])
        XCTAssertEqual(service.events.filter { $0 == "apply" }.count, 1)
        XCTAssertFalse(service.acknowledged)
        XCTAssertFalse(service.events.contains("activate"))
        XCTAssertEqual(entry.repairReceipt?.verification, .verified)
        entry.continueToHelm()
        awaitPhase(.active, entry)
    }

    func testNoRepairCanBeRequestedWithoutRecordedTermsEvenWhenChannelSkipsLegalUI() {
        let service = Service()
        let entry = FirstRunEntryController(expectedManagerIDs: Service.managerIDs)
        entry.begin(client: service.client, requiresLegalAcceptance: false, localAcceptedTerms: nil) {}
        awaitPhase(.brief, entry)
        XCTAssertFalse(entry.canReviewRepair)
        entry.reviewRepair()
        XCTAssertEqual(service.events, ["prepare", "observe"])
    }

    func testAbandoningRepairReviewOnlyObservesAgain() {
        let service = Service()
        let entry = repairEntry(service)
        entry.reviewRepair()
        awaitPhase(.reviewRepair, entry)
        entry.returnToBrief()
        awaitPhase(.brief, entry)
        XCTAssertNil(entry.repairReview)
        XCTAssertFalse(service.events.contains("apply"))
        XCTAssertFalse(service.acknowledged)
    }

    func testFailedAndUnverifiedReceiptsRemainTruthfulAndAllowExplicitContinue() {
        for status in ["failed", "unverified"] {
            let service = Service()
            service.repairVerification = status
            let entry = repairEntry(service)
            entry.reviewRepair()
            awaitPhase(.reviewRepair, entry)
            entry.confirmRepair()
            awaitPhase(.repairReceipt, entry)
            XCTAssertEqual(entry.repairReceipt?.verification.rawValue, status)
            XCTAssertEqual(entry.repairReceipt?.titleKey, "app.first_run.repair." + status)
            XCTAssertTrue(entry.canContinue)
            XCTAssertNil(entry.repairReceipt?.observedVersion)
            XCTAssertFalse(service.events.contains("activate"))
        }
    }

    func testLostApplyReplyRecoversDurableReceiptWithoutRepeatingMutation() {
        let service = Service()
        service.nilApplyReply = true
        let entry = repairEntry(service)
        entry.reviewRepair()
        awaitPhase(.reviewRepair, entry)
        entry.confirmRepair()
        awaitPhase(.repairReceipt, entry)
        XCTAssertEqual(entry.repairReceipt?.verification, .verified)
        XCTAssertEqual(service.events.filter { $0 == "apply" }.count, 1)
    }

    func testMissingDurableReceiptNeverTrustsSuccessfulApplyReplyOrOldReceipt() {
        for oldReceipt in [false, true] {
            let service = Service()
            service.omitAppliedReceipt = true
            if oldReceipt { service.receipts = [service.receipt] }
            let entry = repairEntry(service)
            entry.reviewRepair()
            awaitPhase(.reviewRepair, entry)
            entry.confirmRepair()
            awaitPhase(.repairUnavailable, entry)
            XCTAssertNil(entry.repairReceipt)
            XCTAssertTrue(entry.repairMayHaveChanged)
            XCTAssertEqual(service.events.filter { $0 == "apply" }.count, 1)
        }
    }

    func testDisagreeingApplyAndDurableReceiptsAreNotReportedAsVerified() {
        let service = Service()
        service.mismatchedApplyReply = true
        let entry = repairEntry(service)
        entry.reviewRepair()
        awaitPhase(.reviewRepair, entry)
        entry.confirmRepair()
        awaitPhase(.repairUnavailable, entry)
        XCTAssertNil(entry.repairReceipt)
    }

    func testFailedReadbackCanReviewHistoryWithoutResendingRepair() {
        let service = Service()
        let entry = repairEntry(service)
        entry.reviewRepair()
        awaitPhase(.reviewRepair, entry)
        service.invalidRepairReadback = true
        entry.confirmRepair()
        awaitPhase(.repairUnavailable, entry)
        service.invalidRepairReadback = false
        entry.reviewRepair()
        awaitPhase(.reviewRepair, entry)
        XCTAssertNil(entry.repairReview?.plan)
        XCTAssertEqual(entry.repairReview?.receipts.count, 1)
        entry.confirmRepair()
        XCTAssertTrue(entry.canContinue)
        XCTAssertEqual(service.events.filter { $0 == "apply" }.count, 1)
    }

    func testDuplicateApplyCallbackDoesNotIssueTwoReadbacksOrReactivate() {
        let service = Service()
        service.duplicateApplyReply = true
        let entry = repairEntry(service)
        entry.reviewRepair()
        awaitPhase(.reviewRepair, entry)
        entry.confirmRepair()
        awaitPhase(.repairReceipt, entry)
        RunLoop.main.run(until: Date().addingTimeInterval(0.02))
        XCTAssertEqual(service.events.filter { $0 == "review" }.count, 2)
        XCTAssertFalse(service.events.contains("activate"))
    }

    func testLateRepairCallbackCannotOverwriteReplacementSession() {
        let old = Service()
        old.holdApplyReply = true
        let entry = repairEntry(old)
        entry.reviewRepair()
        awaitPhase(.reviewRepair, entry)
        entry.confirmRepair()
        XCTAssertEqual(entry.phase, .applyingRepair)
        entry.continueToHelm()
        XCTAssertFalse(old.acknowledged)
        entry.disconnect()
        let next = Service()
        entry.begin(client: next.client, requiresLegalAcceptance: true, localAcceptedTerms: nil) {}
        old.heldApplyReply?(old.applyReply)
        awaitPhase(.legal, entry)
        XCTAssertNil(entry.repairReceipt)
        XCTAssertEqual(old.events.filter { $0 == "review" }.count, 1)
    }

    func testRepairDecoderRejectsUnknownUnsafeOrInconsistentCapabilities() throws {
        let service = Service()
        let valid = service.review
        XCTAssertNotNil(FirstRunRepairReview.decode(valid))
        for (from, to) in [
            ("manager.clear_selected_executable_override", "future.action"),
            ("helm_preference", "shell_change"), ("manager.detect_bound_executable", "future"),
            ("\"policy_revision\":1", "\"policy_revision\":2"),
            ("\"requires_network\":false", "\"requires_network\":true"),
            ("\"requires_privilege\":false", "\"requires_privilege\":true"),
            ("\"rollback_eligible\":false", "\"rollback_eligible\":true"),
            ("\"enabled\":true", "\"enabled\":false"),
            ("\"manager\":\"mise\"", "\"manager\":\"cargo\""),
            (Service.repairToken, String(repeating: "b", count: 64) + ":a:123:1")
        ] {
            let invalid = valid.replacingOccurrences(of: from, with: to)
            XCTAssertNotEqual(invalid, valid, from)
            XCTAssertNil(FirstRunRepairReview.decode(invalid), from)
        }
        service.offerRepair = false
        XCTAssertNotNil(FirstRunRepairReview.decode(service.review))
        service.receipts = [service.receipt, service.receipt]
        XCTAssertNil(FirstRunRepairReview.decode(service.review))
    }

    func testRepairReceiptDecoderRequiresHonestStateAndBoundedPayloads() throws {
        let service = Service()
        let valid = service.applyReply
        let decoded = try XCTUnwrap(FirstRunRepairApplyReply.decode(valid))
        XCTAssertTrue(decoded.receipt.isValid)
        for (from, to) in [
            ("\"applied\":true", "\"applied\":false"),
            ("\"receipt_id\":1", "\"receipt_id\":0"),
            ("\"verified\"", "\"future\""), ("\"verified\"", "\"unverified\""),
            ("\"verified\"", "\"failed\""), ("2026.9.13", ""),
            ("2026.9.13", "bad\\noutput")
        ] {
            let invalid = valid.replacingOccurrences(of: from, with: to)
            XCTAssertNotEqual(invalid, valid)
            XCTAssertNil(FirstRunRepairApplyReply.decode(invalid))
        }
        XCTAssertNil(FirstRunRepairReview.decode(String(repeating: "x", count: 2 * 1024 * 1024 + 1)))
        service.repairVerification = "failed"
        XCTAssertNotNil(FirstRunRepairApplyReply.decode(service.applyReply))
        XCTAssertNil(FirstRunRepairApplyReply.decode(service.applyReply.replacingOccurrences(of: "version_check_failed", with: "future")))
    }
}
