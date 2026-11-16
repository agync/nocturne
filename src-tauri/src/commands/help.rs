use nocturne_contract::GuideView;
use tauri::{AppHandle, Runtime};

use super::run;
use crate::error::CommandError;

/// The Nocturne guide as it came with this version, for the guide reader. It is read from inside
/// the app: nothing is fetched.
#[tauri::command]
pub async fn get_guide<R: Runtime>(app: AppHandle<R>) -> Result<GuideView, CommandError> {
    run(app, move |_| async move { Ok(crate::help::view()) }).await
}
