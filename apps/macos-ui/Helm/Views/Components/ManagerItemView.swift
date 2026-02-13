import SwiftUI

struct ManagerItemView: View {
    let manager: ManagerInfo
    let packageCount: Int
    let isDetected: Bool

    private var indicatorColor: Color {
        if !manager.isImplemented {
            return .gray
        }
        return isDetected ? .green : .red
    }

    var body: some View {
        VStack(spacing: 4) {
            ZStack(alignment: .topTrailing) {
                RoundedRectangle(cornerRadius: 8)
                    .fill(manager.isImplemented
                          ? Color.accentColor.opacity(0.15)
                          : Color.gray.opacity(0.1))
                    .frame(width: 44, height: 44)
                    .overlay(
                        Text(manager.firstLetter)
                            .font(.title2)
                            .fontWeight(.bold)
                            .foregroundColor(manager.isImplemented ? .accentColor : .gray)
                    )

                Circle()
                    .fill(indicatorColor)
                    .frame(width: 8, height: 8)
                    .offset(x: 2, y: -2)
            }

            Text(manager.shortName)
                .font(.caption2)
                .foregroundColor(.primary)
                .lineLimit(1)

            if packageCount > 0 {
                Text("\(packageCount)")
                    .font(.caption2)
                    .foregroundColor(.secondary)
            }
        }
        .frame(width: 60)
        .opacity(manager.isImplemented ? 1.0 : 0.5)
    }
}
