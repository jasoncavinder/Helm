import Cocoa
import SwiftUI

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var statusItem: NSStatusItem?
    private var popover: NSPopover?

    func applicationDidFinishLaunching(_ notification: Notification) {
        let popover = NSPopover()
        popover.contentSize = NSSize(width: 300, height: 200)
        popover.behavior = .transient
        popover.contentViewController = NSHostingController(rootView: StatusBarView())
        self.popover = popover

        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        if let button = statusItem?.button {
            button.image = NSImage(named: "MenuBarIcon")
            button.action = #selector(togglePopover(_:))
            button.target = self
        }
    }

    @objc private func togglePopover(_ sender: AnyObject?) {
        guard let button = statusItem?.button else { return }
        if let popover = popover, popover.isShown {
            popover.performClose(sender)
        } else {
            popover?.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
        }
    }
}

private struct StatusBarView: View {
    @StateObject var core = HelmCore.shared
    
    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Helm")
                    .font(.headline)
                Spacer()
                Button(action: {
                    core.triggerRefresh()
                }) {
                    Image(systemName: "arrow.clockwise")
                }
                .buttonStyle(.plain)
            }
            .padding()
            .background(Color(nsColor: .windowBackgroundColor))
            
            Divider()
            
            if core.isInitialized {
                TaskListView()
                    .frame(height: 150)
                Divider()
                PackageListView()
                    .frame(height: 300)
            } else {
                Text("Initializing Core...")
                    .padding()
            }
            
            Divider()
            
            HStack {
                Text("v\(helmVersion)")
                    .font(.caption)
                    .foregroundColor(.secondary)
                Spacer()
                Button("Quit") {
                    NSApplication.shared.terminate(nil)
                }
                .buttonStyle(.plain)
            }
            .padding()
        }
        .frame(width: 350)
    }
}
