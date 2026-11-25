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
/// person can stop it there: its end, by them or by iOS, is the person's Stop, since Nocturne
/// never reads on against them. Before iOS 26, or when the progress cannot start, the app asks
/// for a short while in the background, and when iOS ends it the person gets one quiet line and
/// the app says it paused. Not yet tried on an iPhone: the Info.plist lists the names the
/// progress is made under.
final class Reading {
  static let shared = Reading()
  static let prefix = "app.nocturne.readnow."
  private static let pausedId = "app.nocturne.readnow.paused"

  private let lock = NSLock()
  private var task: AnyObject?
  private var background: UIBackgroundTaskIdentifier = .invalid
  /// A request for the phone's own progress is on its way: no second one is made meanwhile.
  private var pending = false
  private var stopped = false
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
        let wasStopped = self.stopped
        self.paused = false
        self.stopped = false
        self.lock.unlock()
        answer(["stop": wasStopped, "paused": wasPaused, "notes": notes])
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
    // iPhone's own progress is compiled in only with the iOS 26 SDK (Swift 6.2, Xcode 26); an
    // older Xcode builds the short while alone.
    #if compiler(>=6.2)
    if #available(iOS 26.0, *) {
      if let current = task as? BGContinuedProcessingTask {
        current.progress.totalUnitCount = Int64(args.count ?? 0)
        current.progress.completedUnitCount = Int64(args.done ?? 0)
        current.updateTitle(title, subtitle: text)
        return
      }
      if task == nil, !pending, background == .invalid,
        UIApplication.shared.applicationState == .active
      {
        if start(title, text, args) { return }
      } else {
        return
      }
    }
    #endif
    beginShortWhile()
  }

  /// The short while iOS gives an app as it leaves; its end is a pause.
  private func beginShortWhile() {
    guard background == .invalid, UIApplication.shared.applicationState == .active else { return }
    background = UIApplication.shared.beginBackgroundTask(withName: "Reading what you kept") {
      [weak self] in
      self?.expired()
    }
  }

  #if compiler(>=6.2)
  /// Asks for the phone's own progress under a new name, with its launch handler registered for
  /// that name first, as BGTaskScheduler requires. False when iOS refused it.
  @available(iOS 26.0, *)
  private func start(_ title: String, _ text: String, _ args: ReadingArgs) -> Bool {
    let name = Reading.prefix + UUID().uuidString
    let registered = BGTaskScheduler.shared.register(forTaskWithIdentifier: name, using: nil) {
      [weak self] task in
      guard let self, let task = task as? BGContinuedProcessingTask else { return }
      self.open(task, args)
    }
    guard registered else {
      NSLog("Nocturne: the reading's progress could not be registered")
      return false
    }
    let request = BGContinuedProcessingTaskRequest(identifier: name, title: title, subtitle: text)
    request.strategy = .fail
    do {
      try BGTaskScheduler.shared.submit(request)
      pending = true
      return true
    } catch {
      NSLog("Nocturne: the reading's progress was refused: \(error.localizedDescription)")
      return false
    }
  }

  @available(iOS 26.0, *)
  private func open(_ task: BGContinuedProcessingTask, _ args: ReadingArgs) {
    DispatchQueue.main.async {
      self.task = task
      self.pending = false
      task.progress.totalUnitCount = Int64(args.count ?? 0)
      task.progress.completedUnitCount = Int64(args.done ?? 0)
      task.expirationHandler = { [weak self] in
        DispatchQueue.main.async { self?.stoppedThere() }
      }
    }
  }
  #endif

  /// The phone's own progress ended, by the person's Stop there or by iOS: told as a Stop.
  private func stoppedThere() {
    lock.lock()
    stopped = true
    lock.unlock()
    finish(success: false)
  }

  /// iOS ended the short while it gave the app as it left: a pause, with one quiet line.
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
    #if compiler(>=6.2)
    if #available(iOS 26.0, *), let current = task as? BGContinuedProcessingTask {
      current.setTaskCompleted(success: success)
    }
    #endif
    task = nil
    pending = false
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
