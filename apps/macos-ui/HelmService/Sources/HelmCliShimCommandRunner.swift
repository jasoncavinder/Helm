import Foundation
import Darwin

struct HelmCliShimCommandResult {
    let terminationStatus: Int32?
    let stdout: Data
    let stderr: Data
    let didTimeout: Bool
    let launchError: String?
}

/// Runs the bundled CLI without blocking the XPC listener or allowing its output pipes to fill.
final class HelmCliShimCommandRunner {
    static let defaultTimeout: TimeInterval = 15

    private let executionQueue = DispatchQueue(label: "app.jasoncavinder.Helm.HelmService.cli-shim")

    func run(
        executableURL: URL,
        arguments: [String],
        timeout: TimeInterval = defaultTimeout,
        completion: @escaping (HelmCliShimCommandResult) -> Void
    ) {
        executionQueue.async {
            let process = Process()
            process.executableURL = executableURL
            process.arguments = arguments

            let stdoutPipe = Pipe()
            let stderrPipe = Pipe()
            process.standardOutput = stdoutPipe
            process.standardError = stderrPipe

            let stateLock = NSLock()
            var didTimeout = false
            var completed = false
            var stdout = Data()
            var stderr = Data()
            let pipeDrainGroup = DispatchGroup()
            pipeDrainGroup.enter()
            pipeDrainGroup.enter()

            func complete(_ result: HelmCliShimCommandResult) {
                stateLock.lock()
                guard !completed else {
                    stateLock.unlock()
                    return
                }
                completed = true
                stateLock.unlock()
                completion(result)
            }

            process.terminationHandler = { terminatedProcess in
                pipeDrainGroup.notify(queue: self.executionQueue) {
                    stateLock.lock()
                    let timedOut = didTimeout
                    stateLock.unlock()

                    stdoutPipe.fileHandleForReading.closeFile()
                    stderrPipe.fileHandleForReading.closeFile()
                    complete(HelmCliShimCommandResult(
                        terminationStatus: terminatedProcess.terminationStatus,
                        stdout: stdout,
                        stderr: stderr,
                        didTimeout: timedOut,
                        launchError: nil
                    ))
                }
            }

            do {
                try process.run()
            } catch {
                stdoutPipe.fileHandleForReading.closeFile()
                stderrPipe.fileHandleForReading.closeFile()
                complete(HelmCliShimCommandResult(
                    terminationStatus: nil,
                    stdout: Data(),
                    stderr: Data(),
                    didTimeout: false,
                    launchError: error.localizedDescription
                ))
                return
            }

            DispatchQueue.global(qos: .utility).async {
                stdout = stdoutPipe.fileHandleForReading.readDataToEndOfFile()
                pipeDrainGroup.leave()
            }
            DispatchQueue.global(qos: .utility).async {
                stderr = stderrPipe.fileHandleForReading.readDataToEndOfFile()
                pipeDrainGroup.leave()
            }

            self.executionQueue.asyncAfter(deadline: .now() + timeout) {
                stateLock.lock()
                guard !completed, process.isRunning else {
                    stateLock.unlock()
                    return
                }
                didTimeout = true
                stateLock.unlock()

                process.terminate()
                self.executionQueue.asyncAfter(deadline: .now() + 1) {
                    guard process.isRunning else { return }

                    // A process that ignores SIGTERM must not retain the XPC request indefinitely.
                    kill(process.processIdentifier, SIGKILL)
                    stdoutPipe.fileHandleForReading.closeFile()
                    stderrPipe.fileHandleForReading.closeFile()
                }
            }
        }
    }
}
