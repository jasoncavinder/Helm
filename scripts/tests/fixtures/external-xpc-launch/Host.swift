import Darwin
import Foundation

final class ProbeHost {
    private let queue = DispatchQueue(label: "helm.qa.xpc-launch-probe")
    private let child = Process()
    private var connection: NSXPCConnection?
    private var finished = false
    private let host = readSentinel(host: true)
    private var inherited: ReadResult?

    func run() {
        queue.asyncAfter(deadline: .now() + .seconds(20)) { self.finish(service: nil, failure: "deadline") }
        queue.async { self.startChild() }
    }

    private func startChild() {
        if Bundle.main.object(forInfoDictionaryKey: "HelmProbeSkipChild") as? Bool == true {
            connect()
            return
        }
        let output = Pipe()
        child.executableURL = Bundle.main.bundleURL.appendingPathComponent("Contents/XPCServices/Probe.xpc/Contents/MacOS/Probe")
        child.arguments = ["--child"]
        child.standardInput = FileHandle.nullDevice
        child.standardOutput = output
        child.standardError = FileHandle.nullDevice
        child.terminationHandler = { process in
            self.queue.async {
                guard !self.finished else { return }
                do {
                    guard process.terminationStatus == 0 else { throw CocoaError(.executableRuntimeMismatch) }
                    let data = try output.fileHandleForReading.read(upToCount: 4096) ?? Data()
                    self.inherited = try JSONDecoder().decode(ReadResult.self, from: data)
                    self.connect()
                } catch { self.finish(service: nil, failure: "childFailed") }
            }
        }
        do { try child.run() } catch { finish(service: nil, failure: "childLaunchFailed") }
    }

    private func connect() {
        let connection = NSXPCConnection(serviceName: "com.jasoncavinder.Helm.XPCLaunchProbe.Service")
        self.connection = connection
        // Identifier-only gates are deliberate for these ad-hoc VM fixtures.
        // This is not Developer ID/notarization or product peer acceptance.
        connection.setCodeSigningRequirement("identifier \"com.jasoncavinder.Helm.XPCLaunchProbe.Service\"")
        connection.remoteObjectInterface = NSXPCInterface(with: LaunchProbeProtocol.self)
        connection.interruptionHandler = { self.queue.async { self.finish(service: nil, failure: "interrupted") } }
        connection.invalidationHandler = { self.queue.async { self.finish(service: nil, failure: "invalidated") } }
        connection.activate()
        let proxy = connection.remoteObjectProxyWithErrorHandler { _ in
            self.queue.async { self.finish(service: nil, failure: "proxyFailed") }
        }
        guard let proxy = proxy as? LaunchProbeProtocol else { finish(service: nil, failure: "noProxy"); return }
        proxy.inspect { data in
            self.queue.async {
                guard data.count <= 4096, let result = try? JSONDecoder().decode(ReadResult.self, from: data) else {
                    self.finish(service: nil, failure: "invalidReply"); return
                }
                self.finish(service: result, failure: nil)
            }
        }
    }

    private func finish(service: ReadResult?, failure: String?) {
        guard !finished else { return }
        finished = true
        connection?.invalidate()
        if child.isRunning { child.terminate() }
        struct Report: Encodable {
            let host: ReadResult
            let inherited: ReadResult?
            let service: ReadResult?
            let failure: String?
        }
        do {
            guard let run = Bundle.main.object(forInfoDictionaryKey: "HelmProbeRun") as? String,
                  UUID(uuidString: run) != nil else { exit(1) }
            let directory = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask,
                                                         appropriateFor: nil, create: true)
            try JSONEncoder().encode(Report(host: host, inherited: inherited, service: service, failure: failure))
                .write(to: directory.appendingPathComponent("\(run).json"), options: .withoutOverwriting)
        } catch { exit(1) }
        exit(failure == nil ? 0 : 1)
    }
}

@main struct HostMain {
    static func main() {
        let host = ProbeHost()
        host.run()
        withExtendedLifetime(host) { RunLoop.main.run() }
    }
}
