//! Read now: the person's choice to read on battery. It lets the reader run while the phone is
//! not charging; the page and the reflections still wait for the charger. It is set only by the
//! person, held in memory only, and ends at every stop below, each told with why.

use serde::{Deserialize, Serialize};

/// Read now stops at this battery level or below, and is not offered.
pub const LOW: u8 = 20;
/// Read anyway stops here, and neither is offered until the battery is above it.
pub const FLOOR: u8 = 5;

/// Which the person chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReadKind {
    /// Read now: down to [`LOW`], never with battery saving on.
    Now,
    /// Read anyway: down to [`FLOOR`], battery saving or not.
    Anyway,
}

/// Why reading on battery ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stopped {
    /// Everything kept is read.
    Done,
    /// The charger went in: reading carries on by itself.
    Charging,
    /// The battery reached [`LOW`].
    Low,
    /// Battery saving turned on.
    Saver,
    /// The battery reached [`FLOOR`].
    Floor,
    /// The month's limit is reached.
    Limit,
    /// The service is not answering, refused the key, wants credit, or this phone is offline.
    Trouble,
    /// The person pressed Stop, here or where the phone shows the reading.
    Person,
}

/// A stop and how many were read by then.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Stop {
    pub why: Stopped,
    pub done: u32,
}

/// Why Read now or Read anyway did not start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Refused {
    Off,
    /// At or below [`FLOOR`].
    Floor,
    /// Read now at or below [`LOW`]: Read anyway is offered.
    Low,
    /// Read now with battery saving on: Read anyway is offered.
    Saver,
    Limit,
    /// The key was refused or there is no credit.
    Trouble,
    Offline,
    /// The one look found the service still not answering.
    NotAnswering,
}

/// What the person chose, and what came of it, this run.
#[derive(Debug, Default)]
pub(super) struct ReadNow {
    pub on: Option<ReadKind>,
    pub stopped: Option<Stop>,
    /// iPhone paused it while the person was away, after this many.
    pub away: Option<u32>,
    /// The last Read now looked once more and found the service still not answering.
    pub still_not: bool,
    /// Android's notifications are off for Nocturne, so progress shows only in the app.
    pub quiet: bool,
    /// The words the phone shows it with, as the screens last gave them.
    pub words: ReadWords,
}

impl ReadNow {
    /// Starts `kind`: what an earlier one left is forgotten.
    pub fn start(&mut self, kind: ReadKind) {
        *self = ReadNow {
            on: Some(kind),
            quiet: self.quiet,
            words: std::mem::take(&mut self.words),
            ..ReadNow::default()
        };
    }

    /// Ends it, when on, for `why`. True when it was on.
    pub fn end(&mut self, why: Stopped, done: u32) -> bool {
        if self.on.take().is_none() {
            return false;
        }
        self.stopped = Some(Stop { why, done });
        self.away = None;
        true
    }
}

/// Whether the battery stops `kind`: at `percent` (unknown: no stop) with battery saving `saver`.
pub fn battery_stop(kind: ReadKind, percent: Option<u8>, saver: bool) -> Option<Stopped> {
    match (kind, percent) {
        (_, Some(p)) if p <= FLOOR => Some(Stopped::Floor),
        (ReadKind::Now, Some(p)) if p <= LOW => Some(Stopped::Low),
        (ReadKind::Now, _) if saver => Some(Stopped::Saver),
        _ => None,
    }
}

/// The words of the reading's notification and of iPhone's own progress, as the screens give
/// them. `{done}` and `{count}` are filled here. None names a kept thing, a question or a host.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReadWords {
    pub channel: String,
    pub channel_about: String,
    pub title: String,
    /// "{done} of {count}".
    pub progress: String,
    pub stop: String,
    pub done: String,
    pub low_title: String,
    pub saver_title: String,
    /// "{done} read. Open Nocturne to read anyway."
    pub anyway_text: String,
    pub floor_title: String,
    pub floor_text: String,
    pub stopped_title: String,
    pub stopped_text: String,
    pub paused_title: String,
    /// "iPhone paused it when you left Nocturne, after {done} of {count}. …"
    pub paused_text: String,
}

/// `words` with `{done}` and `{count}` filled.
pub fn fill(words: &str, done: u32, count: u32) -> String {
    words
        .replace("{done}", &done.to_string())
        .replace("{count}", &count.to_string())
}

/// The quiet line left when reading on battery ends while the person is elsewhere: a title
/// and its line, or none.
pub fn ended_line(words: &ReadWords, why: Stopped, done: u32) -> Option<(String, String)> {
    let (title, text) = match why {
        Stopped::Done => (&words.title, &words.done),
        Stopped::Low => (&words.low_title, &words.anyway_text),
        Stopped::Saver => (&words.saver_title, &words.anyway_text),
        Stopped::Floor => (&words.floor_title, &words.floor_text),
        Stopped::Limit | Stopped::Trouble => (&words.stopped_title, &words.stopped_text),
        Stopped::Charging | Stopped::Person => return None,
    };
    Some((fill(title, done, done), fill(text, done, done)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_battery_stops_read_now_at_twenty_and_read_anyway_at_five() {
        use ReadKind::{Anyway, Now};
        assert_eq!(battery_stop(Now, Some(21), false), None);
        assert_eq!(battery_stop(Now, Some(20), false), Some(Stopped::Low));
        assert_eq!(battery_stop(Now, Some(80), true), Some(Stopped::Saver));
        assert_eq!(battery_stop(Now, Some(5), true), Some(Stopped::Floor));
        assert_eq!(battery_stop(Now, None, false), None);
        assert_eq!(battery_stop(Anyway, Some(20), true), None);
        assert_eq!(battery_stop(Anyway, Some(6), false), None);
        assert_eq!(battery_stop(Anyway, Some(5), false), Some(Stopped::Floor));
    }

    #[test]
    fn a_new_read_now_forgets_the_last_stop_and_pause() {
        let mut r = ReadNow::default();
        assert!(!r.end(Stopped::Person, 0));
        r.start(ReadKind::Now);
        r.away = Some(4);
        assert!(r.end(Stopped::Low, 7));
        assert_eq!(r.on, None);
        assert_eq!(r.away, None);
        assert_eq!(
            r.stopped,
            Some(Stop {
                why: Stopped::Low,
                done: 7
            })
        );
        r.quiet = true;
        r.start(ReadKind::Anyway);
        assert_eq!(r.stopped, None);
        assert!(r.quiet);
    }

    #[test]
    fn the_lines_left_name_only_counts() {
        let words = ReadWords {
            title: "Reading what you kept".into(),
            done: "Everything you kept is read.".into(),
            low_title: "Read now stopped at 20% battery".into(),
            anyway_text: "{done} read. Open Nocturne to read anyway.".into(),
            stopped_title: "Read now stopped".into(),
            stopped_text: "{done} read. Open Nocturne to see why.".into(),
            ..ReadWords::default()
        };
        assert_eq!(
            ended_line(&words, Stopped::Low, 31),
            Some((
                "Read now stopped at 20% battery".into(),
                "31 read. Open Nocturne to read anyway.".into()
            ))
        );
        assert_eq!(
            ended_line(&words, Stopped::Trouble, 2).unwrap().1,
            "2 read. Open Nocturne to see why."
        );
        assert_eq!(ended_line(&words, Stopped::Person, 2), None);
        assert_eq!(ended_line(&words, Stopped::Charging, 2), None);
        assert_eq!(fill("{done} of {count}", 12, 89), "12 of 89");
    }
}
