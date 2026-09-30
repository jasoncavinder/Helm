// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "HelmExternalUpdateObservation",
    platforms: [.macOS(.v13)],
    products: [
        .library(name: "HelmExternalUpdateObservation", targets: ["HelmExternalUpdateObservation"]),
        .executable(name: "helm-external-observe", targets: ["ObservationProbe"])
    ],
    targets: [
        .target(name: "HelmExternalUpdateObservation"),
        .executableTarget(name: "ObservationProbe", dependencies: ["HelmExternalUpdateObservation"]),
        .testTarget(name: "HelmExternalUpdateObservationTests", dependencies: ["HelmExternalUpdateObservation"])
    ]
)
