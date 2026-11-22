package app.nocturne.speech

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SpeechSeamTest {
    @Test
    fun a_second_of_sound_is_thirty_two_thousand_bytes() {
        assertEquals(32_000, PcmFormat.BYTES_PER_SECOND)
        assertEquals(1_500, PcmFormat.durationMs(48_000))
    }

    @Test
    fun a_half_sample_is_not_whole() {
        assertTrue(PcmFormat.isWhole(32_000))
        assertFalse(PcmFormat.isWhole(32_001))
    }

    @Test
    fun a_missing_language_pack_waits_and_an_unknown_language_is_not_supported() {
        assertEquals(Outcome.LanguageUnavailable, SpeechErrors.outcome(13))
        assertEquals(Outcome.NotSupported, SpeechErrors.outcome(12))
    }

    @Test
    fun the_microphone_prompt_refused_reads_as_no_permission() {
        assertEquals(Outcome.NoPermission, SpeechErrors.outcome(9))
    }

    @Test
    fun silence_is_nothing_heard_and_the_rest_is_a_failure() {
        assertEquals(Outcome.NothingHeard, SpeechErrors.outcome(7))
        assertEquals(Outcome.NothingHeard, SpeechErrors.outcome(6))
        for (code in listOf(1, 2, 3, 4, 5, 8, 10, 11, 14, 99)) {
            assertEquals(Outcome.Failed, SpeechErrors.outcome(code))
        }
    }

    @Test
    fun the_reader_built_for_the_phone_comes_first() {
        assertEquals(Reader.OnDevice, Readers.choose(listOf("en-US"), listOf("en-US"), "en-US"))
        assertEquals(Reader.DefaultOffline, Readers.choose(listOf(), listOf("en-AU", "en-US"), "en-us"))
        assertEquals(Reader.DefaultOffline, Readers.choose(null, listOf("en-US"), "en-US"))
    }

    @Test
    fun a_language_without_a_pack_has_no_reader() {
        assertNull(Readers.choose(listOf(), listOf("en-US"), "en-GB"))
        assertNull(Readers.choose(null, null, "en-US"))
    }

    @Test
    fun a_revised_stretch_is_one_stretch() {
        val t = Transcript()
        for (p in listOf("The", "The purple", "The purple giraffe board", "The purple giraffe bought seven lemons")) t.partial(p)
        assertEquals("The purple giraffe bought seven lemons", t.text())
    }

    @Test
    fun a_first_word_made_longer_is_still_the_same_stretch() {
        val t = Transcript()
        for (p in listOf("Cop", "Copper", "Copper candle")) t.partial(p)
        assertEquals("Copper candle", t.text())
    }

    @Test
    fun a_new_first_word_closes_the_stretch_before() {
        val t = Transcript()
        for (p in listOf("Copper candle whistled while", "Cross", "Crossed the river")) t.partial(p)
        assertEquals("Copper candle whistled while Crossed the river", t.text())
    }

    @Test
    fun empty_results_add_nothing() {
        val t = Transcript()
        t.partial("")
        t.partial("  ")
        assertEquals("", t.text())
    }

    @Test
    fun a_read_gets_time_for_the_sound() {
        assertEquals(15_000 + 3 * 3_600, Reading.deadlineMs(3_600))
    }

    @Test
    fun the_check_ends_before_a_microphone_read_would() {
        assertTrue(Probe.DEADLINE_MS < 5_000)
        assertTrue(Probe.HOLD_MS + Probe.AFTER_CLOSE_MS < Probe.DEADLINE_MS)
    }

    @Test
    fun a_no_match_just_after_the_stream_closes_proves_the_file_was_taken() {
        assertEquals(Outcome.NothingHeard, Probe.onError(7, 185))
        assertEquals(Outcome.NothingHeard, Probe.onError(7, Probe.AFTER_CLOSE_MS))
    }

    @Test
    fun a_reader_that_gives_up_before_or_long_after_the_close_is_on_the_microphone() {
        assertEquals(Outcome.ListensToMic, Probe.onError(7, null))
        assertEquals(Outcome.ListensToMic, Probe.onError(7, Probe.AFTER_CLOSE_MS + 1))
        assertEquals(Outcome.ListensToMic, Probe.onError(6, null))
        assertEquals(Outcome.ListensToMic, Probe.onError(6, 100))
    }

    @Test
    fun words_from_nothing_come_from_the_room() {
        assertEquals(Outcome.ListensToMic, Probe.onWords())
    }

    @Test
    fun a_busy_or_failing_reader_in_the_check_is_a_failure_not_a_microphone() {
        for (code in listOf(1, 2, 3, 4, 5, 8, 10, 11, 14)) {
            assertEquals(Outcome.Failed, Probe.onError(code, 100))
        }
        assertEquals(Outcome.NoPermission, Probe.onError(9, null))
        assertEquals(Outcome.LanguageUnavailable, Probe.onError(13, null))
        assertEquals(Outcome.NotSupported, Probe.onError(12, 100))
    }

    @Test
    fun an_answer_before_the_first_bytes_is_never_words() {
        assertEquals(Verdict(Outcome.ListensToMic), Reading.onError(7, false, ""))
        assertEquals(Verdict(Outcome.ListensToMic), Reading.onError(6, false, "hello"))
        assertEquals(Verdict(Outcome.ListensToMic), Reading.onResults(false, "hello"))
    }

    @Test
    fun a_quiet_end_with_words_gathered_keeps_the_words() {
        assertEquals(Verdict(Outcome.Text, "hello there"), Reading.onError(7, true, "hello there"))
        assertEquals(Verdict(Outcome.Text, "hello there"), Reading.onError(6, true, "hello there"))
        assertEquals(Verdict(Outcome.NothingHeard), Reading.onError(7, true, ""))
    }

    @Test
    fun a_read_that_fails_after_words_waits_rather_than_keeping_half_a_note() {
        assertEquals(Verdict(Outcome.Failed), Reading.onError(2, true, "hello"))
        assertEquals(Verdict(Outcome.NoPermission), Reading.onError(9, true, "hello"))
    }

    @Test
    fun the_final_answer_gives_the_gathered_words_or_nothing() {
        assertEquals(Verdict(Outcome.Text, "hello"), Reading.onResults(true, "hello"))
        assertEquals(Verdict(Outcome.NothingHeard), Reading.onResults(true, ""))
    }

    @Test
    fun a_short_first_word_does_not_hide_the_stretch_before() {
        val t = Transcript()
        for (p in listOf("I am here today", "In the morning")) t.partial(p)
        assertEquals("I am here today In the morning", t.text())
    }

    @Test
    fun a_fragment_of_a_first_word_is_revised_in_place() {
        val t = Transcript()
        for (p in listOf("Co", "Copper", "Copper candle")) t.partial(p)
        assertEquals("Copper candle", t.text())
    }

    @Test
    fun a_stretch_the_reader_revises_to_hold_the_earlier_one_stays_one_stretch() {
        val t = Transcript()
        for (p in listOf("Copper", "A copper candle whistled")) t.partial(p)
        assertEquals("A copper candle whistled", t.text())
    }

    @Test
    fun a_much_shorter_result_with_the_same_first_word_is_a_new_stretch() {
        val t = Transcript()
        for (p in listOf("The cat sat on the mat", "The dog")) t.partial(p)
        assertEquals("The cat sat on the mat The dog", t.text())
    }

    @Test
    fun a_revision_that_drops_one_word_stays_in_the_stretch() {
        val t = Transcript()
        for (p in listOf("purple giraffe", "purple", "purple giraffeine on")) t.partial(p)
        assertEquals("purple giraffeine on", t.text())
    }

    @Test
    fun a_final_answer_holding_the_whole_text_replaces_the_stretches_gathered() {
        val t = Transcript()
        for (p in listOf("Copper candle whistled while", "Cross", "Crossed the river")) t.partial(p)
        t.result("Copper candle whistled while crossed the river")
        assertEquals("Copper candle whistled while crossed the river", t.text())
    }

    @Test
    fun a_final_answer_that_is_the_last_stretch_adds_nothing_twice() {
        val t = Transcript()
        for (p in listOf("Copper candle", "Crossed the river")) t.partial(p)
        t.result("Crossed the river")
        assertEquals("Copper candle Crossed the river", t.text())
    }
}
