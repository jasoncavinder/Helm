import SwiftUI

struct PackageRowView: View {
    let package: PackageItem

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: package.status.iconName)
                .foregroundColor(package.status.iconColor)
                .font(.body)
                .frame(width: 20)

            VStack(alignment: .leading, spacing: 2) {
                Text(package.name)
                    .font(.body)
                    .lineLimit(1)

                Text(package.manager)
                    .font(.caption2)
                    .padding(.horizontal, 4)
                    .padding(.vertical, 1)
                    .background(
                        RoundedRectangle(cornerRadius: 3)
                            .fill(Color.gray.opacity(0.15))
                    )
                    .foregroundColor(.secondary)
            }

            Spacer()

            VStack(alignment: .trailing, spacing: 2) {
                if let latest = package.latestVersion {
                    HStack(spacing: 4) {
                        Text(latest)
                            .font(.system(.caption, design: .monospaced))
                            .foregroundColor(.orange)
                            if package.restartRequired {
                                Image(systemName: "arrow.triangle.2.circlepath")
                                    .font(.caption2)
                                    .foregroundColor(.orange)
                                    .help(L10n.App.Packages.Label.restartRequired.localized)
                            }
                        }
                        Text(package.version)
                            .font(.system(.caption2, design: .monospaced))
                        .foregroundColor(.secondary)
                        .strikethrough()
                } else {
                    Text(package.version)
                        .font(.system(.caption, design: .monospaced))
                }
            }
        }
        .padding(.vertical, 4)
        .padding(.horizontal, 8)
    }
}
