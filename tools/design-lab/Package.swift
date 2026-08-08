// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "HelmDesignLab",
    platforms: [.macOS(.v13)],
    products: [
        .executable(name: "HelmDesignLab", targets: ["HelmDesignLab"]),
    ],
    targets: [
        .executableTarget(name: "HelmDesignLab"),
    ]
)
