import Foundation
import Tauri
import UIKit
import UniformTypeIdentifiers

struct FolderArgs: Decodable {
  var path: String
}

struct SaveArgs: Decodable {
  var path: String
  var name: String
}

struct SecretArgs: Decodable {
  var name: String
  var value: String?
}

struct BackgroundArgs: Decodable {
  var mind: Bool
  var backup: Bool
}

struct PlaceArgs: Decodable {
  var name: String?
  var path: String?
  var prefix: String?
}

/// Room left over after a copy, so the phone is not filled to the last byte.
private let spare: Int64 = 16 * 1024 * 1024

/// The name a chosen file is copied to inside the folder the app names.
private let chosenName = "chosen"

/// One sheet at a time: the plugin holds it, and what to do when it closes.
private final class Sheet: NSObject, UIDocumentPickerDelegate, UIAdaptivePresentationControllerDelegate {
  let picked: ([URL]) -> Void
  let cancelled: () -> Void
  var done = false

  init(picked: @escaping ([URL]) -> Void, cancelled: @escaping () -> Void) {
    self.picked = picked
    self.cancelled = cancelled
  }

  func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
    guard !done else { return }
    done = true
    picked(urls)
  }

  func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
    cancel()
  }

  func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
    cancel()
  }

  private func cancel() {
    guard !done else { return }
    done = true
    cancelled()
  }
}

/// The iPhone side of the system plugin: a folder kept out of the backup, the free space, and the
/// phone's own sheets to save a file and to choose one. Every other command answers its stand-in.
class SystemPlugin: Plugin {
  private var sheet: Sheet?

  /// The backup's calls to iCloud, one at a time and never on the main thread.
  private let placeQueue = DispatchQueue(label: "app.nocturne.place")

  /// One call to iCloud on the backup's own queue; its outcome is always an answer.
  private func place(_ invoke: Invoke, _ work: @escaping () throws -> [String: Any]) {
    placeQueue.async {
      do {
        var out = try work()
        out["outcome"] = "ok"
        invoke.resolve(out)
      } catch {
        let outcome = ICloudPlace.outcome(error)
        invoke.resolve(["outcome": outcome.rawValue, "message": error.localizedDescription])
      }
    }
  }

  /// Whether the app's iCloud space can be reached. There is no permission screen on an iPhone:
  /// the backup goes to the iCloud it is signed in to.
  @objc public func placeConnect(_ invoke: Invoke) throws {
    placeQueue.async {
      invoke.resolve(["outcome": ICloudPlace.state()])
    }
  }

  @objc public func placePut(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(PlaceArgs.self)
    guard let name = args.name, ICloudPlace.validName(name), let path = args.path else {
      return invoke.reject("invalid")
    }
    place(invoke) {
      try ICloudPlace.put(name, from: URL(fileURLWithPath: path))
      return [:]
    }
  }

  @objc public func placeGet(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(PlaceArgs.self)
    guard let name = args.name, ICloudPlace.validName(name), let path = args.path else {
      return invoke.reject("invalid")
    }
    place(invoke) {
      let found = try ICloudPlace.get(name, to: URL(fileURLWithPath: path))
      return ["found": found]
    }
  }

  @objc public func placeList(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(PlaceArgs.self)
    let prefix = args.prefix ?? ""
    guard ICloudPlace.validPrefix(prefix) else { return invoke.reject("invalid") }
    place(invoke) {
      let entries = try ICloudPlace.list(prefix)
      return ["entries": entries]
    }
  }

  @objc public func placeDelete(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(PlaceArgs.self)
    guard let name = args.name, ICloudPlace.validName(name) else { return invoke.reject("invalid") }
    place(invoke) {
      try ICloudPlace.delete(name)
      return [:]
    }
  }

  /// Keeps a secret in the keychain, in place of one of the same name.
  @objc public func secretKeep(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(SecretArgs.self)
    guard Safe.validName(args.name), let value = args.value else { return invoke.reject("invalid") }
    Safe.keep(args.name, value) ? invoke.resolve() : invoke.reject("failed")
  }

  /// A kept secret, or null.
  @objc public func secretRead(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(SecretArgs.self)
    guard Safe.validName(args.name) else { return invoke.reject("invalid") }
    let value: Any = Safe.read(args.name) ?? NSNull()
    invoke.resolve(["value": value])
  }

  @objc public func secretForget(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(SecretArgs.self)
    guard Safe.validName(args.name) else { return invoke.reject("invalid") }
    Safe.forget(args.name) ? invoke.resolve() : invoke.reject("failed")
  }

  /// Whether the phone charges, and whether a background window is open.
  @objc public func power(_ invoke: Invoke) throws {
    DispatchQueue.main.async {
      let charging: Any = Power.charging().map { NSNumber(value: $0) } ?? NSNull()
      let unmetered: Any = NetworkCost.shared.unmetered.map { NSNumber(value: $0) } ?? NSNull()
      invoke.resolve([
        "charging": charging, "window": Background.shared.isOpen, "unmetered": unmetered,
      ])
    }
  }

  /// Background windows while the mind or the backup is on; none while both are off. iOS has no
  /// window that waits for Wi-Fi: a night that would wait for it doesn't run in the window.
  @objc public func background(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(BackgroundArgs.self)
    Background.shared.schedule(args.mind || args.backup)
    invoke.resolve()
  }

  /// The core has nothing left to do in this background window.
  @objc public func backgroundDone(_ invoke: Invoke) throws {
    Background.shared.done()
    invoke.resolve()
  }

  /// Marks a folder of the app's own so iCloud and computer backups leave it out. The mark is
  /// on the folder: what is written in it later stays out too.
  @objc public func excludeFromBackup(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(FolderArgs.self)
    var url = URL(fileURLWithPath: args.path, isDirectory: true)
    var values = URLResourceValues()
    values.isExcludedFromBackup = true
    do {
      try url.setResourceValues(values)
      invoke.resolve()
    } catch {
      invoke.reject(error.localizedDescription)
    }
  }

  /// Whether the phone is set to 24-hour time: its own pattern for an hour has no am or pm.
  @objc public func hourClock(_ invoke: Invoke) throws {
    let pattern = DateFormatter.dateFormat(fromTemplate: "j", options: 0, locale: Locale.current) ?? ""
    invoke.resolve(["is24": !pattern.contains("a")])
  }

  private static func available(_ path: String) -> Int64? {
    let url = URL(fileURLWithPath: path, isDirectory: true)
    let values = try? url.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey])
    return values?.volumeAvailableCapacityForImportantUsage
  }

  /// Bytes free on the volume that holds the folder, counting what the phone would clear for it.
  @objc public func freeSpace(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(FolderArgs.self)
    let bytes: Any = SystemPlugin.available(args.path).map { NSNumber(value: $0) } ?? NSNull()
    invoke.resolve(["bytes": bytes])
  }

  private func present(_ picker: UIDocumentPickerViewController, _ sheet: Sheet, _ invoke: Invoke) {
    DispatchQueue.main.async {
      guard self.sheet == nil, var root = self.manager.viewController else {
        return invoke.resolve(["outcome": "failed"])
      }
      while let shown = root.presentedViewController { root = shown }
      self.sheet = sheet
      picker.delegate = sheet
      picker.presentationController?.delegate = sheet
      root.present(picker, animated: true)
    }
  }

  /// The phone's save sheet, with a copy of the file: the file keeps its own name there.
  @objc public func saveFile(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(SaveArgs.self)
    let source = URL(fileURLWithPath: args.path)
    guard FileManager.default.fileExists(atPath: source.path) else {
      return invoke.resolve(["outcome": "failed"])
    }
    let sheet = Sheet(
      picked: { [weak self] _ in
        self?.sheet = nil
        invoke.resolve(["outcome": "saved"])
      },
      cancelled: { [weak self] in
        self?.sheet = nil
        invoke.resolve(["outcome": "cancelled"])
      })
    DispatchQueue.main.async {
      let picker = UIDocumentPickerViewController(forExporting: [source], asCopy: true)
      self.present(picker, sheet, invoke)
    }
  }

  /// The phone's open sheet. The chosen file is read where it is and copied into the folder the
  /// app names, with its name, size and date as the place it came from gives them.
  @objc public func chooseFile(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(FolderArgs.self)
    let dir = URL(fileURLWithPath: args.path, isDirectory: true)
    let sheet = Sheet(
      picked: { [weak self] urls in
        self?.sheet = nil
        guard let url = urls.first else { return invoke.resolve(["outcome": "cancelled"]) }
        DispatchQueue.global(qos: .userInitiated).async {
          SystemPlugin.copyChosen(url, into: dir, invoke)
        }
      },
      cancelled: { [weak self] in
        self?.sheet = nil
        invoke.resolve(["outcome": "cancelled"])
      })
    DispatchQueue.main.async {
      let picker = UIDocumentPickerViewController(forOpeningContentTypes: [.item], asCopy: false)
      picker.allowsMultipleSelection = false
      self.present(picker, sheet, invoke)
    }
  }

  private static func copyChosen(_ url: URL, into dir: URL, _ invoke: Invoke) {
    let scoped = url.startAccessingSecurityScopedResource()
    defer { if scoped { url.stopAccessingSecurityScopedResource() } }
    let target = dir.appendingPathComponent(chosenName)
    let values = try? url.resourceValues(forKeys: [
      .fileSizeKey, .contentModificationDateKey, .nameKey,
    ])
    let size = Int64(values?.fileSize ?? -1)
    do {
      try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
      if size >= 0, let free = available(dir.path), size + spare > free {
        return invoke.resolve(["outcome": "full"])
      }
      try? FileManager.default.removeItem(at: target)
      var failure: Error?
      var coordinated: NSError?
      NSFileCoordinator().coordinate(readingItemAt: url, options: [], error: &coordinated) { at in
        do {
          try FileManager.default.copyItem(at: at, to: target)
        } catch {
          failure = error
        }
      }
      if let error = failure ?? coordinated { throw error }
      let copied = (try? target.resourceValues(forKeys: [.fileSizeKey]))?.fileSize ?? 0
      var modified: Any = NSNull()
      if let date = values?.contentModificationDate {
        modified = NSNumber(value: Int64(date.timeIntervalSince1970 * 1000))
      }
      invoke.resolve([
        "outcome": "chosen",
        "path": target.path,
        "name": values?.name ?? url.lastPathComponent,
        "size": copied,
        "modified": modified,
      ])
    } catch {
      try? FileManager.default.removeItem(at: target)
      let full = (error as NSError).domain == NSCocoaErrorDomain
        && (error as NSError).code == NSFileWriteOutOfSpaceError
      invoke.resolve(["outcome": full ? "full" : "failed"])
    }
  }
}

@_cdecl("init_plugin_nocturne_system")
func initPlugin() -> Plugin {
  // iOS takes a background task only while the app starts, as the app's setup makes its plugins.
  Background.shared.register()
  _ = NetworkCost.shared
  return SystemPlugin()
}
