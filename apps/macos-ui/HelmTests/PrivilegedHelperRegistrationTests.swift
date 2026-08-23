import XCTest
import ServiceManagement

final class PrivilegedHelperRegistrationTests: XCTestCase {
    func testAvailabilityRequiresDeveloperIdChannelAndBothArtifacts() {
        XCTAssertEqual(
            PrivilegedHelperRegistrationPolicy.availability(
                channel: .developerID,
                helperExecutableExists: true,
                launchDaemonPlistExists: true
            ),
            .available
        )
        XCTAssertEqual(
            PrivilegedHelperRegistrationPolicy.availability(
                channel: .developerID,
                helperExecutableExists: false,
                launchDaemonPlistExists: true
            ),
            .missingArtifacts
        )
        XCTAssertEqual(
            PrivilegedHelperRegistrationPolicy.availability(
                channel: .appStore,
                helperExecutableExists: true,
                launchDaemonPlistExists: true
            ),
            .unsupportedChannel
        )
    }

    func testUnsupportedChannelDoesNotPresentSettingsOrCallService() {
        let service = FakePrivilegedHelperRegistrationService(status: .notRegistered)
        let controller = PrivilegedHelperRegistrationController(
            availability: .unsupportedChannel,
            service: service
        )

        controller.register()
        controller.unregister()
        controller.openSystemSettingsLoginItems()

        XCTAssertFalse(controller.shouldPresentSettings)
        XCTAssertEqual(controller.status, .unavailable)
        XCTAssertEqual(service.registerCallCount, 0)
        XCTAssertEqual(service.unregisterCallCount, 0)
        XCTAssertEqual(service.openSystemSettingsCallCount, 0)
    }

    func testResearchFixtureBlockSuppressesStatusReadsAndOperations() {
        let service = FakePrivilegedHelperRegistrationService(status: .notRegistered)
        let controller = PrivilegedHelperRegistrationController(
            availability: .available,
            service: service,
            blocksLiveOperations: { true }
        )

        XCTAssertFalse(controller.shouldPresentSettings)
        XCTAssertEqual(controller.status, .unavailable)
        XCTAssertEqual(service.registrationStatusReadCount, 0)

        controller.refresh()
        controller.register()
        controller.unregister()
        controller.openSystemSettingsLoginItems()

        XCTAssertEqual(controller.status, .unavailable)
        XCTAssertEqual(service.registrationStatusReadCount, 0)
        XCTAssertEqual(service.registerCallCount, 0)
        XCTAssertEqual(service.unregisterCallCount, 0)
        XCTAssertEqual(service.openSystemSettingsCallCount, 0)
    }

    func testControllerPreservesEveryServiceStatus() {
        let statuses: [PrivilegedHelperRegistrationStatus] = [
            .notRegistered,
            .enabled,
            .requiresApproval,
            .notFound
        ]

        for status in statuses {
            let controller = makeController(
                service: FakePrivilegedHelperRegistrationService(status: status)
            )
            XCTAssertEqual(controller.status, status)
            controller.refresh()
            XCTAssertEqual(controller.status, status)
        }

        let missingArtifacts = PrivilegedHelperRegistrationController(
            availability: .missingArtifacts,
            service: nil
        )
        XCTAssertTrue(missingArtifacts.shouldPresentSettings)
        XCTAssertEqual(missingArtifacts.status, .unavailable)
    }

    func testRegisterRefreshesEnabledStatus() {
        let service = FakePrivilegedHelperRegistrationService(status: .notRegistered)
        service.statusAfterRegister = .enabled
        let controller = makeController(service: service)

        controller.register()

        XCTAssertEqual(controller.status, .enabled)
        XCTAssertEqual(service.registerCallCount, 1)
        XCTAssertNil(controller.operationError)
        XCTAssertFalse(controller.operationInProgress)
    }

    func testApprovalRequiredOpensSystemSettingsWithoutReportingGenericFailure() {
        let service = FakePrivilegedHelperRegistrationService(status: .notRegistered)
        service.statusAfterRegister = .requiresApproval
        service.registerError = TestError.expected
        let controller = makeController(service: service)

        controller.register()

        XCTAssertEqual(controller.status, .requiresApproval)
        XCTAssertEqual(service.openSystemSettingsCallCount, 1)
        XCTAssertNil(controller.operationError)
    }

    func testApprovalDeniedErrorUsesApprovalFlowBeforeStatusRefreshCatchesUp() {
        let service = FakePrivilegedHelperRegistrationService(status: .notRegistered)
        service.registerError = NSError(
            domain: "ServiceManagement",
            code: Int(kSMErrorLaunchDeniedByUser)
        )
        let controller = makeController(service: service)

        controller.register()

        XCTAssertEqual(controller.status, .requiresApproval)
        XCTAssertEqual(service.registerCallCount, 1)
        XCTAssertEqual(service.openSystemSettingsCallCount, 1)
        XCTAssertNil(controller.operationError)
    }

    func testRegisterFailurePreservesStatusAndReportsFailure() {
        let service = FakePrivilegedHelperRegistrationService(status: .notRegistered)
        service.registerError = TestError.expected
        let controller = makeController(service: service)

        controller.register()

        XCTAssertEqual(controller.status, .notRegistered)
        XCTAssertEqual(controller.operationError, .registrationFailed)
        XCTAssertEqual(service.openSystemSettingsCallCount, 0)
    }

    func testRegisterAlreadyCompletedIsTreatedAsEnabled() {
        let service = FakePrivilegedHelperRegistrationService(status: .notRegistered)
        service.statusAfterRegister = .enabled
        service.registerError = TestError.expected
        let controller = makeController(service: service)

        controller.register()

        XCTAssertEqual(controller.status, .enabled)
        XCTAssertNil(controller.operationError)
    }

    func testUnregisterRefreshesStatusAndReportsFailures() {
        let service = FakePrivilegedHelperRegistrationService(status: .enabled)
        service.statusAfterUnregister = .notRegistered
        let controller = makeController(service: service)

        controller.unregister()

        XCTAssertEqual(controller.status, .notRegistered)
        XCTAssertNil(controller.operationError)

        service.registrationStatus = .enabled
        service.statusAfterUnregister = nil
        service.unregisterError = TestError.expected
        controller.unregister()

        XCTAssertEqual(controller.status, .enabled)
        XCTAssertEqual(controller.operationError, .unregistrationFailed)
    }

    func testUnregisterAlreadyCompletedIsTreatedAsNotRegistered() {
        let service = FakePrivilegedHelperRegistrationService(status: .enabled)
        service.statusAfterUnregister = .notRegistered
        service.unregisterError = TestError.expected
        let controller = makeController(service: service)

        controller.unregister()

        XCTAssertEqual(controller.status, .notRegistered)
        XCTAssertNil(controller.operationError)
    }

    private func makeController(
        service: FakePrivilegedHelperRegistrationService
    ) -> PrivilegedHelperRegistrationController {
        PrivilegedHelperRegistrationController(
            availability: .available,
            service: service
        )
    }
}

private enum TestError: Error {
    case expected
}

private final class FakePrivilegedHelperRegistrationService: PrivilegedHelperRegistrationServicing {
    private var storedRegistrationStatus: PrivilegedHelperRegistrationStatus
    var statusAfterRegister: PrivilegedHelperRegistrationStatus?
    var statusAfterUnregister: PrivilegedHelperRegistrationStatus?
    var registerError: Error?
    var unregisterError: Error?
    var registrationStatusReadCount = 0
    var registerCallCount = 0
    var unregisterCallCount = 0
    var openSystemSettingsCallCount = 0

    var registrationStatus: PrivilegedHelperRegistrationStatus {
        get {
            registrationStatusReadCount += 1
            return storedRegistrationStatus
        }
        set {
            storedRegistrationStatus = newValue
        }
    }

    init(status: PrivilegedHelperRegistrationStatus) {
        storedRegistrationStatus = status
    }

    func register() throws {
        registerCallCount += 1
        if let statusAfterRegister {
            registrationStatus = statusAfterRegister
        }
        if let registerError {
            throw registerError
        }
    }

    func unregister() throws {
        unregisterCallCount += 1
        if let statusAfterUnregister {
            registrationStatus = statusAfterUnregister
        }
        if let unregisterError {
            throw unregisterError
        }
    }

    func openSystemSettingsLoginItems() {
        openSystemSettingsCallCount += 1
    }
}
