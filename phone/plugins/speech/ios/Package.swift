// swift-tools-version:5.9

import PackageDescription

let package = Package(
  name: "tauri-plugin-nocturne-speech",
  platforms: [
    .macOS(.v14),
    .iOS(.v17),
  ],
  products: [
    .library(
      name: "tauri-plugin-nocturne-speech",
      type: .static,
      targets: ["tauri-plugin-nocturne-speech"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "tauri-plugin-nocturne-speech",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources"),
    .testTarget(
      name: "SpeechSeamTests",
      dependencies: ["tauri-plugin-nocturne-speech"],
      path: "Tests"),
  ]
)
