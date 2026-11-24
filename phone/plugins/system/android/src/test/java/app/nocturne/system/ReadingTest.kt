package app.nocturne.system

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class ReadingTest {
    @Before
    fun clean() {
        Reading.stopAsked = false
    }

    @Test
    fun theCoreSendsTwoStates() {
        assertEquals(Reading.State.Reading, Reading.state("reading"))
        assertEquals(Reading.State.Ended, Reading.state("ended"))
        assertNull(Reading.state("paused"))
        assertNull(Reading.state(null))
    }

    @Test
    fun aStopIsToldOnce() {
        assertFalse(Reading.answer(true).stop)
        Reading.stopAsked = true
        assertTrue(Reading.answer(true).stop)
        assertFalse(Reading.answer(true).stop)
    }

    @Test
    fun theAnswerCarriesWhetherNotificationsShow() {
        assertTrue(Reading.answer(true).notes)
        assertFalse(Reading.answer(false).notes)
    }

    @Test
    fun aLineIsLeftOnlyWhenThereIsOneAndNocturneIsAway() {
        assertTrue(Reading.leaveLine("Stopped", false))
        assertFalse(Reading.leaveLine("Stopped", true))
        assertFalse(Reading.leaveLine(null, false))
    }

    @Test
    fun theQuestionIsAskedOnceAndOnlyWhereAndroidAsks() {
        assertTrue(Reading.shouldAsk(33, false, false))
        assertTrue(Reading.shouldAsk(36, false, false))
        assertFalse(Reading.shouldAsk(32, false, false))
        assertFalse(Reading.shouldAsk(33, true, false))
        assertFalse(Reading.shouldAsk(33, false, true))
    }
}
