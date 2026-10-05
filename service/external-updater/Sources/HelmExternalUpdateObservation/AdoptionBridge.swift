import CExternalUpdatePolicy
import Foundation

/// Native-only inputs, not Codable or a public API. This transports facts, it
/// does not establish them. No production constructor currently supplies these:
/// live peer/sandbox proof and complete supported ownership observations remain
/// required before the authenticated coordinator may call the grant bridge.
struct NativeAdoptionBoundary: Equatable {
    let helperIdentifier: String
    let helperTeamIdentifier: String
    let helperCodeDirectoryHash: Data
    let callerIdentifier: String
    let callerTeamIdentifier: String
    let authenticatedLiveCaller: Bool
    let developerIDSignatureValid: Bool
    let notarizationAccepted: Bool
    let helmSandboxPreserved: Bool
    let externalHelperUnsandboxed: Bool
    let directConsumerChannel: Bool

    func withBoundary<T>(_ operation: (UnsafePointer<HelmExternalNativeBoundary>) -> T) -> T {
        withBytes(Data(helperIdentifier.utf8)) { helper in
            withBytes(Data(helperTeamIdentifier.utf8)) { team in
                withBytes(helperCodeDirectoryHash) { hash in
                    withBytes(Data(callerIdentifier.utf8)) { caller in
                        withBytes(Data(callerTeamIdentifier.utf8)) { callerTeam in
                            let flags = [authenticatedLiveCaller, developerIDSignatureValid, notarizationAccepted,
                                         helmSandboxPreserved, externalHelperUnsandboxed, directConsumerChannel]
                                .enumerated().reduce(UInt32(0)) { $1.element ? $0 | (1 << $1.offset) : $0 }
                            var input = HelmExternalNativeBoundary(abi_version: 1, helper_identifier: helper,
                                helper_team_identifier: team, helper_code_directory_hash: hash, caller_identifier: caller,
                                caller_team_identifier: callerTeam, observed_flags: flags)
                            return operation(&input)
                        }
                    }
                }
            }
        }
    }
}

enum NativeAdoptionOutcome: Equatable {
    case recorded, reviewChanged, outcomeUnknown

    init(code: UInt32) {
        switch code {
        case 51: self = .recorded
        case 52: self = .reviewChanged
        default: self = .outcomeUnknown
        }
    }
}

/// Worker-confined, one-shot owner of a private Rust review. The future native
/// coordinator must hold the existing-only filesystem lease around each call,
/// bind this object to one authenticated connection and admit confirmation once.
/// Neither this pointer nor any evidence/path is accepted through XPC.
final class NativeAdoptionReview {
    private var handle: OpaquePointer?
    private let userApplications: String?

    init(path: String, request: Data, target: NativeTargetEvidence, boundary: NativeAdoptionBoundary,
         userApplications: String?, now: UInt64) throws {
        self.userApplications = userApplications
        handle = NativePolicyAssessment.withTarget(target, userApplications: userApplications, invalid: nil) { target in
            boundary.withBoundary { boundary in
                withBytes(Data(path.utf8)) { path in
                    withBytes(request) { helm_external_adoption_prepare(path, $0, target, boundary, now) }
                }
            }
        }
        guard handle != nil else { throw HelperLedgerFailure.storageRejected }
    }

    deinit { helm_external_adoption_free(handle) }

    func confirm(path: String, target: NativeTargetEvidence, boundary: NativeAdoptionBoundary,
                 userApplications: String?, now: UInt64) -> NativeAdoptionOutcome {
        guard let handle else { return .reviewChanged }
        self.handle = nil
        var consumed = false
        defer { if !consumed { helm_external_adoption_free(handle) } }
        guard self.userApplications == userApplications else { return .reviewChanged }
        let code = NativePolicyAssessment.withTarget(target, userApplications: userApplications, invalid: UInt32(52)) { target in
            boundary.withBoundary { boundary in
                withBytes(Data(path.utf8)) { path in
                    consumed = true
                    return helm_external_adoption_confirm(handle, path, target, boundary, now)
                }
            }
        }
        return NativeAdoptionOutcome(code: code)
    }
}
