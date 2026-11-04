import Foundation

/// A cask binary links to its source and may change that source's permissions.
/// Both endpoints are denial evidence, never authority to follow or mutate them.
enum NativeCaskBinaryPaths {
    static func inspect(_ value: Any, caskroomPath: String?) throws -> NativeCaskArtifactPaths.Inspection {
        guard let arguments = value as? [Any], (1...2).contains(arguments.count),
              let source = arguments.first as? String else { throw ObservationFailure.unreadableManagerEvidence }
        try validate(source)
        let basename = source.split(separator: "/", omittingEmptySubsequences: false).last.map(String.init) ?? ""
        guard !basename.isEmpty, basename != ".", basename != ".." else {
            throw ObservationFailure.unreadableManagerEvidence
        }
        var destination = basename
        if arguments.count == 2 {
            guard let options = arguments[1] as? [String: Any], options.count == 1,
                  let override = options["target"] as? String else { throw ObservationFailure.unreadableManagerEvidence }
            if !override.isEmpty { destination = override }
        }
        try validate(destination)
        var result = NativeCaskArtifactPaths.Inspection()
        if source.hasPrefix("/"), literal(source) {
            result.paths.append(try normalized(source))
        } else {
            // Relative sources depend on staged_path/only_path, which this
            // receipt-only observer cannot establish. Do not guess from a token.
            result.incomplete = true
        }
        if !literal(destination) {
            result.incomplete = true
        } else if destination.hasPrefix("/") {
            result.paths.append(try normalized(destination))
        } else if let directory = binaryDirectory(caskroomPath: caskroomPath) {
            result.paths.append(try normalized(destination, relativeTo: directory))
        } else {
            result.incomplete = true
        }
        return result
    }

    private static func binaryDirectory(caskroomPath: String?) -> String? {
        // Derive this only from the native scanner's fixed standard root, not
        // receipt architecture, environment, config, or current process CPU.
        switch caskroomPath {
        case "/opt/homebrew/Caskroom": return "/opt/homebrew/bin"
        case "/usr/local/Caskroom": return "/usr/local/bin"
        default: return nil
        }
    }

    private static func normalized(_ path: String, relativeTo base: String = "/") throws -> String {
        let result = NativeManagerObserver.lexicalReferenceURL(path,
            relativeTo: URL(fileURLWithPath: base, isDirectory: true)).path
        try validate(result)
        return result
    }

    private static func literal(_ path: String) -> Bool {
        !path.hasPrefix("~") && (path == "/" || !path.hasSuffix("/")) &&
            path.rangeOfCharacter(from: CharacterSet(charactersIn: "*?[]{}$\\")) == nil &&
            !path.split(separator: "/").contains(where: { $0 == "." || $0 == ".." })
    }

    private static func validate(_ value: String) throws {
        guard !value.isEmpty, value.utf8.count <= 4096,
              !value.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }) else {
            throw ObservationFailure.unreadableManagerEvidence
        }
    }
}
