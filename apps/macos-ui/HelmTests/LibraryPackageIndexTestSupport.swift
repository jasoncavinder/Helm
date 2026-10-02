import SwiftUI

enum L10n {
    enum Common {
        static let unknown = "unknown"
    }

    enum App {
        enum Inspector {
            static let installed = "app.inspector.installed"
            static let targetVersion = "app.inspector.target_version"
        }
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
    static let stateNeedsReview = Color.orange
    static let surfaceElevated = Color.white
    static let borderSubtle = Color.gray
    static let selectionFill = Color.blue
    static let selectionStroke = Color.blue
    static let stateHealthy = Color.green
    static let stateUpdatesReady = Color.blue
    static let actionSecondaryText = Color.secondary
}

extension String {
    var localized: String { self }
}
