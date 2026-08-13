import Darwin
import Foundation

let arguments = Array(CommandLine.arguments.dropFirst())
if arguments == ["--daemon"] {
    guard geteuid() == 0 else {
        exit(77)
    }
    let delegate = PrivilegedHelperDelegate()
    let listener = NSXPCListener(machServiceName: PrivilegedHelperConstants.machServiceName)
    listener.delegate = delegate
    listener.resume()
    RunLoop.current.run()
} else {
    exit(PrivilegedHelperClient.run(arguments: arguments))
}
