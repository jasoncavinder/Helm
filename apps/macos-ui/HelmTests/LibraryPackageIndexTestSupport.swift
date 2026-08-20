import SwiftUI

enum L10n {
    enum Common {
        static let unknown = "unknown"
    }

    enum App {
        enum Packages {
            enum Filter {
                static let installed = "installed"
                static let upgradable = "upgradable"
                static let available = "available"
            }
        }
    }
}

enum HelmTheme {
    static let stateHealthy = Color.green
    static let stateUpdatesReady = Color.blue
    static let actionSecondaryText = Color.secondary
}

extension String {
    var localized: String { self }
}
