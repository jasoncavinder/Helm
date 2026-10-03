import Darwin
import Foundation
import Security

public enum HelperObservationFailure: String, Error {
    case invalidAccount, invalidRequirement, invalidSignature, invalidMetadata
    case unsafeRuntime, unsafeFilesystem, invalidPath, changedDuringObservation
}

/// A local snapshot of this running helper, not a caller assertion or update
/// authorization. It must be recollected at review/confirmation boundaries.
public struct NativeHelperEvidence: Encodable, Equatable {
    public let canonicalPath: String
    public let bundleIdentifier: String
    public let build: String
    public let teamIdentifier: String
    public let codeDirectoryHash: [UInt8]
    public let account: uid_t

    fileprivate init(snapshot: HelperSnapshot, account: uid_t) {
        canonicalPath = snapshot.path
        bundleIdentifier = snapshot.signature.identifier
        build = snapshot.signature.build
        teamIdentifier = snapshot.signature.team
        codeDirectoryHash = Array(snapshot.signature.hash)
        self.account = account
    }
}

/// No path, PID, requirement, channel or claimed trust decision is accepted
/// from a client. Native requirement validation includes notarization.
public struct NativeHelperObserver {
    private let capture: () throws -> HelperSnapshot
    private let accounts: () -> (real: uid_t, effective: uid_t)

    public init() {
        capture = Self.captureSelf
        accounts = { (getuid(), geteuid()) }
    }

    #if DEBUG
    init(testingCapture: @escaping () throws -> HelperSnapshot,
         accounts: @escaping () -> (real: uid_t, effective: uid_t) = { (getuid(), geteuid()) }) {
        capture = testingCapture
        self.accounts = accounts
    }
    #endif

    public func observeSelf() throws -> NativeHelperEvidence {
        let beforeAccount = accounts()
        guard beforeAccount.real != 0, beforeAccount.real == beforeAccount.effective else {
            throw HelperObservationFailure.invalidAccount
        }
        let before = try capture()
        let after = try capture()
        let afterAccount = accounts()
        guard beforeAccount.real == afterAccount.real, beforeAccount.effective == afterAccount.effective else {
            throw HelperObservationFailure.invalidAccount
        }
        guard before == after else { throw HelperObservationFailure.changedDuringObservation }
        return NativeHelperEvidence(snapshot: before, account: beforeAccount.effective)
    }

    private static func captureSelf() throws -> HelperSnapshot {
        var requirement: SecRequirement?
        guard SecRequirementCreateWithString(ExternalUpdaterPeerAuthentication.helperRequirement as CFString,
                                             [], &requirement) == errSecSuccess, let requirement else {
            throw HelperObservationFailure.invalidRequirement
        }
        var live: SecCode?
        guard SecCodeCopySelf([], &live) == errSecSuccess, let live,
              SecCodeCheckValidity(live, [], requirement) == errSecSuccess else {
            throw HelperObservationFailure.invalidSignature
        }
        // These Security APIs explicitly accept a dynamic SecCode through their
        // SecStaticCode parameter. Preserve that dynamic object (and its kernel
        // status); SecCodeCopyStaticCode would discard the live-code evidence.
        let liveReference = unsafeBitCast(live, to: SecStaticCode.self)
        let liveValues = try signingInformation(liveReference, dynamic: true)
        let signature = try HelperSignature(values: liveValues)
        guard let status = liveValues[kSecCodeInfoStatus as String] as? NSNumber,
              status.uint32Value & SecCodeSignatureFlags.runtime.rawValue != 0 else {
            throw HelperObservationFailure.unsafeRuntime
        }
        var codePath: CFURL?
        guard SecCodeCopyPath(liveReference, [], &codePath) == errSecSuccess, let codePath else {
            throw HelperObservationFailure.invalidPath
        }
        let url = codePath as URL
        guard url.isFileURL, url.pathExtension == "app", url.path.hasPrefix("/"),
              url.path == url.resolvingSymlinksInPath().path else {
            throw HelperObservationFailure.invalidPath
        }
        let before = try HelperFilesystemSnapshot.capture(url)
        // Obtain a fresh disk object: the dynamic -> static translation is not
        // itself a secure binding. Compare its sealed identity with the live one.
        var disk: SecStaticCode?
        guard SecStaticCodeCreateWithPath(url as CFURL, [], &disk) == errSecSuccess, let disk else {
            throw HelperObservationFailure.invalidSignature
        }
        let flags = SecCSFlags(rawValue: kSecCSStrictValidate | kSecCSCheckAllArchitectures | kSecCSCheckNestedCode)
        guard SecStaticCodeCheckValidity(disk, flags, requirement) == errSecSuccess else {
            throw HelperObservationFailure.invalidSignature
        }
        let diskSignature = try HelperSignature(values: signingInformation(disk, dynamic: false))
        guard signature == diskSignature, before == (try HelperFilesystemSnapshot.capture(url)),
              url.path == url.resolvingSymlinksInPath().path,
              SecCodeCheckValidity(live, [], requirement) == errSecSuccess else {
            throw HelperObservationFailure.changedDuringObservation
        }
        return HelperSnapshot(path: url.path, filesystem: before, signature: signature)
    }

    private static func signingInformation(_ code: SecStaticCode, dynamic: Bool) throws -> [String: Any] {
        var raw: CFDictionary?
        let flags = SecCSFlags(rawValue: kSecCSSigningInformation | (dynamic ? kSecCSDynamicInformation : 0))
        guard SecCodeCopySigningInformation(code, flags, &raw) == errSecSuccess,
              let values = raw as? [String: Any] else { throw HelperObservationFailure.invalidMetadata }
        return values
    }
}

struct HelperSignature: Equatable {
    let identifier: String
    let team: String
    let build: String
    let executable: String
    let hash: Data
    let flags: UInt32

    init(values: [String: Any]) throws {
        guard let identifier = values[kSecCodeInfoIdentifier as String] as? String,
              identifier == "com.jasoncavinder.Helm.SparkleExternalUpdater",
              let team = values[kSecCodeInfoTeamIdentifier as String] as? String, team == "V73WPJR9M4",
              let info = values[kSecCodeInfoPList as String] as? [String: Any],
              info["CFBundleIdentifier"] as? String == identifier,
              info["CFBundlePackageType"] as? String == "APPL",
              info["HelmDistributionChannel"] as? String == "developer_id",
              let build = info["CFBundleVersion"] as? String, Self.bounded(build, limit: 128),
              let executable = info["CFBundleExecutable"] as? String, Self.bounded(executable, limit: 255),
              !executable.contains("/"), executable != ".", executable != "..",
              let hash = values[kSecCodeInfoUnique as String] as? Data,
              [20, 32].contains(hash.count), hash.contains(where: { $0 != 0 }),
              let flags = values[kSecCodeInfoFlags as String] as? NSNumber else {
            throw HelperObservationFailure.invalidMetadata
        }
        guard flags.uint32Value & SecCodeSignatureFlags.runtime.rawValue != 0 else { throw HelperObservationFailure.unsafeRuntime }
        // This unprivileged helper requires no entitlements. Fail closed for new
        // grants until packaging and their security implications are reviewed.
        if let raw = values[kSecCodeInfoEntitlementsDict as String] {
            guard let entitlements = raw as? [String: Any], entitlements.isEmpty else {
                throw HelperObservationFailure.unsafeRuntime
            }
        } else if values[kSecCodeInfoEntitlements as String] != nil {
            throw HelperObservationFailure.unsafeRuntime
        }
        self.identifier = identifier
        self.team = team
        self.build = build
        self.executable = executable
        self.hash = hash
        self.flags = flags.uint32Value
    }

    private static func bounded(_ text: String, limit: Int) -> Bool {
        !text.isEmpty && text.utf8.count <= limit && text.trimmingCharacters(in: .whitespacesAndNewlines) == text
            && !text.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) })
    }
}

struct HelperSnapshot: Equatable {
    let path: String
    let filesystem: HelperFilesystemSnapshot
    let signature: HelperSignature
}

struct HelperFilesystemSnapshot: Equatable {
    let ancestors: [String: FileIdentity]
    let tree: BundleFilesystem.TreeSnapshot

    static func capture(_ url: URL, entryLimit: Int = 100_000) throws -> Self {
        guard url.isFileURL, url.pathExtension == "app", url.path.hasPrefix("/"),
              url.path == url.resolvingSymlinksInPath().path else {
            throw HelperObservationFailure.invalidPath
        }
        do {
            let filesystem = BundleFilesystem(entryLimit: entryLimit)
            let root = try FileIdentity.read(url)
            guard root.isDirectory else { throw HelperObservationFailure.invalidPath }
            let ancestors = try filesystem.ancestors(of: url)
            let tree = try filesystem.tree(at: url)
            guard !tree.unsafePermissions else { throw HelperObservationFailure.unsafeFilesystem }
            guard tree.entries[url.path] == root,
                  ancestors == (try filesystem.ancestors(of: url)) else {
                throw HelperObservationFailure.changedDuringObservation
            }
            return Self(ancestors: ancestors, tree: tree)
        } catch is ObservationFailure {
            throw HelperObservationFailure.unsafeFilesystem
        }
    }
}
