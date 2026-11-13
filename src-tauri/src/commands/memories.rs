//! Memories and the people, places and things they are about.

use nocturne_contract::{EntityKind, EntityView, MemoryFilter, MemoryView};
use nocturne_engine::CorrectPatch;
use tauri::{AppHandle, Runtime};

use super::{change, not_yet, read};
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

/// People, places and things, optionally of one kind and matching a text (a name, case
/// ignored).
#[tauri::command]
pub async fn list_entities<R: Runtime>(
    app: AppHandle<R>,
    kind: Option<EntityKind>,
    query: Option<String>,
) -> Result<Vec<EntityView>, CommandError> {
    read(app, move |e| {
        let needle = query
            .as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(str::to_lowercase);
        Ok(e.entities(kind)?
            .into_iter()
            .filter(|v| {
                needle.as_deref().is_none_or(|n| {
                    v.name.to_lowercase().contains(n)
                        || v.role
                            .as_deref()
                            .is_some_and(|r| r.to_lowercase().contains(n))
                })
            })
            .collect())
    })
    .await
}

/// Correct a memory: the old one closes into history, the new one is returned.
#[tauri::command]
pub async fn correct_memory<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    text: String,
) -> Result<MemoryView, CommandError> {
    change(app, move |e| {
        let new_id = e.correct(&id, CorrectPatch::object(&text))?;
        e.get_memory(&new_id)
    })
    .await
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

/// Item-level privacy needs `Engine::set_item_private` (engine round 2).
#[tauri::command]
pub async fn set_item_private<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    private: bool,
) -> Result<(), CommandError> {
    let _ = (app, id, private);
    Err(not_yet("making an item private"))
}

/// Item-level keep-forever needs `Engine::set_item_keep_forever` (engine round 2).
#[tauri::command]
pub async fn set_item_keep_forever<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    keep_forever: bool,
) -> Result<(), CommandError> {
    let _ = (app, id, keep_forever);
    Err(not_yet("keeping an item forever"))
}

/// Snoozing a memory needs `Engine::snooze` (engine round 2).
#[tauri::command]
pub async fn snooze<R: Runtime>(
    app: AppHandle<R>,
    memory_id: String,
    days: u32,
) -> Result<(), CommandError> {
    let _ = (app, memory_id, days);
    Err(not_yet("snoozing"))
}

#[tauri::command]
pub async fn restore<R: Runtime>(app: AppHandle<R>, ids: Vec<String>) -> Result<(), CommandError> {
    change(app, move |e| e.restore(&ids)).await
}
