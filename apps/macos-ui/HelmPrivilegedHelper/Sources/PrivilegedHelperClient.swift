import Darwin
import Foundation

enum PrivilegedHelperClient {
    private static let usageExitCode: Int32 = 64
    private static let unavailableExitCode: Int32 = 69

    static func run(arguments: [String]) -> Int32 {
        guard CodeSigningValidator.process(
            getppid(),
            satisfiesIdentifier: PrivilegedHelperConstants.serviceIdentifier
        ) else {
            writeError("Helm privileged execution rejected an untrusted parent process.\n")
            return unavailableExitCode
        }

        guard let request = parse(arguments: arguments) else {
            writeError("Invalid Helm privileged helper invocation.\n")
            return usageExitCode
        }
        guard let requestData = try? JSONEncoder().encode(request) else {
            writeError("Helm could not encode the privileged helper request.\n")
            return unavailableExitCode
        }

        let connection = NSXPCConnection(
            machServiceName: PrivilegedHelperConstants.machServiceName,
            options: .privileged
        )
        connection.remoteObjectInterface = NSXPCInterface(with: HelmPrivilegedHelperProtocol.self)
        let semaphore = DispatchSemaphore(value: 0)
        let state = PrivilegedHelperClientState()
        connection.interruptionHandler = {
            if state.complete(with: nil, failed: true) {
                semaphore.signal()
            }
        }
        connection.invalidationHandler = {
            if state.complete(with: nil, failed: true) {
                semaphore.signal()
            }
        }
        connection.resume()

        guard let remote = connection.remoteObjectProxyWithErrorHandler({ _ in
            if state.complete(with: nil, failed: true) {
                semaphore.signal()
            }
        }) as? HelmPrivilegedHelperProtocol else {
            connection.invalidate()
            writeError("Helm's privileged helper is unavailable.\n")
            return unavailableExitCode
        }

        remote.execute(requestData) { data in
            if state.complete(with: data, failed: false) {
                semaphore.signal()
            }
        }
        semaphore.wait()
        let result = state.snapshot()
        connection.interruptionHandler = nil
        connection.invalidationHandler = nil
        connection.invalidate()

        guard !result.failed,
              let responseData = result.data,
              let response = try? JSONDecoder().decode(
                  PrivilegedHelperResponse.self,
                  from: responseData
              ) else {
            writeError("Helm's privileged helper did not return a valid response.\n")
            return unavailableExitCode
        }
        if !response.standardOutput.isEmpty {
            FileHandle.standardOutput.write(response.standardOutput)
        }
        if !response.standardError.isEmpty {
            FileHandle.standardError.write(response.standardError)
        }
        if let error = response.error {
            writeError("\(error)\n")
        }
        return response.exitCode
    }

    static func parse(arguments: [String]) -> PrivilegedHelperRequest? {
        guard arguments.count >= 5,
              arguments[0] == "--operation",
              arguments[2] == "--program",
              let separator = arguments.firstIndex(of: "--"),
              separator == 4 else {
            return nil
        }
        let operation = arguments[1]
        let program = arguments[3]
        guard !operation.isEmpty, !program.isEmpty else { return nil }
        return PrivilegedHelperRequest(
            operation: operation,
            program: program,
            arguments: Array(arguments.dropFirst(separator + 1))
        )
    }

    private static func writeError(_ message: String) {
        if let data = message.data(using: .utf8) {
            FileHandle.standardError.write(data)
        }
    }
}

private final class PrivilegedHelperClientState {
    private let lock = NSLock()
    private var completed = false
    private var responseData: Data?
    private var connectionFailed = false

    func complete(with data: Data?, failed: Bool) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        guard !completed else { return false }
        completed = true
        responseData = data
        connectionFailed = failed
        return true
    }

    func snapshot() -> (data: Data?, failed: Bool) {
        lock.lock()
        defer { lock.unlock() }
        return (responseData, connectionFailed)
    }
}
