// swift-tools-version:5.9

import PackageDescription

let package = Package(
  name: "tauri-plugin-nocturne-calendar",
  platforms: [
    .macOS(.v14),
    .iOS(.v17),
  ],
  products: [
    .library(
      name: "tauri-plugin-nocturne-calendar",
      type: .static,
      targets: ["tauri-plugin-nocturne-calendar"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "tauri-plugin-nocturne-calendar",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources"),
    .testTarget(
      name: "CalendarRowsTests",
      dependencies: ["tauri-plugin-nocturne-calendar"],
      path: "Tests"),
  ]
)
