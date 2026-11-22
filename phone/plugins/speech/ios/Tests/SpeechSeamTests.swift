import Speech
import XCTest

@testable import tauri_plugin_nocturne_speech

final class SpeechSeamTests: XCTestCase {
  func testASecondOfSoundIsThirtyTwoThousandBytes() {
    XCTAssertEqual(PcmFormat.bytesPerSecond, 32_000)
    XCTAssertEqual(PcmFormat.durationMs(48_000), 1_500)
    XCTAssertEqual(PcmFormat.deadline(32_000 * 50), 165)
  }

  func testTheHeaderSaysPlainSixteenKilohertzSound() {
    let header = PcmFormat.wavHeader(32_000)
    XCTAssertEqual(header.count, 44)
    XCTAssertEqual(String(decoding: header[0..<4], as: UTF8.self), "RIFF")
    XCTAssertEqual(String(decoding: header[8..<16], as: UTF8.self), "WAVEfmt ")
    let u32 = { (at: Int) in header[at..<at + 4].enumerated().reduce(UInt32(0)) {
      $0 | UInt32($1.element) << (8 * UInt32($1.offset))
    } }
    XCTAssertEqual(u32(4), 36 + 32_000)
    XCTAssertEqual(u32(24), 16_000)
    XCTAssertEqual(u32(40), 32_000)
    XCTAssertEqual(header[22], 1, "one channel")
    XCTAssertEqual(header[34], 16, "16 bits")
  }

  func testThePermissionReadsAsTauriNamesIt() {
    XCTAssertEqual(SpeechSeam.access(.authorized), "granted")
    XCTAssertEqual(SpeechSeam.access(.notDetermined), "prompt")
    XCTAssertEqual(SpeechSeam.access(.denied), "denied")
    XCTAssertEqual(SpeechSeam.access(.restricted), "denied")
  }

  func testOnlyAReaderOnThePhoneReads() {
    XCTAssertNil(SpeechSeam.refusal(authorized: true, recognizerExists: true, onDevice: true))
    XCTAssertEqual(
      SpeechSeam.refusal(authorized: true, recognizerExists: true, onDevice: false),
      .languageUnavailable)
    XCTAssertEqual(
      SpeechSeam.refusal(authorized: true, recognizerExists: false, onDevice: false), .notSupported)
    XCTAssertEqual(
      SpeechSeam.refusal(authorized: false, recognizerExists: true, onDevice: true), .noPermission)
  }

  func testNoSpeechIsNothingHeard() {
    XCTAssertEqual(SpeechSeam.outcome(errorDomain: "kAFAssistantErrorDomain", code: 1110), .nothingHeard)
    XCTAssertEqual(SpeechSeam.outcome(errorDomain: "kAFAssistantErrorDomain", code: 1101), .failed)
    XCTAssertEqual(SpeechSeam.outcome(errorDomain: NSCocoaErrorDomain, code: 1110), .failed)
  }

  func testBlankWordsAreNothingHeard() {
    XCTAssertEqual(SpeechSeam.words("  ").0, .nothingHeard)
    XCTAssertEqual(SpeechSeam.words(nil).0, .nothingHeard)
    let (outcome, text) = SpeechSeam.words(" The purple giraffe ")
    XCTAssertEqual(outcome, .text)
    XCTAssertEqual(text, "The purple giraffe")
  }

  func testOutcomesSpeakTheWordsTheAppReads() {
    XCTAssertEqual(Outcome.languageUnavailable.rawValue, "languageUnavailable")
    XCTAssertEqual(Outcome.nothingHeard.rawValue, "nothingHeard")
  }
}
