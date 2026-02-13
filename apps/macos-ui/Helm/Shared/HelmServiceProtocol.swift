import Foundation

@objc public protocol HelmServiceProtocol {
    func listInstalledPackages(withReply reply: @escaping (String?) -> Void)
    func listOutdatedPackages(withReply reply: @escaping (String?) -> Void)
    func listTasks(withReply reply: @escaping (String?) -> Void)
    func triggerRefresh(withReply reply: @escaping (Bool) -> Void)
    func searchLocal(query: String, withReply reply: @escaping (String?) -> Void)
    func triggerRemoteSearch(query: String, withReply reply: @escaping (Int64) -> Void)
    func cancelTask(taskId: Int64, withReply reply: @escaping (Bool) -> Void)
    func listManagerStatus(withReply reply: @escaping (String?) -> Void)
    func setManagerEnabled(managerId: String, enabled: Bool, withReply reply: @escaping (Bool) -> Void)
    func installManager(managerId: String, withReply reply: @escaping (Int64) -> Void)
    func uninstallManager(managerId: String, withReply reply: @escaping (Int64) -> Void)
    func resetDatabase(withReply reply: @escaping (Bool) -> Void)
}
