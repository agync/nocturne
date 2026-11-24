//! The screens' way to the nightly backup. Nothing here shows a screen: the screens lane builds
//! them from `docs/design/phone-first.md` ("Backup"). Each call that reaches the account runs on a
//! blocking worker. A night and a restore report progress as `backup-progress` events, and
//! "Stop" is `backup_stop`, as for a backup file.

use tauri::{AppHandle, Manager, Runtime, State};

use super::{BackupProblem, BackupStatus, FoundBackup, LastNight, Opening, PhoneBackup, Restored};
use crate::home::Home;
use crate::state::AppState;

async fn blocking<R: Runtime, T: Send + 'static>(
    app: AppHandle<R>,
    f: impl FnOnce(&AppHandle<R>) -> Result<T, BackupProblem> + Send + 'static,
) -> Result<T, BackupProblem> {
    tauri::async_runtime::spawn_blocking(move || f(&app))
        .await
        .unwrap_or(Err(BackupProblem::Failed))
}

/// How the backup stands, for This phone.
#[tauri::command]
pub async fn backup_status<R: Runtime>(app: AppHandle<R>) -> Result<BackupStatus, BackupProblem> {
    blocking(app, |app| {
        Ok(app.state::<PhoneBackup>().status_with(&app.state::<Home>()))
    })
    .await
}

/// A phrase of six words made up by Nocturne: step 2, and "Another phrase".
#[tauri::command]
pub fn backup_new_phrase(backup: State<'_, PhoneBackup>) -> String {
    backup.new_phrase()
}

/// "Turn on" with the phrase on screen, Nocturne's or the person's own. The first night runs
/// with `backup_now`.
#[tauri::command]
pub async fn backup_turn_on<R: Runtime>(
    app: AppHandle<R>,
    phrase: String,
) -> Result<(), BackupProblem> {
    blocking(app, move |app| app.state::<PhoneBackup>().turn_on(&phrase)).await
}

/// A recovery key, made now and returned this once. It opens nothing until it is kept.
#[tauri::command]
pub async fn backup_recovery_key<R: Runtime>(app: AppHandle<R>) -> Result<String, BackupProblem> {
    blocking(app, |app| app.state::<PhoneBackup>().recovery_key()).await
}

/// "Print or save…" went through: the key just made opens the backup, in place of any before.
#[tauri::command]
pub async fn backup_keep_recovery_key<R: Runtime>(app: AppHandle<R>) -> Result<(), BackupProblem> {
    blocking(app, |app| app.state::<PhoneBackup>().keep_recovery_key()).await
}

/// "Not now" or "Cancel" before the key was saved: it never opens the backup.
#[tauri::command]
pub fn backup_drop_recovery_key(backup: State<'_, PhoneBackup>) {
    backup.drop_recovery_key();
}

/// "Change the phrase…", then step 2's phrase and "Save".
#[tauri::command]
pub async fn backup_change_phrase<R: Runtime>(
    app: AppHandle<R>,
    phrase: String,
) -> Result<(), BackupProblem> {
    blocking(app, move |app| {
        app.state::<PhoneBackup>().change_phrase(&phrase)
    })
    .await
}

/// The "Wi-Fi only" switch.
#[tauri::command]
pub fn backup_set_wifi_only(backup: State<'_, PhoneBackup>, on: bool) {
    backup.set_wifi_only(on);
}

/// "Back up now", and the first backup after "Turn on". `overMobileData` when the person chose
/// to go past "Wi-Fi only" this once.
#[tauri::command]
pub async fn backup_now<R: Runtime>(
    app: AppHandle<R>,
    over_mobile_data: Option<bool>,
) -> Result<LastNight, BackupProblem> {
    blocking(app, move |app| {
        let progress = crate::home::commands::progress(app.clone());
        app.state::<PhoneBackup>().back_up_now(
            &app.state::<Home>(),
            over_mobile_data.unwrap_or(false),
            &progress,
        )
    })
    .await
}

/// "Turn off", with or without "Also delete the backup".
#[tauri::command]
pub async fn backup_turn_off<R: Runtime>(
    app: AppHandle<R>,
    also_delete: bool,
) -> Result<(), BackupProblem> {
    blocking(app, move |app| {
        app.state::<PhoneBackup>().turn_off(also_delete)
    })
    .await
}

/// The switch, on again after a kept backup was turned off.
#[tauri::command]
pub async fn backup_turn_on_again<R: Runtime>(app: AppHandle<R>) -> Result<(), BackupProblem> {
    blocking(app, |app| app.state::<PhoneBackup>().turn_on_again()).await
}

/// "Bring back your memory": the newest backup in the account, before the phrase. `None` when
/// there is none.
#[tauri::command]
pub async fn backup_found<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<FoundBackup>, BackupProblem> {
    blocking(app, |app| app.state::<PhoneBackup>().found()).await
}

/// "Which night?": the nights the backup holds, newest first. Before the phrase, `opening` is
/// `None` and each night is what the account's listing says; once the phrase opens it, each
/// also says what it holds.
#[tauri::command]
pub async fn backup_nights<R: Runtime>(
    app: AppHandle<R>,
    opening: Option<Opening>,
) -> Result<Vec<NightView>, BackupProblem> {
    blocking(app, move |app| {
        let backup = app.state::<PhoneBackup>();
        let Some(opening) = opening else {
            return Ok(backup
                .listed()?
                .into_iter()
                .map(|n| NightView {
                    count: n.count,
                    made_at: n.made_at.map(|t| t.to_string()),
                    bytes: n.bytes,
                    items: None,
                    first: None,
                })
                .collect());
        };
        Ok(backup
            .nights(&opening)?
            .into_iter()
            .map(|n| NightView {
                count: n.count,
                made_at: Some(n.made_at),
                bytes: Some(n.bytes),
                items: Some(n.items),
                first: n.first,
            })
            .collect())
    })
    .await
}

/// One night, as "Which night?" lists it. What the account does not say is `None`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NightView {
    pub count: u64,
    pub made_at: Option<String>,
    pub bytes: Option<u64>,
    /// The things it holds, once the phrase opened it.
    pub items: Option<u32>,
    /// When the first of them was kept, when the night says.
    pub first: Option<String>,
}

/// This phone's side of "This phone has a memory too": its things, and when the first was kept.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Here {
    pub things: u32,
    pub first: Option<String>,
}

/// What the memory on this phone holds, for the counts before a restore brings things in.
#[tauri::command]
pub async fn backup_here<R: Runtime>(app: AppHandle<R>) -> Result<Here, BackupProblem> {
    blocking(app, |app| {
        let home = app.state::<Home>();
        let (_use, open) = home.using()?;
        let things = open
            .engine
            .stats()
            .map_err(|_| BackupProblem::Failed)?
            .item_count;
        let first = open
            .engine
            .first_kept()
            .map_err(|_| BackupProblem::Failed)?;
        Ok(Here { things, first })
    })
    .await
}

/// Whether the phone is on Wi-Fi now: `false` on mobile data, `None` offline or when it can't
/// say. A restore warns before it comes down over mobile data.
#[tauri::command]
pub fn backup_on_wifi<R: Runtime>(app: AppHandle<R>) -> Option<bool> {
    use tauri_plugin_nocturne_system::SystemExt;
    app.system().power().ok().and_then(|p| p.unmetered)
}

/// "Bring it back" with the phrase or the recovery key: the night of `night`, or the newest.
/// On an empty phone it becomes the memory; on one that holds a memory its things come in.
#[tauri::command]
pub async fn backup_restore<R: Runtime>(
    app: AppHandle<R>,
    opening: Opening,
    night: Option<u64>,
) -> Result<Restored, BackupProblem> {
    let restored = blocking(app.clone(), move |app| {
        let paired = app.state::<AppState>().store.identity().is_some();
        let progress = crate::home::commands::progress(app.clone());
        app.state::<PhoneBackup>()
            .restore(&app.state::<Home>(), paired, &opening, night, &progress)
    })
    .await?;
    // Only a memory that changed places is sent to again; one that things joined is the same.
    if matches!(restored, Restored::Replaced { .. }) {
        crate::home::commands::restored(&app);
    }
    Ok(restored)
}

/// "Make this phone the one in use".
#[tauri::command]
pub async fn backup_make_in_use<R: Runtime>(app: AppHandle<R>) -> Result<(), BackupProblem> {
    blocking(app, |app| {
        app.state::<PhoneBackup>().make_in_use(&app.state::<Home>())
    })
    .await
}
