import EventKit
import Tauri

struct ReadArgs: Decodable {
  var back: Int?
  var ahead: Int?
  var calendarIds: [String]?
  var withNotes: Bool?
}

class CalendarPlugin: Plugin {
  private var store = EKEventStore()
  private let queue = DispatchQueue(label: "app.nocturne.calendar", qos: .userInitiated)

  /// Each read runs off the main thread, because the app's Rust side waits for the answer.
  private func background(_ invoke: Invoke, _ work: @escaping (EKEventStore) throws -> Void) {
    guard CalendarReader.allowed() else { return invoke.reject("notAllowed") }
    queue.async {
      do {
        try work(self.store)
      } catch {
        invoke.reject(error.localizedDescription)
      }
    }
  }

  @objc override func checkPermissions(_ invoke: Invoke) {
    invoke.resolve(["calendar": CalendarReader.access()])
  }

  /// The phone's own prompt, unless it will no longer show it; answers what it says.
  @objc override func requestPermissions(_ invoke: Invoke) {
    guard EKEventStore.authorizationStatus(for: .event) == .notDetermined else {
      return invoke.resolve(["calendar": CalendarReader.access()])
    }
    let asker = EKEventStore()
    asker.requestFullAccessToEvents { [weak self] _, _ in
      withExtendedLifetime(asker) {}
      // A store made before access was given sees nothing. Reads use the store on the queue.
      guard let self else { return invoke.resolve(["calendar": CalendarReader.access()]) }
      self.queue.async {
        self.store = EKEventStore()
        invoke.resolve(["calendar": CalendarReader.access()])
      }
    }
  }

  @objc public func listCalendars(_ invoke: Invoke) {
    background(invoke) { store in
      invoke.resolve(Calendars(calendars: CalendarReader.calendars(store)))
    }
  }

  @objc public func listInstances(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(ReadArgs.self)
    background(invoke) { store in
      invoke.resolve(
        CalendarReader.instances(
          store, back: args.back ?? 2, ahead: args.ahead ?? 14,
          calendarIds: args.calendarIds ?? [], withNotes: args.withNotes ?? false))
    }
  }
}

@_cdecl("init_plugin_nocturne_calendar")
func initPlugin() -> Plugin {
  return CalendarPlugin()
}
