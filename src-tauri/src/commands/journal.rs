//! The journal: the day as it opens, what is written and kept in it, the names it mentions and
//! the days to choose from. Days are `YYYY-MM-DD` on both sides.

use jiff::civil::Date;
use nocturne_contract::{
    CheckIn, CheckInInput, EntryBlock, Feeling, FeelingPart, GoodThing, JournalDay, Kept, Mention,
    PeriodKind, PeriodSummary, RefileTarget, SameNameAnswer, SameNameAnswered, Source,
};
use tauri::{AppHandle, Manager, Runtime};

use super::capture::{announce, note_quick};
use super::{change, engine, parse_date, read, run};
use crate::error::{CommandError, blocking};
use crate::state::AppState;

fn days(days: Vec<Date>) -> Vec<String> {
    days.into_iter().map(|d| d.to_string()).collect()
}

/// One day as the journal opens it; today when `day` is absent.
#[tauri::command]
pub async fn get_journal_day<R: Runtime>(
    app: AppHandle<R>,
    day: Option<String>,
) -> Result<JournalDay, CommandError> {
    let day = day.as_deref().map(parse_date).transpose()?;
    read(app, move |e| {
        let day = match day {
            Some(d) => d,
            None => e.journal_today()?,
        };
        e.journal_day(day)
    })
    .await
}

/// Every day with anything in the journal, oldest first.
#[tauri::command]
pub async fn list_journal_days<R: Runtime>(app: AppHandle<R>) -> Result<Vec<String>, CommandError> {
    read(app, |e| e.journal_days().map(days)).await
}

#[tauri::command]
pub async fn list_periods<R: Runtime>(
    app: AppHandle<R>,
    kind: PeriodKind,
) -> Result<Vec<PeriodSummary>, CommandError> {
    read(app, move |e| e.periods(kind)).await
}

#[tauri::command]
pub async fn dismiss_journal_intro<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    change(app, |e| e.dismiss_journal_intro()).await
}

#[tauri::command]
pub async fn save_entry_text<R: Runtime>(
    app: AppHandle<R>,
    day: String,
    item_id: Option<String>,
    block: u32,
    text: String,
) -> Result<Option<EntryBlock>, CommandError> {
    let day = parse_date(&day)?;
    change(app, move |e| {
        e.save_entry_text(day, item_id.as_deref(), block, &text)
    })
    .await
}

/// Place a voice note or photo kept with `keep_bytes` in the day's entry.
#[tauri::command]
pub async fn place_in_entry<R: Runtime>(
    app: AppHandle<R>,
    item_id: String,
    day: String,
    block: u32,
) -> Result<EntryBlock, CommandError> {
    let day = parse_date(&day)?;
    change(app, move |e| e.place_in_entry(&item_id, day, block)).await
}

#[tauri::command]
pub async fn describe_photo<R: Runtime>(
    app: AppHandle<R>,
    item_id: String,
    text: String,
) -> Result<(), CommandError> {
    change(app, move |e| e.describe_photo(&item_id, &text)).await
}

/// The person left the journal: the day's entry is read now rather than after a pause.
#[tauri::command]
pub async fn read_entry_now<R: Runtime>(
    app: AppHandle<R>,
    day: String,
) -> Result<(), CommandError> {
    let day = parse_date(&day)?;
    run(app, move |app| async move {
        let e = engine(&app)?;
        blocking(move || e.read_entry_now(day)).await?;
        app.state::<AppState>().wake_pipeline();
        Ok(())
    })
    .await
}

/// Keep a quick note, with an optional feeling beside it and the good-thing mark.
#[tauri::command]
pub async fn keep_note<R: Runtime>(
    app: AppHandle<R>,
    webview: tauri::Webview<R>,
    text: String,
    source: Source,
    feeling: Option<Feeling>,
    good: bool,
) -> Result<Kept, CommandError> {
    run(app, move |app| async move {
        let e = engine(&app)?;
        let kept = blocking(move || e.keep_note(&text, source, feeling, good)).await?;
        note_quick(&app, &webview, std::slice::from_ref(&kept));
        announce(&app, std::slice::from_ref(&kept)).await;
        Ok(kept)
    })
    .await
}

/// The four words this person used most in a part, or the first four of the list.
#[tauri::command]
pub async fn top_feeling_words<R: Runtime>(
    app: AppHandle<R>,
    part: FeelingPart,
) -> Result<Vec<String>, CommandError> {
    read(app, move |e| e.top_feeling_words(part)).await
}

#[tauri::command]
pub async fn keep_check_in<R: Runtime>(
    app: AppHandle<R>,
    input: CheckInInput,
) -> Result<CheckIn, CommandError> {
    run(app, move |app| async move {
        let e = engine(&app)?;
        let (checkin, item) = blocking(move || {
            let checkin = e.keep_checkin(input, Source::Journal)?;
            let item = e.get_item(&checkin.id)?;
            Ok((checkin, item))
        })
        .await?;
        announce(&app, &[Kept::New { item }]).await;
        Ok(checkin)
    })
    .await
}

#[tauri::command]
pub async fn mark_tried<R: Runtime>(
    app: AppHandle<R>,
    checkin_id: String,
    idea: String,
) -> Result<(), CommandError> {
    change(app, move |e| e.mark_tried(&checkin_id, &idea)).await
}

/// A good thing on `day`, with a photo already kept.
#[tauri::command]
pub async fn keep_good_thing<R: Runtime>(
    app: AppHandle<R>,
    day: String,
    text: String,
    photo_item_id: Option<String>,
) -> Result<GoodThing, CommandError> {
    let day = parse_date(&day)?;
    run(app, move |app| async move {
        let e = engine(&app)?;
        let (thing, item) = blocking(move || {
            let thing = e.keep_good_thing(day, &text, photo_item_id.as_deref(), Source::Journal)?;
            let item = e.get_item(&thing.id)?;
            Ok((thing, item))
        })
        .await?;
        announce(&app, &[Kept::New { item }]).await;
        Ok(thing)
    })
    .await
}

/// "Leave out of reflections" and its Undo.
#[tauri::command]
pub async fn set_left_out<R: Runtime>(
    app: AppHandle<R>,
    item_id: String,
    left_out: bool,
) -> Result<(), CommandError> {
    change(app, move |e| e.set_left_out(&item_id, left_out)).await
}

/// "Move to {weekday}" for a quick note kept after the day it was about.
#[tauri::command]
pub async fn move_to_day<R: Runtime>(
    app: AppHandle<R>,
    item_id: String,
    day: String,
) -> Result<(), CommandError> {
    let day = parse_date(&day)?;
    change(app, move |e| e.move_to_day(&item_id, day)).await
}

/// "That's right", with the role typed on a new person's card.
#[tauri::command]
pub async fn confirm_mention<R: Runtime>(
    app: AppHandle<R>,
    mention_id: String,
    role: Option<String>,
) -> Result<Mention, CommandError> {
    change(app, move |e| {
        e.confirm_mention(&mention_id, role.as_deref())
    })
    .await
}

/// "Not about {name}".
#[tauri::command]
pub async fn remove_mention<R: Runtime>(
    app: AppHandle<R>,
    mention_id: String,
) -> Result<Mention, CommandError> {
    change(app, move |e| e.remove_mention(&mention_id)).await
}

/// Undo of "Not about {name}".
#[tauri::command]
pub async fn restore_mention<R: Runtime>(
    app: AppHandle<R>,
    mention_id: String,
) -> Result<Mention, CommandError> {
    change(app, move |e| e.restore_mention(&mention_id)).await
}

/// "Someone else…" and "Not a person": only the memories from this item move.
#[tauri::command]
pub async fn refile_mention<R: Runtime>(
    app: AppHandle<R>,
    mention_id: String,
    target: RefileTarget,
) -> Result<Mention, CommandError> {
    change(app, move |e| e.refile_mention(&mention_id, target)).await
}

#[tauri::command]
pub async fn answer_same_name<R: Runtime>(
    app: AppHandle<R>,
    question_id: String,
    answer: SameNameAnswer,
) -> Result<SameNameAnswered, CommandError> {
    change(app, move |e| e.answer_same_name(&question_id, answer)).await
}

/// Days an entity appears in the journal, newest first.
#[tauri::command]
pub async fn entity_journal_days<R: Runtime>(
    app: AppHandle<R>,
    entity_id: String,
) -> Result<Vec<String>, CommandError> {
    read(app, move |e| e.entity_journal_days(&entity_id).map(days)).await
}
