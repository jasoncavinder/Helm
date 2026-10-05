import Foundation

/// Sealed bundle configuration, not evidence of a process's sandbox state.
enum HelperBundleFormat: Equatable {
    case application, privateService

    init(info: [String: Any]) throws {
        switch info["CFBundlePackageType"] as? String {
        case "APPL" where info["XPCService"] == nil:
            self = .application
        case "XPC!":
            guard let service = info["XPCService"] as? [String: String],
                  service == ["ServiceType": "Application"], info["LSUIElement"] == nil else {
                throw HelperObservationFailure.invalidMetadata
            }
            self = .privateService
        default:
            throw HelperObservationFailure.invalidMetadata
        }
    }

    func validate(path url: URL) throws {
        guard url.isFileURL, url.path.hasPrefix("/"),
              url.path == url.resolvingSymlinksInPath().path else {
            throw HelperObservationFailure.invalidPath
        }
        switch self {
        case .application:
            guard url.pathExtension == "app" else { throw HelperObservationFailure.invalidPath }
        case .privateService:
            let services = url.deletingLastPathComponent()
            let contents = services.deletingLastPathComponent()
            guard url.pathExtension == "xpc", services.lastPathComponent == "XPCServices",
                  contents.lastPathComponent == "Contents", contents.deletingLastPathComponent().pathExtension == "app" else {
                throw HelperObservationFailure.invalidPath
            }
        }
    }
}
