//! Model downloads for a job the person changed, on the engine's pull and downloads APIs, inside
//! the shared download window. One download at a time. Pausing gives the window back and keeps
//! what was received; cancelling removes it; a failure keeps it, so trying again carries on.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tokio::sync::{Notify, watch};
use tokio::task::JoinHandle;

use crate::client::{Action, Client, Download};
use crate::error::RuntimeError;
use crate::supervisor::{Control, OnlineHold};

/// A download's progress, in bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobProgress {
    pub received: u64,
    pub total: u64,
    pub paused: bool,
    pub done: bool,
    pub failed: bool,
}

/// Called once when a download finishes: the model and its bytes.
pub type OnDownload = Arc<dyn Fn(&str, u64) + Send + Sync>;

/// How often the engine's jobs are read, and how long a silent or refusing engine is waited for.
#[derive(Debug, Clone, Copy)]
pub struct JobOptions {
    pub poll: Duration,
    pub retry_delay: Duration,
    /// An engine that does not answer, or keeps refusing the pull, for this long fails the job.
    pub give_up: Duration,
}

impl Default for JobOptions {
    fn default() -> Self {
        JobOptions {
            poll: Duration::from_millis(500),
            retry_delay: Duration::from_secs(2),
            give_up: Duration::from_secs(90),
        }
    }
}

struct Run {
    model: String,
    tx: Arc<watch::Sender<JobProgress>>,
    task: JoinHandle<()>,
    gate: Arc<Gate>,
}

/// What the person asked of a running download, read by its task, and its share of the window.
#[derive(Default)]
struct Gate {
    paused: Mutex<bool>,
    hold: Mutex<Option<OnlineHold>>,
    /// The download is over (finished, failed or cancelled) and its share of the window is
    /// given back for good. Set only with `life` held.
    ended: AtomicBool,
    /// Held by whoever asks for the window on this download's behalf, from the check that the
    /// download is not over until the hold is placed, and by the end of the download. So the
    /// window is never asked to open once the download is over, and an ask already under way
    /// is placed before the end gives everything back.
    life: tokio::sync::Mutex<()>,
    wake: Notify,
}

impl Gate {
    fn paused(&self) -> bool {
        *lock(&self.paused)
    }

    fn ended(&self) -> bool {
        self.ended.load(Ordering::SeqCst)
    }

    async fn release(&self) {
        let hold = lock(&self.hold).take();
        if let Some(h) = hold {
            h.release().await;
        }
    }

    /// Gives the window back at the end of the download. What asks for the window later (a
    /// follow of a new engine, a resume) finds the download over and asks for nothing.
    async fn end(&self) {
        let hold = {
            let _life = self.life.lock().await;
            self.ended.store(true, Ordering::SeqCst);
            lock(&self.hold).take()
        };
        if let Some(h) = hold {
            h.release().await;
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn job_id(model: &str) -> String {
    format!("model:{model}")
}

pub struct ModelJobs {
    engine: Arc<Engine>,
    opts: JobOptions,
    run: Mutex<Option<Run>>,
}

/// The engine the downloads talk to. A replaced engine (started again after it gave up) is
/// followed through [`ModelJobs::follow`], never cached.
struct Engine {
    control: Mutex<Control>,
}

impl Engine {
    fn control(&self) -> Control {
        lock(&self.control).clone()
    }

    fn client(&self) -> Client {
        self.control().client()
    }
}

impl ModelJobs {
    pub fn new(control: Control) -> Self {
        Self::with_options(control, JobOptions::default())
    }

    pub fn with_options(control: Control, opts: JobOptions) -> Self {
        ModelJobs {
            engine: Arc::new(Engine {
                control: Mutex::new(control),
            }),
            opts,
            run: Mutex::new(None),
        }
    }

    /// Talks to `control`'s engine from now on. A download that was running carries on there,
    /// with a share of that engine's download window.
    pub async fn follow(&self, control: Control) -> Result<(), RuntimeError> {
        *lock(&self.engine.control) = control.clone();
        if let Some((_, gate, _)) = self.running() {
            let _life = gate.life.lock().await;
            if gate.paused() || gate.ended() {
                return Ok(());
            }
            let hold = control.hold_online().await?;
            // Checked again with the hold's lock taken: a pause meanwhile gave the window back,
            // and this hold goes back with it.
            let mut held = lock(&gate.hold);
            if gate.paused() || gate.ended() {
                drop(held);
                hold.release().await;
                return Ok(());
            }
            let stale = held.replace(hold);
            drop(held);
            drop(stale);
            gate.wake.notify_one();
        }
        Ok(())
    }

    /// The model being fetched, while its download runs or is paused.
    pub fn model(&self) -> Option<String> {
        lock(&self.run)
            .as_ref()
            .filter(|r| !r.task.is_finished())
            .map(|r| r.model.clone())
    }

    /// Fetches `model` inside `hold`, which is given back when the download ends, pauses or is
    /// cancelled. Catalogue models only: a `user.*` id is refused. Asking for the model already
    /// being fetched returns its progress; another while one runs is `Busy`.
    pub async fn get(
        &self,
        model: &str,
        hold: OnlineHold,
        on_download: OnDownload,
    ) -> Result<watch::Receiver<JobProgress>, RuntimeError> {
        if model.starts_with("user.") || model.is_empty() {
            return Err(RuntimeError::NotFound(model.to_string()));
        }
        let mut run = lock(&self.run);
        if let Some(r) = run.as_ref().filter(|r| !r.task.is_finished()) {
            return if r.model == model {
                Ok(r.tx.subscribe())
            } else {
                Err(RuntimeError::Busy)
            };
        }
        let tx = Arc::new(watch::channel(JobProgress::default()).0);
        let gate = Arc::new(Gate::default());
        *lock(&gate.hold) = Some(hold);
        let task = tokio::spawn(fetch(
            self.engine.clone(),
            self.opts,
            model.to_string(),
            tx.clone(),
            gate.clone(),
            on_download,
        ));
        let rx = tx.subscribe();
        *run = Some(Run {
            model: model.to_string(),
            tx,
            task,
            gate,
        });
        Ok(rx)
    }

    /// Stops the download and gives the window back. What was received stays on disk.
    pub async fn pause(&self) -> Result<(), RuntimeError> {
        let Some((model, gate, tx)) = self.running() else {
            return Ok(());
        };
        *lock(&gate.paused) = true;
        gate.wake.notify_one();
        let _ = self
            .engine
            .client()
            .control(&job_id(&model), Action::Pause)
            .await;
        tx.send_modify(|p| p.paused = true);
        gate.release().await;
        Ok(())
    }

    /// Opens the window again and carries on from what was received.
    pub async fn resume(&self) -> Result<(), RuntimeError> {
        let Some((_, gate, tx)) = self.running() else {
            return Ok(());
        };
        let _life = gate.life.lock().await;
        if !gate.paused() || gate.ended() {
            return Ok(());
        }
        let hold = self.engine.control().hold_online().await?;
        let mut held = lock(&gate.hold);
        if gate.ended() {
            drop(held);
            hold.release().await;
            return Ok(());
        }
        let spare = held.replace(hold);
        drop(held);
        drop(spare);
        *lock(&gate.paused) = false;
        tx.send_modify(|p| p.paused = false);
        gate.wake.notify_one();
        Ok(())
    }

    /// Cancels the download and removes what was received.
    pub async fn cancel(&self) -> Result<(), RuntimeError> {
        let Some(run) = lock(&self.run).take() else {
            return Ok(());
        };
        run.task.abort();
        let finished = run.tx.borrow().done;
        let id = job_id(&run.model);
        let client = self.engine.client();
        let _ = client.control(&id, Action::Cancel).await;
        // The engine wants the worker stopped before the partial files go.
        let deadline = Instant::now() + self.opts.give_up;
        while Instant::now() < deadline {
            match client.downloads().await {
                Ok(jobs) if jobs.iter().any(|j| j.id == id && j.running) => {
                    tokio::time::sleep(self.opts.poll).await;
                }
                _ => break,
            }
        }
        let _ = client.control(&id, Action::Remove).await;
        if !finished && let Err(e) = client.delete(&run.model).await {
            tracing::info!("the partial download of {} stays: {e}", run.model);
        }
        run.gate.end().await;
        Ok(())
    }

    /// Deletes a model from disk and returns the bytes freed.
    pub async fn remove(&self, model: &str) -> Result<u64, RuntimeError> {
        remove(&self.engine.client(), model).await
    }

    fn running(&self) -> Option<(String, Arc<Gate>, Arc<watch::Sender<JobProgress>>)> {
        lock(&self.run)
            .as_ref()
            .filter(|r| !r.task.is_finished())
            .map(|r| (r.model.clone(), r.gate.clone(), r.tx.clone()))
    }
}

impl Drop for ModelJobs {
    fn drop(&mut self) {
        if let Some(run) = lock(&self.run).take() {
            run.task.abort();
        }
    }
}

/// Deletes `model`'s files through the engine and returns its size as the catalogue gives it.
/// A model that is not on disk frees nothing and is left alone.
pub async fn remove(client: &Client, model: &str) -> Result<u64, RuntimeError> {
    let info = client
        .models()
        .await?
        .into_iter()
        .find(|m| m.id == model)
        .ok_or_else(|| RuntimeError::NotFound(model.to_string()))?;
    if !info.downloaded {
        return Ok(0);
    }
    client.delete(model).await?;
    Ok(crate::fit::gb(info.size))
}

async fn fetch(
    engine: Arc<Engine>,
    opts: JobOptions,
    model: String,
    tx: Arc<watch::Sender<JobProgress>>,
    gate: Arc<Gate>,
    on_download: OnDownload,
) {
    let finished = follow(&engine, opts, &model, &tx, &gate).await;
    let bytes = match &finished {
        Some(last) => {
            let total = tx.borrow().total;
            let bytes = last
                .as_ref()
                .map(|j| j.bytes().max(j.total_download_size))
                .filter(|b| *b > 0)
                .unwrap_or(total);
            let _ = engine
                .client()
                .control(&job_id(&model), Action::Remove)
                .await;
            on_download(&model, bytes);
            bytes
        }
        None => 0,
    };
    // The window closes before the end is announced: what follows (the re-read of a new
    // finding model) starts on the engine that will keep running, not on one the closing
    // relaunch is about to replace.
    gate.end().await;
    match finished {
        Some(_) => tx.send_modify(|p| {
            p.done = true;
            p.paused = false;
            p.total = p.total.max(bytes);
            p.received = p.total;
        }),
        None => tx.send_modify(|p| p.failed = true),
    }
}

async fn present(client: &Client, model: &str) -> bool {
    client
        .models()
        .await
        .map(|m| m.iter().any(|x| x.id == model && x.present()))
        .unwrap_or(false)
}

/// Starts or rejoins the engine's job for `model` and follows it to the end: `Some` with the
/// last snapshot when the model is on disk, `None` when the download failed.
async fn follow(
    engine: &Engine,
    opts: JobOptions,
    model: &str,
    tx: &watch::Sender<JobProgress>,
    gate: &Gate,
) -> Option<Option<Download>> {
    let id = job_id(model);
    let mut need_start = true;
    let mut errors = 0u32;
    let mut silent_since: Option<Instant> = None;
    let mut last: Option<Download> = None;
    let silent = |since: &mut Option<Instant>| {
        since.get_or_insert_with(Instant::now).elapsed() > opts.give_up
    };
    loop {
        let client = &engine.client();
        while gate.paused() {
            need_start = true;
            tokio::select! {
                _ = gate.wake.notified() => {}
                _ = tokio::time::sleep(opts.poll) => {}
            }
        }
        if need_start {
            match start(client, model, &id).await {
                Ok(job) if job.finished() => return Some(Some(job)),
                Ok(job) => {
                    silent_since = None;
                    record(tx, &job);
                    need_start = false;
                }
                Err(_) if present(client, model).await => return Some(last),
                Err(_) => {
                    // The window may still be opening (the engine relaunches), or the engine
                    // is away for a moment.
                    if silent(&mut silent_since) {
                        return None;
                    }
                    tokio::time::sleep(opts.retry_delay).await;
                    continue;
                }
            }
        }
        tokio::select! {
            _ = gate.wake.notified() => {}
            _ = tokio::time::sleep(opts.poll) => {}
        }
        if gate.paused() {
            continue;
        }
        let jobs = match client.downloads().await {
            Ok(j) => {
                silent_since = None;
                j
            }
            Err(_) => {
                if silent(&mut silent_since) {
                    return None;
                }
                continue;
            }
        };
        let Some(job) = jobs.into_iter().find(|j| j.id == id) else {
            if present(client, model).await {
                return Some(last);
            }
            need_start = true;
            continue;
        };
        if job.finished() {
            return Some(Some(job));
        }
        record(tx, &job);
        last = Some(job.clone());
        match job.status.as_str() {
            "error" => {
                errors += 1;
                tracing::warn!(
                    "the download of {model} failed: {}",
                    job.error.unwrap_or_default()
                );
                if errors > 1 {
                    return None;
                }
                let _ = client.control(&id, Action::Remove).await;
                need_start = true;
            }
            "cancelled" => return None,
            "paused" => need_start = true,
            _ => {}
        }
    }
}

/// Pulls `model`, after clearing away a stopped job of the same id.
async fn start(client: &Client, model: &str, id: &str) -> Result<Download, RuntimeError> {
    if let Ok(jobs) = client.downloads().await
        && let Some(job) = jobs.iter().find(|j| j.id == id)
        && !job.running
        && !job.finished()
        && !matches!(job.status.as_str(), "downloading" | "queued")
    {
        let _ = client.control(id, Action::Remove).await;
    }
    client.pull(model).await
}

fn record(tx: &watch::Sender<JobProgress>, job: &Download) {
    tx.send_modify(|p| {
        if job.total_download_size > 0 {
            p.total = job.total_download_size;
        }
        p.received = job.bytes().min(p.total.max(job.bytes()));
    });
}
