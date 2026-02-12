import Foundation

@objc public protocol HelmServiceProtocol {
    func listInstalledPackages(withReply reply: @escaping (String?) -> Void)
    func listOutdatedPackages(withReply reply: @escaping (String?) -> Void)
    func listTasks(withReply reply: @escaping (String?) -> Void)
    func triggerRefresh(withReply reply: @escaping (Bool) -> Void)
}
