import CryptoKit
import Foundation

/// Equality binding for already bounded, revalidated native observations. This
/// is neither a clearance result nor a token accepted from a client or ledger.
struct NativeOwnershipFingerprint: Equatable {
    private let digest: Data

    init(target: URL, applicationRoots: [URL], caskrooms: [URL], macportsRegistry: URL,
         bundleEntries: [String: FileIdentity], homebrew: NativeManagerObserver.Snapshot,
         installer: NativeInstallerReceiptObserver.Snapshot, macports: NativeMacPortsObserver.Snapshot) throws {
        let input = Evidence(target: target.path, applicationRoots: applicationRoots.map(\.path),
            caskrooms: caskrooms.map(\.path), macportsRegistry: macportsRegistry.path,
            bundleEntries: bundleEntries, homebrew: homebrew, installer: installer, macports: macports)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        digest = Data(SHA256.hash(data: try encoder.encode(input)))
    }

    private struct Evidence: Encodable {
        let domain = "helm.external-updater.native-ownership-evidence.v1"
        let ownershipScopeVersion = 1
        let target: String
        let applicationRoots: [String]
        let caskrooms: [String]
        let macportsRegistry: String
        let bundleEntries: [String: FileIdentity]
        let homebrew: NativeManagerObserver.Snapshot
        let installer: NativeInstallerReceiptObserver.Snapshot
        let macports: NativeMacPortsObserver.Snapshot
    }
}
