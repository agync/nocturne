//! Keeping into the journal: entry blocks, check-ins, good things and the small changes the
//! person makes to them.

use std::collections::{HashMap, HashSet};

use jiff::civil::Date;
use jiff::{SignedDuration, Timestamp};
use nocturne_contract::{
    CheckIn, CheckInInput, EntryBlock, Feeling, FeelingPart, GoodThing, ItemKind, JournalRole,
    Kept, Source,
};
use rusqlite::{Connection, Transaction, params};
use serde_json::{Value, json};

use super::day::{checkin_view, entry_block, good_view};
use super::{Ctx, HOLD_MINUTES, INTRO_KEY, JItem, MAX_GOOD_THINGS, load_item};
use crate::Engine;
use crate::capture::{in_forget_job, text_title, trust_for};
use crate::error::{Error, Result};
use crate::util::{enum_str, fmt_ts, new_id, sha256_hex};

struct NewJournal<'a> {
    kind: ItemKind,
    source: Source,
    title: &'a str,
    text: Option<&'a str>,
    status: &'a str,
    meta: Value,
    hold_until: Option<String>,
}

/// Insert a journal item. Its hash includes its own id, so identical words never collapse into
/// one item.
fn insert(tx: &Transaction<'_>, now: &str, n: &NewJournal<'_>) -> Result<String> {
    let id = new_id("itm");
    let sha = sha256_hex(format!("{}\0{id}", n.text.unwrap_or("")).as_bytes());
    tx.execute(
        "INSERT INTO items(id, kind, source, trust, sha256, title, kept_at, status, text, \
         metadata, hold_until) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            id,
            enum_str(&n.kind),
            enum_str(&n.source),
            enum_str(&trust_for(n.source)),
            sha,
            n.title,
            now,
            n.status,
            n.text,
            n.meta.to_string(),
            n.hold_until,
        ],
    )?;
    Ok(id)
}

fn hold_until(now: Timestamp) -> Result<String> {
    Ok(fmt_ts(
        now.checked_add(SignedDuration::from_mins(HOLD_MINUTES))?,
    ))
}

fn not_future(ctx: &Ctx, day: Date) -> Result<()> {
    if day > ctx.today()? {
        return Err(Error::invalid("that day has not come yet"));
    }
    Ok(())
}

fn part_name(part: FeelingPart) -> &'static str {
    match part {
        FeelingPart::WoundUp => "Wound up",
        FeelingPart::LitUp => "Lit up",
        FeelingPart::Low => "Low",
        FeelingPart::Settled => "Settled",
    }
}

fn validate_feeling(f: &Feeling) -> Result<()> {
    let ok = |v: f32| v.is_finite() && (-1.0..=1.0).contains(&v);
    if !ok(f.point.pleasant) || !ok(f.point.energy) {
        return Err(Error::invalid("a feeling's point is between -1 and 1"));
    }
    if f.words.len() > 3 {
        return Err(Error::invalid("a feeling takes up to three words"));
    }
    Ok(())
}

fn clean(s: &Option<String>) -> Option<String> {
    s.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn is_replaced(conn: &Connection, id: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM items WHERE json_extract(metadata, '$.replaces') = ?1)",
        [id],
        |r| r.get(0),
    )?)
}

/// The id of `id`, every version it replaced, and every version that replaced it.
pub(crate) fn version_chain(conn: &Connection, id: &str) -> Result<Vec<String>> {
    let mut out = vec![id.to_string()];
    let mut seen: HashSet<String> = HashSet::from([id.to_string()]);
    let mut frontier = vec![id.to_string()];
    while let Some(cur) = frontier.pop() {
        let mut next: Vec<String> = Vec::new();
        if let Ok(Some(earlier)) = conn.query_row(
            "SELECT json_extract(metadata, '$.replaces') FROM items WHERE id = ?1",
            [&cur],
            |r| r.get::<_, Option<String>>(0),
        ) {
            next.push(earlier);
        }
        let mut stmt = conn.prepare_cached(
            "SELECT id FROM items WHERE json_extract(metadata, '$.replaces') = ?1",
        )?;
        for later in stmt.query_map([&cur], |r| r.get::<_, String>(0))? {
            next.push(later?);
        }
        for n in next {
            if seen.insert(n.clone()) {
                out.push(n.clone());
                frontier.push(n);
            }
        }
    }
    Ok(out)
}

/// The versions `id` replaced, newest first. Deleting an entry block takes these with it.
pub(crate) fn earlier_versions(conn: &Connection, id: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut cur = id.to_string();
    loop {
        let prev: Option<Option<String>> = rusqlite::OptionalExtension::optional(conn.query_row(
            "SELECT json_extract(metadata, '$.replaces') FROM items WHERE id = ?1",
            [&cur],
            |r| r.get(0),
        ))?;
        match prev.flatten() {
            Some(p) if !out.contains(&p) => {
                out.push(p.clone());
                cur = p;
            }
            _ => return Ok(out),
        }
    }
}

/// Delete an item that nothing has read, with the reflection lines that cite it.
fn delete_unread(tx: &Transaction<'_>, id: &str) -> Result<()> {
    super::store::scrub_items(tx, &[id.to_string()])?;
    tx.execute("DELETE FROM items WHERE id = ?1", [id])?;
    Ok(())
}

fn set_meta(tx: &Transaction<'_>, id: &str, key: &str, value: &Value) -> Result<()> {
    tx.execute(
        &format!(
            "UPDATE items SET metadata = json_set(metadata, '$.{key}', json(?2)) WHERE id = ?1"
        ),
        params![id, value.to_string()],
    )?;
    Ok(())
}

impl Engine {
    /// Save the text of an entry block. A new block when `item_id` is `None` (nothing is made
    /// for empty text). While the block is unread it is changed in place; once it was read, a
    /// new version replaces it and the old one stays as history. Empty text deletes an unread
    /// block and is refused on one that was read. Every save holds the block for ten minutes
    /// before it is read.
    pub fn save_entry_text(
        &self,
        day: Date,
        item_id: Option<&str>,
        block: u32,
        text: &str,
    ) -> Result<Option<EntryBlock>> {
        let now = self.clock_now();
        let stamp = fmt_ts(now);
        let hold = hold_until(now)?;
        let originals = self.originals_dir();
        let saved = self.write(|tx| {
            let ctx = Ctx::load(tx, self.clock())?;
            let empty = text.trim().is_empty();
            let Some(id) = item_id else {
                if empty {
                    return Ok(None);
                }
                not_future(&ctx, day)?;
                let title = text_title(text);
                let id = insert(
                    tx,
                    &stamp,
                    &NewJournal {
                        kind: ItemKind::Text,
                        source: Source::Journal,
                        title: &title,
                        text: Some(text),
                        status: "kept",
                        meta: json!({"journal": "entry", "journal_day": day.to_string(),
                                     "block": block, "saved_at": stamp}),
                        hold_until: Some(hold.clone()),
                    },
                )?;
                return Ok(Some(id));
            };
            let old = load_item(tx, &originals, id)?;
            if old.role() != Some(JournalRole::Entry) || old.view.kind != ItemKind::Text {
                return Err(Error::invalid("that is not a block of text in an entry"));
            }
            if in_forget_job(tx, id)? || is_replaced(tx, id)? {
                return Err(Error::not_found(format!("item {id}")));
            }
            let unread = old.view.status == nocturne_contract::ItemStatus::Kept;
            if empty {
                if !unread {
                    return Err(Error::invalid("a block that was read cannot be emptied"));
                }
                delete_unread(tx, id)?;
                return Ok(None);
            }
            if old.view.text.as_deref() == Some(text) {
                return Ok(Some(id.to_string()));
            }
            let title = text_title(text);
            if unread {
                tx.execute(
                    "UPDATE items SET text = ?2, title = ?3, sha256 = ?4, hold_until = ?5, \
                     metadata = json_set(metadata, '$.saved_at', ?6) WHERE id = ?1",
                    params![
                        id,
                        text,
                        title,
                        sha256_hex(format!("{text}\0{id}").as_bytes()),
                        hold,
                        stamp
                    ],
                )?;
                return Ok(Some(id.to_string()));
            }
            let mut meta = json!({"journal": "entry", "block": old.block(),
                                  "replaces": old.id(), "saved_at": stamp});
            if let Some(d) = old.day_text() {
                meta["journal_day"] = json!(d);
            }
            let new = insert(
                tx,
                &stamp,
                &NewJournal {
                    kind: ItemKind::Text,
                    source: Source::Journal,
                    title: &title,
                    text: Some(text),
                    status: "kept",
                    meta,
                    hold_until: Some(hold.clone()),
                },
            )?;
            Ok(Some(new))
        })?;
        match saved {
            Some(id) => self.entry_block_of(&id).map(Some),
            None => Ok(None),
        }
    }

    fn entry_block_of(&self, id: &str) -> Result<EntryBlock> {
        let originals = self.originals_dir();
        self.read(|conn| {
            let item = load_item(conn, &originals, id)?;
            entry_block(conn, &originals, &item)
        })
    }

    /// Turn a kept photo or voice note into a block of the day's entry.
    pub fn place_in_entry(&self, item_id: &str, day: Date, block: u32) -> Result<EntryBlock> {
        let hold = hold_until(self.clock_now())?;
        let stamp = fmt_ts(self.clock_now());
        let originals = self.originals_dir();
        self.write(|tx| {
            let ctx = Ctx::load(tx, self.clock())?;
            not_future(&ctx, day)?;
            let item = load_item(tx, &originals, item_id)?;
            if item.role().is_some() || !matches!(item.view.kind, ItemKind::Image | ItemKind::Audio)
            {
                return Err(Error::invalid("only a photo or a voice note can be placed"));
            }
            if item.view.private {
                return Err(Error::invalid("a private item cannot go in the journal"));
            }
            tx.execute(
                "UPDATE items SET metadata = json_patch(metadata, ?2), hold_until = ?3 WHERE id = ?1",
                params![
                    item_id,
                    json!({"journal": "entry", "journal_day": day.to_string(), "block": block,
                           "saved_at": stamp})
                    .to_string(),
                    hold
                ],
            )?;
            Ok(())
        })?;
        self.entry_block_of(item_id)
    }

    /// Change the description of a photo ("Describe this photo"). Empty text takes it away.
    pub fn describe_photo(&self, item_id: &str, text: &str) -> Result<()> {
        self.write(|tx| {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM items WHERE id = ?1)",
                [item_id],
                |r| r.get(0),
            )?;
            if !exists {
                return Err(Error::not_found(format!("item {item_id}")));
            }
            let text = text.trim();
            if text.is_empty() {
                tx.execute(
                    "UPDATE items SET metadata = json_remove(metadata, '$.alt') WHERE id = ?1",
                    [item_id],
                )?;
            } else {
                set_meta(tx, item_id, "alt", &json!(text))?;
            }
            Ok(())
        })
    }

    /// Read the day's entry now rather than when the writing stops.
    pub fn read_entry_now(&self, day: Date) -> Result<()> {
        self.write(|tx| {
            let ids: Vec<String> = {
                let mut stmt = tx.prepare(
                    "SELECT id FROM items WHERE json_extract(metadata, '$.journal') = 'entry' \
                     AND json_extract(metadata, '$.journal_day') = ?1 AND hold_until IS NOT NULL",
                )?;
                stmt.query_map([day.to_string()], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?
            };
            for id in &ids {
                tx.execute("UPDATE items SET hold_until = NULL WHERE id = ?1", [id])?;
                crate::capture::bump_priority(tx, id)?;
            }
            Ok(())
        })
    }

    /// Let every held block be read: the person left the journal, or the night begins.
    pub fn release_entry_holds(&self) -> Result<u32> {
        self.write(|tx| {
            Ok(tx.execute(
                "UPDATE items SET hold_until = NULL WHERE hold_until IS NOT NULL",
                [],
            )? as u32)
        })
    }

    /// Keep a note. An ordinary note, or a good thing of today when `good`. With a feeling, a
    /// check-in about the note is kept beside it, on the note's day, or on today when the note
    /// was kept before.
    pub fn keep_note(
        &self,
        text: &str,
        source: Source,
        feeling: Option<Feeling>,
        good: bool,
    ) -> Result<Kept> {
        if let Some(f) = &feeling {
            validate_feeling(f)?;
        }
        let kept = if good {
            let day = self.journal_today()?;
            let thing = self.keep_good_thing(day, text, None, source)?;
            Kept::New {
                item: self.get_item(&thing.id)?,
            }
        } else {
            self.keep_text(text, source)?
        };
        if let Some(f) = feeling {
            // The note may be an old one kept again; the feeling is new, so it is today's.
            let (item, on_today) = match &kept {
                Kept::New { item } => (item, false),
                Kept::Duplicate { item } => (item, true),
            };
            self.insert_checkin(
                CheckInInput {
                    feeling: f,
                    line: None,
                    urge: None,
                    replaces: None,
                },
                source,
                Some(&item.id),
                on_today,
            )?;
        }
        Ok(kept)
    }

    /// A feeling about something already kept, such as a photo or a voice note from a phone.
    pub fn attach_feeling(
        &self,
        item_id: &str,
        feeling: Feeling,
        source: Source,
    ) -> Result<CheckIn> {
        self.insert_checkin(
            CheckInInput {
                feeling,
                line: None,
                urge: None,
                replaces: None,
            },
            source,
            Some(item_id),
            false,
        )
    }

    /// Keep a check-in. With no line and no urge there is nothing to read, so it is understood
    /// at once; otherwise it is read for names only.
    pub fn keep_checkin(&self, input: CheckInInput, source: Source) -> Result<CheckIn> {
        self.insert_checkin(input, source, None, false)
    }

    fn insert_checkin(
        &self,
        input: CheckInInput,
        source: Source,
        about: Option<&str>,
        on_today: bool,
    ) -> Result<CheckIn> {
        validate_feeling(&input.feeling)?;
        let line = clean(&input.line);
        let urge = clean(&input.urge);
        let text = [line.as_deref(), urge.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("\n");
        let text = (!text.is_empty()).then_some(text);
        let title = if input.feeling.words.is_empty() {
            part_name(input.feeling.part).to_string()
        } else {
            input.feeling.words.join(", ")
        };
        let stamp = fmt_ts(self.clock_now());
        let originals = self.originals_dir();
        let id = self.write(|tx| {
            let ctx = Ctx::load(tx, self.clock())?;
            let mut about = about.map(str::to_string);
            let day = if let Some(prev) = &input.replaces {
                let old = load_item(tx, &originals, prev)?;
                if old.role() != Some(JournalRole::Checkin) || is_replaced(tx, prev)? {
                    return Err(Error::invalid("that check-in cannot be changed"));
                }
                if about.is_none() {
                    about = old.about().map(str::to_string);
                }
                old.day(&ctx)?
            } else if let Some(target) = about.as_ref().filter(|_| !on_today) {
                load_item(tx, &originals, target)?.day(&ctx)?
            } else {
                ctx.today()?
            };
            let mut meta = json!({
                "journal": "checkin", "journal_day": day.to_string(),
                "feeling": input.feeling,
            });
            if let Some(l) = &line {
                meta["line"] = json!(l);
            }
            if let Some(u) = &urge {
                meta["urge"] = json!(u);
            }
            if let Some(r) = &input.replaces {
                meta["replaces"] = json!(r);
            }
            if let Some(a) = &about {
                meta["about"] = json!(a);
            }
            let status = if text.is_none() {
                meta["read_at"] = json!(stamp);
                "understood"
            } else {
                "kept"
            };
            insert(
                tx,
                &stamp,
                &NewJournal {
                    kind: ItemKind::Checkin,
                    source,
                    title: &title,
                    text: text.as_deref(),
                    status,
                    meta,
                    hold_until: None,
                },
            )
        })?;
        self.read(|conn| {
            let item = load_item(conn, &originals, &id)?;
            let mentions = super::day::mentions_of(conn, std::slice::from_ref(&id))?;
            checkin_view(&item, &mentions)
        })
    }

    /// Note that the person tried an idea from "Something that might help" after a check-in.
    pub fn mark_tried(&self, checkin_id: &str, idea: &str) -> Result<()> {
        let originals = self.originals_dir();
        self.write(|tx| {
            let item = load_item(tx, &originals, checkin_id)?;
            if item.role() != Some(JournalRole::Checkin) {
                return Err(Error::invalid("that is not a check-in"));
            }
            let mut tried: Vec<String> = item
                .meta
                .get("tried")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            if !tried.iter().any(|t| t == idea) {
                tried.push(idea.to_string());
            }
            set_meta(tx, checkin_id, "tried", &json!(tried))
        })
    }

    /// Keep a good thing on a day: at most three a day, and text may be empty only with a photo.
    pub fn keep_good_thing(
        &self,
        day: Date,
        text: &str,
        photo: Option<&str>,
        source: Source,
    ) -> Result<GoodThing> {
        let text = text.trim();
        if text.is_empty() && photo.is_none() {
            return Err(Error::invalid("a good thing needs words or a photo"));
        }
        let stamp = fmt_ts(self.clock_now());
        let originals = self.originals_dir();
        let id = self.write(|tx| {
            let ctx = Ctx::load(tx, self.clock())?;
            not_future(&ctx, day)?;
            let count: u32 = tx.query_row(
                "SELECT COUNT(*) FROM items WHERE json_extract(metadata, '$.journal') = 'good' \
                 AND json_extract(metadata, '$.journal_day') = ?1",
                [day.to_string()],
                |r| r.get(0),
            )?;
            if count >= MAX_GOOD_THINGS {
                return Err(Error::invalid("a day holds three good things"));
            }
            let mut meta = json!({"journal": "good", "journal_day": day.to_string()});
            let mut title = text_title(text);
            if let Some(p) = photo {
                let photo = load_item(tx, &originals, p)?;
                if photo.view.kind != ItemKind::Image {
                    return Err(Error::invalid("a good thing's photo is a photo"));
                }
                meta["photo"] = json!(p);
                if title.is_empty() {
                    title = photo.view.title.clone();
                }
            }
            insert(
                tx,
                &stamp,
                &NewJournal {
                    kind: ItemKind::Text,
                    source,
                    title: &title,
                    text: (!text.is_empty()).then_some(text),
                    status: if text.is_empty() {
                        "understood"
                    } else {
                        "kept"
                    },
                    meta,
                    hold_until: None,
                },
            )
        })?;
        self.read(|conn| {
            let item = load_item(conn, &originals, &id)?;
            let mentions = super::day::mentions_of(conn, std::slice::from_ref(&id))?;
            good_view(conn, &originals, &item, &mentions)
        })
    }

    /// "Leave out of reflections": reversible, and covers every version of an entry block.
    pub fn set_left_out(&self, item_id: &str, on: bool) -> Result<()> {
        self.write(|tx| {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM items WHERE id = ?1)",
                [item_id],
                |r| r.get(0),
            )?;
            if !exists {
                return Err(Error::not_found(format!("item {item_id}")));
            }
            for id in version_chain(tx, item_id)? {
                tx.execute(
                    "UPDATE items SET left_out = ?2 WHERE id = ?1",
                    params![id, on],
                )?;
            }
            Ok(())
        })
    }

    /// Move an ordinary note to another day's timeline. Journal items stay where they were
    /// written.
    pub fn move_to_day(&self, item_id: &str, day: Date) -> Result<()> {
        let originals = self.originals_dir();
        self.write(|tx| {
            let ctx = Ctx::load(tx, self.clock())?;
            not_future(&ctx, day)?;
            let item = load_item(tx, &originals, item_id)?;
            if item.role().is_some() || item.view.kind != ItemKind::Text {
                return Err(Error::invalid("only a note you kept can be moved to another day"));
            }
            let natural = ctx.day_of(crate::util::parse_ts(&item.view.kept_at)?)?;
            if day == natural {
                tx.execute(
                    "UPDATE items SET metadata = json_remove(metadata, '$.journal_day') WHERE id = ?1",
                    [item_id],
                )?;
            } else {
                set_meta(tx, item_id, "journal_day", &json!(day.to_string()))?;
            }
            // A feeling tapped with the note goes where the note goes.
            tx.execute(
                "UPDATE items SET metadata = json_set(metadata, '$.journal_day', ?2) \
                 WHERE json_extract(metadata, '$.journal') = 'checkin' \
                 AND json_extract(metadata, '$.about') = ?1",
                params![item_id, day.to_string()],
            )?;
            Ok(())
        })
    }

    /// The four words this person used most in a part of the field, most used first. Empty
    /// when there is no history.
    pub fn top_feeling_words(&self, part: FeelingPart) -> Result<Vec<String>> {
        self.read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, metadata FROM items WHERE json_extract(metadata, '$.journal') = 'checkin' \
                 ORDER BY kept_at, rowid",
            )?;
            let rows: Vec<(String, String)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            let metas: Vec<(String, Value)> = rows
                .into_iter()
                .map(|(id, m)| (id, serde_json::from_str(&m).unwrap_or(Value::Null)))
                .collect();
            let replaced: HashSet<&str> = metas
                .iter()
                .filter_map(|(_, m)| m.get("replaces").and_then(Value::as_str))
                .collect();
            let mut counts: HashMap<String, (u32, usize)> = HashMap::new();
            let mut order = 0usize;
            for (id, meta) in &metas {
                if replaced.contains(id.as_str()) {
                    continue;
                }
                let Some(feeling) = meta
                    .get("feeling")
                    .and_then(|f| serde_json::from_value::<Feeling>(f.clone()).ok())
                else {
                    continue;
                };
                if feeling.part != part {
                    continue;
                }
                for w in feeling.words {
                    let e = counts.entry(w).or_insert_with(|| {
                        order += 1;
                        (0, order)
                    });
                    e.0 += 1;
                }
            }
            let mut words: Vec<(String, (u32, usize))> = counts.into_iter().collect();
            words.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(a.1.1.cmp(&b.1.1)));
            Ok(words.into_iter().take(4).map(|(w, _)| w).collect())
        })
    }

    /// The person dismissed the first-use card, "Your journal".
    pub fn dismiss_journal_intro(&self) -> Result<()> {
        self.write(|tx| {
            tx.execute(
                "INSERT OR REPLACE INTO settings(key, value) VALUES (?1, 'true')",
                [INTRO_KEY],
            )?;
            Ok(())
        })
    }
}

impl JItem {
    /// The feeling of a check-in item.
    pub(crate) fn feeling(&self) -> Option<Feeling> {
        self.meta
            .get("feeling")
            .and_then(|f| serde_json::from_value(f.clone()).ok())
    }
}
