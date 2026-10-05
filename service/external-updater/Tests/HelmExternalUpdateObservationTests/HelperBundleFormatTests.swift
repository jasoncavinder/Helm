import Foundation
import XCTest
@testable import HelmExternalUpdateObservation

final class HelperBundleFormatTests: XCTestCase {
    private let serviceInfo: [String: Any] = ["CFBundlePackageType": "XPC!", "XPCService": ["ServiceType": "Application"]]

    func testAcceptsOnlyTheTwoSealedConfigurations() throws {
        XCTAssertEqual(try HelperBundleFormat(info: ["CFBundlePackageType": "APPL"]), .application)
        XCTAssertEqual(try HelperBundleFormat(info: serviceInfo), .privateService)
    }

    func testRejectsAdditionalOrMalformedServiceConfiguration() {
        for service: Any in [[:], "Application", ["ServiceType": "System"], ["ServiceType": true],
                             ["ServiceType": "Application", "RunLoopType": "dispatch_main"],
                             ["ServiceType": "Application", "JoinExistingSession": "true"]] {
            var info = serviceInfo
            info["XPCService"] = service
            XCTAssertThrowsError(try HelperBundleFormat(info: info))
        }
        for type in ["APPL", "BNDL", "XPC", ""] {
            var info = serviceInfo
            info["CFBundlePackageType"] = type
            XCTAssertThrowsError(try HelperBundleFormat(info: info))
        }
        var info = serviceInfo
        info["LSUIElement"] = false
        XCTAssertThrowsError(try HelperBundleFormat(info: info))
    }

    func testSealedTypeMustMatchPathAndPrivateServicePlacement() throws {
        let application = URL(fileURLWithPath: "/Applications/Helm.app")
        let service = application.appendingPathComponent("Contents/XPCServices/Updater.xpc")
        XCTAssertNoThrow(try HelperBundleFormat.application.validate(path: application))
        XCTAssertNoThrow(try HelperBundleFormat.privateService.validate(path: service))
        XCTAssertThrowsError(try HelperBundleFormat.application.validate(path: service))
        for path in ["/Applications/Updater.xpc", "/Applications/Helm.app/Contents/Helpers/Updater.xpc",
                     "/Applications/Contents/XPCServices/Updater.xpc", "/Applications/Helm.app"] {
            XCTAssertThrowsError(try HelperBundleFormat.privateService.validate(path: URL(fileURLWithPath: path)))
        }
    }
}
