import BackgroundTasks
import Foundation
import UIKit
import UserNotifications

/// What the core says about Read now: the words it shows while the person is away, and how far it is.
struct ReadingArgs: Decodable {
  var state: String
  var title: String?
  var text: String?
  var done: Int?
  var count: Int?
  var pausedTitle: String?
  var pausedText: String?
  var why: String?
}

/// Read now while the person is away. On iOS 26 the phone's own progress carries it, and the
/// person can stop it there; before that the app asks for a short while in the background, and
/// when iOS ends it the person gets one quiet line and the app says it paused. Not yet tried on an
/// iPhone: the Info.plist lists the name the progress is made under.
final class Reading {
  static let shared = Reading()
  static let prefix = "app.nocturne.readnow."
  private static let pausedId = "app.nocturne.readnow.paused"

  private let lock = NSLock()
  private var task: AnyObject?
  private var background: UIBackgroundTaskIdentifier = .invalid
  private var registered = false
  private var paused = false
  private var pausedTitle = ""
  private var pausedText = ""
  private var watching = false

  /// Handles one call from the core and answers what it needs back.
  func handle(_ args: ReadingArgs, _ answer: @escaping ([String: Any]) -> Void) {
    DispatchQueue.main.async {
      self.watch()
      if args.state == "reading" {
        self.reading(args)
      } else {
        self.ended(args)
      }
      self.notesAllowed { notes in
        self.lock.lock()
        let wasPaused = self.paused
        self.paused = false
        self.lock.unlock()
        answer(["stop": false, "paused": wasPaused, "notes": notes])
      }
    }
  }

  private func watch() {
    guard !watching else { return }
    watching = true
    NotificationCenter.default.addObserver(
      forName: UIApplication.willEnterForegroundNotification, object: nil, queue: .main
    ) { _ in
      UNUserNotificationCenter.current().removeDeliveredNotifications(withIdentifiers: [
        Reading.pausedId
      ])
    }
  }

  private func reading(_ args: ReadingArgs) {
    pausedTitle = args.pausedTitle ?? pausedTitle
    pausedText = args.pausedText ?? pausedText
    let title = args.title ?? ""
    let text = args.text ?? ""
    if #available(iOS 26.0, *) {
      if let current = task as? BGContinuedProcessingTask {
        current.progress.totalUnitCount = Int64(args.count ?? 0)
        current.progress.completedUnitCount = Int64(args.done ?? 0)
        current.updateTitle(title, subtitle: text)
        return
      }
      if task == nil, UIApplication.shared.applicationState == .active {
        start(title, text, args)
      }
      return
    }
    if background == .invalid, UIApplication.shared.applicationState == .active {
      background = UIApplication.shared.beginBackgroundTask(withName: "Reading what you kept") {
        [weak self] in
        self?.expired()
      }
    }
  }

  @available(iOS 26.0, *)
  private func start(_ title: String, _ text: String, _ args: ReadingArgs) {
    if !registered {
      registered = BGTaskScheduler.shared.register(
        forTaskWithIdentifier: Reading.prefix + "*", using: nil
      ) { [weak self] task in
        guard let self, let task = task as? BGContinuedProcessingTask else { return }
        self.open(task, args)
      }
    }
    guard registered else { return }
    let request = BGContinuedProcessingTaskRequest(
      identifier: Reading.prefix + UUID().uuidString, title: title, subtitle: text)
    request.strategy = .fail
    try? BGTaskScheduler.shared.submit(request)
  }

  @available(iOS 26.0, *)
  private func open(_ task: BGContinuedProcessingTask, _ args: ReadingArgs) {
    DispatchQueue.main.async {
      self.task = task
      task.progress.totalUnitCount = Int64(args.count ?? 0)
      task.progress.completedUnitCount = Int64(args.done ?? 0)
      task.expirationHandler = { [weak self] in
        DispatchQueue.main.async { self?.expired() }
      }
    }
  }

  /// iOS, or the person in the system's own progress, ended the time the app was given.
  private func expired() {
    lock.lock()
    paused = true
    lock.unlock()
    finish(success: false)
    postPaused()
  }

  private func ended(_ args: ReadingArgs) {
    finish(success: true)
    guard let title = args.title, args.why != "done",
      UIApplication.shared.applicationState != .active
    else { return }
    post(id: Reading.pausedId, title: title, body: args.text ?? "")
  }

  private func finish(success: Bool) {
    if #available(iOS 26.0, *), let current = task as? BGContinuedProcessingTask {
      current.setTaskCompleted(success: success)
    }
    task = nil
    if background != .invalid {
      UIApplication.shared.endBackgroundTask(background)
      background = .invalid
    }
  }

  private func postPaused() {
    post(id: Reading.pausedId, title: pausedTitle, body: pausedText)
  }

  private func post(id: String, title: String, body: String) {
    let center = UNUserNotificationCenter.current()
    center.getNotificationSettings { settings in
      guard settings.authorizationStatus == .authorized
        || settings.authorizationStatus == .provisional
      else { return }
      let content = UNMutableNotificationContent()
      content.title = title
      content.body = body
      center.add(UNNotificationRequest(identifier: id, content: content, trigger: nil))
    }
  }

  /// Notifications may show unless the person has refused them; unknown counts as allowed.
  private func notesAllowed(_ done: @escaping (Bool) -> Void) {
    UNUserNotificationCenter.current().getNotificationSettings { settings in
      done(settings.authorizationStatus != .denied)
    }
  }
}
