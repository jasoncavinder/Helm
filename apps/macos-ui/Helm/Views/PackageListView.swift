import SwiftUI

struct PackageListView: View {
    @StateObject var core = HelmCore.shared
    
    var body: some View {
        VStack(alignment: .leading) {
            if !core.isConnected {
                HStack {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundColor(.yellow)
                    Text("Reconnecting to service...")
                        .font(.caption)
                    Spacer()
                }
                .padding(8)
                .background(Color.yellow.opacity(0.2))
            }
            
            HStack {
                Text("Packages")
                    .font(.headline)
                Spacer()
            }
            .padding(.horizontal)
            
            if core.installedPackages.isEmpty && core.outdatedPackages.isEmpty {
                Text("No packages found.")
                    .foregroundColor(.secondary)
                    .padding()
            } else {
                List {
                    if !core.outdatedPackages.isEmpty {
                        Section(header: Text("Updates Available")) {
                            ForEach(core.outdatedPackages) { package in
                                HStack {
                                    VStack(alignment: .leading) {
                                        Text(package.name)
                                            .font(.body)
                                        Text(package.manager)
                                            .font(.caption)
                                            .foregroundColor(.secondary)
                                    }
                                    Spacer()
                                    VStack(alignment: .trailing) {
                                        if let latest = package.latestVersion {
                                            Text(latest)
                                                .font(.monospacedDigit(.body)())
                                                .foregroundColor(.blue)
                                            Text(package.version)
                                                .font(.caption)
                                                .foregroundColor(.secondary)
                                                .strikethrough()
                                        } else {
                                            Text(package.version)
                                                .font(.monospacedDigit(.body)())
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if !core.installedPackages.isEmpty {
                        Section(header: Text("Installed")) {
                            ForEach(core.installedPackages) { package in
                                HStack {
                                    VStack(alignment: .leading) {
                                        Text(package.name)
                                            .font(.body)
                                        Text(package.manager)
                                            .font(.caption)
                                            .foregroundColor(.secondary)
                                    }
                                    Spacer()
                                    Text(package.version)
                                        .font(.monospacedDigit(.body)())
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
