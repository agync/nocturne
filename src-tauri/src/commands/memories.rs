//! Memories and the people, places and things they are about.

use nocturne_contract::{
    EntityKind, EntityView, MemoryFilter, MemoryView, MergeOutcome, RefileTarget, Refiled,
    RenameOutcome,
};
use nocturne_engine::CorrectPatch;
use tauri::{AppHandle, Runtime};

use super::{change, read};
use crate::error::CommandError;

#[tauri::command]
pub async fn confirm_memory<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    confirmed: bool,
) -> Result<(), CommandError> {
    change(app, move |e| e.confirm(&id, confirmed)).await
}

#[tauri::command]
pub async fn list_memories<R: Runtime>(
    app: AppHandle<R>,
    filter: MemoryFilter,
) -> Result<Vec<MemoryView>, CommandError> {
    read(app, move |e| e.memories(filter)).await
}

#[tauri::command]
pub async fn get_entity<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<EntityView, CommandError> {
    read(app, move |e| e.entity(&id).map(|(view, _)| view)).await
}

/// People, places and things, optionally of one kind and matching a text.
#[tauri::command]
pub async fn list_entities<R: Runtime>(
    app: AppHandle<R>,
    kind: Option<EntityKind>,
    query: Option<String>,
) -> Result<Vec<EntityView>, CommandError> {
    read(app, move |e| e.entities_matching(kind, query.as_deref())).await
}

/// Correct a memory: the old one closes into history, the new one is returned. `text` is the
/// whole corrected sentence.
#[tauri::command]
pub async fn correct_memory<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    text: String,
) -> Result<MemoryView, CommandError> {
    change(app, move |e| {
        let new_id = e.correct(&id, CorrectPatch::text(&text))?;
        e.get_memory(&new_id)
    })
    .await
}

/// Rename a person, place or thing. A name another already has changes nothing and says which,
/// so the interface can offer a merge.
#[tauri::command]
pub async fn rename_entity<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    name: String,
) -> Result<RenameOutcome, CommandError> {
    change(app, move |e| e.rename_entity(&id, &name)).await
}

/// Make two people, places or things one, named as `into_id` is.
#[tauri::command]
pub async fn merge_entities<R: Runtime>(
    app: AppHandle<R>,
    from_id: String,
    into_id: String,
) -> Result<MergeOutcome, CommandError> {
    change(app, move |e| e.merge_entities(&from_id, &into_id)).await
}

/// Move one memory to another person, place or thing, or to a new one.
#[tauri::command]
pub async fn refile_memory<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    target: RefileTarget,
) -> Result<Refiled, CommandError> {
    change(app, move |e| e.refile_memory(&id, target)).await
}

/// Take back a rename, a merge or a move.
#[tauri::command]
pub async fn undo_change<R: Runtime>(
    app: AppHandle<R>,
    change_id: String,
) -> Result<(), CommandError> {
    change(app, move |e| e.undo_change(&change_id)).await
}

#[tauri::command]
pub async fn set_keep_forever<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    keep_forever: bool,
) -> Result<(), CommandError> {
    change(app, move |e| e.set_keep_forever(&id, keep_forever)).await
}

#[tauri::command]
pub async fn set_private<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    private: bool,
) -> Result<(), CommandError> {
    change(app, move |e| e.set_private(&id, private)).await
}

/// Make an item (and what was read from it) private, or not.
#[tauri::command]
pub async fn set_item_private<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    private: bool,
) -> Result<(), CommandError> {
    change(app, move |e| e.set_item_private(&id, private)).await
}

/// Keep an item (and what was read from it) forever, or stop.
#[tauri::command]
pub async fn set_item_keep_forever<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    keep_forever: bool,
) -> Result<(), CommandError> {
    change(app, move |e| e.set_item_keep_forever(&id, keep_forever)).await
}

/// Hide a memory for some days; it comes back.
#[tauri::command]
pub async fn snooze<R: Runtime>(
    app: AppHandle<R>,
    memory_id: String,
    days: u32,
) -> Result<(), CommandError> {
    change(app, move |e| e.snooze(&memory_id, days)).await
}

#[tauri::command]
pub async fn restore<R: Runtime>(app: AppHandle<R>, ids: Vec<String>) -> Result<(), CommandError> {
    change(app, move |e| e.restore(&ids)).await
}
