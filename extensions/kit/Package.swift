// swift-tools-version:5.9
// SamoKit (ADR-005): what Samo's Swift extensions share. The protocol and its channel; the worker started as its own
// app, so macOS gives permissions to the extension alone; Samo's theme, fonts and notes; global shortcuts; and the
// way to System Settings.
import PackageDescription

let package = Package(
    name: "SamoKit",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "SamoKit", targets: ["SamoKit"]),
    ],
    targets: [
        .target(name: "SamoKit", path: "Sources/SamoKit"),
        .testTarget(name: "SamoKitTests", dependencies: ["SamoKit"], path: "Tests/SamoKitTests"),
    ],
    swiftLanguageVersions: [.v5]
)
