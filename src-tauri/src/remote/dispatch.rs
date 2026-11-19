//! What a paired phone may do: a typed match over [`RemoteCall`], the ten allowed calls and
//! the two phone verbs, each through the same engine and command cores the windows use. Never a
//! generic forwarder: nothing here names a command, a folder or another phone's item.
//!
//! Private stays home: asks are `public_only`, pages are the phone edition, items and originals
//! only when cited to this phone in the last hour and not private, and every answer passes an
//! outbound filter that drops anything naming a private item or memory.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use jiff::Timestamp;
use nocturne_contract::{
    AskDelta, AskPhase, CheckInInput, Edition, Feeling, GlanceReason, ItemView, Kept, Period,
    PeriodKind, PhoneError, PhoneItem, PhoneKept, PhoneReadiness, PhoneStatus, ReflectionView,
    RemoteAction, RemoteAnswer, RemoteCall, RemoteResult, SameNameAnswer, Source,
};
use nocturne_engine::{DeviceRow, Engine};
use tauri::{AppHandle, Manager, Runtime};
use tokio::sync::mpsc;

use super::crypto;
use super::incoming::Uploaded;
use super::journal as journal_view;
use super::limits::Bucket;
use super::protocol::*;
use super::session::{BoxFuture, CallCtx, Core, Dispatch, ReplySink};
use crate::ask::{self, AskLane, AskOptions, AskSink};
use crate::error::{CommandError, ErrorCode, blocking};
use crate::state::AppState;

/// How long a keep of bytes waits for its last chunk.
const UPLOAD_WAIT: Duration = Duration::from_secs(300);

/// The longest word of a feeling a phone may send. Nocturne's own words are far shorter.
const MAX_FEELING_WORD: usize = 40;

pub struct AppDispatch<R: Runtime> {
    pub app: AppHandle<R>,
}

impl<R: Runtime> Dispatch for AppDispatch<R> {
    fn call(&self, core: Arc<Core>, ctx: CallCtx) -> BoxFuture<'static, ()> {
        let app = self.app.clone();
        Box::pin(async move {
            let CallCtx {
                device,
                req,
                out,
                upload,
            } = ctx;
            let had_upload = upload.is_some();
            let call = Call {
                app,
                core: core.clone(),
                device,
                out,
                made_up: std::sync::Mutex::new(None),
            };
            let answer = call.run(req.call, upload).await;
            if had_upload {
                core.incoming
                    .cancel_call(&call.device.id, call.out.conv_id(), call.out.id);
            }
            if let Some(answer) = answer {
                call.send(answer).await;
            }
        })
    }
}

struct Call<R: Runtime> {
    app: AppHandle<R>,
    core: Arc<Core>,
    device: DeviceRow,
    out: ReplySink,
    /// The new id of a keep answered from what the phone sent (see `sent_answer`): it names
    /// nothing in the store, so the outbound filter has nothing to check and nothing to cite.
    made_up: std::sync::Mutex<Option<String>>,
}

fn err(code: PhoneError) -> Option<RemoteAnswer> {
    Some(RemoteAnswer::Error { code })
}

fn is_date(s: &str) -> bool {
    s.len() == 10 && s.parse::<jiff::civil::Date>().is_ok()
}

/// An item as a phone sees it: no path, text and summary cut to fit one frame.
pub fn phone_item(v: &ItemView) -> PhoneItem {
    let mut truncated = false;
    let mut cut = |t: &Option<String>| {
        t.as_deref().map(|t| {
            let (c, was) = crypto::cut_text(t);
            truncated |= was;
            c.to_string()
        })
    };
    let summary = cut(&v.summary);
    let text = cut(&v.text);
    PhoneItem {
        id: v.id.clone(),
        kind: v.kind,
        title: v.title.clone(),
        kept_at: v.kept_at.clone(),
        duration_ms: v.duration_ms,
        pages: v.pages,
        has_original: v.has_original,
        summary,
        text,
        truncated,
    }
}

/// Every item and memory id an answer cites, from its id fields only: text the phone sent
/// (a question echoed back, a note) is never read as an id.
pub fn cited_ids(answer: &RemoteAnswer) -> Vec<String> {
    use nocturne_contract::{PageLine, Receipt};
    let mut out: Vec<String> = Vec::new();
    let mut add = |id: &str| {
        if !out.iter().any(|o| o == id) {
            out.push(id.to_string());
        }
    };
    let receipts = |rs: &[Receipt], add: &mut dyn FnMut(&str)| {
        for r in rs {
            add(&r.item_id);
        }
    };
    let line = |l: &PageLine, add: &mut dyn FnMut(&str)| {
        l.memory_ids.iter().for_each(|m| add(m));
        receipts(&l.receipts, add);
        for c in &l.choices {
            add(&c.memory_id);
            add(&c.receipt.item_id);
        }
    };
    match answer {
        RemoteAnswer::AskDelta { delta } => {
            receipts(&delta.receipts, &mut add);
            delta.memory_ids.iter().for_each(|m| add(m));
            for n in &delta.near {
                add(&n.receipt.item_id);
            }
        }
        RemoteAnswer::Page { page } => {
            for l in page
                .needs_you
                .iter()
                .chain(&page.changed)
                .chain(&page.let_go)
                .chain(&page.upcoming)
                .chain(&page.learned)
            {
                line(l, &mut add);
            }
            if let Some(q) = &page.question {
                if let Some(m) = &q.memory_id {
                    add(m);
                }
                if let Some(r) = &q.receipt {
                    add(&r.item_id);
                }
            }
        }
        RemoteAnswer::Item { item } => add(&item.id),
        RemoteAnswer::Kept { kept } => add(&kept.item.id),
        RemoteAnswer::JournalDay { day } => journal_view::day_ids(day, &mut add),
        RemoteAnswer::LookBack { view } => journal_view::view_ids(view, &mut add),
        RemoteAnswer::CheckIn { checkin } => {
            add(&checkin.id);
            if let Some(about) = &checkin.about {
                add(about);
            }
        }
        _ => {}
    }
    out
}

impl<R: Runtime> Call<R> {
    fn engine(&self) -> Option<Engine> {
        self.app.state::<AppState>().engine().ok()
    }

    fn audit(&self, action: RemoteAction, result: RemoteResult, bytes: f64) {
        super::audit::log(&self.core, Some(&self.device.id), action, result, bytes);
    }

    /// The outbound filter: an answer naming anything private does not leave. Whatever it
    /// names that is public, the phone may open for the next hour.
    async fn send(&self, answer: RemoteAnswer) -> bool {
        if let RemoteAnswer::Kept { kept } = &answer
            && self
                .made_up
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_deref()
                == Some(kept.item.id.as_str())
        {
            return self.out.send(answer);
        }
        if let RemoteAnswer::Item { item } = &answer
            && self.core.made_up(&self.device.id, &item.id).is_some()
        {
            return self.out.send(answer);
        }
        let ids = cited_ids(&answer);
        if !ids.is_empty() {
            if self.engine().is_none() {
                return false;
            }
            let private = self
                .withheld(ids.clone())
                .await
                .unwrap_or_else(|_| ids.clone());
            if !private.is_empty() {
                tracing::warn!("an answer for a phone named something private; it was held back");
                return match answer {
                    RemoteAnswer::AskDelta { delta } if terminal(delta.phase) => {
                        self.out.send(RemoteAnswer::AskDelta {
                            delta: blank(&delta.ask_id, AskPhase::Unavailable),
                        })
                    }
                    RemoteAnswer::AskDelta { .. } => true,
                    _ => self.out.error(PhoneError::NotFound),
                };
            }
            self.core.cite(&self.device.id, ids);
        }
        self.out.send(answer)
    }

    async fn run(
        &self,
        call: RemoteCall,
        upload: Option<super::incoming::UploadDone>,
    ) -> Option<RemoteAnswer> {
        let me = &self.device.id;
        match call {
            RemoteCall::GetStatus => Some(self.status().await),
            RemoteCall::KeepText {
                text,
                client_id,
                feeling,
                good,
            } => {
                if crypto::check_id(&client_id).is_err()
                    || text.trim().is_empty()
                    || feeling.as_ref().is_some_and(|f| !feeling_ok(f))
                {
                    return err(PhoneError::Invalid);
                }
                if text.len() > MAX_TEXT {
                    return err(PhoneError::TooLarge);
                }
                if let Some(dup) = self.duplicate(&client_id).await {
                    return Some(dup);
                }
                if !self.core.limits.take(Bucket::Keeps, me) {
                    self.audit(RemoteAction::Limited, RemoteResult::Limited, 0.0);
                    return err(PhoneError::TooMany);
                }
                let size = text.len() as u64;
                if let Err(code) = self.core.incoming.charge_day(&self.core, me, size) {
                    return err(code);
                }
                let e = self.engine()?;
                let sent = Sent::Text(text.clone());
                let good = good.unwrap_or(false);
                let kept = blocking(move || e.keep_note(&text, Source::Phone, feeling, good)).await;
                let answer = self
                    .kept(kept.ok(), &client_id, RemoteAction::KeptText, 0.0, sent)
                    .await;
                if matches!(answer, Some(RemoteAnswer::Error { .. })) {
                    self.core.incoming.refund_day(&self.core, me, size);
                }
                answer
            }
            RemoteCall::KeepBytes {
                mime,
                size,
                client_id,
                caption,
                feeling,
                good,
            } => {
                let opts = UploadOpts {
                    caption,
                    recording: false,
                    feeling,
                    good: good.unwrap_or(false),
                };
                self.keep_upload(upload, &mime, size, &client_id, opts)
                    .await
            }
            RemoteCall::KeepRecording {
                mime,
                size,
                client_id,
                feeling,
                good,
            } => {
                let opts = UploadOpts {
                    caption: None,
                    recording: true,
                    feeling,
                    good: good.unwrap_or(false),
                };
                self.keep_upload(upload, &mime, size, &client_id, opts)
                    .await
            }
            RemoteCall::Unkeep { id } => self.unkeep(id).await,
            RemoteCall::Ask { question } => self.ask(question).await,
            RemoteCall::CancelAsk { ask_id } => {
                if crypto::check_id(&ask_id).is_err() {
                    return err(PhoneError::Invalid);
                }
                self.app
                    .state::<AppState>()
                    .asks
                    .cancel_in(&AskLane::Phone(me.clone()), &ask_id);
                Some(RemoteAnswer::Done)
            }
            RemoteCall::GetPage { date } => self.page(date).await,
            RemoteCall::ListPages => {
                let pages = crate::commands::page::pages_for(&self.app, Edition::Phone)
                    .await
                    .ok()?;
                Some(RemoteAnswer::Pages {
                    pages: pages
                        .into_iter()
                        .filter(|p| self.in_window(&p.date))
                        .collect(),
                })
            }
            RemoteCall::GetItem { id } => match self.item_for_phone(&id).await {
                Ok(item) => Some(RemoteAnswer::Item { item }),
                Err(code) => err(code),
            },
            RemoteCall::GetOriginal { id } => self.original(&id).await,
            RemoteCall::GetJournalDay { date } => self.journal_day(date).await,
            RemoteCall::GetLookBack { period } => self.look_back(period, false).await,
            RemoteCall::LookBack { period } => self.look_back(period, true).await,
            RemoteCall::KeepCheckIn { input, client_id } => self.check_in(input, client_id).await,
            RemoteCall::AnswerSameName {
                question_id,
                answer,
            } => self.same_name(question_id, answer).await,
            RemoteCall::Disconnect => {
                // The phone forgot this computer: it is removed here too, as from the desktop.
                let e = self.engine()?;
                let id = me.clone();
                let removed = blocking(move || e.revoke_device(&id)).await;
                self.audit(RemoteAction::Disconnected, RemoteResult::Ok, 0.0);
                self.out.send(RemoteAnswer::Done);
                self.out.end();
                if removed.is_ok() {
                    self.core.drop_device(me);
                    self.core.shell.changed();
                }
                None
            }
        }
    }

    async fn status(&self) -> RemoteAnswer {
        let state = self.app.state::<AppState>();
        let readiness = if state.status.is_paused(state.now()) {
            PhoneReadiness::Paused
        } else if state.mind.get().is_some() && !state.mind.is_loading() {
            PhoneReadiness::Ready
        } else {
            PhoneReadiness::GettingReady
        };
        let latest = crate::commands::page::pages_for(&self.app, Edition::Phone)
            .await
            .ok()
            .and_then(|p| p.into_iter().next());
        RemoteAnswer::Status {
            status: PhoneStatus {
                readiness,
                page_date: latest.as_ref().map(|p| p.date.clone()),
                needs_you: latest.map(|p| p.needs_you).unwrap_or(0),
            },
        }
    }

    /// A keep with this client id already made: the first item again.
    async fn duplicate(&self, client_id: &str) -> Option<RemoteAnswer> {
        let e = self.engine()?;
        let (me, cid) = (self.device.id.clone(), client_id.to_string());
        let item = blocking(move || {
            let Some(id) = e.remote_keep(&me, &cid)? else {
                return Ok(None);
            };
            e.get_item(&id).map(Some)
        })
        .await
        .ok()
        .flatten()?;
        Some(self.kept_answer(&item, true))
    }

    fn kept_answer(&self, item: &ItemView, duplicate: bool) -> RemoteAnswer {
        let until = item
            .kept_at
            .parse::<Timestamp>()
            .ok()
            .and_then(|t| {
                t.checked_add(jiff::SignedDuration::from_secs(TAKE_BACK_SECS as i64))
                    .ok()
            })
            .map(super::pairing::fmt_wall)
            .unwrap_or_default();
        RemoteAnswer::Kept {
            kept: PhoneKept {
                item: phone_item(item),
                duplicate,
                take_back_until: until,
            },
        }
    }

    async fn kept(
        &self,
        kept: Option<Kept>,
        client_id: &str,
        action: RemoteAction,
        bytes: f64,
        sent: Sent,
    ) -> Option<RemoteAnswer> {
        let Some(kept) = kept else {
            self.audit(action, RemoteResult::Failed, bytes);
            return err(PhoneError::Invalid);
        };
        let (item, duplicate) = match &kept {
            Kept::New { item } => (item.clone(), false),
            Kept::Duplicate { item } => (item.clone(), true),
        };
        if duplicate && !self.own_public(&item.id).await {
            // Already kept, but not by this phone, or private now: the answer a new keep gets,
            // from what the phone sent. It names no item, so it opens nothing, and a private
            // match answers exactly as a public one does.
            crate::commands::capture::announce(&self.app, std::slice::from_ref(&kept)).await;
            self.audit(action, RemoteResult::Ok, bytes);
            return Some(self.sent_answer(&item, sent));
        }
        // Only a new item is this phone's: a duplicate may be someone else's keep.
        if !duplicate && let Some(e) = self.engine() {
            let (id, me, cid) = (
                item.id.clone(),
                self.device.id.clone(),
                client_id.to_string(),
            );
            let _ = blocking(move || e.link_remote_keep(&id, &me, &cid)).await;
        }
        crate::commands::capture::announce(&self.app, std::slice::from_ref(&kept)).await;
        self.audit(action, RemoteResult::Ok, bytes);
        let item = match self.engine() {
            Some(e) => {
                let id = item.id.clone();
                blocking(move || e.get_item(&id)).await.unwrap_or(item)
            }
            None => item,
        };
        Some(self.kept_answer(&item, duplicate))
    }

    /// Whether this phone kept `item` itself and it may still go out.
    async fn own_public(&self, item: &str) -> bool {
        let Some(e) = self.engine() else {
            return false;
        };
        let (id, me) = (item.to_string(), self.device.id.clone());
        blocking(move || {
            let mine = e.remote_keep_owner(&id)?.is_some_and(|(d, _)| d == me);
            Ok(mine && e.private_ids(std::slice::from_ref(&id))?.is_empty())
        })
        .await
        .unwrap_or(false)
    }

    /// A keep's answer built only from what the phone sent and a new id of its own.
    fn sent_answer(&self, item: &ItemView, sent: Sent) -> RemoteAnswer {
        let now = self.app.state::<AppState>().now();
        let (title, text) = match sent {
            Sent::Text(t) => (nocturne_engine::text_title(&t), Some(t)),
            Sent::File(name) => (name, None),
        };
        let fresh = format!(
            "itm_{}",
            crypto::random::<8>()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let mut truncated = false;
        let text = text.map(|t| {
            let (cut, was) = crypto::cut_text(&t);
            truncated = was;
            cut.to_string()
        });
        let until = now
            .checked_add(jiff::SignedDuration::from_secs(TAKE_BACK_SECS as i64))
            .map(super::pairing::fmt_wall)
            .unwrap_or_default();
        *self.made_up.lock().unwrap_or_else(|e| e.into_inner()) = Some(fresh.clone());
        let made = PhoneItem {
            id: fresh,
            kind: item.kind,
            title,
            kept_at: format!("{now:.3}"),
            duration_ms: None,
            pages: None,
            has_original: item.has_original,
            summary: None,
            text,
            truncated,
        };
        self.core
            .remember_made_up(&self.device.id, made.clone(), now);
        RemoteAnswer::Kept {
            kept: PhoneKept {
                item: made,
                duplicate: false,
                take_back_until: until,
            },
        }
    }

    async fn keep_upload(
        &self,
        upload: Option<super::incoming::UploadDone>,
        mime: &str,
        size: f64,
        client_id: &str,
        opts: UploadOpts,
    ) -> Option<RemoteAnswer> {
        let me = self.device.id.clone();
        let Some(upload) = upload else {
            return err(PhoneError::Invalid);
        };
        if crypto::check_id(client_id).is_err() {
            return err(PhoneError::Invalid);
        }
        if opts
            .caption
            .as_deref()
            .is_some_and(|c| c.chars().count() > MAX_CAPTION)
        {
            return err(PhoneError::TooLarge);
        }
        if opts.feeling.as_ref().is_some_and(|f| !feeling_ok(f))
            || (opts.good && (opts.recording || !mime.starts_with("image/")))
        {
            return err(PhoneError::Invalid);
        }
        if let Some(dup) = self.duplicate(client_id).await {
            return Some(dup);
        }
        if opts.good && !self.good_room().await {
            return err(PhoneError::Invalid);
        }
        if !self.core.limits.take(Bucket::Keeps, &me) {
            self.audit(RemoteAction::Limited, RemoteResult::Limited, 0.0);
            return err(PhoneError::TooMany);
        }
        // Admitted: the day is charged now and given back if the keep does not happen.
        let size = size as u64;
        if let Err(code) = self.core.incoming.charge_day(&self.core, &me, size) {
            return err(code);
        }
        let answer = self.receive_upload(upload, mime, client_id, opts).await;
        if !matches!(answer, Some(RemoteAnswer::Kept { .. })) {
            self.core.incoming.refund_day(&self.core, &me, size);
        }
        answer
    }

    /// Whether today can take another good thing (three a day).
    async fn good_room(&self) -> bool {
        let Some(e) = self.engine() else {
            return false;
        };
        blocking(move || Ok(e.journal_day(e.journal_today()?)?.good.len() < 3))
            .await
            .unwrap_or(false)
    }

    async fn receive_upload(
        &self,
        upload: super::incoming::UploadDone,
        mime: &str,
        client_id: &str,
        opts: UploadOpts,
    ) -> Option<RemoteAnswer> {
        let UploadOpts {
            caption,
            recording,
            feeling,
            good,
        } = opts;
        let done = tokio::select! {
            d = upload => d,
            () = self.out.closed() => return None,
            () = tokio::time::sleep(UPLOAD_WAIT) => return err(PhoneError::TooLate),
        };
        let Uploaded { path, bytes, .. } = match done {
            Ok(Ok(u)) => u,
            Ok(Err(code)) => return err(code),
            Err(_) => return err(PhoneError::Invalid),
        };
        let ext = crate::commands::capture::extension_for(mime).unwrap_or("bin");
        let (action, base) = match (recording, mime) {
            (true, _) => (RemoteAction::KeptVoiceNote, "Voice note"),
            (_, m) if m.starts_with("image/") => (RemoteAction::KeptPhoto, "Photo"),
            (_, m) if m.starts_with("audio/") => (RemoteAction::KeptVoiceNote, "Voice note"),
            _ => (RemoteAction::KeptDocument, "Document"),
        };
        let name = caption
            .as_deref()
            .map(sanitise)
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| base.to_string());
        let named = match place(&path, &name, ext) {
            Ok(p) => p,
            Err(_) => {
                let _ = std::fs::remove_file(&path);
                return err(PhoneError::Busy);
            }
        };
        let e = self.engine()?;
        let file = named.clone();
        let words = caption
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .to_string();
        let kept = blocking(move || {
            let kept = e.keep_file(&file, Source::Phone)?;
            if let Kept::New { item } = &kept {
                // The item is kept either way; a feeling or a mark that cannot be added is
                // logged, not allowed to undo the keep.
                if let Some(f) = feeling
                    && let Err(error) = e.attach_feeling(&item.id, f, Source::Phone)
                {
                    tracing::warn!("a phone's feeling was not kept: {error}");
                }
                if good
                    && let Err(error) =
                        e.keep_good_thing(e.journal_today()?, &words, Some(&item.id), Source::Phone)
                {
                    tracing::warn!("a phone's good thing was not kept: {error}");
                }
            }
            Ok(kept)
        })
        .await;
        if let Some(dir) = named.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
        let sent = Sent::File(format!("{name}.{ext}"));
        self.kept(kept.ok(), client_id, action, bytes as f64, sent)
            .await
    }

    async fn unkeep(&self, id: String) -> Option<RemoteAnswer> {
        if crypto::check_id(&id).is_err() {
            return err(PhoneError::Invalid);
        }
        if let Some(made) = self.core.made_up(&self.device.id, &id) {
            return self.unkeep_made_up(&id, made.kept_at);
        }
        let e = self.engine()?;
        let check = id.clone();
        let owner = blocking(move || e.remote_keep_owner(&check)).await.ok()?;
        let Some((device, kept_at)) = owner else {
            return err(PhoneError::NotYours);
        };
        if device != self.device.id {
            return err(PhoneError::NotYours);
        }
        let now = self.app.state::<AppState>().now();
        if now.duration_since(kept_at) >= jiff::SignedDuration::from_secs(TAKE_BACK_SECS as i64) {
            return err(PhoneError::TooLate);
        }
        // Only this phone's asks: one may cite what is being taken back. The desktop's and
        // other phones' go on.
        self.app
            .state::<AppState>()
            .asks
            .cancel_lane(&AskLane::Phone(self.device.id.clone()));
        let e = self.engine()?;
        match blocking(move || {
            // What the same keep made beside it goes with it: its feeling, its good thing.
            for dependent in dependents(&e, &id).unwrap_or_default() {
                let _ = e.unkeep(&dependent);
            }
            e.unkeep(&id)
        })
        .await
        {
            Ok(_) => {
                self.audit(RemoteAction::TookBack, RemoteResult::Ok, 0.0);
                Some(RemoteAnswer::Done)
            }
            Err(_) => err(PhoneError::TooLate),
        }
    }

    /// Taking back a keep that was answered from what the phone sent: the same answers as a new
    /// keep gets, with nothing in the store to remove.
    fn unkeep_made_up(&self, id: &str, kept_at: Timestamp) -> Option<RemoteAnswer> {
        let now = self.app.state::<AppState>().now();
        if now.duration_since(kept_at) >= jiff::SignedDuration::from_secs(TAKE_BACK_SECS as i64) {
            return err(PhoneError::TooLate);
        }
        self.app
            .state::<AppState>()
            .asks
            .cancel_lane(&AskLane::Phone(self.device.id.clone()));
        self.core.forget_made_up(&self.device.id, id);
        self.audit(RemoteAction::TookBack, RemoteResult::Ok, 0.0);
        Some(RemoteAnswer::Done)
    }

    async fn ask(&self, question: String) -> Option<RemoteAnswer> {
        let question = question.trim().to_string();
        if question.is_empty() {
            return err(PhoneError::Invalid);
        }
        if question.len() > MAX_QUESTION {
            return err(PhoneError::TooLarge);
        }
        if !self.core.limits.take(Bucket::Asks, &self.device.id) {
            self.audit(RemoteAction::Limited, RemoteResult::Limited, 0.0);
            return err(PhoneError::TooMany);
        }
        let (tx, mut rx) = mpsc::unbounded_channel::<AskDelta>();
        let lane = AskLane::Phone(self.device.id.clone());
        let ask_id = ask::start_with(
            &self.app,
            question,
            AskOptions {
                public_only: true,
                lane: lane.clone(),
                sink: AskSink::Channel(tx),
                place: nocturne_contract::AskPlace::Auto,
                help: false,
            },
        );
        self.audit(RemoteAction::Asked, RemoteResult::Ok, 0.0);
        if !self.out.send(RemoteAnswer::AskStarted {
            ask_id: ask_id.clone(),
        }) {
            self.app.state::<AppState>().asks.cancel_in(&lane, &ask_id);
            return None;
        }
        let flush = Duration::from_millis(DELTA_FLUSH_MS);
        let mut held: Option<AskDelta> = None;
        let mut last_sent = tokio::time::Instant::now() - flush;
        loop {
            let wait = (last_sent + flush).saturating_duration_since(tokio::time::Instant::now());
            let next = tokio::select! {
                d = rx.recv() => d,
                () = self.out.closed() => {
                    self.app.state::<AppState>().asks.cancel_in(&lane, &ask_id);
                    return None;
                }
                () = tokio::time::sleep(wait), if held.is_some() => {
                    let d = held.take().expect("held");
                    if !self.send(RemoteAnswer::AskDelta { delta: d }).await {
                        self.app.state::<AppState>().asks.cancel_in(&lane, &ask_id);
                        return None;
                    }
                    last_sent = tokio::time::Instant::now();
                    continue;
                }
            };
            let Some(delta) = next else {
                break;
            };
            let end = terminal(delta.phase);
            held = Some(match held.take() {
                Some(mut h)
                    if h.phase == AskPhase::Streaming && delta.phase == AskPhase::Streaming =>
                {
                    let more = delta.text_delta.unwrap_or_default();
                    h.text_delta = Some(h.text_delta.unwrap_or_default() + &more);
                    h
                }
                Some(h) => {
                    let wait =
                        (last_sent + flush).saturating_duration_since(tokio::time::Instant::now());
                    tokio::time::sleep(wait).await;
                    if !self.send(RemoteAnswer::AskDelta { delta: h }).await {
                        self.app.state::<AppState>().asks.cancel_in(&lane, &ask_id);
                        return None;
                    }
                    last_sent = tokio::time::Instant::now();
                    delta
                }
                None => delta,
            });
            if end {
                let wait =
                    (last_sent + flush).saturating_duration_since(tokio::time::Instant::now());
                tokio::time::sleep(wait).await;
                if let Some(d) = held.take() {
                    self.send(RemoteAnswer::AskDelta { delta: d }).await;
                }
                return None;
            }
        }
        if let Some(d) = held.take() {
            self.send(RemoteAnswer::AskDelta { delta: d }).await;
        }
        None
    }

    /// A phone reads today's page and the seven days before it, by the local date.
    fn in_window(&self, date: &str) -> bool {
        let state = self.app.state::<AppState>();
        let today = state.now().to_zoned(state.clock.tz()).date();
        let Ok(day) = date.parse::<jiff::civil::Date>() else {
            return false;
        };
        let oldest = today
            .checked_sub(jiff::Span::new().days(i64::from(PAGES_BACK)))
            .unwrap_or(today);
        (oldest..=today).contains(&day)
    }

    async fn page(&self, date: Option<String>) -> Option<RemoteAnswer> {
        let day = match date.as_deref() {
            None => None,
            Some(d) if is_date(d) => {
                if !self.in_window(d) {
                    return err(PhoneError::NotFound);
                }
                d.parse().ok()
            }
            Some(_) => return err(PhoneError::Invalid),
        };
        // Again here: the page command adds what it knows of the tidy (unread items) after
        // the engine has made the phone edition.
        let page = crate::commands::page::page_view(&self.app, day, Edition::Phone)
            .await
            .ok()
            .and_then(|p| nocturne_engine::phone_view(p).ok());
        match page {
            Some(page) => {
                self.audit(RemoteAction::ReadPage, RemoteResult::Ok, 0.0);
                Some(RemoteAnswer::Page {
                    page: Box::new(page),
                })
            }
            None => err(PhoneError::NotFound),
        }
    }

    /// What `GetItem` answers: a made-up keep gives back only what the phone sent.
    async fn item_for_phone(&self, id: &str) -> Result<PhoneItem, PhoneError> {
        if crypto::check_id(id).is_ok()
            && let Some(made) = self.core.made_up(&self.device.id, id)
        {
            return Ok(made.item);
        }
        self.cited_item(id).await.map(|v| phone_item(&v))
    }

    /// An item this phone was sent in the last hour that is not private.
    async fn cited_item(&self, id: &str) -> Result<ItemView, PhoneError> {
        if crypto::check_id(id).is_err() {
            return Err(PhoneError::Invalid);
        }
        if !self.core.was_cited(&self.device.id, id) {
            return Err(PhoneError::NotFound);
        }
        let e = self.engine().ok_or(PhoneError::NotReady)?;
        let item = id.to_string();
        let v = blocking(move || e.get_item(&item))
            .await
            .map_err(|_| PhoneError::NotFound)?;
        match self.withheld(vec![id.to_string()]).await {
            Ok(held) if held.is_empty() => Ok(v),
            _ => Err(PhoneError::NotFound),
        }
    }

    /// Which of these ids must not reach a phone: what the engine withholds, other than a
    /// journal item, which cannot be marked private and is read in full. Fails closed.
    async fn withheld(&self, ids: Vec<String>) -> Result<Vec<String>, PhoneError> {
        let e = self.engine().ok_or(PhoneError::NotReady)?;
        blocking(move || {
            let mut held = Vec::new();
            for id in e.private_ids(&ids)? {
                let journal_item = !id.starts_with("mem_")
                    && e.get_item(&id)
                        .is_ok_and(|v| v.journal.is_some() && !v.private);
                if !journal_item {
                    held.push(id);
                }
            }
            Ok(held)
        })
        .await
        .map_err(|_| PhoneError::NotReady)
    }

    /// The ids a journal answer names that must stay home, for the scrub to take out.
    async fn withheld_in(&self, ids: Vec<String>) -> Result<journal_view::Withheld, PhoneError> {
        Ok(self.withheld(ids).await?.into_iter().collect())
    }

    async fn journal_day(&self, date: Option<String>) -> Option<RemoteAnswer> {
        let wanted = match date.as_deref() {
            None => None,
            Some(d) if is_date(d) => d.parse::<jiff::civil::Date>().ok(),
            Some(_) => return err(PhoneError::Invalid),
        };
        let e = self.engine()?;
        let read = blocking(move || {
            let day = match wanted {
                Some(d) => d,
                None => e.journal_today()?,
            };
            let journal = e.journal_day(day)?;
            let saved = match journal.reflection {
                Some(_) => e.saved_reflection(&e.period_of(PeriodKind::Day, day)?)?,
                None => None,
            };
            Ok((journal, saved))
        })
        .await;
        let (journal, saved) = match read {
            Ok(r) => r,
            Err(e) => return err(phone_code(&e)),
        };
        let mut ids = Vec::new();
        journal_view::day_ids(&journal, &mut |i| ids.push(i.to_string()));
        if let Some(v) = &saved {
            journal_view::view_ids(v, &mut |i| ids.push(i.to_string()));
        }
        let held = match self.withheld_in(ids).await {
            Ok(h) => h,
            Err(code) => return err(code),
        };
        let mut day = journal_view::scrub_day(journal, &held);
        if let Some(summary) = day.reflection.as_mut() {
            // The line it opens with is read again from what remains, never kept as it was.
            summary.first_sentence = saved
                .and_then(|v| journal_view::first_sentence(&journal_view::scrub_view(v, &held)))
                .unwrap_or_default();
        }
        self.audit(RemoteAction::ReadPage, RemoteResult::Ok, 0.0);
        Some(RemoteAnswer::JournalDay { day: Box::new(day) })
    }

    /// A day, week, month or year as the journal shows it. `write` asks the computer to write
    /// it first.
    async fn look_back(&self, period: Period, write: bool) -> Option<RemoteAnswer> {
        let e = self.engine()?;
        let asked = period.clone();
        let valid = blocking(move || {
            let start = asked
                .start
                .parse::<jiff::civil::Date>()
                .map_err(|_| nocturne_engine::Error::Invalid("a period starts on a day".into()))?;
            if e.period_of(asked.kind, start)? != asked {
                return Err(nocturne_engine::Error::Invalid(
                    "that is not a period".into(),
                ));
            }
            Ok(())
        })
        .await;
        if let Err(e) = valid {
            return err(phone_code(&e));
        }
        if write {
            if !self.core.limits.take(Bucket::Asks, &self.device.id) {
                self.audit(RemoteAction::Limited, RemoteResult::Limited, 0.0);
                return err(PhoneError::TooMany);
            }
            self.audit(RemoteAction::Asked, RemoteResult::Failed, 0.0);
            return err(PhoneError::NotReady);
        }
        let e = self.engine()?;
        let wanted = period.clone();
        let view = match blocking(move || stored_look_back(&e, &wanted)).await {
            Ok(v) => v,
            Err(e) => return err(phone_code(&e)),
        };
        let mut ids = Vec::new();
        journal_view::view_ids(&view, &mut |i| ids.push(i.to_string()));
        let held = match self.withheld_in(ids).await {
            Ok(h) => h,
            Err(code) => return err(code),
        };
        self.audit(RemoteAction::ReadPage, RemoteResult::Ok, 0.0);
        Some(RemoteAnswer::LookBack {
            view: Box::new(journal_view::scrub_view(view, &held)),
        })
    }

    async fn check_in(&self, input: CheckInInput, client_id: String) -> Option<RemoteAnswer> {
        let me = &self.device.id;
        // A phone keeps a check-in; changing one is for the computer.
        if crypto::check_id(&client_id).is_err()
            || input.replaces.is_some()
            || !feeling_ok(&input.feeling)
        {
            return err(PhoneError::Invalid);
        }
        let said =
            input.line.as_deref().map_or(0, str::len) + input.urge.as_deref().map_or(0, str::len);
        if said > MAX_TEXT {
            return err(PhoneError::TooLarge);
        }
        if let Some(earlier) = self.kept_check_in(&client_id).await {
            return Some(RemoteAnswer::CheckIn {
                checkin: Box::new(earlier),
            });
        }
        if !self.core.limits.take(Bucket::Keeps, me) {
            self.audit(RemoteAction::Limited, RemoteResult::Limited, 0.0);
            return err(PhoneError::TooMany);
        }
        let e = self.engine()?;
        let device = me.clone();
        let made = blocking(move || {
            let made = e.keep_checkin(input, Source::Phone)?;
            e.link_remote_keep(&made.id, &device, &client_id)?;
            let item = e.get_item(&made.id)?;
            // As the phone will read it: with the name of the phone it came from.
            Ok((find_check_in(&e, &made.id)?.unwrap_or(made), item))
        })
        .await;
        let Ok((checkin, item)) = made else {
            self.audit(RemoteAction::KeptText, RemoteResult::Failed, 0.0);
            return err(PhoneError::Invalid);
        };
        crate::commands::capture::announce(&self.app, &[Kept::New { item }]).await;
        self.audit(RemoteAction::KeptText, RemoteResult::Ok, 0.0);
        Some(RemoteAnswer::CheckIn {
            checkin: Box::new(checkin),
        })
    }

    /// The check-in this phone already kept under `client_id`, as it was kept.
    async fn kept_check_in(&self, client_id: &str) -> Option<nocturne_contract::CheckIn> {
        let e = self.engine()?;
        let (me, cid) = (self.device.id.clone(), client_id.to_string());
        blocking(move || {
            let Some(id) = e.remote_keep(&me, &cid)? else {
                return Ok(None);
            };
            find_check_in(&e, &id)
        })
        .await
        .ok()
        .flatten()
    }

    /// "Which {name} is this?": the one change a phone may make beyond keeping. It files one
    /// item and can be changed back on the computer.
    async fn same_name(&self, question_id: String, answer: SameNameAnswer) -> Option<RemoteAnswer> {
        if crypto::check_id(&question_id).is_err() {
            return err(PhoneError::Invalid);
        }
        let e = self.engine()?;
        let asked = question_id.clone();
        let found = blocking(move || {
            Ok(e.same_name_questions(None)?
                .into_iter()
                .find(|q| q.id == asked))
        })
        .await;
        let Ok(Some(question)) = found else {
            return err(PhoneError::NotFound);
        };
        if let SameNameAnswer::Entity { entity_id } = &answer
            && !question.options.iter().any(|o| &o.entity_id == entity_id)
        {
            return err(PhoneError::Invalid);
        }
        match self.withheld(vec![question.receipt.item_id.clone()]).await {
            Ok(held) if held.is_empty() => {}
            _ => return err(PhoneError::NotFound),
        }
        let e = self.engine()?;
        match blocking(move || e.answer_same_name(&question_id, answer)).await {
            Ok(answered) => {
                self.audit(RemoteAction::AnsweredName, RemoteResult::Ok, 0.0);
                Some(RemoteAnswer::SameName { answered })
            }
            Err(e) => {
                self.audit(RemoteAction::AnsweredName, RemoteResult::Failed, 0.0);
                err(phone_code(&e))
            }
        }
    }

    async fn original(&self, id: &str) -> Option<RemoteAnswer> {
        let item = match self.cited_item(id).await {
            Ok(v) => v,
            Err(code) => return err(code),
        };
        let Some(path) = item.original_path.as_deref().map(std::path::PathBuf::from) else {
            return err(PhoneError::NotFound);
        };
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let mime = nocturne_engine::kind_for_ext(&ext)
            .map(|(_, m)| m)
            .unwrap_or("application/octet-stream");
        let Ok(mut file) = tokio::fs::File::open(&path).await else {
            return err(PhoneError::NotFound);
        };
        let size = file.metadata().await.map(|m| m.len()).unwrap_or(0);
        if size > MAX_ITEM {
            return err(PhoneError::TooLarge);
        }
        let Some(_slot) = self.core.original_slot(&self.device.id) else {
            return err(PhoneError::Busy);
        };
        if !self.out.send(RemoteAnswer::OriginalStart {
            mime: mime.to_string(),
            size: size as f64,
        }) {
            return None;
        }
        self.audit(RemoteAction::OpenedOriginal, RemoteResult::Ok, size as f64);
        use tokio::io::AsyncReadExt;
        let mut seq = 0u32;
        let mut sent = 0u64;
        let mut buf = vec![0u8; CHUNK];
        loop {
            let mut n = 0;
            while n < CHUNK {
                match file.read(&mut buf[n..]).await {
                    Ok(0) => break,
                    Ok(k) => n += k,
                    Err(_) => return None,
                }
            }
            sent += n as u64;
            let last = sent >= size || n < CHUNK;
            if !self.out.chunk(seq, last, &buf[..n]).await || last {
                return None;
            }
            seq += 1;
        }
    }
}

/// What a phone sent as a feeling must be a point on the field, up to three short words.
fn feeling_ok(f: &Feeling) -> bool {
    let on_field = |v: f32| v.is_finite() && (-1.0..=1.0).contains(&v);
    on_field(f.point.pleasant)
        && on_field(f.point.energy)
        && f.words.len() <= 3
        && f.words
            .iter()
            .all(|w| (1..=MAX_FEELING_WORD).contains(&w.chars().count()))
}

fn phone_code(e: &CommandError) -> PhoneError {
    match e.code {
        ErrorCode::NotFound => PhoneError::NotFound,
        ErrorCode::Invalid => PhoneError::Invalid,
        ErrorCode::TooLate => PhoneError::TooLate,
        ErrorCode::Busy => PhoneError::Busy,
        _ => PhoneError::NotReady,
    }
}

/// What the computer holds for a period: what was written, else the period at a glance, else
/// nothing. The desktop's journal module, which also writes, replaces this.
fn stored_look_back(e: &Engine, period: &Period) -> nocturne_engine::Result<ReflectionView> {
    if let Some(view) = e.saved_reflection(period)? {
        return Ok(view);
    }
    let glance = e.glance(period)?;
    if glance.kept.is_empty()
        && glance.felt.is_empty()
        && glance.good.is_empty()
        && glance.entry_lines.is_empty()
    {
        return Ok(ReflectionView::Nothing {
            period: period.clone(),
        });
    }
    Ok(ReflectionView::Glance {
        glance,
        reason: GlanceReason::NotOnThisComputer,
    })
}

/// A check-in as the journal shows it, found on the day it was kept or the day before.
fn find_check_in(
    e: &Engine,
    id: &str,
) -> nocturne_engine::Result<Option<nocturne_contract::CheckIn>> {
    let kept = e.journal_day_of(e.get_item(id)?.kept_at.parse()?)?;
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

/// The items kept beside this one in the same keep: a check-in about it, a good thing made
/// of it.
fn dependents(e: &Engine, id: &str) -> nocturne_engine::Result<Vec<String>> {
    let kept = e.get_item(id)?.kept_at.parse()?;
    let day = e.journal_day(e.journal_day_of(kept)?)?;
    let mut out: Vec<String> = day
        .checkins
        .iter()
        .filter(|c| c.about.as_deref() == Some(id))
        .map(|c| c.id.clone())
        .collect();
    out.extend(
        day.good
            .iter()
            .filter(|g| g.photo.as_ref().is_some_and(|p| p.id == id))
            .map(|g| g.id.clone()),
    );
    Ok(out)
}

/// What a phone sent with the bytes of a keep besides the file itself.
struct UploadOpts {
    caption: Option<String>,
    recording: bool,
    feeling: Option<Feeling>,
    good: bool,
}

/// What a phone sent with a keep: the text, or the name its file was kept under.
enum Sent {
    Text(String),
    File(String),
}

fn terminal(p: AskPhase) -> bool {
    matches!(
        p,
        AskPhase::Done | AskPhase::NotSure | AskPhase::Nothing | AskPhase::Unavailable
    )
}

fn blank(ask_id: &str, phase: AskPhase) -> AskDelta {
    AskDelta {
        ask_id: ask_id.to_string(),
        phase,
        text_delta: None,
        receipts: Vec::new(),
        memory_ids: Vec::new(),
        near: Vec::new(),
        found: None,
        eta_seconds: None,
        plain: false,
        answered_by: None,
        left_out: None,
        send_id: None,
        help: None,
    }
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

/// Move an upload into a folder of its own under its final name.
fn place(path: &Path, name: &str, ext: &str) -> std::io::Result<std::path::PathBuf> {
    let dir = path.with_extension("named");
    crate::fsutil::create_private_dir(&dir)?;
    let named = dir.join(format!("{name}.{ext}"));
    std::fs::rename(path, &named)?;
    Ok(named)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captions_become_plain_names() {
        assert_eq!(sanitise("Bin day / ../../etc"), "Bin day etc");
        assert_eq!(sanitise("  receipt: café  "), "receipt café");
        assert_eq!(sanitise("\u{202e}gpj.exe"), "gpj exe");
    }

    #[test]
    fn only_id_fields_are_cited() {
        let echoed = RemoteAnswer::AskStarted {
            ask_id: "see itm_0123456789abcdef and mem_fedcba9876543210".into(),
        };
        assert!(cited_ids(&echoed).is_empty(), "text is not an id");
        let delta = AskDelta {
            memory_ids: vec!["mem_fedcba9876543210".into()],
            text_delta: Some("itm_1111111111111111".into()),
            ..blank("ask_1", AskPhase::Done)
        };
        assert_eq!(
            cited_ids(&RemoteAnswer::AskDelta { delta }),
            vec!["mem_fedcba9876543210"]
        );
    }
}
