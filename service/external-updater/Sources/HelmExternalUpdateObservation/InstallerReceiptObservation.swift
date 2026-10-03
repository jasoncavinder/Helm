import Darwin
import Foundation

/// Exclusion evidence from macOS's supported receipt query, never an installer.
/// The process/environment/arguments are fixed locally, not supplied over XPC.
struct NativeInstallerReceiptObserver {
    var query: (String) throws -> Data = { path in
        try BoundedSystemQuery.run(executable: "/usr/sbin/pkgutil", arguments: ["--volume", "/", "--file-info-plist", path])
    }

    struct Snapshot: Equatable {
        let replies: [Data]
        let identifiers: [String]
    }

    func snapshot(target: URL, executable: String) throws -> Snapshot {
        guard !executable.isEmpty, executable.utf8.count <= 255,
              executable != ".", executable != "..", !executable.contains("/"),
              !executable.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }) else {
            throw ObservationFailure.invalidMetadata
        }
        let paths = [target.path, target.appendingPathComponent("Contents/Info.plist", isDirectory: false).path,
                     target.appendingPathComponent("Contents/MacOS", isDirectory: true)
                        .appendingPathComponent(executable, isDirectory: false).path]
        var replies: [Data] = []
        var identifiers = Set<String>()
        for path in paths {
            let reply = try query(path)
            identifiers.formUnion(try Self.identifiers(reply, path: path))
            replies.append(reply)
        }
        return Snapshot(replies: replies, identifiers: identifiers.sorted())
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
