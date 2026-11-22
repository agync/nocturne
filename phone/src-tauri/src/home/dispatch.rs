//! The screens' calls answered from the phone's own memory: the same [`RemoteCall`]s and
//! [`RemoteAnswer`]s a companion phone speaks with its computer, so the screens do not change.
//! The engine bodies follow the computer's listener, without what guards a computer from a phone:
//! no limits, no activity list, and nothing withheld as private, since this phone is the home.
//! Kept things belong to this phone's own row, so a keep sent twice is kept once.
//!
//! Every function here blocks: call it off the async workers.

use std::path::Path;

use jiff::{SignedDuration, Timestamp};
use nocturne_channel::crypto::check_id;
use nocturne_channel::protocol::{MAX_CAPTION, MAX_TEXT, TAKE_BACK_SECS};
use nocturne_contract::{
    CheckIn, CheckInInput, Edition, Feeling, ItemView, Kept, PageView, PhoneError, PhoneItem,
    PhoneKept, PhoneReadiness, PhoneStatus, QuestionChoice, QuestionKind, RemoteAnswer, RemoteCall,
    SameNameAnswer, Source,
};
use nocturne_engine::{Engine, KeepPlace};

use super::Open;

/// The longest word of a feeling the screens may send. Nocturne's own words are far shorter.
const MAX_FEELING_WORD: usize = 40;

fn err(code: PhoneError) -> RemoteAnswer {
    RemoteAnswer::Error { code }
}

fn code(e: &nocturne_engine::Error) -> PhoneError {
    use nocturne_engine::Error;
    match e {
        Error::NotFound(_) => PhoneError::NotFound,
        Error::Invalid(_) => PhoneError::Invalid,
        Error::TooLate => PhoneError::TooLate,
        Error::Busy | Error::LeaseLost => PhoneError::Busy,
        _ => PhoneError::NotReady,
    }
}

fn is_date(s: &str) -> bool {
    s.len() == 10 && s.parse::<jiff::civil::Date>().is_ok()
}

fn fmt_wall(t: Timestamp) -> String {
    t.strftime("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// An item as the screens read it. Nothing is cut: no frame to fit on the phone itself.
pub fn phone_item(v: &ItemView) -> PhoneItem {
    PhoneItem {
        id: v.id.clone(),
        kind: v.kind,
        title: v.title.clone(),
        kept_at: v.kept_at.clone(),
        duration_ms: v.duration_ms,
        pages: v.pages,
        has_original: v.has_original,
        summary: v.summary.clone(),
        text: v.text.clone(),
        truncated: false,
    }
}

/// A feeling must be a point on the field, up to three short words.
fn feeling_ok(f: &Feeling) -> bool {
    let on_field = |v: f32| v.is_finite() && (-1.0..=1.0).contains(&v);
    on_field(f.point.pleasant)
        && on_field(f.point.energy)
        && f.words.len() <= 3
        && f.words
            .iter()
            .all(|w| (1..=MAX_FEELING_WORD).contains(&w.chars().count()))
}

/// Answer one screen call. `GetOriginal` and the keeps with bytes have their own ways in
/// ([`original`], [`keep_bytes`]); asking a mind waits for one.
pub fn answer(open: &Open, call: RemoteCall) -> RemoteAnswer {
    let e = &open.engine;
    let me = open.device.as_str();
    match call {
        RemoteCall::GetStatus => status(e),
        RemoteCall::KeepText {
            text,
            client_id,
            feeling,
            good,
            captured_at,
            web,
        } => keep_text(e, me, text, client_id, feeling, good, captured_at, web),
        RemoteCall::Unkeep { id } => unkeep(e, me, &id),
        RemoteCall::GetPage { date } => page(e, date),
        RemoteCall::ListPages => match e.pages() {
            Ok(pages) => RemoteAnswer::Pages { pages },
            Err(x) => err(code(&x)),
        },
        RemoteCall::GetItem { id } => {
            if check_id(&id).is_err() {
                return err(PhoneError::Invalid);
            }
            match e.get_item(&id) {
                Ok(v) => RemoteAnswer::Item {
                    item: phone_item(&v),
                },
                Err(x) => err(code(&x)),
            }
        }
        RemoteCall::GetJournalDay { date } => journal_day(e, date),
        RemoteCall::KeepCheckIn {
            input,
            client_id,
            captured_at,
        } => check_in(e, me, input, client_id, captured_at),
        RemoteCall::AnswerSameName {
            question_id,
            answer,
        } => same_name(e, question_id, answer),
        RemoteCall::AnswerJournalQuestion {
            page_date,
            choice,
            line,
            client_id,
        } => journal_answer(e, me, page_date, choice, line, client_id),
        // Answers, reflections and recaps need a mind.
        RemoteCall::Ask { .. }
        | RemoteCall::CancelAsk { .. }
        | RemoteCall::GetLookBack { .. }
        | RemoteCall::LookBack { .. } => err(PhoneError::NotReady),
        // Bytes come in through `keep_bytes`, an original goes out through `original`, and the
        // rest are spoken only between a phone and a computer.
        RemoteCall::KeepBytes { .. }
        | RemoteCall::KeepRecording { .. }
        | RemoteCall::GetOriginal { .. }
        | RemoteCall::SendCalendar { .. }
        | RemoteCall::SendReadings { .. }
        | RemoteCall::Introduce { .. }
        | RemoteCall::SeeOnlineChange
        | RemoteCall::OfferMemory { .. }
        | RemoteCall::MemoryDone { .. }
        | RemoteCall::Disconnect => err(PhoneError::Invalid),
    }
}

/// Ready whenever the memory is open; the newest saved page, if a mind ever wrote one.
fn status(e: &Engine) -> RemoteAnswer {
    let latest = e.pages().ok().and_then(|p| p.into_iter().next());
    RemoteAnswer::Status {
        status: PhoneStatus {
            readiness: PhoneReadiness::Ready,
            page_date: latest.as_ref().map(|p| p.date.clone()),
            needs_you: latest.map(|p| p.needs_you).unwrap_or(0),
        },
    }
}

fn kept_answer(item: &ItemView, duplicate: bool) -> RemoteAnswer {
    let until = item
        .kept_at
        .parse::<Timestamp>()
        .ok()
        .and_then(|t| {
            t.checked_add(SignedDuration::from_secs(TAKE_BACK_SECS as i64))
                .ok()
        })
        .map(fmt_wall)
        .unwrap_or_default();
    RemoteAnswer::Kept {
        kept: PhoneKept {
            item: phone_item(item),
            duplicate,
            take_back_until: until,
        },
    }
}

/// A keep with this client id already made, with the feeling and the good thing it asked for:
/// the first item again. One still missing a mark goes on to make it.
fn duplicate(
    e: &Engine,
    me: &str,
    client_id: &str,
    feeling: bool,
    good: bool,
) -> Option<RemoteAnswer> {
    let id = e.remote_keep(me, client_id).ok()??;
    let place = KeepPlace {
        day: None,
        phone: Some((me, client_id)),
    };
    if !e.keep_is_marked(&place, feeling, good).ok()? {
        return None;
    }
    let item = e.get_item(&id).ok()?;
    Some(kept_answer(&item, true))
}

/// The keep's item, linked to this phone when it is new, and its answer. A feeling or a mark
/// that failed leaves the item kept and says so, so the screens send the keep again.
fn finish_keep(
    e: &Engine,
    me: &str,
    client_id: &str,
    made: nocturne_engine::Result<(Kept, nocturne_engine::Result<()>)>,
) -> RemoteAnswer {
    let (kept, marks) = match made {
        Ok(m) => m,
        Err(x) => {
            tracing::warn!("a keep on this phone was not made: {x}");
            return err(match x {
                nocturne_engine::Error::Io(_) => PhoneError::Busy,
                _ => PhoneError::Invalid,
            });
        }
    };
    let (item, duplicate) = match &kept {
        Kept::New { item } => (item, false),
        Kept::Duplicate { item } => (item, true),
    };
    if !duplicate && let Err(x) = e.link_remote_keep(&item.id, me, client_id) {
        tracing::warn!("a keep could not be tied to this phone: {x}");
    }
    if let Err(x) = marks {
        tracing::warn!("a feeling or good thing was not kept: {x}");
        return err(match x {
            nocturne_engine::Error::Invalid(_) => PhoneError::Invalid,
            _ => PhoneError::Busy,
        });
    }
    let item = e.get_item(&item.id).unwrap_or_else(|_| item.clone());
    kept_answer(&item, duplicate)
}

#[allow(clippy::too_many_arguments)]
fn keep_text(
    e: &Engine,
    me: &str,
    text: String,
    client_id: String,
    feeling: Option<Feeling>,
    good: Option<bool>,
    captured_at: Option<String>,
    web: bool,
) -> RemoteAnswer {
    if check_id(&client_id).is_err()
        || text.trim().is_empty()
        || feeling.as_ref().is_some_and(|f| !feeling_ok(f))
    {
        return err(PhoneError::Invalid);
    }
    if text.len() > MAX_TEXT {
        return err(PhoneError::TooLarge);
    }
    let good = good.unwrap_or(false);
    if let Some(dup) = duplicate(e, me, &client_id, feeling.is_some(), good) {
        return dup;
    }
    let made = (|| {
        let place = KeepPlace {
            day: Some(e.capture_day(captured_at.as_deref())?),
            phone: Some((me, &client_id)),
        };
        // Something shared from a web page is kept as pasted web content: untrusted.
        let source = if web {
            Source::PastedWeb
        } else {
            Source::Phone
        };
        let kept = e.keep_note_item(&text, source, good, &place)?;
        let marks = match feeling {
            Some(f) => e.feel_kept(&kept, f, Source::Phone, &place).map(drop),
            None => Ok(()),
        };
        Ok((kept, marks))
    })();
    finish_keep(e, me, &client_id, made)
}

/// The file extension for a kept file's type.
pub fn extension_for(mime: &str) -> Option<&'static str> {
    let essence = mime.split(';').next()?.trim().to_ascii_lowercase();
    Some(match essence.as_str() {
        "audio/webm" => "webm",
        "audio/ogg" | "audio/opus" => "ogg",
        "audio/wav" | "audio/x-wav" | "audio/wave" => "wav",
        "audio/mp4" | "audio/x-m4a" | "audio/aac" => "m4a",
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/flac" | "audio/x-flac" => "flac",
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/heic" => "heic",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "application/pdf" => "pdf",
        "text/plain" => "txt",
        "text/markdown" => "md",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => "docx",
        _ => return None,
    })
}

/// A caption as a file name: letters, digits, spaces and a few marks, nothing that names a
/// folder.
pub fn sanitise(caption: &str) -> String {
    let cleaned: String = caption
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " -_,()'".contains(c) {
                c
            } else {
                ' '
            }
        })
        .collect();
    cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_CAPTION)
        .collect()
}

/// A photo, a voice note or a document, with its bytes put together from the screens' slices.
/// The file is written under its kept name in a folder of its own in `scratch`, kept, and
/// removed.
pub fn keep_bytes(open: &Open, call: RemoteCall, bytes: &[u8], scratch: &Path) -> RemoteAnswer {
    let e = &open.engine;
    let me = open.device.as_str();
    let (mime, client_id, caption, feeling, good, captured_at, recording) = match call {
        RemoteCall::KeepBytes {
            mime,
            client_id,
            caption,
            feeling,
            good,
            captured_at,
            ..
        } => (
            mime,
            client_id,
            caption,
            feeling,
            good.unwrap_or(false),
            captured_at,
            false,
        ),
        RemoteCall::KeepRecording {
            mime,
            client_id,
            feeling,
            good,
            captured_at,
            ..
        } => (
            mime,
            client_id,
            None,
            feeling,
            good.unwrap_or(false),
            captured_at,
            true,
        ),
        _ => return err(PhoneError::Invalid),
    };
    if check_id(&client_id).is_err() || bytes.is_empty() {
        return err(PhoneError::Invalid);
    }
    if caption
        .as_deref()
        .is_some_and(|c| c.chars().count() > MAX_CAPTION)
    {
        return err(PhoneError::TooLarge);
    }
    if feeling.as_ref().is_some_and(|f| !feeling_ok(f))
        || (good && (recording || !mime.starts_with("image/")))
    {
        return err(PhoneError::Invalid);
    }
    if let Some(dup) = duplicate(e, me, &client_id, feeling.is_some(), good) {
        return dup;
    }
    if good {
        let room = (|| {
            let place = KeepPlace {
                day: None,
                phone: Some((me, &client_id)),
            };
            e.good_room(e.capture_day(captured_at.as_deref())?, &place)
        })();
        if !room.unwrap_or(false) {
            return err(PhoneError::Invalid);
        }
    }
    let ext = extension_for(&mime).unwrap_or("bin");
    let base = match (recording, mime.as_str()) {
        (true, _) => "Voice note",
        (_, m) if m.starts_with("image/") => "Photo",
        (_, m) if m.starts_with("audio/") => "Voice note",
        _ => "Document",
    };
    let name = caption
        .as_deref()
        .map(sanitise)
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| base.to_string());
    let folder = scratch.join(&client_id);
    let file = folder.join(format!("{name}.{ext}"));
    let written = std::fs::create_dir_all(&folder).and_then(|()| std::fs::write(&file, bytes));
    if let Err(x) = written {
        let _ = std::fs::remove_dir_all(&folder);
        tracing::warn!("a keep could not be written: {x}");
        return err(PhoneError::Busy);
    }
    let words = caption
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let made = (|| {
        let day = e.capture_day(captured_at.as_deref())?;
        let place = KeepPlace {
            day: Some(day),
            phone: Some((me, &client_id)),
        };
        let kept = e.keep_file(&file, Source::Phone)?;
        let (Kept::New { item } | Kept::Duplicate { item }) = &kept;
        if matches!(kept, Kept::New { .. }) {
            e.link_remote_keep(&item.id, me, &client_id)?;
            e.place_on_day(&item.id, day)?;
        }
        let marks = (|| {
            if let Some(f) = feeling {
                e.feel_kept(&kept, f, Source::Phone, &place)?;
            }
            if good {
                // A private item is not shown beside a good thing; its words stand alone.
                let photo = (!item.private).then_some(item.id.as_str());
                if photo.is_some() || !words.is_empty() {
                    e.keep_good_thing_at(day, &words, photo, Source::Phone, &place)?;
                }
            }
            Ok(())
        })();
        Ok((kept, marks))
    })();
    let _ = std::fs::remove_dir_all(&folder);
    finish_keep(e, me, &client_id, made)
}

/// Take back a keep this phone made, within its window. What the same keep made beside it (its
/// feeling, its good thing) goes with it.
fn unkeep(e: &Engine, me: &str, id: &str) -> RemoteAnswer {
    if check_id(id).is_err() {
        return err(PhoneError::Invalid);
    }
    let owner = match e.remote_keep_owner(id) {
        Ok(o) => o,
        Err(x) => return err(code(&x)),
    };
    let Some((device, kept_at)) = owner else {
        return err(PhoneError::NotYours);
    };
    if device != me {
        return err(PhoneError::NotYours);
    }
    if e.clock_now().duration_since(kept_at) >= SignedDuration::from_secs(TAKE_BACK_SECS as i64) {
        return err(PhoneError::TooLate);
    }
    for dependent in dependents(e, me, id).unwrap_or_default() {
        let _ = e.unkeep(&dependent);
    }
    match e.unkeep(id) {
        Ok(_) => RemoteAnswer::Done,
        Err(_) => err(PhoneError::TooLate),
    }
}

/// What the same keep made beside this item: its feeling, its good thing.
fn dependents(e: &Engine, me: &str, id: &str) -> nocturne_engine::Result<Vec<String>> {
    let Some(client) = e.remote_keep_client(id)? else {
        return Ok(Vec::new());
    };
    let place = KeepPlace {
        day: None,
        phone: Some((me, &client)),
    };
    Ok(e.keep_records(&place)?
        .into_iter()
        .filter(|r| r != id)
        .collect())
}

/// A saved page for `day` (the newest when none), or the next page composed from what is kept
/// now. With no mind nothing saves pages, so the page is composed each time.
fn page_view(e: &Engine, day: Option<jiff::civil::Date>) -> nocturne_engine::Result<PageView> {
    if let Some(p) = e.page(day)? {
        return Ok(p);
    }
    let next = e.next_page_date()?;
    match day {
        Some(d) if d != next => Err(nocturne_engine::Error::NotFound(format!("page {d}"))),
        _ => e.compose_page(next, Edition::Screen),
    }
}

fn page(e: &Engine, date: Option<String>) -> RemoteAnswer {
    let day = match date.as_deref() {
        None => None,
        Some(d) if is_date(d) => d.parse().ok(),
        Some(_) => return err(PhoneError::Invalid),
    };
    match page_view(e, day) {
        Ok(page) => RemoteAnswer::Page {
            page: Box::new(page),
        },
        Err(x) => err(code(&x)),
    }
}

fn journal_day(e: &Engine, date: Option<String>) -> RemoteAnswer {
    let wanted = match date.as_deref() {
        None => None,
        Some(d) if is_date(d) => d.parse::<jiff::civil::Date>().ok(),
        Some(_) => return err(PhoneError::Invalid),
    };
    let read = (|| {
        let day = match wanted {
            Some(d) => d,
            None => e.journal_today()?,
        };
        e.journal_day(day)
    })();
    match read {
        Ok(day) => RemoteAnswer::JournalDay { day: Box::new(day) },
        Err(x) => err(code(&x)),
    }
}

fn check_in(
    e: &Engine,
    me: &str,
    input: CheckInInput,
    client_id: String,
    captured_at: Option<String>,
) -> RemoteAnswer {
    // A check-in is kept here, never changed, as from a companion.
    if check_id(&client_id).is_err() || input.replaces.is_some() || !feeling_ok(&input.feeling) {
        return err(PhoneError::Invalid);
    }
    let said =
        input.line.as_deref().map_or(0, str::len) + input.urge.as_deref().map_or(0, str::len);
    if said > MAX_TEXT {
        return err(PhoneError::TooLarge);
    }
    match e.remote_keep(me, &client_id) {
        Ok(Some(id)) => {
            return match find_check_in(e, &id) {
                Ok(Some(earlier)) => RemoteAnswer::CheckIn {
                    checkin: Box::new(earlier),
                },
                // Kept before, since replaced or taken back: never kept again.
                Ok(None) => RemoteAnswer::Done,
                Err(x) => err(code(&x)),
            };
        }
        Ok(None) => {}
        Err(x) => return err(code(&x)),
    }
    let made = (|| {
        let place = KeepPlace {
            day: Some(e.capture_day(captured_at.as_deref())?),
            phone: None,
        };
        let made = e.keep_checkin_at(input, Source::Phone, &place)?;
        e.link_remote_keep(&made.id, me, &client_id)?;
        Ok::<_, nocturne_engine::Error>(find_check_in(e, &made.id)?.unwrap_or(made))
    })();
    match made {
        Ok(checkin) => RemoteAnswer::CheckIn {
            checkin: Box::new(checkin),
        },
        Err(x) => {
            tracing::warn!("a check-in was not kept: {x}");
            err(PhoneError::Invalid)
        }
    }
}

/// A check-in as the journal shows it, found on its day or the day before.
fn find_check_in(e: &Engine, id: &str) -> nocturne_engine::Result<Option<CheckIn>> {
    let kept = e.journal_item_day(id)?;
    for day in [Some(kept), kept.yesterday().ok()].into_iter().flatten() {
        if let Some(c) = e
            .journal_day(day)?
            .checkins
            .into_iter()
            .find(|c| c.id == id)
        {
            return Ok(Some(c));
        }
    }
    Ok(None)
}

/// "Which {name} is this?": it files one item, and can be changed back.
fn same_name(e: &Engine, question_id: String, answer: SameNameAnswer) -> RemoteAnswer {
    if check_id(&question_id).is_err() {
        return err(PhoneError::Invalid);
    }
    let question = match e.same_name_questions(None) {
        Ok(qs) => qs.into_iter().find(|q| q.id == question_id),
        Err(x) => return err(code(&x)),
    };
    let Some(question) = question else {
        return err(PhoneError::NotFound);
    };
    if let SameNameAnswer::Entity { entity_id } = &answer
        && !question.options.iter().any(|o| &o.entity_id == entity_id)
    {
        return err(PhoneError::Invalid);
    }
    match e.answer_same_name(&question_id, answer) {
        Ok(answered) => RemoteAnswer::SameName { answered },
        Err(x) => err(code(&x)),
    }
}

/// The page's question from the journal, kept once per client id; answered with the page as it
/// stands after.
fn journal_answer(
    e: &Engine,
    me: &str,
    page_date: String,
    choice: QuestionChoice,
    line: Option<String>,
    client_id: String,
) -> RemoteAnswer {
    if check_id(&client_id).is_err() || !is_date(&page_date) {
        return err(PhoneError::Invalid);
    }
    if line.as_deref().map_or(0, str::len) > MAX_TEXT {
        return err(PhoneError::TooLarge);
    }
    let Ok(day) = page_date.parse::<jiff::civil::Date>() else {
        return err(PhoneError::Invalid);
    };
    let asked = page_view(e, Some(day))
        .ok()
        .and_then(|p| p.question)
        .is_some_and(|q| q.kind == QuestionKind::Journal);
    if !asked {
        return err(PhoneError::NotFound);
    }
    if let Err(x) = e.answer_from_phone(day, choice, line.as_deref(), me, &client_id) {
        return err(code(&x));
    }
    page(e, Some(page_date))
}

/// An item's original: its type and bytes. Only a kept file has one.
pub fn original(open: &Open, id: &str) -> Result<(String, Vec<u8>), PhoneError> {
    if check_id(id).is_err() {
        return Err(PhoneError::Invalid);
    }
    let item = open.engine.get_item(id).map_err(|x| code(&x))?;
    let path = item
        .original_path
        .as_deref()
        .map(std::path::PathBuf::from)
        .ok_or(PhoneError::NotFound)?;
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let mime = nocturne_engine::kind_for_ext(&ext)
        .map(|(_, m)| m)
        .unwrap_or("application/octet-stream");
    let bytes = std::fs::read(&path).map_err(|_| PhoneError::NotFound)?;
    Ok((mime.to_string(), bytes))
}
