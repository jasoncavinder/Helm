import Foundation

enum PrivilegedHelperConstants {
    static let machServiceName = "com.jasoncavinder.Helm.PrivilegedHelper"
    static let helperIdentifier = "com.jasoncavinder.Helm.PrivilegedHelper"
    static let serviceIdentifier = "app.jasoncavinder.Helm.HelmService"
    static let teamIdentifier = "V73WPJR9M4"
}

@objc protocol HelmPrivilegedHelperProtocol {
    func execute(_ requestData: Data, withReply reply: @escaping (Data) -> Void)
}

struct PrivilegedHelperRequest: Codable, Equatable {
    let operation: String
    let program: String
    let arguments: [String]
}

struct PrivilegedHelperResponse: Codable, Equatable {
    let exitCode: Int32
    let standardOutput: Data
    let standardError: Data
    let error: String?

    static func failure(_ message: String, exitCode: Int32 = 1) -> Self {
        Self(
            exitCode: exitCode,
            standardOutput: Data(),
            standardError: Data(),
            error: message
        )
    }
}
