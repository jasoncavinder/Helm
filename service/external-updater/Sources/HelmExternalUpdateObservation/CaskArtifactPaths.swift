import Foundation

/// A deliberately finite grammar for installed receipt paths. Never expands
/// globs, resolves destination aliases, loads Ruby, or runs uninstall directives.
enum NativeCaskArtifactPaths {
    struct Inspection {
        var paths: [String] = []
        var incomplete = false
    }

    static func inspect(kind: String, value: Any, appDirectory: () throws -> URL) throws -> Inspection {
        switch kind {
        case "app", "suite", "artifact":
            return try moved(kind: kind, value: value, appDirectory: appDirectory)
        case "uninstall", "zap":
            return try removal(value)
        default:
            return Inspection(incomplete: true)
        }
    }

    static func overlaps(_ path: String, target: String) -> Bool {
        let key = NativeManagerObserver.denialKey(path)
        let targetKey = NativeManagerObserver.denialKey(target)
        return key == "/" || key == targetKey || key.hasPrefix(targetKey + "/") || targetKey.hasPrefix(key + "/")
    }

    private static func moved(kind: String, value: Any, appDirectory: () throws -> URL) throws -> Inspection {
        guard let arguments = value as? [Any], (1...2).contains(arguments.count),
              let source = arguments.first as? String else { throw ObservationFailure.unreadableManagerEvidence }
        try validateText(source)
        guard !source.hasSuffix("/"), let name = source.split(separator: "/").last,
              name != ".", name != ".." else { throw ObservationFailure.unreadableManagerEvidence }
        var destination = String(name)
        if arguments.count == 2 {
            guard let options = arguments[1] as? [String: Any], options.count == 1,
                  let override = options["target"] as? String else { throw ObservationFailure.unreadableManagerEvidence }
            if !override.isEmpty { destination = override }
            if kind == "artifact", override.isEmpty { return Inspection(incomplete: true) }
        } else if kind == "artifact" {
            throw ObservationFailure.unreadableManagerEvidence
        }
        try validateText(destination)
        guard !destination.hasPrefix("~") else { throw ObservationFailure.unreadableManagerEvidence }
        // An absolute generic target has no appdir semantics. All other generic
        // forms and expansion-dependent paths remain explicitly uninspected.
        guard literal(source), literal(destination), !source.hasPrefix("~"),
              kind != "artifact" || destination.hasPrefix("/") else { return Inspection(incomplete: true) }
        let base = try kind == "artifact" ? URL(fileURLWithPath: "/", isDirectory: true) : appDirectory()
        guard literal(base.path) else { return Inspection(incomplete: true) }
        let path = NativeManagerObserver.lexicalReferenceURL(destination, relativeTo: base).path
        try validateText(path)
        return Inspection(paths: [path])
    }

    private static func removal(_ value: Any) throws -> Inspection {
        guard let arguments = value as? [Any], arguments.count == 1,
              let directives = arguments[0] as? [String: Any] else { throw ObservationFailure.unreadableManagerEvidence }
        var result = Inspection(incomplete: directives.isEmpty)
        for name in directives.keys.sorted() {
            guard ["delete", "trash", "rmdir"].contains(name) else {
                result.incomplete = true
                continue
            }
            let paths: [String]
            if let path = directives[name] as? String {
                paths = [path]
            } else if let values = directives[name] as? [String] {
                paths = values
            } else {
                throw ObservationFailure.unreadableManagerEvidence
            }
            guard paths.count <= 512 else { throw ObservationFailure.limitExceeded }
            if paths.isEmpty { result.incomplete = true }
            for path in paths {
                try validateText(path)
                // Homebrew expands these as globs and skips relative/dot-segment
                // paths. Do not infer claims by executing those rules or probing.
                guard path.hasPrefix("/"), literal(path),
                      !path.split(separator: "/").contains(where: { $0 == "." || $0 == ".." }) else {
                    result.incomplete = true
                    continue
                }
                result.paths.append(NativeManagerObserver.lexicalReferenceURL(
                    path, relativeTo: URL(fileURLWithPath: "/", isDirectory: true)).path)
            }
        }
        guard result.paths.count <= 512 else { throw ObservationFailure.limitExceeded }
        return result
    }

    private static func literal(_ value: String) -> Bool {
        !value.contains(where: { "*?[]{}$\\".contains($0) })
    }

    private static func validateText(_ value: String) throws {
        guard !value.isEmpty, value.utf8.count <= 4096,
              !value.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }) else {
            throw ObservationFailure.unreadableManagerEvidence
        }
    }
}
