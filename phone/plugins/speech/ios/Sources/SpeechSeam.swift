import Foundation
import Speech

/// What one ask to the phone's reader came to, as the app's Rust side takes it.
enum Outcome: String {
  case ready
  case text
  case languageUnavailable
  case notSupported
  case noPermission
  case nothingHeard
  case failed
}

/// The raw sound the reader takes from a file: 16-bit, mono, 16 kHz, no header.
enum PcmFormat {
  static let sampleRate = 16_000
  static let bytesPerSecond = sampleRate * 2

  static func durationMs(_ bytes: Int) -> Int { bytes * 1000 / bytesPerSecond }

  /// How long a read may take before it is given up: the reader works at about the speed the
  /// sound plays, so 15 seconds plus three times its length.
  static func deadline(_ bytes: Int) -> TimeInterval {
    15 + 3 * Double(durationMs(bytes)) / 1000
  }

  /// A WAV header for `bytes` of raw sound, so the phone's reader can open the file.
  static func wavHeader(_ bytes: Int) -> Data {
    var out = Data()
    func put32(_ v: UInt32) { withUnsafeBytes(of: v.littleEndian) { out.append(contentsOf: $0) } }
    func put16(_ v: UInt16) { withUnsafeBytes(of: v.littleEndian) { out.append(contentsOf: $0) } }
    out.append(contentsOf: Array("RIFF".utf8))
    put32(UInt32(36 + bytes))
    out.append(contentsOf: Array("WAVEfmt ".utf8))
    put32(16)
    put16(1)
    put16(1)
    put32(UInt32(sampleRate))
    put32(UInt32(bytesPerSecond))
    put16(2)
    put16(16)
    out.append(contentsOf: Array("data".utf8))
    put32(UInt32(bytes))
    return out
  }
}

enum SpeechSeam {
  /// The permission as Tauri names its states.
  static func access(_ status: SFSpeechRecognizerAuthorizationStatus) -> String {
    switch status {
    case .authorized: return "granted"
    case .notDetermined: return "prompt"
    default: return "denied"
    }
  }

  /// Whether a reader of the language can read a file now, without the network. `nil` when it
  /// can; the outcome that says why not, otherwise.
  static func refusal(
    authorized: Bool, recognizerExists: Bool, onDevice: Bool
  ) -> Outcome? {
    if !authorized { return .noPermission }
    if !recognizerExists { return .notSupported }
    if !onDevice { return .languageUnavailable }
    return nil
  }

  /// The reader's error after a read: no speech in the sound is nothing heard, not a failure.
  static func outcome(errorDomain: String, code: Int) -> Outcome {
    // kAFAssistantErrorDomain 1110: no speech detected.
    if errorDomain == "kAFAssistantErrorDomain" && code == 1110 { return .nothingHeard }
    return .failed
  }

  /// The words of a finished read; blank words are nothing heard.
  static func words(_ text: String?) -> (Outcome, String?) {
    let trimmed = text?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    return trimmed.isEmpty ? (.nothingHeard, nil) : (.text, trimmed)
  }
}
