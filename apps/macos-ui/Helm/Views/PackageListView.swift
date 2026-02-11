import SwiftUI

struct PackageListView: View {
    @State private var packages: [PackageItem] = []
    
    var body: some View {
        VStack(alignment: .leading) {
            Text("Installed Packages")
                .font(.headline)
                .padding(.horizontal)
            
            List(packages) { package in
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
