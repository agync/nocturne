import BackgroundTasks
import Foundation
import UIKit

/// A time iOS lets the app work in the background while the phone charges and is online, as a
/// processing task. It is used only when the app's settings list its name among the tasks it
/// may run; until then nothing is registered and no window opens.
final class Background {
  static let shared = Background()
  static let name = "app.nocturne.mind"

  private let lock = NSLock()
  private var task: BGProcessingTask?
  private var registered = false

  private var permitted: Bool {
    let names = Bundle.main.object(forInfoDictionaryKey: "BGTaskSchedulerPermittedIdentifiers")
    return (names as? [String])?.contains(Background.name) ?? false
  }

  /// Registers the task. iOS takes this only before the app has finished starting, so it is
  /// called as the plugin is made, during the app's setup. Not yet tried on an iPhone.
  func register() {
    guard permitted, !registered else { return }
    registered = BGTaskScheduler.shared.register(forTaskWithIdentifier: Background.name, using: nil) {
      [weak self] task in
      guard let self, let task = task as? BGProcessingTask else { return }
      self.open(task)
    }
  }

  private func open(_ task: BGProcessingTask) {
    lock.lock()
    self.task = task
    lock.unlock()
    task.expirationHandler = { [weak self] in self?.done() }
    schedule(true)
  }

  var isOpen: Bool {
    lock.lock()
    defer { lock.unlock() }
    return task != nil
  }

  func done() {
    lock.lock()
    let ending = task
    task = nil
    lock.unlock()
    ending?.setTaskCompleted(success: true)
  }

  func schedule(_ on: Bool) {
    guard registered else { return }
    guard on else {
      BGTaskScheduler.shared.cancel(taskRequestWithIdentifier: Background.name)
      done()
      return
    }
    let request = BGProcessingTaskRequest(identifier: Background.name)
    request.requiresExternalPower = true
    request.requiresNetworkConnectivity = true
    request.earliestBeginDate = Date(timeIntervalSinceNow: 60 * 60)
    try? BGTaskScheduler.shared.submit(request)
  }
}

/// Whether the phone charges, from the battery's own report; nil when it does not say.
enum Power {
  static func charging() -> Bool? {
    let device = UIDevice.current
    device.isBatteryMonitoringEnabled = true
    switch device.batteryState {
    case .charging, .full: return true
    case .unplugged: return false
    default: return nil
    }
  }
}
