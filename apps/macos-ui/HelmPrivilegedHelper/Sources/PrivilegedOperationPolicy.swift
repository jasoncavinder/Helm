import Foundation

struct ValidatedPrivilegedCommand: Equatable {
    let executableURL: URL
    let arguments: [String]
}

enum PrivilegedOperationPolicyError: LocalizedError, Equatable {
    case unsupportedOperation(String)
    case invalidProgram(operation: String, program: String)
    case invalidArguments(operation: String)

    var errorDescription: String? {
        switch self {
        case .unsupportedOperation(let operation):
            return "Privileged operation '\(operation)' is not supported by this helper."
        case .invalidProgram(let operation, let program):
            return "Privileged operation '\(operation)' rejected program '\(program)'."
        case .invalidArguments(let operation):
            return "Privileged operation '\(operation)' rejected its arguments."
        }
    }
}

enum PrivilegedOperationPolicy {
    private static let softwareUpdateProgram = "/usr/sbin/softwareupdate"
    private static let xcodeLabelPrefix = "Command Line Tools for Xcode-"
    private static let maximumXcodeLabelLength = 256

    static func validate(_ request: PrivilegedHelperRequest) throws -> ValidatedPrivilegedCommand {
        switch request.operation {
        case "software_update.install_all":
            return try fixedCommand(
                request,
                program: softwareUpdateProgram,
                arguments: ["-i", "-a"]
            )
        case "rosetta.install":
            return try fixedCommand(
                request,
                program: softwareUpdateProgram,
                arguments: ["--install-rosetta", "--agree-to-license"]
            )
        case "xcode_command_line_tools.update":
            guard request.program == softwareUpdateProgram else {
                throw PrivilegedOperationPolicyError.invalidProgram(
                    operation: request.operation,
                    program: request.program
                )
            }
            guard request.arguments.count == 2,
                  request.arguments[0] == "-i",
                  isValidXcodeLabel(request.arguments[1]) else {
                throw PrivilegedOperationPolicyError.invalidArguments(operation: request.operation)
            }
            return ValidatedPrivilegedCommand(
                executableURL: URL(fileURLWithPath: softwareUpdateProgram),
                arguments: request.arguments
            )
        default:
            throw PrivilegedOperationPolicyError.unsupportedOperation(request.operation)
        }
    }

    private static func fixedCommand(
        _ request: PrivilegedHelperRequest,
        program: String,
        arguments: [String]
    ) throws -> ValidatedPrivilegedCommand {
        guard request.program == program else {
            throw PrivilegedOperationPolicyError.invalidProgram(
                operation: request.operation,
                program: request.program
            )
        }
        guard request.arguments == arguments else {
            throw PrivilegedOperationPolicyError.invalidArguments(operation: request.operation)
        }
        return ValidatedPrivilegedCommand(
            executableURL: URL(fileURLWithPath: program),
            arguments: arguments
        )
    }

    private static func isValidXcodeLabel(_ label: String) -> Bool {
        guard label.hasPrefix(xcodeLabelPrefix),
              label.count <= maximumXcodeLabelLength else {
            return false
        }
        let suffix = label.dropFirst(xcodeLabelPrefix.count)
        guard !suffix.isEmpty else { return false }
        return suffix.unicodeScalars.allSatisfy { scalar in
            CharacterSet.alphanumerics.contains(scalar)
                || CharacterSet(charactersIn: " ._+-()").contains(scalar)
        }
    }
}
