// swift-tools-version:5.9
// Clips (ADR-005): a clipboard history extension for Samo. `ClipsCore` holds everything that can be tested without a
// screen; `ClipsUI` the list the tests drive without showing it; `SamoClips` is the helper Samo runs.
import PackageDescription

let infoPlist = Context.packageDirectory + "/Sources/SamoClips/Info.plist"
let kit = Target.Dependency.product(name: "SamoKit", package: "kit")

let package = Package(
    name: "SamoClips",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "samo-clips", targets: ["SamoClips"]),
    ],
    dependencies: [
        .package(path: "../kit"),
    ],
    targets: [
        .target(name: "ClipsCore", dependencies: [kit], path: "Sources/ClipsCore"),
        .executableTarget(
            name: "SamoClips",
            dependencies: ["ClipsCore", kit],
            path: "Sources/SamoClips",
            exclude: ["Info.plist"],
            // Its own Info.plist, so macOS treats the worker as its own app, "Clips", for its permissions.
            linkerSettings: [
                .unsafeFlags([
                    "-Xlinker", "-sectcreate", "-Xlinker", "__TEXT", "-Xlinker", "__info_plist",
                    "-Xlinker", infoPlist,
                ]),
            ]
        ),
        .testTarget(name: "ClipsTests", dependencies: ["ClipsCore", kit], path: "Tests/ClipsTests"),
    ],
    swiftLanguageVersions: [.v5]
)
