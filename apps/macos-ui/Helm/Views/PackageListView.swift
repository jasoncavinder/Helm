import SwiftUI

struct PackageListView: View {
    @StateObject var core = HelmCore.shared
    
    var body: some View {
        VStack(alignment: .leading) {
            HStack {
                Text("Installed Packages")
                    .font(.headline)
                Spacer()
                Button(action: { core.fetchPackages() }) {
                    Image(systemName: "arrow.clockwise")
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal)
            
            if core.installedPackages.isEmpty {
                Text("No packages found.")
                    .foregroundColor(.secondary)
                    .padding()
            } else {
                List(core.installedPackages) { package in
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
