import Foundation
@testable import HelmExternalUpdateObservation

enum ReceiptFixtures {
    static func reply(path: String, identifiers: [String] = []) throws -> Data {
        try PropertyListSerialization.data(fromPropertyList: ["path": path, "path-info": identifiers.map { ["pkgid": $0] }],
                                           format: .xml, options: 0)
    }
    static var empty: NativeInstallerReceiptObserver {
        NativeInstallerReceiptObserver(query: { try reply(path: $0) })
    }
}

// Existing filesystem fixtures intentionally avoid host receipt state. New
// receipt tests pass the observer explicitly, including real VM system queries.
extension NativeTargetObserver {
    init(roots: [URL], entryLimit: Int = 100_000, managers: NativeManagerObserver = NativeManagerObserver(),
         signer: @escaping (URL) throws -> NativeSigningEvidence) {
        self.init(roots: roots, entryLimit: entryLimit, managers: managers, receipts: ReceiptFixtures.empty, signer: signer)
    }
}
