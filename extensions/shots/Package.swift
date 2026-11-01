// swift-tools-version:5.9
// Shots (ADR-004): a screenshot extension for Samo. `ShotsCore` holds everything that can be tested without a
// screen; `SamoShots` is the helper Samo runs.
import PackageDescription

let infoPlist = Context.packageDirectory + "/Sources/SamoShots/Info.plist"

let package = Package(
    name: "SamoShots",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "samo-shots", targets: ["SamoShots"]),
    ],
    targets: [
        .target(name: "ShotsCore", path: "Sources/ShotsCore"),
        .executableTarget(
            name: "SamoShots",
            dependencies: ["ShotsCore"],
            path: "Sources/SamoShots",
            exclude: ["Info.plist"],
            // Its own Info.plist, so macOS treats the worker as its own app, "Shots", for Screen Recording.
            linkerSettings: [
                .unsafeFlags([
                    "-Xlinker", "-sectcreate", "-Xlinker", "__TEXT", "-Xlinker", "__info_plist",
                    "-Xlinker", infoPlist,
                ]),
            ]
        ),
        .testTarget(name: "SamoShotsTests", dependencies: ["ShotsCore"], path: "Tests/SamoShotsTests"),
    ],
    swiftLanguageVersions: [.v5]
)
