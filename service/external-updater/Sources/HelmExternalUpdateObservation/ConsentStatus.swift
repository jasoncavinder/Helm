import Foundation

/// Advisory history, never an eligibility flag or reusable permission token.
public enum ExternalConsentStatus: String, Encodable {
    case notRecorded, recorded, revoked, identityChanged, ledgerUnavailable, targetRejected, scopeChanged

    init?(code: UInt32) {
        switch code {
        case 20: self = .notRecorded
        case 21: self = .recorded
        case 22: self = .revoked
        case 23: self = .identityChanged
        case 24: self = .ledgerUnavailable
        case 25: self = .targetRejected
        case 26: self = .scopeChanged
        default: return nil
        }
    }

    var code: UInt32 {
        switch self {
        case .notRecorded: return 20
        case .recorded: return 21
        case .revoked: return 22
        case .identityChanged: return 23
        case .ledgerUnavailable: return 24
        case .targetRejected: return 25
        case .scopeChanged: return 26
        }
    }
}

struct NativeConsentProcessor {
    let identity: NativeHelperEvidence
    var helper: () throws -> NativeHelperEvidence = { try NativeHelperObserver().observeSelf() }
    var target: (String) throws -> NativeTargetEvidence = { try NativeTargetObserver().observe(path: $0) }
    var userApplications: () -> String? = { NativeApplicationRoots.userApplications }
    var inspect: (NativeHelperEvidence, NativeTargetEvidence, Data, String?) throws -> ExternalConsentStatus = {
        try NativeHelperLedger(identity: $0).consentStatus(evidence: $1, request: $2, userApplications: $3)
    }

    func assess(_ data: Data) throws -> ExternalConsentStatus {
        let root = userApplications()
        let request = try ExternalPreflightRequest.decode(data, userApplications: root)
        guard try helper() == identity else { throw BootstrapFailure.invalidated }
        var result = ExternalConsentStatus.targetRejected
        if let observed = try? target(request.targetPath),
           NativePolicyAssessment.assess(observed, userApplications: root, request: data) == .unresolved {
            result = (try? inspect(identity, observed, data, root)) ?? .ledgerUnavailable
            // A history read cannot hide a target change or new competing claim.
            guard let current = try? target(request.targetPath), current == observed else {
                throw BootstrapFailure.invalidated
            }
        }
        guard try helper() == identity, userApplications() == root else { throw BootstrapFailure.invalidated }
        return result
    }
}
