import Darwin
import Foundation
import Security
import XCTest
@testable import HelmExternalUpdateObservation

#if DEBUG
final class PrivateServiceBoundaryTests: XCTestCase {
    private func identity(build: String = "1", path: String? = nil) throws -> NativeHelperEvidence {
        let identifier = ExternalUpdaterPeerAuthentication.bundledServiceName
        let signature = try HelperSignature(values: [
            kSecCodeInfoIdentifier as String: identifier, kSecCodeInfoTeamIdentifier as String: "V73WPJR9M4",
            kSecCodeInfoUnique as String: Data(repeating: 7, count: 20),
            kSecCodeInfoFlags as String: NSNumber(value: SecCodeSignatureFlags.runtime.rawValue),
            kSecCodeInfoPList as String: ["CFBundleIdentifier": identifier, "CFBundleVersion": build,
                "CFBundlePackageType": "XPC!", "XPCService": ["ServiceType": "Application"],
                "CFBundleExecutable": "Updater", "HelmDistributionChannel": "developer_id"]])
        let snapshot = HelperSnapshot(path: path ?? "/Applications/Helm.app/Contents/XPCServices/Updater.xpc",
            filesystem: HelperFilesystemSnapshot(ancestors: [:], tree: .init(entries: [:], unsafePermissions: false)),
            signature: signature)
        return try NativeHelperObserver(testingCapture: { snapshot }).observeSelf()
    }

    func testInactiveAndClosedContextCannotProduceFacts() throws {
        let identity = try identity()
        var reads = 0
        let boundary = NativePrivateServiceBoundary(testingIdentity: identity, helper: { reads += 1; return identity })
        XCTAssertThrowsError(try boundary.withObservation { _ in XCTFail("pre-hello facts") })
        XCTAssertEqual(reads, 0)
        try boundary.establish()
        boundary.close()
        XCTAssertFalse(boundary.deliveredMessage())
        XCTAssertThrowsError(try boundary.establish())
        XCTAssertThrowsError(try boundary.withObservation { _ in XCTFail("closed facts") })
        XCTAssertEqual(reads, 0)
    }

    func testFreshHelperAndFixedCallerFactsAreScopedToEachObservation() throws {
        let identity = try identity()
        var reads = 0
        let boundary = NativePrivateServiceBoundary(testingIdentity: identity, helper: { reads += 1; return identity })
        try boundary.establish()
        XCTAssertThrowsError(try boundary.establish())
        for _ in 0..<2 {
            try boundary.withObservation { facts in
                XCTAssertEqual(facts.helperIdentifier, identity.bundleIdentifier)
                XCTAssertEqual(facts.helperTeamIdentifier, identity.teamIdentifier)
                XCTAssertEqual(facts.helperCodeDirectoryHash, Data(identity.codeDirectoryHash))
                XCTAssertEqual(facts.callerIdentifier, "com.jasoncavinder.Helm")
                XCTAssertEqual(facts.callerTeamIdentifier, "V73WPJR9M4")
                XCTAssertTrue(facts.authenticatedLiveCaller && facts.developerIDSignatureValid && facts.notarizationAccepted)
                XCTAssertTrue(facts.helmSandboxPreserved && facts.externalHelperUnsandboxed && facts.directConsumerChannel)
            }
        }
        XCTAssertEqual(reads, 4)
    }

    func testHelperDriftAndCollectionFailureBeforeOrAfterWorkFailClosed() throws {
        let identity = try identity()
        let changed = try self.identity(build: "2")
        for failingRead in [1, 2] {
            for throwsFailure in [false, true] {
                var reads = 0
                var worked = false
                let boundary = NativePrivateServiceBoundary(testingIdentity: identity, helper: {
                    reads += 1
                    if reads == failingRead {
                        if throwsFailure { throw HelperObservationFailure.invalidSignature }
                        return changed
                    }
                    return identity
                })
                try boundary.establish()
                XCTAssertThrowsError(try boundary.withObservation { _ in worked = true })
                XCTAssertEqual(worked, failingRead == 2)
            }
        }
    }

    func testApplicationOrMisplacedServiceCannotBecomePrivateLaunchFacts() throws {
        for path in ["/Applications/Updater.app", "/Applications/Updater.xpc",
                     "/Applications/Helm.app/Contents/Helpers/Updater.xpc"] {
            let identity = try identity(path: path)
            let boundary = NativePrivateServiceBoundary(testingIdentity: identity, helper: { identity })
            try boundary.establish()
            XCTAssertThrowsError(try boundary.withObservation { _ in XCTFail("non-private context") })
        }
    }

    func testHandshakeAndSessionExpiryDoNotDependOnTimerDelivery() throws {
        let identity = try identity()
        for elapsed in [BootstrapWire.handshakeNanoseconds, UInt64.max - 10] {
            var now: UInt64 = 10
            let boundary = NativePrivateServiceBoundary(testingIdentity: identity, helper: { identity }, clock: { now })
            now += elapsed
            XCTAssertThrowsError(try boundary.establish())
        }
        for invalidNow in [UInt64(9), 10 + BootstrapWire.lifetimeNanoseconds, UInt64.max] {
            var now: UInt64 = 10
            let boundary = NativePrivateServiceBoundary(testingIdentity: identity, helper: { identity }, clock: { now })
            try boundary.establish()
            now = invalidNow
            XCTAssertThrowsError(try boundary.withObservation { _ in XCTFail("expired facts") })
        }
    }

    func testLossExpiryAndAccountDriftDuringWorkSuppressResult() throws {
        let identity = try identity()
        for change in 0..<5 {
            var now: UInt64 = 10
            var real = getuid()
            var effective = geteuid()
            var peer = geteuid()
            let boundary = NativePrivateServiceBoundary(testingIdentity: identity, peerAccount: { peer },
                accounts: { (real, effective) }, helper: { identity }, clock: { now })
            try boundary.establish()
            XCTAssertThrowsError(try boundary.withObservation { _ in
                switch change {
                case 0: boundary.close()
                case 1: now += BootstrapWire.lifetimeNanoseconds
                case 2: real = 0
                case 3: effective += 1
                default: peer += 1
                }
            })
        }
    }

    func testForeignAndRootAccountsCannotEstablishOrDeliver() throws {
        let identity = try identity()
        for account in [(uid_t(0), uid_t(0)), (getuid(), uid_t(0)), (getuid() + 1, geteuid() + 1)] {
            let boundary = NativePrivateServiceBoundary(testingIdentity: identity, accounts: { account }, helper: { identity })
            XCTAssertFalse(boundary.deliveredMessage())
            XCTAssertThrowsError(try boundary.establish())
        }
    }

    func testLossDuringHelperCollectionPreventsWork() throws {
        let identity = try identity()
        var boundary: NativePrivateServiceBoundary?
        boundary = NativePrivateServiceBoundary(testingIdentity: identity, helper: {
            boundary?.close()
            return identity
        })
        let context = try XCTUnwrap(boundary)
        defer { boundary = nil }
        try context.establish()
        XCTAssertThrowsError(try context.withObservation { _ in XCTFail("loss during collection admitted work") })
    }

    func testServerRechecksDeliveryAfterHelloBeforeObservation() throws {
        let identity = try identity()
        let lock = NSLock()
        var delivered = true
        let delegate = BootstrapDelegate(assess: { _ in XCTFail("forged request reached target"); return .unresolved },
            boundary: { connection in
                NativePrivateServiceBoundary(testingIdentity: identity, currentMessage: {
                    lock.lock(); defer { lock.unlock() }; return delivered && NSXPCConnection.current() === connection
                }, helper: { XCTFail("forged request reached helper"); return identity })
            })
        let listener = NSXPCListener.anonymous()
        listener.delegate = delegate
        listener.activate()
        let hello = expectation(description: "real hello")
        let client = try ExternalUpdaterBootstrapClient(testingConnection: NSXPCConnection(listenerEndpoint: listener.endpoint)) {
            if case .ready = $0 { hello.fulfill() }
        }
        defer { client.cancel(); delegate.cancel(); listener.invalidate() }
        client.begin()
        wait(for: [hello], timeout: 3)
        lock.lock(); delivered = false; lock.unlock()
        let rejected = expectation(description: "subsequent invocation lacks delivery")
        let request = ExternalPreflightRequest(targetPath: "/Applications/Example.app", bundleIdentifier: "org.example.App", installedBuild: "1")
        client.preflight(request) { result in
            if case .success = result { XCTFail("request without delivery accepted") }
            rejected.fulfill()
        }
        wait(for: [rejected], timeout: 3)
    }

    func testServerRejectsHelloWithoutCurrentConnectionDelivery() throws {
        let identity = try identity()
        let boundary = NativePrivateServiceBoundary(testingIdentity: identity,
            currentMessage: { NSXPCConnection.current() != nil }, helper: { identity })
        let connection = NSXPCConnection(machServiceName: "unused.test.service")
        let server = ExternalUpdaterBootstrapServer(testingConnection: connection, boundary: boundary, event: { _ in })
        defer { server.cancel() }
        let rejected = expectation(description: "direct invocation has no native delivery")
        server.hello(version: 1, challenge: Data(repeating: 1, count: 32)) { version, _, token in
            XCTAssertEqual(version, 0)
            XCTAssertNil(token)
            rejected.fulfill()
        }
        wait(for: [rejected], timeout: 3)
    }

    func testRealDeliveryWrapsWorkerAndLossDiscardsItsResult() throws {
        for cancel in [false, true] {
            let identity = try identity()
            let ready = expectation(description: "hello")
            let entered = expectation(description: "native worker")
            let replied = expectation(description: "bounded result")
            let release = DispatchSemaphore(value: 0)
            let delegate = BootstrapDelegate(assess: { _ in
                entered.fulfill()
                if cancel { _ = release.wait(timeout: .now() + 5) }
                return .unresolved
            }, boundary: { connection in
                NativePrivateServiceBoundary(testingIdentity: identity,
                    currentMessage: { NSXPCConnection.current() === connection }, helper: { identity })
            })
            let listener = NSXPCListener.anonymous()
            listener.delegate = delegate
            listener.activate()
            let client = try ExternalUpdaterBootstrapClient(testingConnection: NSXPCConnection(listenerEndpoint: listener.endpoint)) {
                if case .ready = $0 { ready.fulfill() }
            }
            defer { release.signal(); client.cancel(); delegate.cancel(); listener.invalidate() }
            client.begin()
            wait(for: [ready], timeout: 3)
            let request = ExternalPreflightRequest(targetPath: "/Applications/Example.app", bundleIdentifier: "org.example.App", installedBuild: "1")
            client.preflight(request) { result in
                if cancel {
                    if case .success = result { XCTFail("closed boundary published") }
                } else {
                    XCTAssertEqual(try? result.get(), .unresolved)
                }
                replied.fulfill()
            }
            wait(for: [entered], timeout: 3)
            if cancel { delegate.cancel(); release.signal() }
            wait(for: [replied], timeout: 3)
        }
    }
}
#endif
