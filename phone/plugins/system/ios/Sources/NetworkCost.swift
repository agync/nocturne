import Foundation
import Network

/// Whether the network in use costs nothing by the byte, as the system judges it.
final class NetworkCost {
  static let shared = NetworkCost()
  private let monitor = NWPathMonitor()
  private let lock = NSLock()
  private var path: NWPath?

  private init() {
    monitor.pathUpdateHandler = { [weak self] path in
      self?.lock.lock()
      self?.path = path
      self?.lock.unlock()
    }
    monitor.start(queue: DispatchQueue(label: "app.nocturne.network"))
  }

  /// Whether the phone is online; nil before the first look.
  var online: Bool? {
    lock.lock()
    defer { lock.unlock() }
    return path.map { $0.status == .satisfied }
  }

  /// Nil while offline or before the first look.
  var unmetered: Bool? {
    lock.lock()
    defer { lock.unlock() }
    guard let path = path, path.status == .satisfied else { return nil }
    return !path.isExpensive && !path.isConstrained
  }
}
