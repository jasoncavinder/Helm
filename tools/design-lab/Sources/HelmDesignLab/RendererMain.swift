import AppKit
import SwiftUI

@main
@MainActor
struct HelmDesignLab {
  static func main() throws {
    let outputDirectory = try outputDirectory(from: CommandLine.arguments)
    try FileManager.default.createDirectory(at: outputDirectory, withIntermediateDirectories: true)

    for direction in DesignDirection.allCases {
      for scheme in [ColorScheme.light, .dark] {
        let schemeName = scheme == .light ? "light" : "dark"
        let view = ControlCenterProposal(direction: direction)
          .environment(\.colorScheme, scheme)
        let renderer = ImageRenderer(content: view)
        renderer.scale = 1
        renderer.proposedSize = ProposedViewSize(width: 1348, height: 868)

        guard let image = renderer.nsImage,
          let tiff = image.tiffRepresentation,
          let bitmap = NSBitmapImageRep(data: tiff),
          let png = bitmap.representation(using: .png, properties: [:])
        else {
          throw RenderError.failed(direction.rawValue, schemeName)
        }

        let destination =
          outputDirectory
          .appendingPathComponent("\(direction.rawValue)-\(schemeName).png")
        try png.write(to: destination, options: .atomic)
        print("Rendered \(destination.path)")
      }
    }
  }

  private static func outputDirectory(from arguments: [String]) throws -> URL {
    guard let flagIndex = arguments.firstIndex(of: "--output"),
      arguments.indices.contains(flagIndex + 1)
    else {
      throw RenderError.usage
    }

    let path = arguments[flagIndex + 1]
    return URL(
      fileURLWithPath: path,
      relativeTo: URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
    ).standardizedFileURL
  }
}

private enum RenderError: LocalizedError {
  case usage
  case failed(String, String)

  var errorDescription: String? {
    switch self {
    case .usage:
      "Usage: HelmDesignLab --output <directory>"
    case .failed(let direction, let scheme):
      "Could not render \(direction) in \(scheme) appearance"
    }
  }
}
