import CExternalUpdatePolicy
import Foundation

/// A read-only diagnostic. Unresolved is NOT eligibility, adoption or consent.
/// No API accepts saved JSON or caller-provided observations/roots as authority.
public struct NativePolicyReport: Encodable {
    public let observation: NativeTargetEvidence
    public let assessment: NativePolicyAssessment
    public let canUpdate = false
}

public enum NativePolicyAssessment: String, Encodable {
    case unresolved, otherManager, outsideRoots, helmSelfUpdate, unsupportedTarget
    case invalidEvidence, internalFailure

    // Internal on purpose: public callers must perform a fresh native observation.
    static func assess(_ evidence: NativeTargetEvidence, userApplications: String?) -> Self {
        let slices: [[UInt8]] = [
            Array(evidence.canonicalPath.utf8), Array(evidence.bundleIdentifier.utf8),
            Array(evidence.build.utf8), Array(evidence.teamIdentifier.utf8),
            evidence.codeDirectoryHash, evidence.ed25519PublicKey,
            Array(evidence.feedURL.utf8), Array((userApplications ?? "").utf8)
        ]
        var offsets = [Int]()
        var bytes = [UInt8]()
        for slice in slices {
            offsets.append(bytes.count)
            bytes.append(contentsOf: slice)
        }
        guard let framework = UInt32(exactly: evidence.frameworkMajor) else { return .invalidEvidence }
        var exclusions: UInt32 = 0
        for exclusion in evidence.managerEvidence.exclusions {
            switch exclusion {
            case .appStoreReceipt: exclusions |= 1
            case .homebrewCaskReference: exclusions |= 2
            case .setappLocation: exclusions |= 4
            }
        }
        let code = bytes.withUnsafeBufferPointer { storage -> UInt32 in
            func slice(_ index: Int) -> HelmExternalBytes {
                HelmExternalBytes(data: slices[index].isEmpty ? nil : storage.baseAddress!.advanced(by: offsets[index]),
                                  length: slices[index].count)
            }
            var input = HelmExternalNativeTarget(
                abi_version: 1, canonical_path: slice(0), device: evidence.device, inode: evidence.inode,
                bundle_identifier: slice(1), build: slice(2), team_identifier: slice(3),
                code_directory_hash: slice(4), ed25519_public_key: slice(5), feed_url: slice(6),
                framework_major: framework, has_store_receipt: evidence.hasStoreReceipt ? 1 : 0,
                writable_by_others: evidence.writableByOthers ? 1 : 0, manager_exclusions: exclusions,
                user_applications_root: slice(7)
            )
            return helm_external_target_preflight(&input)
        }
        switch code {
        case UInt32(HELM_EXTERNAL_UNRESOLVED): return .unresolved
        case UInt32(HELM_EXTERNAL_OTHER_MANAGER): return .otherManager
        case UInt32(HELM_EXTERNAL_OUTSIDE_ROOTS): return .outsideRoots
        case UInt32(HELM_EXTERNAL_SELF_UPDATE): return .helmSelfUpdate
        case UInt32(HELM_EXTERNAL_UNSUPPORTED_TARGET): return .unsupportedTarget
        case UInt32(HELM_EXTERNAL_INVALID): return .invalidEvidence
        default: return .internalFailure
        }
    }
}

extension NativeTargetObserver {
    public func observeForPolicy(path: String) throws -> NativePolicyReport {
        let evidence = try observe(path: path)
        return NativePolicyReport(observation: evidence, assessment: NativePolicyAssessment.assess(
            evidence, userApplications: NativeApplicationRoots.userApplications
        ))
    }
}
