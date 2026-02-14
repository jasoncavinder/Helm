import SwiftUI

struct PackageRowView: View {
    let package: PackageItem
    var onTogglePin: (() -> Void)? = nil

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: package.status.iconName)
                .foregroundColor(package.status.iconColor)
                .font(.body)
                .frame(width: 20)

            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 4) {
                    Text(package.name)
                        .font(.body)
                        .lineLimit(1)
                    if package.pinned {
                        Image(systemName: "pin.fill")
                            .font(.caption2)
                            .foregroundColor(.secondary)
                            .help("Pinned")
                    }
                }

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
                if let onTogglePin {
                    Button(action: onTogglePin) {
                        Image(systemName: package.pinned ? "pin.slash.fill" : "pin")
                            .font(.caption)
                            .foregroundColor(package.pinned ? .orange : .secondary)
                    }
                    .buttonStyle(.borderless)
                    .help(package.pinned ? "Unpin package" : "Pin package")
                }

                if let latest = package.latestVersion {
                    HStack(spacing: 4) {
                        Text(latest)
                            .font(.system(.caption, design: .monospaced))
                            .foregroundColor(.orange)
                        if package.restartRequired {
                            Image(systemName: "arrow.triangle.2.circlepath")
                                .font(.caption2)
                                .foregroundColor(.orange)
                                .help("Restart required")
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
