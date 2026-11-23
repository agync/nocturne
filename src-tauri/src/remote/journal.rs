//! The journal as a phone reads it: the same day, reflection or recap the computer shows, with
//! whatever must stay home taken out first.
//!
//! The phone reads the whole journal. What it does not read is an ordinary thing kept (a quick
//! note, a photo, a voice note) that is private or that a private memory was read from, and
//! every private memory. Those are removed here, together with whatever named them: a timeline
//! row, a line that cites one, a name only they mentioned. Journal items themselves (entries,
//! check-ins, good things) are never in the withheld set: they cannot be marked private.
//!
//! Nothing here reads the store. The caller names the ids that are withheld.

use std::collections::HashSet;

use nocturne_contract::{
    CheckIn, DayCount, DayReflection, EarlierVersion, EntryBlock, Glance, GoodThing, HealthSeries,
    Highlight, ItemStatus, ItemView, JournalDay, MemoryView, NameRef, Noticed, NoticedTopic,
    PageLine, Recap, RecapHealth, Receipt, ReflectionLine, ReflectionView, SameNameQuestion,
    TimelineRow, Together,
};

use super::crypto;

/// Item and memory ids that must not reach the phone.
pub type Withheld = HashSet<String>;

/// Every item and memory id a day names.
pub fn day_ids(day: &JournalDay, add: &mut dyn FnMut(&str)) {
    for b in &day.entry {
        add(&b.item.id);
        mention_items(&b.mentions, add);
        for e in &b.earlier {
            add(&e.item_id);
        }
    }
    for r in &day.rows {
        add(&r.item.id);
        mention_items(&r.mentions, add);
        if let Some(c) = &r.checkin {
            checkin_ids(c, add);
        }
        if let Some(c) = &r.feeling {
            checkin_ids(c, add);
        }
        if let Some(g) = &r.good {
            good_ids(g, add);
        }
    }
    day.checkins.iter().for_each(|c| checkin_ids(c, add));
    day.good.iter().for_each(|g| good_ids(g, add));
    day.people.iter().for_each(|n| name_ids(n, add));
    for q in &day.same_name {
        add(&q.receipt.item_id);
    }
    if let Some(h) = &day.health {
        health_ids(h, add);
    }
    if let Some(b) = &day.from_phones {
        b.calendar
            .iter()
            .chain(&b.health)
            .for_each(|i| add(&i.item_id));
    }
}

/// Every reading "From Health" cites, so the phone may open them. Readings cannot be marked
/// private; one held back all the same is taken out by `scrub_day`.
fn health_ids(h: &nocturne_contract::DayHealth, add: &mut dyn FnMut(&str)) {
    if let Some(n) = &h.night {
        add(&n.item_id);
    }
    for s in h.resting.iter().chain(h.steps.iter()) {
        s.points.iter().for_each(|p| add(&p.item_id));
    }
    if let Some(r) = &h.heart {
        add(&r.item_id);
    }
    h.pressure.iter().for_each(|p| add(&p.item_id));
    h.workouts.iter().for_each(|w| add(&w.item_id));
}

/// Every item and memory id a reflection, recap or glance names.
pub fn view_ids(view: &ReflectionView, add: &mut dyn FnMut(&str)) {
    match view {
        ReflectionView::Written { reflection: r } => {
            r.the_day.iter().for_each(|l| receipt_ids(&l.receipts, add));
            r.felt.iter().for_each(|c| checkin_ids(c, add));
            r.good.iter().for_each(|g| good_ids(g, add));
            r.people.iter().for_each(|n| name_ids(n, add));
            r.still_open.iter().for_each(|m| memory_ids(m, add));
        }
        ReflectionView::Recap { recap: r } => {
            for h in &r.highlights {
                receipt_ids(&h.receipts, add);
                if let Some(p) = &h.photo {
                    add(&p.id);
                }
            }
            r.checkins.iter().for_each(|c| checkin_ids(c, add));
            for t in &r.together {
                name_ids(&t.with, add);
                receipt_ids(&t.receipts, add);
            }
            if let Some(h) = &r.health {
                for n in &h.noticed {
                    if let NoticedTopic::Name { with } = &n.topic {
                        name_ids(with, add);
                    }
                    receipt_ids(&n.body, add);
                    receipt_ids(&n.journal, add);
                }
                receipt_ids(&h.workouts, add);
                h.nights.iter().for_each(|n| add(&n.item_id));
                for s in h.sleep.iter().chain(&h.steps).chain(&h.resting) {
                    s.points.iter().for_each(|p| add(&p.item_id));
                }
            }
            r.good.iter().for_each(|g| good_ids(g, add));
            r.people.iter().for_each(|d| name_ids(&d.with, add));
            r.new_names.iter().for_each(|n| name_ids(n, add));
            for t in &r.tried {
                add(&t.checkin_id);
            }
            r.changed.iter().for_each(|l| line_ids(l, add));
            r.still_open.iter().for_each(|m| memory_ids(m, add));
        }
        ReflectionView::Glance { glance, .. }
        | ReflectionView::NotYet { glance }
        | ReflectionView::Writing { glance } => glance_ids(glance, add),
        ReflectionView::Nothing { .. } => {}
    }
}

fn glance_ids(g: &Glance, add: &mut dyn FnMut(&str)) {
    for k in &g.kept {
        k.item_ids.iter().for_each(|i| add(i));
    }
    g.felt.iter().for_each(|c| checkin_ids(c, add));
    g.good.iter().for_each(|x| good_ids(x, add));
    g.with.iter().for_each(|n| name_ids(n, add));
    if let Some(i) = &g.entry_item_id {
        add(i);
    }
}

fn mention_items(ms: &[nocturne_contract::Mention], add: &mut dyn FnMut(&str)) {
    ms.iter().for_each(|m| add(&m.item_id));
}

fn checkin_ids(c: &CheckIn, add: &mut dyn FnMut(&str)) {
    add(&c.id);
    if let Some(a) = &c.about {
        add(a);
    }
    if let Some(n) = &c.after_sleep {
        add(&n.item_id);
    }
    mention_items(&c.mentions, add);
}

fn good_ids(g: &GoodThing, add: &mut dyn FnMut(&str)) {
    add(&g.id);
    if let Some(p) = &g.photo {
        add(&p.id);
    }
    mention_items(&g.mentions, add);
}

fn name_ids(n: &NameRef, add: &mut dyn FnMut(&str)) {
    n.item_ids.iter().for_each(|i| add(i));
}

fn receipt_ids(rs: &[Receipt], add: &mut dyn FnMut(&str)) {
    rs.iter().for_each(|r| add(&r.item_id));
}

fn line_ids(l: &PageLine, add: &mut dyn FnMut(&str)) {
    l.memory_ids.iter().for_each(|m| add(m));
    receipt_ids(&l.receipts, add);
    for c in &l.choices {
        add(&c.memory_id);
        add(&c.receipt.item_id);
    }
}

fn memory_ids(m: &MemoryView, add: &mut dyn FnMut(&str)) {
    add(&m.id);
    receipt_ids(&m.receipts, add);
}

/// An item as a phone sees it: no path, text and summary cut to fit one frame.
fn item(mut v: ItemView) -> ItemView {
    v.original_path = None;
    v.text = v.text.map(|t| crypto::cut_text(&t).0.to_string());
    v.summary = v.summary.map(|t| crypto::cut_text(&t).0.to_string());
    v
}

fn photo(v: Option<ItemView>, w: &Withheld) -> Option<ItemView> {
    v.filter(|v| !w.contains(&v.id)).map(item)
}

fn checkin(mut c: CheckIn, w: &Withheld) -> CheckIn {
    if c.about.as_ref().is_some_and(|a| w.contains(a)) {
        c.about = None;
    }
    if c.after_sleep
        .as_ref()
        .is_some_and(|n| w.contains(&n.item_id))
    {
        c.after_sleep = None;
    }
    c
}

fn good(mut g: GoodThing, w: &Withheld) -> GoodThing {
    g.photo = photo(g.photo, w);
    g
}

fn names(refs: Vec<NameRef>, w: &Withheld) -> Vec<NameRef> {
    refs.into_iter().filter_map(|n| name(n, w)).collect()
}

/// A name with only the items that remain; gone when none do.
fn name(mut n: NameRef, w: &Withheld) -> Option<NameRef> {
    n.item_ids.retain(|i| !w.contains(i));
    (!n.item_ids.is_empty()).then_some(n)
}

fn receipts_ok(rs: &[Receipt], w: &Withheld) -> bool {
    rs.iter().all(|r| !w.contains(&r.item_id))
}

fn line_ok(l: &PageLine, w: &Withheld) -> bool {
    receipts_ok(&l.receipts, w)
        && l.memory_ids.iter().all(|m| !w.contains(m))
        && l.choices
            .iter()
            .all(|c| !w.contains(&c.memory_id) && !w.contains(&c.receipt.item_id))
}

fn memory_ok(m: &MemoryView, w: &Withheld) -> bool {
    !m.private && !w.contains(&m.id) && receipts_ok(&m.receipts, w)
}

fn pending(item: &ItemView) -> bool {
    matches!(item.status, ItemStatus::Kept | ItemStatus::Reading)
}

/// A day for a phone: ordinary things withheld are not rows, no mention or name rests on
/// them, a check-in tapped with one no longer points at it, and the count of things still
/// waiting to be read leaves them out. Everything else is journal content and stays.
pub fn scrub_day(day: JournalDay, w: &Withheld) -> JournalDay {
    let mut waiting_removed = 0u32;
    let rows: Vec<TimelineRow> = day
        .rows
        .into_iter()
        .filter_map(|r| {
            if w.contains(&r.item.id) {
                waiting_removed += u32::from(pending(&r.item));
                return None;
            }
            Some(TimelineRow {
                item: item(r.item),
                checkin: r.checkin.map(|c| checkin(c, w)),
                good: r.good.map(|g| good(g, w)),
                feeling: r.feeling.map(|c| checkin(c, w)),
                ..r
            })
        })
        .collect();
    JournalDay {
        entry: day
            .entry
            .into_iter()
            .map(|b| EntryBlock {
                item: item(b.item),
                earlier: b
                    .earlier
                    .into_iter()
                    .map(|e| EarlierVersion {
                        text: e.text.map(|t| crypto::cut_text(&t).0.to_string()),
                        ..e
                    })
                    .collect(),
                ..b
            })
            .collect(),
        rows,
        checkins: day.checkins.into_iter().map(|c| checkin(c, w)).collect(),
        good: day.good.into_iter().map(|g| good(g, w)).collect(),
        people: names(day.people, w),
        reading: day.reading.saturating_sub(waiting_removed),
        same_name: day
            .same_name
            .into_iter()
            .filter(|q: &SameNameQuestion| !w.contains(&q.receipt.item_id))
            .collect(),
        health: day.health.map(|h| health(h, w)),
        from_phones: day.from_phones.map(|mut b| {
            b.calendar.retain(|i| !w.contains(&i.item_id));
            b.health.retain(|i| !w.contains(&i.item_id));
            b
        }),
        ..day
    }
}

/// "From Health" without anything held back.
fn health(mut h: nocturne_contract::DayHealth, w: &Withheld) -> nocturne_contract::DayHealth {
    h.night = h.night.filter(|n| !w.contains(&n.item_id));
    h.heart = h.heart.filter(|r| !w.contains(&r.item_id));
    for s in [h.resting.as_mut(), h.steps.as_mut()].into_iter().flatten() {
        s.points.retain(|p| !w.contains(&p.item_id));
    }
    h.pressure.retain(|p| !w.contains(&p.item_id));
    h.workouts.retain(|x| !w.contains(&x.item_id));
    h
}

/// What each person the day's "Which {name} is this?" questions offer rests on: for each
/// memory filed under them, its id and the items it came from.
pub type Rests = std::collections::HashMap<String, Vec<Vec<String>>>;

/// Every id the offered people rest on, for the withheld check.
pub fn same_name_ids(day: &JournalDay, rests: &Rests, add: &mut dyn FnMut(&str)) {
    for o in day.same_name.iter().flat_map(|q| &q.options) {
        rests
            .get(&o.entity_id)
            .into_iter()
            .flatten()
            .flatten()
            .for_each(|i| add(i));
    }
}

/// Takes out a choice that rests only on what stays home, and a question left with fewer than
/// two choices. A person nothing is known about stays: nothing of theirs is withheld.
pub fn scrub_same_name(mut day: JournalDay, rests: &Rests, w: &Withheld) -> JournalDay {
    let seen = |entity: &str| {
        rests.get(entity).is_none_or(|memories| {
            memories.is_empty()
                || memories
                    .iter()
                    .any(|ids| ids.iter().all(|i| !w.contains(i)))
        })
    };
    for q in &mut day.same_name {
        q.options.retain(|o| seen(&o.entity_id));
    }
    day.same_name.retain(|q| q.options.len() >= 2);
    day
}

fn the_day(lines: Vec<ReflectionLine>, w: &Withheld) -> Vec<ReflectionLine> {
    lines
        .into_iter()
        .filter(|l| receipts_ok(&l.receipts, w))
        .collect()
}

fn glance(g: Glance, w: &Withheld) -> Glance {
    Glance {
        kept: g
            .kept
            .into_iter()
            .filter_map(|mut k| {
                k.item_ids.retain(|i| !w.contains(i));
                (!k.item_ids.is_empty()).then_some(k)
            })
            .collect(),
        felt: g.felt.into_iter().map(|c| checkin(c, w)).collect(),
        good: g.good.into_iter().map(|x| good(x, w)).collect(),
        with: names(g.with, w),
        ..g
    }
}

/// What a recap says about Health for a phone: a sentence that rests on anything withheld is
/// dropped, and so is a name only withheld things mentioned.
fn scrub_health(h: RecapHealth, w: &Withheld) -> RecapHealth {
    let series = |s: Option<HealthSeries>| {
        s.map(|mut s| {
            s.points.retain(|p| !w.contains(&p.item_id));
            s
        })
        .filter(|s| !s.points.is_empty())
    };
    RecapHealth {
        nights: h
            .nights
            .into_iter()
            .filter(|n| !w.contains(&n.item_id))
            .collect(),
        sleep: series(h.sleep),
        steps: series(h.steps),
        resting: series(h.resting),
        workouts: h
            .workouts
            .into_iter()
            .filter(|r| !w.contains(&r.item_id))
            .collect(),
        noticed: h
            .noticed
            .into_iter()
            .filter(|n: &Noticed| receipts_ok(&n.body, w) && receipts_ok(&n.journal, w))
            .filter_map(|n| {
                let topic = match n.topic {
                    NoticedTopic::Name { with } => NoticedTopic::Name {
                        with: name(with, w)?,
                    },
                    other => other,
                };
                Some(Noticed { topic, ..n })
            })
            .collect(),
    }
}

/// A look back for a phone: lines, highlights, names and open promises that rest on anything
/// withheld are dropped; the rest is as written.
pub fn scrub_view(view: ReflectionView, w: &Withheld) -> ReflectionView {
    match view {
        ReflectionView::Written { reflection: r } => ReflectionView::Written {
            reflection: DayReflection {
                the_day: the_day(r.the_day, w),
                felt: r.felt.into_iter().map(|c| checkin(c, w)).collect(),
                good: r.good.into_iter().map(|g| good(g, w)).collect(),
                people: names(r.people, w),
                still_open: r
                    .still_open
                    .into_iter()
                    .filter(|m| memory_ok(m, w))
                    .collect(),
                ..r
            },
        },
        ReflectionView::Recap { recap: r } => ReflectionView::Recap {
            recap: Recap {
                highlights: r
                    .highlights
                    .into_iter()
                    .filter(|h: &Highlight| receipts_ok(&h.receipts, w))
                    .map(|h| Highlight {
                        photo: photo(h.photo, w),
                        ..h
                    })
                    .collect(),
                checkins: r.checkins.into_iter().map(|c| checkin(c, w)).collect(),
                together: r
                    .together
                    .into_iter()
                    .filter(|t: &Together| receipts_ok(&t.receipts, w))
                    .filter_map(|t| {
                        let with = name(t.with, w)?;
                        Some(Together { with, ..t })
                    })
                    .collect(),
                health: r.health.map(|h| Box::new(scrub_health(*h, w))),
                good: r.good.into_iter().map(|g| good(g, w)).collect(),
                people: r
                    .people
                    .into_iter()
                    .filter_map(|d: DayCount| {
                        let with = name(d.with, w)?;
                        Some(DayCount { with, ..d })
                    })
                    .collect(),
                new_names: names(r.new_names, w),
                changed: r.changed.into_iter().filter(|l| line_ok(l, w)).collect(),
                still_open: r
                    .still_open
                    .into_iter()
                    .filter(|m| memory_ok(m, w))
                    .collect(),
                ..r
            },
        },
        ReflectionView::Glance { glance: g, reason } => ReflectionView::Glance {
            glance: glance(g, w),
            reason,
        },
        ReflectionView::NotYet { glance: g } => ReflectionView::NotYet {
            glance: glance(g, w),
        },
        ReflectionView::Writing { glance: g } => ReflectionView::Writing {
            glance: glance(g, w),
        },
        nothing @ ReflectionView::Nothing { .. } => nothing,
    }
}

/// The first line of a written day, as the journal day shows it: no markers around spans.
pub fn first_sentence(view: &ReflectionView) -> Option<String> {
    match view {
        ReflectionView::Written { reflection } => {
            reflection.the_day.first().map(|l| l.text.replace("**", ""))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nocturne_contract::*;

    fn view(id: &str, status: ItemStatus) -> ItemView {
        ItemView {
            id: id.into(),
            kind: ItemKind::Text,
            title: id.into(),
            source: Source::Phone,
            trust: Trust::Trusted,
            kept_at: "2026-11-19T09:00:00Z".into(),
            status,
            reason: None,
            duration_ms: None,
            pages: None,
            file_name: None,
            has_original: false,
            original_path: Some("/path/to/original.jpg".into()),
            private: false,
            memory_count: 0,
            understood_in_ms: None,
            summary: None,
            text: Some("words".into()),
            language: None,
            from_device: None,
            also_kept_on: Vec::new(),
            journal: None,
            checkin: None,
            calendar: None,
            heard_by: None,
        }
    }

    fn row(id: &str) -> TimelineRow {
        TimelineRow {
            kind: RowKind::Note,
            part: DayPart::Morning,
            item: view(id, ItemStatus::Kept),
            checkin: None,
            good: None,
            feeling: None,
            mentions: Vec::new(),
            left_out: false,
            move_to: None,
            answer: None,
        }
    }

    fn day() -> JournalDay {
        JournalDay {
            day: "2026-11-19".into(),
            today: true,
            first_use: false,
            entry: Vec::new(),
            entry_saved_at: None,
            entry_read_at: None,
            rows: vec![row("itm_public"), row("itm_secret")],
            checkins: Vec::new(),
            good: Vec::new(),
            people: vec![
                NameRef {
                    entity_id: "ent_1".into(),
                    name: "Gran".into(),
                    kind: EntityKind::Person,
                    role: None,
                    new: false,
                    item_ids: vec!["itm_secret".into()],
                },
                NameRef {
                    entity_id: "ent_2".into(),
                    name: "Sam".into(),
                    kind: EntityKind::Person,
                    role: None,
                    new: false,
                    item_ids: vec!["itm_secret".into(), "itm_public".into()],
                },
            ],
            reflection: None,
            reading: 2,
            same_name: Vec::new(),
            health: None,
            from_phones: None,
        }
    }

    #[test]
    fn a_withheld_row_and_what_rests_on_it_are_gone() {
        let w: Withheld = ["itm_secret".to_string()].into();
        let out = scrub_day(day(), &w);
        assert_eq!(out.rows.len(), 1);
        assert_eq!(out.rows[0].item.id, "itm_public");
        assert_eq!(out.rows[0].item.original_path, None);
        assert_eq!(out.people.len(), 1, "a name only it mentioned is gone");
        assert_eq!(out.people[0].item_ids, vec!["itm_public"]);
        assert_eq!(out.reading, 1);
        let mut ids = Vec::new();
        day_ids(&out, &mut |i| ids.push(i.to_string()));
        assert!(!ids.contains(&"itm_secret".to_string()));
    }

    #[test]
    fn a_check_in_no_longer_points_at_a_withheld_note() {
        let c = CheckIn {
            id: "itm_c".into(),
            kept_at: "2026-11-19T09:00:00Z".into(),
            day: "2026-11-19".into(),
            feeling: Feeling {
                part: FeelingPart::Settled,
                point: FeelingPoint {
                    pleasant: 0.5,
                    energy: -0.5,
                },
                words: vec!["calm".into()],
            },
            line: None,
            urge: None,
            about: Some("itm_secret".into()),
            tried: Vec::new(),
            from_device: None,
            mentions: Vec::new(),
            left_out: false,
            after_sleep: None,
        };
        let w: Withheld = ["itm_secret".to_string()].into();
        assert_eq!(checkin(c.clone(), &w).about, None);
        assert_eq!(
            checkin(c, &Withheld::new()).about.as_deref(),
            Some("itm_secret")
        );
    }

    #[test]
    fn a_check_in_no_longer_points_at_a_withheld_night() {
        let c = CheckIn {
            id: "itm_c".into(),
            kept_at: "2026-11-19T09:00:00Z".into(),
            day: "2026-11-19".into(),
            feeling: Feeling {
                part: FeelingPart::Settled,
                point: FeelingPoint {
                    pleasant: 0.5,
                    energy: -0.5,
                },
                words: Vec::new(),
            },
            line: None,
            urge: None,
            about: None,
            tried: Vec::new(),
            from_device: None,
            mentions: Vec::new(),
            left_out: false,
            after_sleep: Some(AfterSleep {
                minutes: 420,
                item_id: "itm_secret".into(),
            }),
        };
        let w: Withheld = ["itm_secret".to_string()].into();
        assert_eq!(checkin(c.clone(), &w).after_sleep, None);
        assert!(checkin(c, &Withheld::new()).after_sleep.is_some());
    }

    #[test]
    fn a_line_citing_a_withheld_item_is_dropped() {
        let receipt = |id: &str| Receipt {
            item_id: id.into(),
            kind: ItemKind::Text,
            source: Source::Phone,
            label: id.into(),
            kept_at: "2026-11-19T09:00:00Z".into(),
            duration_ms: None,
            span: None,
            unconfirmed: false,
            from_device: None,
            calendar: None,
        };
        let line = |t: &str, id: &str| ReflectionLine {
            text: t.into(),
            receipts: vec![receipt(id)],
        };
        let r = DayReflection {
            id: "ref_1".into(),
            period: Period {
                kind: PeriodKind::Day,
                start: "2026-11-19".into(),
                end: "2026-11-19".into(),
            },
            written_at: "2026-11-19T20:00:00Z".into(),
            written: WrittenWhen::OnDemand,
            so_far: true,
            from_count: 2,
            the_day: vec![
                line("**Tea** with Sam.", "itm_secret"),
                line("A walk.", "itm_ok"),
            ],
            felt: Vec::new(),
            good: Vec::new(),
            people: Vec::new(),
            still_open: Vec::new(),
            still_reading: 0,
            lines_removed: 0,
        };
        let w: Withheld = ["itm_secret".to_string()].into();
        let out = scrub_view(ReflectionView::Written { reflection: r }, &w);
        assert_eq!(first_sentence(&out).as_deref(), Some("A walk."));
    }

    #[test]
    fn a_recaps_health_leaves_out_withheld_readings_and_the_sentences_citing_them() {
        let reading = |id: &str| Receipt {
            item_id: id.into(),
            kind: ItemKind::Reading,
            source: Source::Phone,
            label: "Sleep".into(),
            kept_at: "2026-11-19T12:00:00Z".into(),
            duration_ms: None,
            span: None,
            unconfirmed: false,
            from_device: None,
            calendar: None,
        };
        let point = |id: &str| SeriesPoint {
            day: "2026-11-19".into(),
            value: 400.0,
            item_id: id.into(),
        };
        let series = |kind, ids: &[&str]| HealthSeries {
            kind,
            from: "2026-11-16".into(),
            to: "2026-11-22".into(),
            points: ids.iter().map(|i| point(i)).collect(),
            usual: None,
            journal_days: Vec::new(),
            pressure_usual: None,
            usual_bed: None,
            usual_wake: None,
        };
        let night = |id: &str| ReadingRow {
            item_id: id.into(),
            day: "2026-11-19".into(),
            start: "2026-11-18T23:00:00Z".into(),
            end: None,
            value: ReadingValue::Steps { total: 1 },
            source_device: None,
            phone: "Pixel".into(),
            removed: false,
            device_id: String::new(),
        };
        let line = |body: &str, journal: &str| Noticed {
            measure: NoticedMeasure::Sleep,
            topic: NoticedTopic::Good,
            days: 3,
            held: 3,
            other_days: 3,
            difference: 40,
            more: false,
            body: vec![reading(body)],
            journal: vec![reading(journal)],
        };
        let health = RecapHealth {
            nights: vec![night("itm_secret"), night("itm_ok")],
            sleep: Some(series(ReadingKind::Sleep, &["itm_secret", "itm_ok"])),
            steps: Some(series(ReadingKind::Steps, &["itm_secret"])),
            resting: Some(series(ReadingKind::RestingHeartRate, &["itm_ok"])),
            workouts: vec![reading("itm_secret"), reading("itm_ok")],
            noticed: vec![
                line("itm_secret", "itm_ok"),
                line("itm_ok", "itm_secret"),
                line("itm_ok", "itm_ok"),
            ],
        };
        let w: Withheld = ["itm_secret".to_string()].into();
        let out = scrub_health(health, &w);
        assert_eq!(out.nights.len(), 1);
        assert_eq!(out.sleep.unwrap().points.len(), 1);
        assert!(out.steps.is_none(), "a series with nothing left is gone");
        assert!(out.resting.is_some());
        assert_eq!(out.workouts.len(), 1);
        assert_eq!(
            out.noticed.len(),
            1,
            "a sentence citing either side is dropped"
        );
    }
}
