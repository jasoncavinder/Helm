import Darwin
import Foundation
import os.log

private let helperLogger = Logger(
    subsystem: PrivilegedHelperConstants.helperIdentifier,
    category: "service"
)

final class PrivilegedHelperService: NSObject, HelmPrivilegedHelperProtocol {
    private static let maximumCapturedBytes = 4 * 1024 * 1024
    private static let stateLock = NSLock()
    private static var activeProcess: Process?
    private static var activeOwner: UUID?
    private let owner = UUID()

    func execute(_ requestData: Data, withReply reply: @escaping (Data) -> Void) {
        guard geteuid() == 0 else {
            reply(encoded(.failure("The privileged helper is not running as root.")))
            return
        }

        let request: PrivilegedHelperRequest
        do {
            request = try JSONDecoder().decode(PrivilegedHelperRequest.self, from: requestData)
        } catch {
            reply(encoded(.failure("The privileged helper request could not be decoded.")))
            return
        }

        let command: ValidatedPrivilegedCommand
        do {
            command = try PrivilegedOperationPolicy.validate(request)
        } catch {
            helperLogger.error("Rejected privileged operation: \(error.localizedDescription, privacy: .public)")
            reply(encoded(.failure(error.localizedDescription)))
            return
        }

        let process = Process()
        let standardOutput = Pipe()
        let standardError = Pipe()
        process.executableURL = command.executableURL
        process.arguments = command.arguments
        process.environment = [
            "LANG": "C",
            "LC_ALL": "C",
            "PATH": "/usr/bin:/bin:/usr/sbin:/sbin"
        ]
        process.standardOutput = standardOutput
        process.standardError = standardError

        Self.stateLock.lock()
        guard Self.activeProcess == nil else {
            Self.stateLock.unlock()
            reply(encoded(.failure("The privileged helper is already running an operation.")))
            return
        }
        Self.activeProcess = process
        Self.activeOwner = owner
        Self.stateLock.unlock()

        let outputGroup = DispatchGroup()
        let outputCapture = CapturedOutput()
        let errorCapture = CapturedOutput()
        outputGroup.enter()
        DispatchQueue.global(qos: .utility).async {
            outputCapture.store(Self.drain(standardOutput.fileHandleForReading))
            outputGroup.leave()
        }
        outputGroup.enter()
        DispatchQueue.global(qos: .utility).async {
            errorCapture.store(Self.drain(standardError.fileHandleForReading))
            outputGroup.leave()
        }

        let operationOwner = owner
        process.terminationHandler = { terminatedProcess in
            outputGroup.notify(queue: .global(qos: .utility)) {
                Self.clearActiveProcess(terminatedProcess, owner: operationOwner)
                let response = PrivilegedHelperResponse(
                    exitCode: terminatedProcess.terminationStatus,
                    standardOutput: outputCapture.snapshot(),
                    standardError: errorCapture.snapshot(),
                    error: nil
                )
                reply(Self.encoded(response))
            }
        }

        do {
            try process.run()
        } catch {
            standardOutput.fileHandleForWriting.closeFile()
            standardError.fileHandleForWriting.closeFile()
            Self.clearActiveProcess(process, owner: owner)
            reply(encoded(.failure("The privileged operation could not be started.")))
            return
        }
    }

    func cancel() {
        Self.stateLock.lock()
        let process = Self.activeOwner == owner ? Self.activeProcess : nil
        Self.stateLock.unlock()
        guard let process, process.isRunning else { return }
        process.terminate()
        let processIdentifier = process.processIdentifier
        DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + 2) {
            if process.isRunning {
                kill(processIdentifier, SIGKILL)
            }
        }
    }

    private static func clearActiveProcess(_ process: Process, owner: UUID) {
        Self.stateLock.lock()
        if activeOwner == owner, activeProcess === process {
            Self.activeProcess = nil
            Self.activeOwner = nil
        }
        Self.stateLock.unlock()
    }

    private static func drain(_ handle: FileHandle) -> Data {
        var captured = Data()
        while true {
            let chunk = handle.readData(ofLength: 64 * 1024)
            if chunk.isEmpty { break }
            if captured.count < maximumCapturedBytes {
                captured.append(chunk.prefix(maximumCapturedBytes - captured.count))
            }
        }
        return captured
    }

    private static func encoded(_ response: PrivilegedHelperResponse) -> Data {
        (try? JSONEncoder().encode(response)) ?? Data()
    }

    private func encoded(_ response: PrivilegedHelperResponse) -> Data {
        Self.encoded(response)
    }
}

private final class CapturedOutput {
    private let lock = NSLock()
    private var data = Data()

    func store(_ newData: Data) {
        lock.lock()
        data = newData
        lock.unlock()
    }

    func snapshot() -> Data {
        lock.lock()
        defer { lock.unlock() }
        return data
    }
}
