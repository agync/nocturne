import Foundation

/// Why a call to the person's iCloud did not do what was asked, as the core names it.
enum PlaceOutcome: String, Error {
  case noRoom
  case noAccess
  case offline
  case failed
}

/// The backup's folder in the app's own iCloud container. It sits outside the container's
/// Documents folder, so it doesn't show among the person's files. Every call blocks: the plugin
/// runs them on its own queue, never the main one.
enum ICloudPlace {
  static let container = "iCloud.is.nocturne.phone"
  private static let folderName = "Backup"
  /// How long a file may take to go up to iCloud or come down from it.
  private static let transferWait: TimeInterval = 300
  /// How long iCloud's index of the container may take to answer.
  private static let indexWait: TimeInterval = 60

  static func validName(_ name: String) -> Bool {
    (1...128).contains(name.utf8.count) && validPrefix(name)
  }

  static func validPrefix(_ prefix: String) -> Bool {
    prefix.utf8.count <= 128
      && prefix.utf8.allSatisfy { ($0 >= 97 && $0 <= 122) || ($0 >= 48 && $0 <= 57) || $0 == 45 }
  }

  /// Whether this iPhone can reach the app's iCloud space: "connected", "noAccount" when it is
  /// signed out of iCloud, or "turnedOff" when iCloud is off for Nocturne.
  static func state() -> String {
    if NetworkCost.shared.online == false { return "offline" }
    if FileManager.default.ubiquityIdentityToken == nil { return "noAccount" }
    return (try? folder()) != nil ? "connected" : "turnedOff"
  }

  static func folder() throws -> URL {
    guard FileManager.default.ubiquityIdentityToken != nil,
      let root = FileManager.default.url(forUbiquityContainerIdentifier: container)
    else { throw PlaceOutcome.noAccess }
    let dir = root.appendingPathComponent(folderName, isDirectory: true)
    do {
      try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    } catch {
      throw outcome(error)
    }
    return dir
  }

  /// What a file error means for the backup.
  static func outcome(_ error: Error) -> PlaceOutcome {
    if let place = error as? PlaceOutcome { return place }
    let e = error as NSError
    guard e.domain == NSCocoaErrorDomain else { return .failed }
    switch e.code {
    case NSFileWriteOutOfSpaceError, NSUbiquitousFileNotUploadedDueToQuotaError: return .noRoom
    case NSUbiquitousFileUbiquityServerNotAvailable: return .offline
    case NSFileWriteNoPermissionError, NSFileReadNoPermissionError: return .noAccess
    default: return .failed
    }
  }

  private static func coordinated(
    _ url: URL, writing options: NSFileCoordinator.WritingOptions, _ work: (URL) throws -> Void
  ) throws {
    var failure: Error?
    var coordination: NSError?
    NSFileCoordinator().coordinate(writingItemAt: url, options: options, error: &coordination) {
      at in
      do { try work(at) } catch { failure = error }
    }
    if let error = coordination ?? failure { throw outcome(error) }
  }

  /// Writes the file at `source` as `name`, whole or not at all, and drops every earlier version
  /// iCloud kept of it. Returns once iCloud holds it: a copy only on this iPhone is no backup.
  static func put(_ name: String, from source: URL) throws {
    let target = try folder().appendingPathComponent(name)
    let data: Data
    do { data = try Data(contentsOf: source) } catch { throw PlaceOutcome.failed }
    try coordinated(target, writing: .forReplacing) { at in
      try data.write(to: at, options: .atomic)
      try NSFileVersion.removeOtherVersionsOfItem(at: at)
    }
    try upload(target)
  }

  /// Waits until iCloud holds the file, or says why it can't: a full iCloud is no room. A file
  /// still waiting when the time runs out counts as offline, and the next night sends it again.
  private static func upload(_ url: URL) throws {
    let keys: Set<URLResourceKey> = [.ubiquitousItemIsUploadedKey, .ubiquitousItemUploadingErrorKey]
    let until = Date().addingTimeInterval(transferWait)
    while Date() < until {
      var url = url
      url.removeAllCachedResourceValues()
      let values = try? url.resourceValues(forKeys: keys)
      if let error = values?.ubiquitousItemUploadingError { throw outcome(error) }
      if values?.ubiquitousItemIsUploaded == true { return }
      Thread.sleep(forTimeInterval: 0.5)
    }
    throw PlaceOutcome.offline
  }

  /// A file in the backup's folder as iCloud's own index knows it, also on a new iPhone where
  /// nothing has come down yet.
  private struct Indexed {
    let url: URL
    let size: Int64
    let modified: Date?
    let uploaded: Bool
  }

  /// Every file iCloud's index holds in the backup's folder, by name.
  private static func indexed() throws -> [String: Indexed] {
    let dir = try folder().resolvingSymlinksInPath().path
    let query = NSMetadataQuery()
    query.searchScopes = [NSMetadataQueryUbiquitousDataScope]
    query.predicate = NSPredicate(format: "%K LIKE %@", NSMetadataItemFSNameKey, "*")
    let gathered = DispatchSemaphore(value: 0)
    var items: [NSMetadataItem] = []
    var started = false
    var observer: NSObjectProtocol?
    DispatchQueue.main.async {
      observer = NotificationCenter.default.addObserver(
        forName: .NSMetadataQueryDidFinishGathering, object: query, queue: .main
      ) { _ in
        query.disableUpdates()
        items = (0..<query.resultCount).compactMap { query.result(at: $0) as? NSMetadataItem }
        query.stop()
        gathered.signal()
      }
      started = query.start()
      if !started { gathered.signal() }
    }
    let answered = gathered.wait(timeout: .now() + indexWait) == .success
    DispatchQueue.main.sync {
      if let observer = observer { NotificationCenter.default.removeObserver(observer) }
      query.stop()
    }
    guard answered else { throw PlaceOutcome.offline }
    guard started else { throw PlaceOutcome.failed }
    var out: [String: Indexed] = [:]
    for item in items {
      guard let url = item.value(forAttribute: NSMetadataItemURLKey) as? URL,
        let name = item.value(forAttribute: NSMetadataItemFSNameKey) as? String,
        url.deletingLastPathComponent().resolvingSymlinksInPath().path == dir
      else { continue }
      let size = (item.value(forAttribute: NSMetadataItemFSSizeKey) as? NSNumber)?.int64Value
      let uploaded = item.value(forAttribute: NSMetadataUbiquitousItemIsUploadedKey) as? NSNumber
      out[name] = Indexed(
        url: url,
        size: size ?? 0,
        modified: item.value(forAttribute: NSMetadataItemFSContentChangeDateKey) as? Date,
        uploaded: uploaded?.boolValue ?? false)
    }
    return out
  }

  /// The placeholder an older iOS leaves for a file that is only in iCloud.
  private static func placeholder(_ dir: URL, _ name: String) -> URL {
    dir.appendingPathComponent(".\(name).icloud")
  }

  /// Copies `name` to `target` and says whether it was there. A file only in iCloud is brought
  /// down first.
  static func get(_ name: String, to target: URL) throws -> Bool {
    let dir = try folder()
    var url = dir.appendingPathComponent(name)
    let fm = FileManager.default
    if !fm.fileExists(atPath: url.path) && !fm.fileExists(atPath: placeholder(dir, name).path) {
      guard let found = try indexed()[name] else { return false }
      url = found.url
    }
    try download(url)
    var failure: Error?
    var coordination: NSError?
    NSFileCoordinator().coordinate(readingItemAt: url, options: [], error: &coordination) { at in
      do {
        try? fm.removeItem(at: target)
        try fm.copyItem(at: at, to: target)
      } catch {
        failure = error
      }
    }
    if let error = coordination ?? failure {
      if (error as NSError).code == NSFileReadNoSuchFileError { return false }
      throw outcome(error)
    }
    return true
  }

  private static func download(_ url: URL) throws {
    let keys: Set<URLResourceKey> = [
      .ubiquitousItemDownloadingStatusKey, .ubiquitousItemDownloadingErrorKey,
    ]
    var asked = false
    let until = Date().addingTimeInterval(transferWait)
    while Date() < until {
      var url = url
      url.removeAllCachedResourceValues()
      let values = try? url.resourceValues(forKeys: keys)
      if let error = values?.ubiquitousItemDownloadingError { throw outcome(error) }
      let status = values?.ubiquitousItemDownloadingStatus
      if status == nil || status == .current { return }
      if !asked {
        do { try FileManager.default.startDownloadingUbiquitousItem(at: url) } catch {
          throw outcome(error)
        }
        asked = true
      }
      Thread.sleep(forTimeInterval: 0.5)
    }
    throw PlaceOutcome.offline
  }

  /// Every file iCloud holds whose name starts with `prefix`, with its size and when it last
  /// changed: what its index knows, and what this iPhone wrote that the index has not caught up
  /// with. A file not yet up in iCloud is left out, so the next night sends it again.
  static func list(_ prefix: String) throws -> [[String: Any]] {
    var out: [String: [String: Any]] = [:]
    func add(_ name: String, _ size: Int64, _ modified: Date?) {
      guard validName(name), name.hasPrefix(prefix) else { return }
      var entry: [String: Any] = ["name": name, "size": size]
      if let modified = modified {
        entry["modified"] = Int64(modified.timeIntervalSince1970 * 1000)
      }
      out[name] = entry
    }
    for (name, item) in try indexed() where item.uploaded {
      add(name, item.size, item.modified)
    }
    let keys: [URLResourceKey] = [
      .fileSizeKey, .contentModificationDateKey, .ubiquitousItemIsUploadedKey,
    ]
    let local =
      (try? FileManager.default.contentsOfDirectory(
        at: folder(), includingPropertiesForKeys: keys, options: [])) ?? []
    for url in local where out[url.lastPathComponent] == nil {
      let values = try? url.resourceValues(forKeys: Set(keys))
      guard values?.ubiquitousItemIsUploaded == true else { continue }
      add(url.lastPathComponent, Int64(values?.fileSize ?? 0), values?.contentModificationDate)
    }
    return Array(out.values)
  }

  /// Removes `name` from iCloud, with every version kept of it. A file not there is fine.
  static func delete(_ name: String) throws {
    let dir = try folder()
    for url in [dir.appendingPathComponent(name), placeholder(dir, name)] {
      guard FileManager.default.fileExists(atPath: url.path) else { continue }
      try coordinated(url, writing: .forDeleting) { at in
        try FileManager.default.removeItem(at: at)
      }
    }
  }
}
