import Darwin
import Foundation

/// Exclusion evidence from macOS's supported receipt query, never an installer.
/// The process/environment/arguments are fixed locally, not supplied over XPC.
struct NativeInstallerReceiptObserver {
    static let pathLimit = 4096
    static let batchSize = 32
    static let byteLimit = 4 * 1024 * 1024
    static let snapshotNanoseconds: UInt64 = 3_000_000_000
    var batchQuery: ([String], UInt64) throws -> Data = { paths, remaining in
        try BoundedSystemQuery.run(executable: "/usr/sbin/pkgutil",
                                   arguments: ["--volume", "/"] + paths.flatMap { ["--file-info-plist", $0] },
                                   timeoutNanoseconds: min(remaining, 1_000_000_000))
    }
    var clock: () -> UInt64 = { DispatchTime.now().uptimeNanoseconds }
    var catalog = NativeReceiptCatalog()
    var workers = 4

    struct Snapshot: Equatable {
        let replies: [Data]
        let identifiers: [String]
    }

    func snapshot(target: URL, paths: [String]) throws -> Snapshot {
        // Only the already inspected native bundle tree supplies this scope.
        // A truncated scope must not become apparently unclaimed evidence.
        guard !paths.isEmpty, paths.count <= Self.pathLimit else { throw ObservationFailure.limitExceeded }
        guard paths.contains(target.path), Set(paths).count == paths.count,
              paths.allSatisfy({ path in
                  (path == target.path || path.hasPrefix(target.path + "/")) && path.hasPrefix("/")
                      && path.utf8.count <= 4096 && !path.contains("//") && !path.hasSuffix("/")
                      && !path.split(separator: "/").contains(where: { $0 == "." || $0 == ".." })
                      && !path.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) })
              }) else {
            throw ObservationFailure.unreadableManagerEvidence
        }
        let paths = paths.sorted()
        let started = clock()
        func remaining() throws -> UInt64 {
            let now = clock()
            guard now >= started else { throw ObservationFailure.unreadableManagerEvidence }
            guard now - started < Self.snapshotNanoseconds else { throw ObservationFailure.limitExceeded }
            return Self.snapshotNanoseconds - (now - started)
        }
        let catalog = try catalog.snapshot(target: target, remaining: remaining)
        let batches = stride(from: 0, to: paths.count, by: Self.batchSize).map {
            Array(paths[$0..<min($0 + Self.batchSize, paths.count)])
        }
        let collector = ReceiptBatchCollector(count: batches.count)
        // At most four system children, not one worker per bundle entry. Join
        // all workers before returning; each child retains its own deadline.
        DispatchQueue.concurrentPerform(iterations: min(max(workers, 1), 4, batches.count)) { _ in
            while let index = collector.next() {
                do {
                    let reply = try batchQuery(batches[index], remaining())
                    let ids = try Self.batchIdentifiers(reply, paths: batches[index])
                    _ = try remaining()
                    try collector.record(index: index, reply: reply, identifiers: ids)
                } catch {
                    collector.reject(error)
                    return
                }
            }
        }
        let result = try collector.result()
        _ = try remaining()
        return Snapshot(replies: catalog.replies + result.replies,
                        identifiers: Set(catalog.identifiers + result.identifiers).sorted())
    }

    static func batchIdentifiers(_ data: Data, paths: [String]) throws -> [String] {
        guard !data.isEmpty, data.count <= BoundedSystemQuery.maximumBytes,
              !paths.isEmpty, paths.count <= batchSize else { throw ObservationFailure.unreadableManagerEvidence }
        return try zip(paths, documents(data, count: paths.count)).flatMap { path, data in
            try identifiers(data, path: path)
        }
    }

    static func documents(_ data: Data, count: Int) throws -> [Data] {
        // pkgutil emits one XML document per repeated --file-info-plist option,
        // not a single array. Require every document, in order, with no extras.
        let end = Data("</plist>".utf8)
        var remainder = data
        var documents: [Data] = []
        for _ in 0..<count {
            guard let boundary = remainder.range(of: end) else { throw ObservationFailure.unreadableManagerEvidence }
            let document = Data(remainder[..<boundary.upperBound])
            documents.append(document)
            remainder = Data(remainder[boundary.upperBound...])
        }
        guard remainder.allSatisfy({ [9, 10, 13, 32].contains($0) }) else { throw ObservationFailure.unreadableManagerEvidence }
        return documents
    }

    static func identifiers(_ data: Data, path: String) throws -> [String] {
        guard !data.isEmpty, data.count <= BoundedSystemQuery.maximumBytes,
              let value = try PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any],
              Set(value.keys) == ["path", "path-info"], value["path"] as? String == path,
              let entries = value["path-info"] as? [[String: Any]], entries.count <= 128 else {
            throw ObservationFailure.unreadableManagerEvidence
        }
        return try entries.map { entry in
            guard let identifier = entry["pkgid"] as? String, !identifier.isEmpty,
                  identifier.utf8.count <= 255,
                  identifier.trimmingCharacters(in: .whitespacesAndNewlines) == identifier,
                  !identifier.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }) else {
                throw ObservationFailure.unreadableManagerEvidence
            }
            return identifier
        }
    }
}

private final class ReceiptBatchCollector {
    private let lock = NSLock()
    private var index = 0
    private var bytes = 0
    private var replies: [Data?]
    private var identifiers = Set<String>()
    private var failure: Error?

    init(count: Int) { replies = Array(repeating: nil, count: count) }

    func next() -> Int? {
        lock.lock()
        defer { lock.unlock() }
        guard failure == nil, index < replies.count else { return nil }
        defer { index += 1 }
        return index
    }

    func record(index: Int, reply: Data, identifiers: [String]) throws {
        lock.lock()
        defer { lock.unlock() }
        guard reply.count <= NativeInstallerReceiptObserver.byteLimit - bytes else {
            throw ObservationFailure.limitExceeded
        }
        replies[index] = reply
        bytes += reply.count
        self.identifiers.formUnion(identifiers)
    }

    func reject(_ error: Error) {
        lock.lock()
        defer { lock.unlock() }
        if failure == nil { failure = error }
    }

    func result() throws -> NativeInstallerReceiptObserver.Snapshot {
        lock.lock()
        defer { lock.unlock() }
        if let failure { throw failure }
        guard replies.allSatisfy({ $0 != nil }) else { throw ObservationFailure.unreadableManagerEvidence }
        return .init(replies: replies.compactMap { $0 }, identifiers: identifiers.sorted())
    }
}

/// Internal synchronous system-tool transport. POSIX argv never passes through
/// a shell. Nonblocking bounded drains avoid pipe deadlock; only our unreaped
/// child may be killed during rejection cleanup (never a target application).
enum BoundedSystemQuery {
    static let maximumBytes = 64 * 1024

    static func run(executable: String, arguments: [String], timeoutNanoseconds: UInt64 = 1_000_000_000,
                    maximumBytes: Int = maximumBytes) throws -> Data {
        guard executable.hasPrefix("/"), maximumBytes > 0, timeoutNanoseconds > 0,
              !([executable] + arguments).contains(where: { $0.utf8.contains(0) }) else {
            throw ObservationFailure.unreadableManagerEvidence
        }
        let output = try QueryPipe()
        let error = try QueryPipe()
        var actions: posix_spawn_file_actions_t?
        var attributes: posix_spawnattr_t?
        guard posix_spawn_file_actions_init(&actions) == 0 else { throw ObservationFailure.unreadableManagerEvidence }
        defer { posix_spawn_file_actions_destroy(&actions) }
        guard posix_spawnattr_init(&attributes) == 0 else { throw ObservationFailure.unreadableManagerEvidence }
        defer { posix_spawnattr_destroy(&attributes) }
        var signals = sigset_t()
        sigemptyset(&signals)
        guard posix_spawn_file_actions_addopen(&actions, STDIN_FILENO, "/dev/null", O_RDONLY, 0) == 0,
              posix_spawn_file_actions_adddup2(&actions, output.writeFD, STDOUT_FILENO) == 0,
              posix_spawn_file_actions_adddup2(&actions, error.writeFD, STDERR_FILENO) == 0,
              posix_spawn_file_actions_addchdir_np(&actions, "/") == 0,
              posix_spawnattr_setsigmask(&attributes, &signals) == 0,
              posix_spawnattr_setflags(&attributes, Int16(POSIX_SPAWN_CLOEXEC_DEFAULT | POSIX_SPAWN_SETSIGMASK)) == 0 else {
            throw ObservationFailure.unreadableManagerEvidence
        }
        let argv = ([executable] + arguments).map { strdup($0) }
        let environmentValues: [String] = ["PATH=/usr/bin:/bin:/usr/sbin:/sbin", "HOME=/var/empty", "LANG=C", "LC_ALL=C"]
        let env = environmentValues.map { strdup($0) }
        defer { for pointer in argv + env { free(pointer) } }
        guard !argv.contains(where: { $0 == nil }), !env.contains(where: { $0 == nil }) else {
            throw ObservationFailure.unreadableManagerEvidence
        }
        var pid: pid_t = 0
        let started = DispatchTime.now().uptimeNanoseconds
        let spawned = (argv + [nil]).withUnsafeBufferPointer { args in
            (env + [nil]).withUnsafeBufferPointer { environment in
                posix_spawn(&pid, executable, &actions, &attributes, args.baseAddress!, environment.baseAddress!)
            }
        }
        guard spawned == 0 else { throw ObservationFailure.unreadableManagerEvidence }
        var reaped = false
        defer {
            if !reaped {
                // Retain ownership of the child until waitpid, avoiding PID reuse.
                kill(pid, SIGKILL)
                while waitpid(pid, nil, 0) < 0 && errno == EINTR {}
            }
        }
        output.closeWriter()
        error.closeWriter()
        var stdout = Data()
        var stderr = Data()
        var outputEOF = false
        var errorEOF = false
        var status: Int32 = 0
        while true {
            let now = DispatchTime.now().uptimeNanoseconds
            guard now >= started, now - started < timeoutNanoseconds else { throw ObservationFailure.unreadableManagerEvidence }
            try output.drain(into: &stdout, eof: &outputEOF, limit: maximumBytes - stderr.count)
            try error.drain(into: &stderr, eof: &errorEOF, limit: maximumBytes - stdout.count)
            if !reaped {
                let result = waitpid(pid, &status, WNOHANG)
                if result == pid { reaped = true }
                if result < 0, errno != EINTR {
                    // ECHILD means another reaper already consumed it; never signal
                    // a potentially reused PID or interpret missing status as success.
                    if errno == ECHILD { reaped = true }
                    throw ObservationFailure.unreadableManagerEvidence
                }
            }
            if reaped && outputEOF && errorEOF { break }
            usleep(5_000)
        }
        guard status == 0, stderr.isEmpty else { throw ObservationFailure.unreadableManagerEvidence }
        return stdout
    }
}

private final class QueryPipe {
    private let readFD: Int32
    private(set) var writeFD: Int32

    init() throws {
        var descriptors = [Int32](repeating: -1, count: 2)
        guard pipe(&descriptors) == 0 else { throw ObservationFailure.unreadableManagerEvidence }
        // Do not alias standard descriptors if a caller has closed one of them.
        guard descriptors.allSatisfy({ $0 > STDERR_FILENO }),
              fcntl(descriptors[0], F_SETFD, FD_CLOEXEC) == 0, fcntl(descriptors[1], F_SETFD, FD_CLOEXEC) == 0,
              fcntl(descriptors[0], F_SETFL, O_NONBLOCK) == 0 else {
            for descriptor in descriptors { close(descriptor) }
            throw ObservationFailure.unreadableManagerEvidence
        }
        readFD = descriptors[0]
        writeFD = descriptors[1]
    }

    deinit { close(readFD); if writeFD >= 0 { close(writeFD) } }
    func closeWriter() { close(writeFD); writeFD = -1 }

    func drain(into data: inout Data, eof: inout Bool, limit: Int) throws {
        guard !eof else { return }
        var buffer = [UInt8](repeating: 0, count: 4096)
        while true {
            let byteCount = read(readFD, &buffer, buffer.count)
            if byteCount == 0 { eof = true; return }
            if byteCount < 0 {
                if errno == EINTR { continue }
                if errno == EAGAIN { return }
                throw ObservationFailure.unreadableManagerEvidence
            }
            guard byteCount <= limit - data.count else { throw ObservationFailure.limitExceeded }
            data.append(contentsOf: buffer.prefix(byteCount))
        }
    }
}
