import Cocoa

// MARK: - Status Badge

enum StatusBadge {
    case count(Int, NSColor)
    case symbol(String, NSColor)
    case dot(NSColor)
}

extension NSColor {
    static let helmUpdatesReady = NSColor(name: NSColor.Name("HelmUpdatesReady")) { appearance in
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            ? NSColor(red: 79.0 / 255.0, green: 199.0 / 255.0, blue: 192.0 / 255.0, alpha: 1)
            : NSColor(red: 20.0 / 255.0, green: 158.0 / 255.0, blue: 153.0 / 255.0, alpha: 1)
    }

    static let helmNeedsReview = NSColor(name: NSColor.Name("HelmNeedsReview")) { appearance in
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            ? NSColor(red: 240.0 / 255.0, green: 160.0 / 255.0, blue: 75.0 / 255.0, alpha: 1)
            : NSColor(red: 166.0 / 255.0, green: 83.0 / 255.0, blue: 0, alpha: 1)
    }
}

func drawBadge(_ badge: StatusBadge, in bounds: NSRect) {
    switch badge {
    case let .count(value, color):
        let text = "\(value)"
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 7, weight: .bold),
            .foregroundColor: NSColor.white
        ]
        let textSize = text.size(withAttributes: attributes)
        let width = max(10, textSize.width + 5)
        let badgeRect = NSRect(x: bounds.maxX - width - 1, y: bounds.maxY - 10, width: width, height: 10)
        NSBezierPath(roundedRect: badgeRect, xRadius: 5, yRadius: 5).addClip()
        color.setFill()
        badgeRect.fill()
        text.draw(
            at: NSPoint(
                x: badgeRect.minX + (badgeRect.width - textSize.width) / 2,
                y: badgeRect.minY + (badgeRect.height - textSize.height) / 2 - 0.2
            ),
            withAttributes: attributes
        )
    case let .symbol(symbol, color):
        let badgeRect = NSRect(x: bounds.maxX - 10, y: bounds.maxY - 10, width: 9, height: 9)
        color.setFill()
        NSBezierPath(ovalIn: badgeRect).fill()
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 7, weight: .bold),
            .foregroundColor: NSColor.white
        ]
        let textSize = symbol.size(withAttributes: attributes)
        symbol.draw(
            at: NSPoint(
                x: badgeRect.minX + (badgeRect.width - textSize.width) / 2,
                y: badgeRect.minY + (badgeRect.height - textSize.height) / 2 - 0.4
            ),
            withAttributes: attributes
        )
    case let .dot(color):
        let dotRect = NSRect(x: bounds.maxX - 7.5, y: bounds.maxY - 7.5, width: 5.5, height: 5.5)
        color.setFill()
        NSBezierPath(ovalIn: dotRect).fill()
    }
}
