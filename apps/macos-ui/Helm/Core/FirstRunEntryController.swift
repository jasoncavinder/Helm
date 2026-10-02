import Combine
import Foundation

enum FirstRunReplyDelivery {
    static func deliver<Value>(_ value: Value, isCurrent: @escaping () -> Bool, reply: @escaping (Value) -> Void) {
        // XPC success replies may arrive off-main. Validate only after joining
        // the queue that owns connection generation and disconnect handling.
        DispatchQueue.main.async {
            guard isCurrent() else { return }
            reply(value)
        }
    }
}

/// Presentation state for the real storage-only startup boundary, never fixture progress.
final class FirstRunEntryController: ObservableObject {
    enum Phase: Equatable {
        case idle, preparing, legal, observing, brief, saving, activating, active
        case reviewingRepair, reviewRepair, applyingRepair, readingRepairReceipt, repairReceipt, repairUnavailable
        case failed
    }

    struct Client {
        let prepare: (@escaping (String?) -> Void) -> Void
        let acceptTerms: (String, @escaping (Bool) -> Void) -> Void
        let observe: (@escaping (String?) -> Void) -> Void
        let acknowledge: (String, @escaping (Bool) -> Void) -> Void
        let activate: (@escaping (Bool) -> Void) -> Void
        var reviewRepair: (@escaping (String?) -> Void) -> Void = { $0(nil) }
        var applyRepair: (String, @escaping (String?) -> Void) -> Void = { _, reply in reply(nil) }
    }

    @Published private(set) var phase: Phase = .idle
    @Published private(set) var observation: FirstRunLocalEvidence?
    @Published private(set) var repairReview: FirstRunRepairReview?
    @Published private(set) var repairReceipt: FirstRunRepairReceipt?
    @Published private(set) var repairMayHaveChanged = false
    private(set) var snapshot: ServiceStartupSnapshot?
    private var client: Client?
    private var generation = 0
    private let expectedManagerIDs: Set<String>
    private var requiresLegalAcceptance = true
    private var onActivated: (() -> Void)?

    var isPresenting: Bool { phase != .active }
    var canReviewRepair: Bool {
        snapshot?.acceptedLicenseTermsVersion == AppUpdateConfiguration.currentLicenseTermsVersion
    }
    var canContinue: Bool {
        phase == .brief || phase == .repairReceipt || phase == .repairUnavailable
            || (phase == .reviewRepair && repairReview?.plan == nil)
    }

    init(expectedManagerIDs: Set<String>) {
        self.expectedManagerIDs = expectedManagerIDs
    }

    func begin(
        client: Client,
        requiresLegalAcceptance: Bool,
        localAcceptedTerms: String?,
        onActivated: @escaping () -> Void
    ) {
        generation += 1
        let token = generation
        self.client = client
        self.requiresLegalAcceptance = requiresLegalAcceptance
        self.onActivated = onActivated
        snapshot = nil
        observation = nil
        repairReview = nil
        repairReceipt = nil
        repairMayHaveChanged = false
        phase = .preparing
        client.prepare { [weak self] json in
            self?.receive(token) { controller in
                guard let snapshot = ServiceStartupSnapshot.decode(json, requiringAcknowledgment: true) else {
                    controller.phase = .failed
                    return
                }
                controller.snapshot = snapshot
                let terms = AppUpdateConfiguration.currentLicenseTermsVersion
                if requiresLegalAcceptance && snapshot.acceptedLicenseTermsVersion != terms {
                    if localAcceptedTerms == terms {
                        // Preserve already-recorded GUI consent when the older shared store is unset.
                        controller.saveTerms()
                    } else {
                        controller.phase = .legal
                    }
                } else {
                    controller.afterLegalGate()
                }
            }
        }
    }

    func acceptTerms() {
        guard phase == .legal else { return }
        saveTerms()
    }

    private func saveTerms() {
        guard let client else { return }
        let token = generation
        phase = .saving
        client.acceptTerms(AppUpdateConfiguration.currentLicenseTermsVersion) { [weak self] success in
            self?.receive(token) { controller in
                guard success else { controller.phase = .failed; return }
                client.prepare { [weak controller] json in
                    controller?.receive(token) { current in
                        guard let snapshot = ServiceStartupSnapshot.decode(json, requiringAcknowledgment: true),
                              snapshot.acceptedLicenseTermsVersion == AppUpdateConfiguration.currentLicenseTermsVersion else {
                            current.phase = .failed
                            return
                        }
                        current.snapshot = snapshot
                        current.afterLegalGate()
                    }
                }
            }
        }
    }

    private func afterLegalGate() {
        if snapshot?.experience.acknowledged == true {
            activate()
        } else {
            observe()
        }
    }

    func observe() {
        guard phase == .preparing || phase == .saving || phase == .brief,
              let client else { return }
        generation += 1
        let token = generation
        phase = .observing
        observation = nil
        client.observe { [weak self] json in
            self?.receive(token) { controller in
                guard let evidence = FirstRunLocalEvidence.decode(json, expectedManagerIDs: controller.expectedManagerIDs) else {
                    controller.phase = .failed
                    return
                }
                controller.observation = evidence
                controller.phase = .brief
            }
        }
    }

    func continueToHelm() {
        guard canContinue, let client else { return }
        generation += 1
        let token = generation
        phase = .saving
        client.acknowledge("wayfinder-v0.20") { [weak self] success in
            self?.receive(token) { controller in
                guard success else { controller.phase = .failed; return }
                // Re-read the durable state; a successful reply alone is not completion.
                client.prepare { [weak controller] json in
                    controller?.receive(token) { current in
                        guard let snapshot = ServiceStartupSnapshot.decode(json, requiringAcknowledgment: true),
                              snapshot.experience.acknowledged else {
                            current.phase = .failed
                            return
                        }
                        current.snapshot = snapshot
                        if current.requiresLegalAcceptance
                            && snapshot.acceptedLicenseTermsVersion != AppUpdateConfiguration.currentLicenseTermsVersion {
                            current.phase = .legal
                            return
                        }
                        current.activate()
                    }
                }
            }
        }
    }

    private func activate() {
        guard let client, snapshot?.experience.acknowledged == true else {
            phase = .failed
            return
        }
        let token = generation
        phase = .activating
        client.activate { [weak self] success in
            self?.receive(token) { controller in
                guard success else { controller.phase = .failed; return }
                controller.phase = .active
                let completion = controller.onActivated
                controller.onActivated = nil
                controller.client = nil
                completion?()
            }
        }
    }

    func disconnect() {
        generation += 1
        client = nil
        onActivated = nil
        snapshot = nil
        observation = nil
        repairReview = nil
        repairReceipt = nil
        phase = .failed
    }

    func reviewRepair() {
        guard phase == .brief || phase == .repairUnavailable, canReviewRepair, let client else { return }
        generation += 1
        let token = generation
        repairReview = nil
        repairReceipt = nil
        phase = .reviewingRepair
        client.reviewRepair { [weak self] json in
            self?.receive(token) { current in
                guard current.phase == .reviewingRepair else { return }
                guard let review = FirstRunRepairReview.decode(json) else {
                    current.phase = .repairUnavailable
                    return
                }
                current.repairReview = review
                current.phase = .reviewRepair
            }
        }
    }

    func confirmRepair() {
        guard phase == .reviewRepair, let review = repairReview,
              let plan = review.plan, let reviewToken = review.reviewToken, let client else { return }
        generation += 1
        let token = generation
        let previousIDs = Set(review.receipts.map(\.receiptId))
        repairMayHaveChanged = true
        phase = .applyingRepair
        client.applyRepair(reviewToken) { [weak self] json in
            self?.receive(token) { current in
                guard current.phase == .applyingRepair else { return }
                current.phase = .readingRepairReceipt
                let replied = FirstRunRepairApplyReply.decode(json)?.receipt
                // Read the ledger even after a timeout/nil reply: a write may have
                // committed. Never infer rollback or resend the mutation here.
                client.reviewRepair { [weak current] savedJSON in
                    current?.receive(token) { controller in
                        guard controller.phase == .readingRepairReceipt else { return }
                        guard let saved = FirstRunRepairReview.decode(savedJSON) else {
                            controller.phase = .repairUnavailable
                            return
                        }
                        let matches = saved.receipts.filter {
                            !previousIDs.contains($0.receiptId) && $0.matches(plan)
                        }
                        guard matches.count == 1, let receipt = matches.first,
                              replied == nil || replied == receipt else {
                            controller.phase = .repairUnavailable
                            return
                        }
                        controller.repairReview = saved
                        controller.repairReceipt = receipt
                        controller.phase = .repairReceipt
                    }
                }
            }
        }
    }

    func returnToBrief() {
        guard phase == .reviewRepair || phase == .repairReceipt || phase == .repairUnavailable else { return }
        repairReview = nil
        repairReceipt = nil
        phase = .brief
        observe()
    }

    private func receive(_ token: Int, _ body: @escaping (FirstRunEntryController) -> Void) {
        DispatchQueue.main.async { [weak self] in
            guard let self, self.generation == token else { return }
            body(self)
        }
    }
}

struct FirstRunLocalEvidence: Decodable {
    struct Manager: Decodable, Identifiable {
        struct CachedDetection: Decodable {
            let installed: Bool
            let executablePath: String?
            let version: String?
        }

        let managerId: String
        let configuredEnabled: Bool?
        let selectedExecutablePath: String?
        let candidateScanStatus: String
        let inspectedPathCount: Int
        let candidatePaths: [String]
        let cachedDetection: CachedDetection?
        var id: String { managerId }
    }

    let schemaVersion: Int
    let experienceId: String
    let managers: [Manager]

    static func decode(_ json: String?, expectedManagerIDs: Set<String>) -> Self? {
        guard let data = json?.data(using: .utf8), data.count <= 2 * 1024 * 1024 else { return nil }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let evidence = try? decoder.decode(Self.self, from: data),
              evidence.schemaVersion == 1, evidence.experienceId == "wayfinder-v0.20",
              !expectedManagerIDs.isEmpty,
              Set(evidence.managers.map(\.id)) == expectedManagerIDs,
              evidence.managers.count == expectedManagerIDs.count,
              evidence.managers.allSatisfy({ manager in
                  ["complete", "partial", "not_supported"].contains(manager.candidateScanStatus)
                      && (0...256).contains(manager.inspectedPathCount)
                      && manager.candidatePaths.count <= manager.inspectedPathCount
                      && Set(manager.candidatePaths).count == manager.candidatePaths.count
                      && manager.candidatePaths.allSatisfy { $0.hasPrefix("/") && !$0.utf8.contains(0) }
              }) else { return nil }
        return evidence
    }
}

enum ProductionFirstRunGate {
    static var isEnabled: Bool {
        isEnabled(environment: ProcessInfo.processInfo.environment)
    }

    static func isEnabled(environment: [String: String]) -> Bool {
        #if DEBUG
        // Research/preview sessions must never acknowledge the shipping experience.
        return !ResearchFixtureSafetyPolicy.blocksLiveOperations(environment: environment)
            && EnvironmentBriefFixtureProvider.active(environment: environment) == nil
            && EnvironmentBriefFirstRunConfiguration.mode(environment: environment) == .disabled
        #else
        // Shipping entry is not an environment opt-in (or opt-out).
        return true
        #endif
    }
}
