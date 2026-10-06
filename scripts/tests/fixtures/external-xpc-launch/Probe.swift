import CryptoKit
import Darwin
import Foundation

// QA-only launch experiment. Never linked into Helm or its updater helper.
// The only resource is a newly created harness sentinel beside the host bundle.
@objc protocol LaunchProbeProtocol {
    func inspect(reply: @escaping (Data) -> Void)
}

struct ReadResult: Codable {
    let digest: String?
    let error: Int32
}

func readSentinel(host: Bool) -> ReadResult {
    var root = Bundle.main.bundleURL
    if host {
        root.deleteLastPathComponent()
    } else {
        for _ in 0..<4 { root.deleteLastPathComponent() }
    }
    let path = root.appendingPathComponent("sentinel", isDirectory: false).path
    let descriptor = open(path, O_RDONLY | O_CLOEXEC | O_NOFOLLOW | O_NONBLOCK)
    guard descriptor >= 0 else { return ReadResult(digest: nil, error: errno) }
    defer { close(descriptor) }
    var metadata = stat()
    guard fstat(descriptor, &metadata) == 0, metadata.st_mode & S_IFMT == S_IFREG,
          metadata.st_size == 32 else { return ReadResult(digest: nil, error: EINVAL) }
    var bytes = [UInt8](repeating: 0, count: 33)
    guard read(descriptor, &bytes, bytes.count) == 32 else { return ReadResult(digest: nil, error: EIO) }
    return ReadResult(digest: SHA256.hash(data: Data(bytes.prefix(32))).map { String(format: "%02x", $0) }.joined(), error: 0)
}
