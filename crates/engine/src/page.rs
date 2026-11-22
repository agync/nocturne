//! The morning page: composing, ranking, variants, the daily question and saved pages.
//!
//! `compose_page` only reads. The caller (the intelligence layer) may write a headline and dek
//! and then `save_page`, which stores a screen-edition snapshot with every line, private ones
//! included. Audio and print editions are filtered at read time against each memory's current
//! `private` flag, so marking something private takes effect on pages already saved.

use std::collections::{HashMap, HashSet};

use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use nocturne_contract::{
    Choice, Edition, JournalAsk, LineKind, MemoryView, NeedsYouEntry, PageLine, PageSummary,
    PageVariant, Period, PeriodKind, Question, QuestionChoice, QuestionKind, Receipt,
};
use rusqlite::{Connection, OptionalExtension, params};

use crate::Engine;
use crate::audio::{drop_audio_rows, shred_all};
use crate::day::{PrevPage, Window, window};
use crate::error::{Error, Result};
use crate::journal::Ctx;
use crate::journal::question as journal_question;
use crate::lifecycle::mark_used_tx;
use crate::memory::{confirm_tx, is_journal_chain, is_private_chain, needs_you_on};
use crate::rows::{MemRow, mem_view, query_mems};
use crate::settings::{parse_page_time, read_settings, set_page_time};
use crate::util::{enum_str, fmt_ts, new_id, opt_enum, parse_enum, parse_ts};

/// The page shows about this many needs-you and changed lines; the rest are counted.
pub const PAGE_LINE_CAP: usize = 12;
const LET_GO_CAP: usize = 3;
const UPCOMING_CAP: usize = 4;
/// More items than this in a window makes a heavy page.
pub const HEAVY_ITEMS: u32 = 40;
const QUESTION_GAP_DAYS: i64 = 14;
const UPCOMING_DAYS: i32 = 6;

/// The headline used when no mind has written one.
pub(crate) fn template_headline(date: Date) -> String {
    date.strftime("%A %-d %B").to_string()
}

pub(crate) fn template_dek(kept: u32, needs: u32, let_go: u32) -> String {
    let things = if kept == 1 { "thing" } else { "things" };
    let need = if needs == 1 { "needs" } else { "need" };
    let were = if let_go == 1 { "was" } else { "were" };
    format!(
        "Last night Nocturne read {kept} {things} you kept. {needs} {need} you. {let_go} {were} let go."
    )
}

/// The phone edition's dek: no kept or let-go totals, which count what stays home.
pub(crate) fn phone_dek(needs: u32) -> String {
    let need = if needs == 1 { "needs" } else { "need" };
    format!("{needs} {need} you.")
}

fn bold(sentence: &str, object: &str) -> String {
    if !object.is_empty() && sentence.contains(object) {
        sentence.replacen(object, &format!("**{object}**"), 1)
    } else {
        sentence.to_string()
    }
}

fn local_date(ts: &str, tz: &TimeZone) -> Option<String> {
    parse_ts(ts)
        .ok()
        .map(|t| t.to_zoned(tz.clone()).date().to_string())
}

fn base_line(kind: LineKind, text: String) -> PageLine {
    PageLine {
        id: new_id("pln"),
        kind,
        text,
        memory_ids: Vec::new(),
        receipts: Vec::new(),
        choices: Vec::new(),
        conflict_id: None,
        subject: None,
        done: false,
        forgotten: false,
        on: None,
        last_used: None,
    }
}

fn memory_line(kind: LineKind, m: &MemoryView) -> PageLine {
    let mut line = base_line(kind, bold(&m.sentence, &m.object));
    line.memory_ids = vec![m.id.clone()];
    line.receipts = m.receipts.clone();
    line.subject = Some(m.subject.clone());
    line.done = false;
    line
}

struct Planned {
    line: PageLine,
    private: bool,
}

/// Private now, in any version of the memory. Fails closed: an unknown id counts as private.
fn is_private(conn: &Connection, id: &str) -> Result<bool> {
    is_private_chain(conn, id)
}

fn prev_page(conn: &Connection, day: Date) -> Result<Option<PrevPage>> {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT date, window_end FROM pages WHERE date < ?1 ORDER BY date DESC LIMIT 1",
            [day.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    row.map(|(d, e)| {
        Ok(PrevPage {
            date: d.parse()?,
            end: parse_ts(&e)?,
        })
    })
    .transpose()
}

fn window_for(conn: &Connection, day: Date, tz: &TimeZone) -> Result<Window> {
    let hm = parse_page_time(&read_settings(conn)?.page_time)?;
    let earliest: Option<String> =
        conn.query_row("SELECT MIN(kept_at) FROM items", [], |r| r.get(0))?;
    let earliest = earliest.map(|e| parse_ts(&e)).transpose()?;
    window(day, hm, tz, prev_page(conn, day)?, earliest)
}

fn page_number(conn: &Connection, day: Date) -> Result<u32> {
    let existing: Option<u32> = conn
        .query_row(
            "SELECT number FROM pages WHERE date = ?1",
            [day.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    match existing {
        Some(n) => Ok(n),
        None => Ok(conn.query_row(
            "SELECT COUNT(*) + 1 FROM pages WHERE date < ?1",
            [day.to_string()],
            |r| r.get(0),
        )?),
    }
}

fn first_receipt(m: &MemoryView) -> Option<Receipt> {
    m.receipts.first().cloned()
}

fn question_for(
    conn: &Connection,
    day: Date,
    number: u32,
    edition: Edition,
    now: Timestamp,
    tz: &TimeZone,
    page_time: &str,
) -> Result<Option<Question>> {
    let saved = saved_question(conn, &day.to_string())?;
    let last_asked = |id: &str| -> Result<Option<String>> {
        Ok(conn.query_row(
            "SELECT MAX(asked_at) FROM questions WHERE memory_id = ?1 AND page_date != ?2",
            params![id, day.to_string()],
            |r| r.get(0),
        )?)
    };
    if let Some(QuestionRow {
        memory_id,
        kind,
        text,
        answer,
        journal,
    }) = saved
    {
        let answered: Option<QuestionChoice> = opt_enum(answer)?;
        let kind: QuestionKind = parse_enum(&kind)?;
        return Ok(match memory_id {
            None if kind == QuestionKind::Journal => None,
            None => Some(Question {
                kind,
                text,
                memory_id: None,
                receipt: None,
                last_asked: None,
                answered,
                journal: None,
            }),
            Some(id) => {
                let Some(row) = crate::rows::load_mem(conn, &id)? else {
                    return Ok(None);
                };
                let view = mem_view(conn, &row)?;
                if edition != Edition::Screen && view.private {
                    return Ok(None);
                }
                let journal =
                    journal_question::read_ask(conn, &day.to_string(), journal.as_deref())?;
                // A journal question quotes its block: with the block gone there is nothing to
                // show, and no other source stands in for it.
                let receipt = match &journal {
                    Some(ask) => match view.receipts.iter().find(|r| r.item_id == ask.item_id) {
                        Some(r) => Some(r.clone()),
                        None => return Ok(None),
                    },
                    None => first_receipt(&view),
                };
                Some(Question {
                    kind,
                    text,
                    memory_id: Some(id.clone()),
                    receipt,
                    last_asked: last_asked(&id)?,
                    answered,
                    journal,
                })
            }
        });
    }
    if number == 1 {
        return Ok(Some(Question {
            kind: QuestionKind::PageTime,
            text: page_time.to_string(),
            memory_id: None,
            receipt: None,
            last_asked: None,
            answered: None,
            journal: None,
        }));
    }
    // A day the rule finds a thing to ask about, and the week allows it, takes the one slot.
    let ctx = Ctx {
        tz: tz.clone(),
        page_hm: parse_page_time(page_time)?,
        now,
    };
    if let Some(chosen) = journal_question::pick(conn, &ctx, day)? {
        let view = mem_view(conn, &chosen.memory)?;
        let receipt = view
            .receipts
            .iter()
            .find(|r| r.item_id == chosen.ask.item_id)
            .cloned();
        return Ok(Some(Question {
            kind: QuestionKind::Journal,
            text: chosen.sentence,
            memory_id: Some(view.id),
            receipt,
            last_asked: None,
            answered: None,
            journal: Some(chosen.ask),
        }));
    }
    // Eligible: active, not keep-forever, not asked in the last 14 days, and never one the
    // journal asked about. What a memory is about never keeps it out.
    let cutoff = now
        .checked_sub(SignedDuration::from_hours(24 * QUESTION_GAP_DAYS))
        .map_err(Error::from)?;
    let mut rows: Vec<MemRow> = query_mems(
        conn,
        "m.superseded_at IS NULL AND m.status = 'active' AND m.keep_forever = 0 \
         AND m.id NOT IN (SELECT memory_id FROM questions \
                          WHERE memory_id IS NOT NULL AND asked_at > ?1 AND page_date != ?2) \
         AND m.id NOT IN (SELECT memory_id FROM questions \
                          WHERE memory_id IS NOT NULL AND kind = 'journal')",
        params![fmt_ts(cutoff), day.to_string()],
    )?;
    rows.retain(|m| edition == Edition::Screen || !m.private);
    // Prefer memories with one source, then ones the person has not confirmed, oldest first.
    rows.sort_by(|a, b| {
        ((a.sources != 1), a.confirmed, &a.recorded_at, &a.id).cmp(&(
            (b.sources != 1),
            b.confirmed,
            &b.recorded_at,
            &b.id,
        ))
    });
    let Some(m) = rows.first() else {
        return Ok(None);
    };
    let view = mem_view(conn, m)?;
    Ok(Some(Question {
        kind: QuestionKind::YesNo,
        text: view.sentence.clone(),
        memory_id: Some(view.id.clone()),
        receipt: first_receipt(&view),
        last_asked: last_asked(&view.id)?,
        answered: None,
        journal: None,
    }))
}

fn compose(
    conn: &Connection,
    now: Timestamp,
    tz: &TimeZone,
    day: Date,
    edition: Edition,
    end_at: Option<Timestamp>,
) -> Result<nocturne_contract::PageView> {
    let settings = read_settings(conn)?;
    let mut w = window_for(conn, day, tz)?;
    if let Some(end) = end_at {
        w.end = end.max(w.start);
    }
    let (start, end) = (fmt_ts(w.start), fmt_ts(w.end));
    let number = page_number(conn, day)?;
    let screen = edition == Edition::Screen;
    let mut skipped = 0u32;

    let kept_count: u32 = conn.query_row(
        "SELECT COUNT(*) FROM items WHERE kept_at >= ?1 AND kept_at < ?2",
        [&start, &end],
        |r| r.get(0),
    )?;
    let pending: u32 = conn.query_row(
        "SELECT COUNT(*) FROM items WHERE kept_at >= ?1 AND kept_at < ?2 \
         AND status IN ('kept', 'reading')",
        [&start, &end],
        |r| r.get(0),
    )?;

    // Needs you, already ranked by `needs_you_on` (due soonest, promises, which, unconfirmed).
    let mut needs_ids: HashSet<String> = HashSet::new();
    let mut needs: Vec<Planned> = Vec::new();
    let mut name_questions = 0u32;
    for entry in needs_you_on(conn, day)? {
        let planned = match entry {
            NeedsYouEntry::Due { memory, due, .. } => {
                let mut line = memory_line(LineKind::Due, &memory);
                line.on = Some(due).filter(|d| !d.is_empty());
                needs_ids.insert(memory.id.clone());
                Planned {
                    private: memory.private,
                    line,
                }
            }
            NeedsYouEntry::Promise { memory, .. } => {
                needs_ids.insert(memory.id.clone());
                Planned {
                    private: memory.private,
                    line: memory_line(LineKind::Promise, &memory),
                }
            }
            NeedsYouEntry::Unconfirmed { memory, .. } => {
                needs_ids.insert(memory.id.clone());
                Planned {
                    private: memory.private,
                    line: memory_line(LineKind::Unconfirmed, &memory),
                }
            }
            NeedsYouEntry::Which {
                conflict_id,
                title,
                options,
            } => {
                let mut line = base_line(LineKind::Which, title);
                line.conflict_id = Some(conflict_id);
                if let Some(first) = options.first() {
                    line.subject = conn
                        .query_row(
                            "SELECT subject FROM memories WHERE id = ?1",
                            [&first.memory_id],
                            |r| r.get(0),
                        )
                        .optional()?;
                }
                let mut private = false;
                for o in options {
                    private |= is_private(conn, &o.memory_id)?;
                    needs_ids.insert(o.memory_id.clone());
                    line.memory_ids.push(o.memory_id.clone());
                    line.receipts.push(o.receipt.clone());
                    line.choices.push(Choice {
                        memory_id: o.memory_id,
                        text: o.text,
                        receipt: o.receipt,
                    });
                }
                Planned { line, private }
            }
            // A name question lives in Needs you and the journal, not on the page. It is still
            // waiting, so the page counts it.
            NeedsYouEntry::SameName { .. } => {
                name_questions += 1;
                continue;
            }
        };
        if screen || !planned.private {
            needs.push(planned);
        } else {
            skipped += 1;
        }
    }

    // What changed and what was learned in the window, changes first, then by how often the
    // subject is used.
    let freq: HashMap<String, u32> = {
        let mut stmt = conn.prepare(
            "SELECT m.subject_key, COUNT(*) FROM memory_sources s JOIN memories m ON m.id = s.memory_id \
             WHERE m.superseded_at IS NULL GROUP BY m.subject_key",
        )?;
        stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    let fresh = query_mems(
        conn,
        "m.recorded_at >= ?1 AND m.recorded_at < ?2 AND m.superseded_at IS NULL \
         AND m.status NOT IN ('unconfirmed', 'letGo')",
        [&start, &end],
    )?;
    let mut changes: Vec<(u32, String, String, bool, Planned)> = Vec::new();
    for m in fresh.iter().filter(|m| !needs_ids.contains(&m.id)) {
        let view = mem_view(conn, m)?;
        if !screen && view.private {
            skipped += 1;
            continue;
        }
        let kind = if m.was.is_some() {
            LineKind::Changed
        } else {
            LineKind::Learned
        };
        changes.push((
            freq.get(&m.subject_key).copied().unwrap_or(0),
            m.recorded_at.clone(),
            m.id.clone(),
            m.was.is_some(),
            Planned {
                private: view.private,
                line: memory_line(kind, &view),
            },
        ));
    }
    // Changes first, so the cap below cuts what was merely learned before what changed.
    changes.sort_by(|a, b| {
        b.3.cmp(&a.3)
            .then_with(|| b.0.cmp(&a.0))
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| a.2.cmp(&b.2))
    });

    // The cap: about twelve lines of needs you and changed together; neither starves the other.
    let (ny_total, ch_total) = (needs.len(), changes.len());
    let (ny_show, ch_show) = if ny_total + ch_total <= PAGE_LINE_CAP {
        (ny_total, ch_total)
    } else {
        let ny = ny_total.min(PAGE_LINE_CAP - ch_total.min(4));
        (ny, ch_total.min(PAGE_LINE_CAP - ny))
    };
    needs.truncate(ny_show);
    changes.truncate(ch_show);
    let mut changed = Vec::new();
    let mut learned = Vec::new();
    for (_, _, _, is_changed, p) in changes {
        if is_changed {
            changed.push(p.line)
        } else {
            learned.push(p.line)
        }
    }

    // Let go during the window.
    let gone = query_mems(
        conn,
        "m.let_go_at >= ?1 AND m.let_go_at < ?2 AND m.superseded_at IS NULL \
         ORDER BY m.let_go_at DESC, m.id",
        [&start, &end],
    )?;
    let mut let_go: Vec<PageLine> = Vec::new();
    let mut let_go_count = 0u32;
    for m in &gone {
        let view = mem_view(conn, m)?;
        if !screen && view.private {
            skipped += 1;
            continue;
        }
        let_go_count += 1;
        if let_go.len() < LET_GO_CAP {
            let mut line = memory_line(LineKind::LetGo, &view);
            line.last_used = local_date(m.last_used.as_deref().unwrap_or(&m.recorded_at), tz);
            let_go.push(line);
        }
    }

    // Upcoming this week: scheduled things not already above.
    let until = day
        .checked_add(jiff::Span::new().days(UPCOMING_DAYS))
        .map_err(Error::from)?;
    let soon = query_mems(
        conn,
        "m.predicate = 'scheduled' AND m.superseded_at IS NULL AND m.done = 0 \
         AND m.status IN ('active', 'fading') AND COALESCE(m.due, m.valid_from) >= ?1 \
         AND COALESCE(m.due, m.valid_from) <= ?2 \
         ORDER BY COALESCE(m.due, m.valid_from), m.id",
        [day.to_string(), until.to_string()],
    )?;
    let mut upcoming: Vec<PageLine> = Vec::new();
    for m in soon.iter().filter(|m| !needs_ids.contains(&m.id)) {
        let view = mem_view(conn, m)?;
        if !screen && view.private {
            skipped += 1;
            continue;
        }
        if upcoming.len() < UPCOMING_CAP {
            let mut line = memory_line(LineKind::Upcoming, &view);
            line.on = m.due.clone().or_else(|| m.valid_from.clone());
            upcoming.push(line);
        }
    }

    let question = question_for(conn, day, number, edition, now, tz, &settings.page_time)?;

    // A page composed up to an explicit end is the one to be saved: it never claims to be
    // still being written (what is unread is counted in `pending`).
    let variant = if pending > 0 && end_at.is_none() {
        PageVariant::BeingWritten
    } else if w.catch_up_days >= 1 {
        PageVariant::CatchUp
    } else if number == 1 {
        PageVariant::First
    } else if kept_count > HEAVY_ITEMS {
        PageVariant::Heavy
    } else if kept_count == 0 {
        PageVariant::Quiet
    } else {
        PageVariant::Normal
    };

    let needs_total = ny_total as u32 + name_questions;
    Ok(nocturne_contract::PageView {
        date: day.to_string(),
        page_number: number,
        edition,
        variant,
        headline: template_headline(day),
        dek: template_dek(kept_count, needs_total, let_go_count),
        needs_you: needs.into_iter().map(|p| p.line).collect(),
        changed,
        let_go,
        upcoming,
        learned,
        question,
        more_needs_you: (ny_total - ny_show) as u32 + name_questions,
        more_changed: (ch_total - ch_show) as u32,
        kept_count,
        changes_count: ch_total as u32,
        let_go_count,
        catch_up_days: w.catch_up_days,
        private_skipped: skipped,
        pending,
        tidied_from: None,
        tidied_to: None,
        update: None,
        journal_line: None,
        journal_skipped: None,
    })
}

impl Engine {
    /// Compose the page for `day`. Reads only: nothing is saved and nothing counts as used.
    pub fn compose_page(&self, day: Date, edition: Edition) -> Result<nocturne_contract::PageView> {
        let now = self.clock().now();
        let tz = self.clock().tz();
        self.read(|conn| {
            let mut page = compose(conn, now, &tz, day, edition, None)?;
            phone_only(conn, &mut page)?;
            Ok(page)
        })
    }

    /// Compose the page for `day` over the window ending at `end` instead of the page time
    /// (clamped to the window's start). The tidy composes and saves with the same `end`, so
    /// what the page counted and what it covers agree. The variant is never `beingWritten`;
    /// unread items in the window are counted in `pending`.
    pub fn compose_page_until(
        &self,
        day: Date,
        end: Timestamp,
        edition: Edition,
    ) -> Result<nocturne_contract::PageView> {
        let now = self.clock().now();
        let tz = self.clock().tz();
        self.read(|conn| {
            let mut page = compose(conn, now, &tz, day, edition, Some(end))?;
            phone_only(conn, &mut page)?;
            Ok(page)
        })
    }

    /// Save a composed page (screen edition, every line) as the snapshot for its date. Cited
    /// memories count as used once; let-go lines do not, or the page would undo its own news.
    pub fn save_page(&self, page: &nocturne_contract::PageView) -> Result<()> {
        self.save_page_inner(page, None, false).map(|_| ())
    }

    /// Like `save_page`, with the window ending at `end` (the `end` the page was composed
    /// with by `compose_page_until`).
    pub fn save_page_until(
        &self,
        page: &nocturne_contract::PageView,
        end: Timestamp,
    ) -> Result<()> {
        self.save_page_inner(page, Some(end), false).map(|_| ())
    }

    /// Replace an already saved page once, when the backlog behind it has drained. Allowed only
    /// while the person has not opened the page (`page_opened`) and it has not been re-saved
    /// before. Returns whether it was saved.
    pub fn resave_page(&self, page: &nocturne_contract::PageView, end: Timestamp) -> Result<bool> {
        self.save_page_inner(page, Some(end), true)
    }

    /// The person opened the page for `day`. Only the first open is recorded. Returns true when
    /// this was the first.
    pub fn page_opened(&self, day: Date) -> Result<bool> {
        let now = fmt_ts(self.clock().now());
        self.write(|tx| {
            Ok(tx.execute(
                "UPDATE pages SET opened_at = ?2 WHERE date = ?1 AND opened_at IS NULL",
                params![day.to_string(), now],
            )? > 0)
        })
    }

    /// Whether the saved page for `day` could still be replaced by `resave_page`.
    pub fn can_resave_page(&self, day: Date) -> Result<bool> {
        self.read(|conn| {
            Ok(conn
                .query_row(
                    "SELECT opened_at IS NULL AND resaves < 1 FROM pages WHERE date = ?1",
                    [day.to_string()],
                    |r| r.get::<_, bool>(0),
                )
                .optional()?
                .unwrap_or(false))
        })
    }

    fn save_page_inner(
        &self,
        page: &nocturne_contract::PageView,
        until: Option<Timestamp>,
        resave: bool,
    ) -> Result<bool> {
        if page.edition != Edition::Screen {
            return Err(Error::invalid(
                "only the screen edition is saved: it holds every line",
            ));
        }
        let day: Date = page.date.parse()?;
        let tz = self.clock().tz();
        let now = fmt_ts(self.clock().now());
        let saved = self.write(|tx| {
            if resave {
                let allowed: bool = tx
                    .query_row(
                        "SELECT opened_at IS NULL AND resaves < 1 FROM pages WHERE date = ?1",
                        [&page.date],
                        |r| r.get(0),
                    )
                    .optional()?
                    .unwrap_or(false);
                if !allowed {
                    return Ok(None);
                }
            }
            // Anything that cached audio for this page is now stale.
            self.bump_audio();
            // A page composed before a forget finished may cite what is now gone or going.
            let stripped = strip_stale(tx, page)?;
            let page = &stripped;
            let w = window_for(tx, day, &tz)?;
            // A page saved before its time ends at now, so what is kept between now and the page
            // time belongs to the next page instead of falling between two.
            let end = until
                .unwrap_or_else(|| w.end.min(self.clock().now()))
                .max(w.start);
            let number = page_number(tx, day)?;
            let already: HashSet<String> = {
                let mut stmt = tx.prepare(
                    "SELECT DISTINCT pm.memory_id FROM page_line_memories pm \
                     JOIN page_lines l ON l.id = pm.line_id WHERE l.page_date = ?1",
                )?;
                stmt.query_map([&page.date], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?
            };
            tx.execute("DELETE FROM page_lines WHERE page_date = ?1", [&page.date])?;
            tx.execute(
                "INSERT INTO pages(date, number, window_start, window_end, headline, dek, variant, \
                 tidied_from, tidied_to, body) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
                 ON CONFLICT(date) DO UPDATE SET window_start = excluded.window_start, \
                 window_end = excluded.window_end, headline = excluded.headline, dek = excluded.dek, \
                 variant = excluded.variant, tidied_from = excluded.tidied_from, \
                 tidied_to = excluded.tidied_to, body = excluded.body",
                params![
                    page.date,
                    number,
                    fmt_ts(w.start),
                    fmt_ts(end),
                    page.headline,
                    page.dek,
                    enum_str(&page.variant),
                    page.tidied_from,
                    page.tidied_to,
                    serde_json::to_string(page)?,
                ],
            )?;
            let sections: [(&str, &Vec<PageLine>); 5] = [
                ("needsYou", &page.needs_you),
                ("changed", &page.changed),
                ("learned", &page.learned),
                ("upcoming", &page.upcoming),
                ("letGo", &page.let_go),
            ];
            let mut used: Vec<&str> = Vec::new();
            for (section, lines) in sections {
                for (ord, line) in lines.iter().enumerate() {
                    tx.execute(
                        "INSERT INTO page_lines(id, page_date, section, kind, ord, text, done, forgotten) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        params![
                            line.id,
                            page.date,
                            section,
                            enum_str(&line.kind),
                            ord as i64,
                            line.text,
                            line.done,
                            line.forgotten,
                        ],
                    )?;
                    for m in &line.memory_ids {
                        tx.execute(
                            "INSERT OR IGNORE INTO page_line_memories(line_id, memory_id) VALUES (?1, ?2)",
                            params![line.id, m],
                        )?;
                        if section != "letGo" && !already.contains(m) && !used.contains(&m.as_str()) {
                            used.push(m);
                        }
                    }
                }
            }
            for m in used {
                mark_used_tx(tx, m, &now)?;
            }
            if let Some(q) = &page.question {
                tx.execute(
                    "INSERT OR IGNORE INTO questions(page_date, memory_id, kind, text, asked_at, \
                     journal) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        page.date,
                        q.memory_id,
                        enum_str(&q.kind),
                        q.text,
                        now,
                        q.journal.as_ref().map(journal_question::ask_json)
                    ],
                )?;
            }
            if resave {
                tx.execute(
                    "UPDATE pages SET resaves = resaves + 1 WHERE date = ?1",
                    [&page.date],
                )?;
            }
            Ok(Some(drop_audio_rows(tx, std::slice::from_ref(&page.date))?))
        })?;
        let Some(files) = saved else {
            return Ok(false);
        };
        shred_all(files);
        Ok(true)
    }

    /// The date of the next page to write: today until today's page is saved, then tomorrow.
    pub fn next_page_date(&self) -> Result<Date> {
        let today: Date = self.today().parse()?;
        let saved: bool = self.read(|conn| {
            Ok(conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM pages WHERE date = ?1)",
                [today.to_string()],
                |r| r.get(0),
            )?)
        })?;
        if saved {
            Ok(today.tomorrow()?)
        } else {
            Ok(today)
        }
    }

    /// A saved page (screen edition), or the latest when `day` is `None`.
    pub fn page(&self, day: Option<Date>) -> Result<Option<nocturne_contract::PageView>> {
        self.read(|conn| load_page(conn, day))
    }

    /// A saved page for an edition. Every edition but the screen drops lines about memories
    /// that are private now, whatever they were when the page was saved.
    pub fn page_for(
        &self,
        day: Option<Date>,
        edition: Edition,
    ) -> Result<Option<nocturne_contract::PageView>> {
        self.read(|conn| edition_page(conn, day, edition))
    }

    /// Every saved page, newest first, as an edition sees it. Every edition but the screen
    /// takes the headline and the needs-you count from `page_for`: the template headline when a
    /// line is private now, and public needs-you lines only.
    pub fn pages_for(&self, edition: Edition) -> Result<Vec<PageSummary>> {
        if edition == Edition::Screen {
            return self.pages();
        }
        self.read(|conn| {
            let dates: Vec<String> = conn
                .prepare("SELECT date FROM pages ORDER BY date DESC")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            let mut out = Vec::with_capacity(dates.len());
            for date in dates {
                if let Some(page) = edition_page(conn, Some(date.parse()?), edition)? {
                    out.push(PageSummary {
                        needs_you: needs_you_count(&page),
                        date: page.date,
                        page_number: page.page_number,
                        headline: page.headline,
                    });
                }
            }
            Ok(out)
        })
    }

    /// Every saved page, newest first.
    pub fn pages(&self) -> Result<Vec<PageSummary>> {
        self.read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT p.date, p.number, p.headline, \
                 (SELECT COUNT(*) FROM page_lines l WHERE l.page_date = p.date \
                    AND l.section = 'needsYou' AND l.forgotten = 0) \
                 FROM pages p ORDER BY p.date DESC",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(PageSummary {
                    date: r.get(0)?,
                    page_number: r.get(1)?,
                    headline: r.get(2)?,
                    needs_you: r.get(3)?,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
    }

    /// The person answers the day's question. Yes confirms and counts as use; No makes the memory
    /// unconfirmed and asks for the right value; Not sure only records. On page one, Earlier and
    /// Later move the page time by half an hour. A question from the journal takes Fine, Not
    /// great or Not yet: each keeps an answer on today's journal, and Fine and Not great count
    /// as use of the memory, as a yes does.
    pub fn answer_question(&self, day: Date, choice: QuestionChoice) -> Result<()> {
        self.write(|tx| answer_tx(tx, self.clock(), day, choice))?;
        self.bump_audio();
        Ok(())
    }
}

/// The body of `answer_question`, inside the caller's transaction. Returns the item kept on
/// today's journal when the question is from the journal.
pub(crate) fn answer_tx(
    tx: &rusqlite::Transaction<'_>,
    clock: &dyn crate::Clock,
    day: Date,
    choice: QuestionChoice,
) -> Result<Option<String>> {
    let now = fmt_ts(clock.now());
    let today = clock.now().to_zoned(clock.tz()).date().to_string();
    let date = day.to_string();
    let ctx = Ctx::load(tx, clock)?;
    let row = saved_question(tx, &date)?;
    let row = match row {
        Some(row) => Some(row),
        None => unsaved_question(tx, &ctx, day, &today, &now)?,
    };
    let row = row.ok_or_else(|| Error::not_found(format!("a question on {date}")))?;
    if row.answer.is_some() {
        return Err(Error::invalid("that question has already been answered"));
    }
    let kind: QuestionKind = parse_enum(&row.kind)?;
    let mut kept = None;
    match (kind, choice, &row.memory_id) {
        (QuestionKind::YesNo, QuestionChoice::Yes, Some(id)) => {
            confirm_tx(tx, id, true, &now, &today)?
        }
        (QuestionKind::YesNo, QuestionChoice::No, Some(id)) => {
            confirm_tx(tx, id, false, &now, &today)?
        }
        (QuestionKind::YesNo, QuestionChoice::NotSure, _) => {}
        (QuestionKind::PageTime, QuestionChoice::Right, _) => {}
        (QuestionKind::PageTime, c @ (QuestionChoice::Earlier | QuestionChoice::Later), _) => {
            let (h, m) = parse_page_time(&read_settings(tx)?.page_time)?;
            let minutes = i32::from(h) * 60 + i32::from(m);
            let shifted = if c == QuestionChoice::Earlier {
                minutes - 30
            } else {
                minutes + 30
            };
            let shifted = shifted.clamp(0, 23 * 60 + 30);
            set_page_time(tx, (shifted / 60) as i8, (shifted % 60) as i8)?;
        }
        (
            QuestionKind::Journal,
            c @ (QuestionChoice::Fine | QuestionChoice::NotGreat | QuestionChoice::NotYet),
            Some(id),
        ) => {
            let ask: JournalAsk = serde_json::from_str(
                row.journal
                    .as_deref()
                    .ok_or_else(|| Error::invalid("that question has no block"))?,
            )?;
            kept = Some(journal_question::keep_answer_tx(
                tx, &ctx, &date, &ask, &row.text, c,
            )?);
            if c != QuestionChoice::NotYet {
                mark_used_tx(tx, id, &now)?;
            }
        }
        _ => return Err(Error::invalid("that answer does not fit this question")),
    }
    tx.execute(
        "UPDATE questions SET answer = ?2, answered_at = ?3 WHERE page_date = ?1",
        params![date, enum_str(&choice), now],
    )?;
    Ok(kept)
}

/// A saved question: its memory, kind, words, answer and, from the journal, where it came from.
struct QuestionRow {
    memory_id: Option<String>,
    kind: String,
    answer: Option<String>,
    journal: Option<String>,
    text: String,
}

fn saved_question(conn: &Connection, date: &str) -> Result<Option<QuestionRow>> {
    Ok(conn
        .query_row(
            "SELECT memory_id, kind, answer, journal, text FROM questions WHERE page_date = ?1",
            [date],
            |r| {
                Ok(QuestionRow {
                    memory_id: r.get(0)?,
                    kind: r.get(1)?,
                    answer: r.get(2)?,
                    journal: r.get(3)?,
                    text: r.get(4)?,
                })
            },
        )
        .optional()?)
}

/// The question of the next page, recorded when it is answered before the page is saved. That
/// page is composed on the spot until the tidy saves it, so its question has no row yet. The
/// row is the one `save_page` would write (and that write leaves it alone), so the answer
/// survives the save. Only the page-time question of the first page and a question from the
/// journal can be answered this way; any other day without a saved question has nothing to
/// answer.
fn unsaved_question(
    tx: &Connection,
    ctx: &Ctx,
    day: Date,
    today: &str,
    now: &str,
) -> Result<Option<QuestionRow>> {
    let exists = |d: &str| -> Result<bool> {
        Ok(tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM pages WHERE date = ?1)",
            [d],
            |r| r.get(0),
        )?)
    };
    let next = if exists(today)? {
        today.parse::<Date>()?.tomorrow()?
    } else {
        today.parse::<Date>()?
    };
    if day != next {
        return Ok(None);
    }
    if page_number(tx, day)? == 1 {
        tx.execute(
            "INSERT INTO questions(page_date, memory_id, kind, text, asked_at) \
             VALUES (?1, NULL, ?2, ?3, ?4)",
            params![
                day.to_string(),
                enum_str(&QuestionKind::PageTime),
                read_settings(tx)?.page_time,
                now
            ],
        )?;
        return Ok(Some(QuestionRow {
            memory_id: None,
            kind: enum_str(&QuestionKind::PageTime),
            answer: None,
            journal: None,
            text: String::new(),
        }));
    }
    let Some(chosen) = journal_question::pick(tx, ctx, day)? else {
        return Ok(None);
    };
    let journal = journal_question::ask_json(&chosen.ask);
    tx.execute(
        "INSERT INTO questions(page_date, memory_id, kind, text, asked_at, journal) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            day.to_string(),
            chosen.memory.id,
            enum_str(&QuestionKind::Journal),
            chosen.sentence,
            now,
            journal
        ],
    )?;
    Ok(Some(QuestionRow {
        memory_id: Some(chosen.memory.id),
        kind: enum_str(&QuestionKind::Journal),
        answer: None,
        journal: Some(journal),
        text: chosen.sentence,
    }))
}

/// Redact lines and the question that cite memories which no longer exist or are being
/// forgotten, and fall back to the template headline if anything was redacted.
fn strip_stale(
    conn: &Connection,
    page: &nocturne_contract::PageView,
) -> Result<nocturne_contract::PageView> {
    let mut page = page.clone();
    let live = |id: &str| -> Result<bool> {
        Ok(conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM memories WHERE id = ?1 AND forgetting = 0)",
            [id],
            |r| r.get(0),
        )?)
    };
    // An item that is gone, being undone, or listed in a forget that will delete its original
    // is cited by nothing. A forget that keeps originals leaves its items alone.
    let item_live = |id: &str| -> Result<bool> {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM items WHERE id = ?1)",
            [id],
            |r| r.get(0),
        )?;
        let undoing: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM forget_jobs WHERE (kind = 'unkeep' OR include_originals = 1) \
             AND instr(items, '\"' || ?1 || '\"') > 0)",
            [id],
            |r| r.get(0),
        )?;
        Ok(exists && !undoing)
    };
    let mut changed = false;
    for list in [
        &mut page.needs_you,
        &mut page.changed,
        &mut page.let_go,
        &mut page.upcoming,
        &mut page.learned,
    ] {
        for line in list.iter_mut() {
            if line.forgotten {
                continue;
            }
            let mut gone = false;
            for r in line
                .receipts
                .iter()
                .chain(line.choices.iter().map(|c| &c.receipt))
            {
                gone |= !item_live(&r.item_id)?;
            }
            for m in line
                .memory_ids
                .iter()
                .chain(line.choices.iter().map(|c| &c.memory_id))
            {
                gone |= !live(m)?;
            }
            if gone {
                crate::forget::redact_line(line);
                changed = true;
            }
        }
    }
    if let Some(q) = page.question.as_ref() {
        let mut gone = false;
        if let Some(m) = &q.memory_id {
            gone |= !live(m)?;
        }
        if let Some(r) = &q.receipt {
            gone |= !item_live(&r.item_id)?;
        }
        if gone {
            page.question = None;
            changed = true;
        }
    }
    if changed {
        let day: Date = page.date.parse()?;
        let needs =
            page.needs_you.iter().filter(|l| !l.forgotten).count() as u32 + page.more_needs_you;
        page.headline = template_headline(day);
        page.dek = template_dek(page.kept_count, needs, page.let_go_count);
    }
    Ok(page)
}

/// The period the page's quiet line looks back on: the morning after the night wrote the
/// journal day's reflection, or after a week, month or year closed. The larger period wins.
fn quiet_line(conn: &Connection, date: &str) -> Result<Option<Period>> {
    let Ok(day) = date.parse::<Date>() else {
        return Ok(None);
    };
    let before = day.yesterday().map_err(Error::from)?.to_string();
    let row: Option<(String, String, String)> = conn
        .query_row(
            "SELECT kind, start, end FROM reflections WHERE so_far = 0 AND \
             ((kind = 'day' AND start = ?1 AND written = 'overnight') \
              OR (kind IN ('week', 'month', 'year') AND end = ?1)) \
             ORDER BY CASE kind WHEN 'year' THEN 0 WHEN 'month' THEN 1 WHEN 'week' THEN 2 \
             ELSE 3 END LIMIT 1",
            [&before],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    row.map(|(kind, start, end)| {
        Ok(Period {
            kind: parse_enum::<PeriodKind>(&kind)?,
            start,
            end,
        })
    })
    .transpose()
}

pub(crate) fn load_page(
    conn: &Connection,
    day: Option<Date>,
) -> Result<Option<nocturne_contract::PageView>> {
    let row: Option<(String, String)> = match day {
        Some(d) => conn
            .query_row(
                "SELECT date, body FROM pages WHERE date = ?1",
                [d.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?,
        None => conn
            .query_row(
                "SELECT date, body FROM pages ORDER BY date DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?,
    };
    let Some((date, body)) = row else {
        return Ok(None);
    };
    let mut page: nocturne_contract::PageView = serde_json::from_str(&body)?;
    // Ticks and redactions made since the snapshot was taken.
    let mut stmt =
        conn.prepare("SELECT id, done, forgotten FROM page_lines WHERE page_date = ?1")?;
    let state: HashMap<String, (bool, bool)> = stmt
        .query_map([&date], |r| {
            Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?)))
        })?
        .collect::<rusqlite::Result<_>>()?;
    for list in [
        &mut page.needs_you,
        &mut page.changed,
        &mut page.let_go,
        &mut page.upcoming,
        &mut page.learned,
    ] {
        for line in list.iter_mut() {
            if let Some((done, forgotten)) = state.get(&line.id) {
                line.done = *done;
                line.forgotten = *forgotten || line.forgotten;
            }
        }
    }
    page.journal_line = quiet_line(conn, &date)?;
    if let Some(q) = page.question.as_mut() {
        let answer: Option<String> = conn
            .query_row(
                "SELECT answer FROM questions WHERE page_date = ?1",
                [&date],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        q.answered = opt_enum(answer)?;
        if q.kind == QuestionKind::Journal
            && let Some(ask) = q.journal.as_mut()
        {
            match journal_question::answer_of(conn, &date)? {
                Some((id, line)) => (ask.answer_item_id, ask.line) = (Some(id), line),
                None => (ask.answer_item_id, ask.line) = (None, None),
            }
        }
    }
    Ok(Some(page))
}

/// The needs-you lines a page shows that are not forgotten: the count a page summary carries.
pub fn needs_you_count(page: &nocturne_contract::PageView) -> u32 {
    page.needs_you.iter().filter(|l| !l.forgotten).count() as u32
}

/// A saved page for an edition, inside a read.
fn edition_page(
    conn: &Connection,
    day: Option<Date>,
    edition: Edition,
) -> Result<Option<nocturne_contract::PageView>> {
    let Some(mut page) = load_page(conn, day)? else {
        return Ok(None);
    };
    page.edition = edition;
    if edition == Edition::Screen {
        return Ok(Some(page));
    }
    let spoken = edition == Edition::Audio;
    if spoken {
        page.journal_line = None;
    }
    let mut skipped = 0u32;
    let mut journal_skipped = 0u32;
    let mut dropped_let_go = 0u32;
    for (i, list) in [
        &mut page.needs_you,
        &mut page.changed,
        &mut page.let_go,
        &mut page.upcoming,
        &mut page.learned,
    ]
    .into_iter()
    .enumerate()
    {
        let mut kept = Vec::new();
        for line in std::mem::take(list) {
            let mut private = false;
            let mut journal = false;
            for m in &line.memory_ids {
                private |= is_private(conn, m)?;
                journal |= spoken && is_journal_chain(conn, m)?;
            }
            if private || journal {
                if private {
                    skipped += 1;
                } else {
                    journal_skipped += 1;
                }
                if i == 2 {
                    dropped_let_go += 1;
                }
            } else {
                kept.push(line);
            }
        }
        *list = kept;
    }
    if let Some(q) = page.question.take() {
        let memory = q.memory_id.clone();
        page.question = match memory {
            None => Some(q),
            Some(m) if is_private(conn, &m)? => {
                skipped += 1;
                None
            }
            // Never read aloud, and not counted as a line left out: the reading says the
            // question is on the page. It keeps only its kind, for that.
            Some(_) if spoken && q.kind == QuestionKind::Journal => Some(Question {
                kind: QuestionKind::Journal,
                text: String::new(),
                memory_id: None,
                receipt: None,
                last_asked: None,
                answered: q.answered,
                journal: None,
            }),
            Some(m) if spoken && is_journal_chain(conn, &m)? => {
                journal_skipped += 1;
                None
            }
            Some(_) => Some(q),
        };
    }
    page.private_skipped += skipped;
    if spoken {
        page.journal_skipped = Some(journal_skipped);
    }
    if skipped + journal_skipped > 0 {
        // The stored headline and dek were written with the private content in view.
        // Never hand them to audio or print: fall back to the template, from the
        // counts that remain.
        let day: Date = page.date.parse()?;
        let needs =
            page.needs_you.iter().filter(|l| !l.forgotten).count() as u32 + page.more_needs_you;
        page.headline = template_headline(day);
        page.dek = template_dek(
            page.kept_count,
            needs,
            page.let_go_count.saturating_sub(dropped_let_go),
        );
    }
    phone_only(conn, &mut page)?;
    Ok(Some(page))
}

/// What only the phone edition leaves out: lines and a question whose receipts name an item a
/// phone must not see, and every count that would tell it how much stayed home. The headline
/// and dek are always the templates, from the needs-you lines the phone gets, since the stored
/// ones were written with everything in view.
fn phone_only(conn: &Connection, page: &mut nocturne_contract::PageView) -> Result<()> {
    if page.edition != Edition::Phone {
        return Ok(());
    }
    let withheld = |r: &nocturne_contract::Receipt| -> Result<bool> {
        crate::remote::withheld_from_phones(conn, &r.item_id)
    };
    for list in [
        &mut page.needs_you,
        &mut page.changed,
        &mut page.let_go,
        &mut page.upcoming,
        &mut page.learned,
    ] {
        let mut kept = Vec::new();
        for line in std::mem::take(list) {
            let mut hide = false;
            for r in line
                .receipts
                .iter()
                .chain(line.choices.iter().map(|c| &c.receipt))
            {
                hide |= withheld(r)?;
            }
            if !hide {
                kept.push(line);
            }
        }
        *list = kept;
    }
    if let Some(q) = &page.question
        && let Some(r) = &q.receipt
        && withheld(r)?
    {
        page.question = None;
    }
    // Lines past the cap are not known one by one, but the name questions a phone can answer
    // are: they are the only thing left to count beyond its lines.
    page.more_needs_you = phone_name_questions(conn)?;
    *page = phone_view(std::mem::replace(page, empty_like(page)))?;
    Ok(())
}

/// The open "Which {name} is this?" questions a phone is shown: the questions whose block and
/// people it may read, the same ones its journal day offers.
fn phone_name_questions(conn: &Connection) -> Result<u32> {
    let withheld = |item: &str| crate::remote::withheld_from_phones(conn, item);
    let mut count = 0;
    for q in crate::journal::mentions::open_questions(conn, None)? {
        if withheld(&q.receipt.item_id)? {
            continue;
        }
        let mut seen = 0;
        for o in &q.options {
            let memories: Vec<String> = conn
                .prepare_cached(
                    "SELECT id FROM memories WHERE entity_id = ?1 AND superseded_at IS NULL \
                     AND forgetting = 0",
                )?
                .query_map([&o.entity_id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            let mut visible = memories.is_empty();
            for m in &memories {
                let mut shown = !is_private_chain(conn, m)?;
                for r in crate::rows::receipts_for(conn, m)? {
                    shown &= !withheld(&r.item_id)?;
                }
                visible |= shown;
            }
            seen += u32::from(visible);
        }
        if seen >= 2 {
            count += 1;
        }
    }
    Ok(count)
}

fn empty_like(page: &nocturne_contract::PageView) -> nocturne_contract::PageView {
    nocturne_contract::PageView {
        date: page.date.clone(),
        page_number: 0,
        edition: page.edition,
        variant: PageVariant::Normal,
        headline: String::new(),
        dek: String::new(),
        needs_you: Vec::new(),
        changed: Vec::new(),
        let_go: Vec::new(),
        upcoming: Vec::new(),
        learned: Vec::new(),
        question: None,
        more_needs_you: 0,
        more_changed: 0,
        kept_count: 0,
        changes_count: 0,
        let_go_count: 0,
        catch_up_days: 0,
        private_skipped: 0,
        pending: 0,
        tidied_from: None,
        tidied_to: None,
        update: None,
        journal_line: None,
        journal_skipped: None,
    }
}

/// A phone edition page carrying only what a phone may see, field by field: the lines it gets
/// and what follows from them alone. Every total that could count something kept at home is
/// left out or recounted from those lines, and the headline and dek are the templates, since
/// the stored ones were written with everything in view. Safe to apply more than once.
pub fn phone_view(page: nocturne_contract::PageView) -> Result<nocturne_contract::PageView> {
    let day: Date = page.date.parse()?;
    // On a page already made for a phone, `more_needs_you` counts the name questions the phone
    // can answer, which the lines do not show. On any other it is not known to be safe.
    let waiting = if page.edition == Edition::Phone {
        page.more_needs_you
    } else {
        0
    };
    let needs = needs_you_count(&page) + waiting;
    let changes = page.changed.len() as u32;
    Ok(nocturne_contract::PageView {
        headline: template_headline(day),
        dek: phone_dek(needs),
        // Quiet and heavy are judged from everything kept, private included.
        variant: match page.variant {
            PageVariant::Quiet | PageVariant::Heavy => PageVariant::Normal,
            v => v,
        },
        // Lines past the cap are not known one by one, so none is claimed.
        more_needs_you: waiting,
        more_changed: 0,
        changes_count: changes,
        kept_count: 0,
        let_go_count: 0,
        private_skipped: 0,
        // Unread items may be private: how many there are stays home.
        pending: 0,
        // An offer to update the app is for the computer.
        update: None,
        // The phone reads the whole journal; the quiet line stays as it is.
        journal_line: page.journal_line,
        journal_skipped: None,
        // Not content: when the page was tidied and how many days it catches up on.
        catch_up_days: page.catch_up_days,
        tidied_from: page.tidied_from,
        tidied_to: page.tidied_to,
        date: page.date,
        page_number: page.page_number,
        edition: Edition::Phone,
        needs_you: page.needs_you,
        changed: page.changed,
        let_go: page.let_go,
        upcoming: page.upcoming,
        learned: page.learned,
        question: page.question,
    })
}
