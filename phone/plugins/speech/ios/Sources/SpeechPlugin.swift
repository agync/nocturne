import AVFoundation
import Foundation
import Speech
import Tauri

struct ReadArgs: Decodable {
  var path: String
  var language: String
}

struct DecodeArgs: Decodable {
  var path: String
  var dest: String
}

/// The iPhone side of the speech plugin: a kept voice note read into words by the phone's own
/// reader, only on the phone. A language the phone cannot read without the network is never read:
/// the note waits. One ask runs at a time; the others wait their turn.
class SpeechPlugin: Plugin {
  private let queue = DispatchQueue(label: "app.nocturne.speech", qos: .userInitiated)

  private func answer(_ invoke: Invoke, _ outcome: Outcome, _ text: String? = nil) {
    var out: JsonObject = ["outcome": outcome.rawValue]
    if let text { out["text"] = text }
    invoke.resolve(out)
  }

  @objc override func checkPermissions(_ invoke: Invoke) {
    invoke.resolve(["speech": SpeechSeam.access(SFSpeechRecognizer.authorizationStatus())])
  }

  /// The phone's own prompt, unless it will no longer show it; answers what it says.
  @objc override func requestPermissions(_ invoke: Invoke) {
    guard SFSpeechRecognizer.authorizationStatus() == .notDetermined else {
      return checkPermissions(invoke)
    }
    SFSpeechRecognizer.requestAuthorization { status in
      invoke.resolve(["speech": SpeechSeam.access(status)])
    }
  }

  /// A reader of the language that reads on the phone, or the outcome that says why there is none.
  private func reader(_ language: String) -> (SFSpeechRecognizer?, Outcome?) {
    let recognizer = SFSpeechRecognizer(locale: Locale(identifier: language))
    let refusal = SpeechSeam.refusal(
      authorized: SFSpeechRecognizer.authorizationStatus() == .authorized,
      recognizerExists: recognizer != nil,
      onDevice: recognizer?.supportsOnDeviceRecognition ?? false)
    return (refusal == nil ? recognizer : nil, refusal)
  }

  @objc public func checkSupport(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(ReadArgs.self)
    let (_, refusal) = reader(args.language)
    answer(invoke, refusal ?? .ready)
  }

  /// Reads the raw sound at `path`. The request may only be read on the phone: a reader that
  /// would need the network fails instead of sending the sound anywhere.
  @objc public func transcribe(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(ReadArgs.self)
    queue.async {
      let (recognizer, refusal) = self.reader(args.language)
      guard let recognizer else { return self.answer(invoke, refusal ?? .failed) }
      guard let sound = FileManager.default.contents(atPath: args.path), !sound.isEmpty else {
        return self.answer(invoke, .failed)
      }
      let wav = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString)
        .appendingPathExtension("wav")
      do {
        try (PcmFormat.wavHeader(sound.count) + sound).write(to: wav)
      } catch {
        return self.answer(invoke, .failed)
      }
      let done = DispatchSemaphore(value: 0)
      let guarded = NSLock()
      var result: (Outcome, String?)?
      let settle = { (value: (Outcome, String?)) in
        guarded.lock()
        defer { guarded.unlock() }
        if result == nil {
          result = value
          done.signal()
        }
      }
      let request = SFSpeechURLRecognitionRequest(url: wav)
      request.requiresOnDeviceRecognition = true
      request.shouldReportPartialResults = false
      request.addsPunctuation = true
      let task = recognizer.recognitionTask(with: request) { found, error in
        if let found, found.isFinal {
          settle(SpeechSeam.words(found.bestTranscription.formattedString))
        } else if let error = error as NSError? {
          settle((SpeechSeam.outcome(errorDomain: error.domain, code: error.code), nil))
        }
      }
      if done.wait(timeout: .now() + PcmFormat.deadline(sound.count)) == .timedOut {
        task.cancel()
        settle((.failed, nil))
      }
      try? FileManager.default.removeItem(at: wav)
      guarded.lock()
      let (outcome, text) = result ?? (.failed, nil)
      guarded.unlock()
      self.answer(invoke, outcome, text)
    }
  }

  /// A recording in any form the phone can play (m4a from the recorder or another app), written
  /// as the raw sound `transcribe` takes. Answers how long it plays, or nothing when the phone
  /// cannot decode it.
  @objc public func decode(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(DecodeArgs.self)
    queue.async {
      let dest = URL(fileURLWithPath: args.dest)
      do {
        let bytes = try SpeechPlugin.toPcm(URL(fileURLWithPath: args.path), dest)
        let ms: Any = bytes > 0 ? NSNumber(value: PcmFormat.durationMs(bytes)) : NSNull()
        invoke.resolve(["durationMs": ms])
      } catch {
        try? FileManager.default.removeItem(at: dest)
        invoke.resolve(["durationMs": NSNull()])
      }
    }
  }

  private static func toPcm(_ src: URL, _ dest: URL) throws -> Int {
    let file = try AVAudioFile(forReading: src)
    let from = file.processingFormat
    guard
      let to = AVAudioFormat(
        commonFormat: .pcmFormatInt16, sampleRate: Double(PcmFormat.sampleRate), channels: 1,
        interleaved: true),
      let converter = AVAudioConverter(from: from, to: to),
      let input = AVAudioPCMBuffer(pcmFormat: from, frameCapacity: 8192),
      let output = AVAudioPCMBuffer(pcmFormat: to, frameCapacity: 8192)
    else { throw CocoaError(.fileReadCorruptFile) }
    converter.downmix = true
    FileManager.default.createFile(atPath: dest.path, contents: nil)
    let out = try FileHandle(forWritingTo: dest)
    defer { try? out.close() }
    var written = 0
    var ended = false
    while true {
      var failure: NSError?
      let status = converter.convert(to: output, error: &failure) { _, state in
        if ended {
          state.pointee = .endOfStream
          return nil
        }
        do {
          try file.read(into: input)
        } catch {
          input.frameLength = 0
        }
        if input.frameLength == 0 {
          ended = true
          state.pointee = .endOfStream
          return nil
        }
        state.pointee = .haveData
        return input
      }
      if let failure { throw failure }
      let count = Int(output.frameLength) * 2
      if count > 0, let samples = output.int16ChannelData {
        try out.write(contentsOf: Data(bytes: samples[0], count: count))
        written += count
      }
      if status == .endOfStream || status == .error { break }
    }
    return written
  }
}

@_cdecl("init_plugin_nocturne_speech")
func initPlugin() -> Plugin {
  return SpeechPlugin()
}
