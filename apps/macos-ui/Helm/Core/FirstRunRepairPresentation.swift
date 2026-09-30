import Foundation

/// Strict transport decoding only. Eligibility, mutation and verification stay in Rust.
struct FirstRunRepairReview: Decodable {
    struct Plan: Decodable {
        struct Preference: Decodable {
            let manager: String
            let enabled: Bool
            let selectedExecutablePath: String
        }
        struct Executable: Decodable { let path: String }
        let fingerprint: String
        let policyRevision: Int
        let actionId: String
        let mutationClass: String
        let requiresNetwork: Bool
        let requiresPrivilege: Bool
        let rollbackEligible: Bool
        let verificationMethodId: String
        let before: Preference
        let executable: Executable

        var isValid: Bool {
            FirstRunRepairTransport.isFingerprint(fingerprint) && policyRevision == 1
                && actionId == FirstRunRepairTransport.actionID && mutationClass == "helm_preference"
                && !requiresNetwork && !requiresPrivilege && !rollbackEligible
                && verificationMethodId == "manager.detect_bound_executable"
                && before.manager == "mise" && before.enabled
                && FirstRunRepairTransport.isPath(before.selectedExecutablePath)
                && FirstRunRepairTransport.isPath(executable.path)
                && before.selectedExecutablePath != executable.path
        }
    }

    let schemaVersion: Int
    let plan: Plan?
    let reviewToken: String?
    let receipts: [FirstRunRepairReceipt]

    static func decode(_ json: String?) -> Self? {
        guard let value: Self = FirstRunRepairTransport.decode(json), value.schemaVersion == 1,
              value.receipts.count <= 100, value.receipts.allSatisfy(\.isValid),
              Set(value.receipts.map(\.receiptId)).count == value.receipts.count else { return nil }
        if let plan = value.plan {
            guard plan.isValid, let token = value.reviewToken,
                  (70...128).contains(token.utf8.count), token.hasPrefix(plan.fingerprint + ":"),
                  token.utf8.allSatisfy({ (48...57).contains($0) || (97...102).contains($0) || $0 == 58 }) else { return nil }
        } else if value.reviewToken != nil { return nil }
        return value
    }
}

struct FirstRunRepairReceipt: Decodable, Equatable, Identifiable {
    enum Verification: String, Decodable { case unverified, verified, failed }
    let receiptId: Int64
    let planFingerprint: String
    let actionId: String
    let previousPath: String
    let checkedPath: String
    let applied: Bool
    let verification: Verification
    let observedVersion: String?
    let reason: String?
    var id: Int64 { receiptId }

    var isValid: Bool {
        guard receiptId > 0, FirstRunRepairTransport.isFingerprint(planFingerprint),
              actionId == FirstRunRepairTransport.actionID, applied,
              FirstRunRepairTransport.isPath(previousPath), FirstRunRepairTransport.isPath(checkedPath),
              previousPath != checkedPath else { return false }
        switch verification {
        case .unverified:
            return observedVersion == nil && reason == nil
        case .verified:
            guard let version = observedVersion, !version.isEmpty, version.utf8.count <= 256 else { return false }
            return reason == nil && version.unicodeScalars.allSatisfy { !CharacterSet.controlCharacters.contains($0) }
        case .failed:
            guard let reason else { return false }
            return observedVersion == nil && ["version_check_failed", "evidence_changed", "executable_changed", "preference_or_policy_changed"].contains(reason)
        }
    }

    func matches(_ plan: FirstRunRepairReview.Plan) -> Bool {
        planFingerprint == plan.fingerprint && previousPath == plan.before.selectedExecutablePath
            && checkedPath == plan.executable.path
    }

    var titleKey: String {
        switch verification {
        case .verified: return "app.first_run.repair.verified"
        case .failed: return "app.first_run.repair.failed"
        case .unverified: return "app.first_run.repair.unverified"
        }
    }
}

struct FirstRunRepairApplyReply: Decodable {
    let schemaVersion: Int
    let receipt: FirstRunRepairReceipt
    static func decode(_ json: String?) -> Self? {
        guard let value: Self = FirstRunRepairTransport.decode(json),
              value.schemaVersion == 1, value.receipt.isValid else { return nil }
        return value
    }
}

private enum FirstRunRepairTransport {
    static let actionID = "manager.clear_selected_executable_override"
    static func decode<Value: Decodable>(_ json: String?) -> Value? {
        guard let data = json?.data(using: .utf8), data.count <= 2 * 1024 * 1024 else { return nil }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try? decoder.decode(Value.self, from: data)
    }
    static func isFingerprint(_ text: String) -> Bool {
        text.utf8.count == 64 && text.utf8.allSatisfy { (48...57).contains($0) || (97...102).contains($0) }
    }
    static func isPath(_ text: String) -> Bool {
        text.hasPrefix("/") && text.utf8.count <= 4096 && !text.utf8.contains(0)
    }
}
