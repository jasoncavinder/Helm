import CExternalUpdatePolicy
import Foundation

/// Untrusted intent only. This is not adoption consent or an update request.
public struct ExternalPreflightRequest: Codable {
    public let schemaVersion: UInt32
    public let requestId: String
    public let targetPath: String
    public let expectedBundleIdentifier: String
    public let expectedInstalledBuild: String

    public init(targetPath: String, bundleIdentifier: String, installedBuild: String) {
        schemaVersion = 1
        requestId = UUID().uuidString.lowercased()
        self.targetPath = targetPath
        expectedBundleIdentifier = bundleIdentifier
        expectedInstalledBuild = installedBuild
    }

    static func decode(_ data: Data, userApplications: String?) throws -> Self {
        let root = Data((userApplications ?? "").utf8)
        let result = data.withUnsafeBytes { bytes in
            root.withUnsafeBytes { rootBytes in
                helm_external_preflight_request(
                    HelmExternalBytes(data: bytes.bindMemory(to: UInt8.self).baseAddress, length: bytes.count),
                    HelmExternalBytes(data: rootBytes.bindMemory(to: UInt8.self).baseAddress, length: rootBytes.count)
                )
            }
        }
        guard result == UInt32(HELM_EXTERNAL_UNRESOLVED) else { throw BootstrapFailure.invalidMessage }
        // Rust has already rejected extra/duplicate keys, oversized input and
        // unsafe paths. Swift decoding does not grant or duplicate core policy.
        return try JSONDecoder().decode(Self.self, from: data)
    }
}

struct NativePreflightProcessor {
    let identity: NativeHelperEvidence
    var helper: () throws -> NativeHelperEvidence = { try NativeHelperObserver().observeSelf() }
    var target: (String) throws -> NativeTargetEvidence = { try NativeTargetObserver().observe(path: $0) }
    var userApplications: () -> String? = { NativeApplicationRoots.userApplications }

    func assess(_ data: Data) throws -> NativePolicyAssessment {
        let root = userApplications()
        let request = try ExternalPreflightRequest.decode(data, userApplications: root)
        guard try helper() == identity else { throw BootstrapFailure.invalidated }
        let result: NativePolicyAssessment
        do {
            result = NativePolicyAssessment.assess(try target(request.targetPath), userApplications: root, request: data)
        } catch {
            result = .observationFailed
        }
        guard try helper() == identity, userApplications() == root else { throw BootstrapFailure.invalidated }
        return result
    }
}
