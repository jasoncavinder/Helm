import AppKit
import Foundation

private let mainAppBundleIdentifier = "com.jasoncavinder.Helm"

private func parentAppURL() -> URL {
    var url = Bundle.main.bundleURL
    // Helm.app/Contents/Library/LoginItems/HelmLoginHelper.app
    //                 ^ remove 4 components to reach Helm.app
    for _ in 0..<4 {
        url.deleteLastPathComponent()
    }
    return url
}

let appIsRunning = NSWorkspace.shared.runningApplications.contains {
    $0.bundleIdentifier == mainAppBundleIdentifier
}

if !appIsRunning {
    let configuration = NSWorkspace.OpenConfiguration()
    configuration.activates = false
    NSWorkspace.shared.openApplication(
        at: parentAppURL(),
        configuration: configuration,
        completionHandler: nil
    )
}
