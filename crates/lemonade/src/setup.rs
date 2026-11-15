//! First-run setup: installs the backends and pulls the models a tier needs, as downloads the
//! engine owns, and reports progress as a [`SetupView`]. It survives pauses, a dropped network,
//! a full disk and a restart of either side.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use nocturne_contract::{
    ComponentRole, LedgerKind, PauseReason, SetupError, SetupPhase, SetupView, Tier,
};
use tokio::sync::{Notify, watch};
use tokio::task::JoinHandle;

use crate::client::{Action, BackendState, Client, Download, SystemInfo};
use crate::error::RuntimeError;
use crate::npu;
use crate::record::{self, SetupRecord};
use crate::selection;
use crate::supervisor::{Control, OnlineHold, Supervisor};
use crate::tier::{self, Component, DISK_HEADROOM};

const GIB: u64 = 1 << 30;
const ENGINE_SILENT_LIMIT: Duration = Duration::from_secs(90);

/// Timings and host overrides. The defaults are the product's; tests shrink them.
#[derive(Debug, Clone)]
pub struct SetupOptions {
    /// How often download jobs are read.
    pub poll: Duration,
    /// No byte of progress for this long while a job is active means the network is gone.
    pub offline_after: Duration,
    /// Wait before trying a download again after a network failure.
    pub retry_delay: Duration,
    /// Memory to assume instead of reading it from the machine.
    pub ram_bytes: Option<u64>,
    /// Install these instead of the tier's own models. For tests that must stay small.
    #[doc(hidden)]
    pub components: Option<Vec<Component>>,
}

impl Default for SetupOptions {
    fn default() -> Self {
        SetupOptions {
            poll: Duration::from_millis(500),
            offline_after: Duration::from_secs(30),
            retry_delay: Duration::from_secs(5),
            ram_bytes: None,
            components: None,
        }
    }
}

type OnDownload = Arc<dyn Fn(LedgerKind, String, u64) + Send + Sync>;

#[derive(Debug, Clone)]
enum Step {
    Backend {
        recipe: &'static str,
        backend: String,
    },
    Model(Component),
}

impl Step {
    fn id(&self) -> String {
        match self {
            Step::Backend { recipe, backend } => format!("backend:{recipe}:{backend}"),
            Step::Model(c) => format!("model:{}", c.model),
        }
    }

    fn label(&self) -> String {
        match self {
            Step::Backend { recipe, backend } => format!("{recipe}:{backend}"),
            Step::Model(c) => c.model.to_string(),
        }
    }

    fn estimate(&self) -> u64 {
        match self {
            Step::Backend { .. } => 0,
            Step::Model(c) => c.bytes,
        }
    }
}

enum Fail {
    Error(SetupError),
    /// `remaining` is what is still to fetch; `short` is how far under the engine says it is.
    Disk {
        remaining: u64,
        short: Option<u64>,
    },
}

enum Begin {
    Running,
    Finished(Option<Download>),
}

enum Classified {
    Disk,
    Network,
    Other,
}

fn classify(message: &str) -> Classified {
    let m = message.to_lowercase();
    if [
        "no space",
        "disk full",
        "enospc",
        "not enough space",
        "out of space",
        "insufficient disk space",
    ]
    .iter()
    .any(|k| m.contains(k))
    {
        Classified::Disk
    } else if [
        "network",
        "resolve",
        "connect",
        "timeout",
        "timed out",
        "offline",
        "unreachable",
        "ssl",
        "curl",
        "dns",
    ]
    .iter()
    .any(|k| m.contains(k))
    {
        Classified::Network
    } else {
        Classified::Other
    }
}

/// How many bytes short the engine says the disk is, from
/// "Insufficient disk space: download requires 2.4 GB but only 0.3 GB is available on ...".
/// Its gigabytes are 1024 cubed, written to one decimal place.
fn disk_shortfall(message: &str) -> Option<u64> {
    fn number_after(text: &str, marker: &str) -> Option<f64> {
        let rest = &text[text.find(marker)? + marker.len()..];
        let end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rest.len());
        rest[..end].parse().ok()
    }
    let lower = message.to_lowercase();
    let need = number_after(&lower, "requires ")?;
    let have = number_after(&lower, "but only ")?;
    Some(((need - have).max(0.0) * GIB as f64).round() as u64)
}

/// A guess at what installing a backend downloads, for the disk check before setup starts.
fn backend_estimate(recipe: &str, backend: &str) -> u64 {
    match (recipe, backend) {
        ("llamacpp", "rocm") => 1_000_000_000,
        _ => 150_000_000,
    }
}

#[derive(Default)]
struct Ctl {
    running: bool,
    /// The tier the running setup is installing.
    tier: Option<Tier>,
    paused: bool,
    job: Option<String>,
    handle: Option<JoinHandle<()>>,
}

struct StepProgress {
    id: String,
    total: u64,
    done: u64,
}

#[derive(Default)]
struct Progress {
    steps: Vec<StepProgress>,
    samples: VecDeque<(Instant, u64)>,
    reported: Vec<String>,
}

impl Progress {
    fn totals(&self) -> (u64, u64) {
        (
            self.steps.iter().map(|s| s.done).sum(),
            self.steps.iter().map(|s| s.total).sum(),
        )
    }

    fn step(&mut self, id: &str) -> Option<&mut StepProgress> {
        self.steps.iter_mut().find(|s| s.id == id)
    }
}

struct Inner {
    client: Client,
    control: Control,
    models_dir: PathBuf,
    config_dir: PathBuf,
    on_download: OnDownload,
    opts: SetupOptions,
    view: watch::Sender<SetupView>,
    ctl: Mutex<Ctl>,
    progress: Mutex<Progress>,
    history: Mutex<Vec<SetupPhase>>,
    wake: Notify,
    /// The tier of a change that check() found unfinished and dropped, until it is taken.
    dropped: Mutex<Option<Tier>>,
    /// Setup's share of the download window, which model changes may hold at the same time.
    window: Mutex<Option<OnlineHold>>,
    /// This run also fetches the NPU's runtime and model, because they will be used.
    npu: Mutex<bool>,
}

/// Drives setup for one engine. Dropping it stops its work; the engine's downloads carry on.
pub struct Setup {
    inner: Arc<Inner>,
}

impl Drop for Setup {
    fn drop(&mut self) {
        {
            let mut ctl = self.inner.ctl();
            if let Some(handle) = ctl.handle.take() {
                handle.abort();
            }
            ctl.running = false;
        }
        // Dropping the hold gives it back; the window closes once nothing else holds it.
        drop(lock(&self.inner.window).take());
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn memory_bytes() -> u64 {
    sysinfo::System::new()
        .map(|mut sys| {
            sys.refresh_memory();
            sys.total_memory()
        })
        .unwrap_or(0)
}

/// Free bytes on the volume that holds `path`.
fn disk_free(path: &Path) -> Option<u64> {
    let mut probe = path.to_path_buf();
    while !probe.exists() {
        if !probe.pop() {
            return None;
        }
    }
    let probe = dunce::canonicalize(probe).ok()?;
    let disks = sysinfo::Disks::new_with_refreshed_list().ok()?;
    disks
        .iter()
        .filter(|d| probe.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

fn blank_view(ram_bytes: u64) -> SetupView {
    SetupView {
        phase: SetupPhase::NotStarted,
        detected: None,
        chosen: None,
        ram_gib: (ram_bytes / GIB) as u32,
        free_disk_bytes: 0.0,
        options: Vec::new(),
        downloaded_bytes: 0.0,
        total_bytes: 0.0,
        eta_seconds: None,
        paused: None,
        error: None,
        needed_bytes: None,
    }
}

impl Setup {
    pub fn new(
        sup: &Supervisor,
        models_dir: PathBuf,
        on_download: impl Fn(LedgerKind, String, u64) + Send + Sync + 'static,
    ) -> Setup {
        Setup::with_options(sup, models_dir, on_download, SetupOptions::default())
    }

    pub fn with_options(
        sup: &Supervisor,
        models_dir: PathBuf,
        on_download: impl Fn(LedgerKind, String, u64) + Send + Sync + 'static,
        opts: SetupOptions,
    ) -> Setup {
        let control = sup.control();
        let ram = opts.ram_bytes.unwrap_or_else(memory_bytes);
        let (view, _) = watch::channel(blank_view(ram));
        Setup {
            inner: Arc::new(Inner {
                client: sup.client(),
                config_dir: control.dirs().config(),
                control,
                models_dir,
                on_download: Arc::new(on_download),
                opts,
                view,
                ctl: Mutex::new(Ctl::default()),
                progress: Mutex::new(Progress::default()),
                history: Mutex::new(vec![SetupPhase::NotStarted]),
                wake: Notify::new(),
                dropped: Mutex::new(None),
                window: Mutex::new(None),
                npu: Mutex::new(false),
            }),
        }
    }

    pub fn view(&self) -> watch::Receiver<SetupView> {
        self.inner.view.subscribe()
    }

    /// Every phase the view has passed through, oldest first, without repeats. A watch channel
    /// coalesces quick changes; this does not.
    pub fn history(&self) -> Vec<SetupPhase> {
        lock(&self.inner.history).clone()
    }

    /// The tier that is fully installed. While a change to another tier is being fetched (or
    /// was interrupted) this is still the one that was working.
    pub fn installed(&self) -> Option<Tier> {
        record::read(&self.inner.config_dir).and_then(|r| r.working_tier())
    }

    /// The installed tier, when every one of its components is on disk.
    pub async fn installed_ready(&self) -> Option<Tier> {
        let tier = self.installed()?;
        let missing = self.inner.missing(tier).await.ok()?;
        missing.is_empty().then_some(tier)
    }

    /// Looks at the machine and what is already on disk: `Choosing`, `Paused` when an earlier
    /// run was interrupted, or `Done`.
    pub async fn check(&self) -> Result<SetupView, RuntimeError> {
        self.inner.check().await
    }

    /// Begins installing `tier`. Progress arrives through [`Setup::view`].
    pub async fn start(&self, tier: Tier) -> Result<(), RuntimeError> {
        self.inner.start(tier).await
    }

    /// The tier of an unfinished change that `check` dropped, once: the person asked for it and
    /// has to be told it did not finish.
    pub fn take_dropped(&self) -> Option<Tier> {
        lock(&self.inner.dropped).take()
    }

    pub async fn pause(&self) -> Result<(), RuntimeError> {
        self.inner.pause().await
    }

    /// Continues a paused run, or restarts an interrupted one from what is recorded on disk.
    pub async fn resume(&self) -> Result<(), RuntimeError> {
        self.inner.resume().await
    }
}

impl Inner {
    fn ctl(&self) -> MutexGuard<'_, Ctl> {
        lock(&self.ctl)
    }

    /// Takes setup's share of the download window, once.
    async fn open_window(&self) -> Result<(), RuntimeError> {
        if lock(&self.window).is_some() {
            return Ok(());
        }
        let hold = self.control.hold_online().await?;
        let spare = lock(&self.window).replace(hold);
        drop(spare);
        Ok(())
    }

    /// Gives setup's share back and waits until the window is closed, unless a model change
    /// still holds it.
    async fn close_window(&self) {
        let hold = lock(&self.window).take();
        if let Some(h) = hold {
            h.release().await;
        }
    }

    fn update(&self, f: impl FnOnce(&mut SetupView)) {
        self.view.send_modify(|v| {
            f(v);
            let mut history = lock(&self.history);
            if history.last() != Some(&v.phase) {
                history.push(v.phase);
            }
        });
    }

    async fn host(&self) -> (u64, u64, SystemInfo) {
        let info = self.client.system_info().await.unwrap_or_default();
        let ram = self.opts.ram_bytes.unwrap_or_else(memory_bytes);
        let free = info
            .free_bytes()
            .or_else(|| disk_free(&self.models_dir))
            .unwrap_or(0);
        (ram, free, info)
    }

    /// What setup installs for `tier`.
    fn comps(&self, tier: Tier) -> Vec<Component> {
        self.opts
            .components
            .clone()
            .unwrap_or_else(|| tier::components(tier))
    }

    /// What setup also fetches for `tier` when it is missing, without making the tier depend on
    /// it: an install from before an extra existed stays installed. None when the components are
    /// given (tests that must stay small).
    fn extras(&self, tier: Tier) -> Vec<Component> {
        if self.opts.components.is_some() {
            Vec::new()
        } else {
            let mut out = tier::extras(tier);
            if *lock(&self.npu) {
                out.push(npu::SPEECH);
            }
            out
        }
    }

    /// Whether recordings will be read on the NPU with `tier`, so that its runtime and model
    /// are worth fetching: the engine can use the NPU, the person has not chosen a speech model
    /// and the NPU model fits beside the others. Decided once per run, before anything is
    /// fetched.
    fn decide_npu(&self, tier: Tier, info: &SystemInfo, ram: u64) -> bool {
        let chosen = selection::Selection {
            tier,
            ..selection::read(&self.config_dir)
        };
        let wanted = self.opts.components.is_none()
            && npu::readiness(info, false).wanted()
            && !chosen.custom.contains_key(&ComponentRole::Speech)
            && npu::fits(&chosen, ram);
        *lock(&self.npu) = wanted;
        wanted
    }

    /// True for a step setup may leave out: the tier works without it.
    fn optional(&self, tier: Tier, step: &Step) -> bool {
        match step {
            Step::Backend { recipe, .. } => *recipe == npu::RECIPE,
            Step::Model(c) => self.extras(tier).iter().any(|e| e.model == c.model),
        }
    }

    /// The backends those models need.
    fn recipes(&self, tier: Tier) -> Vec<&'static str> {
        let mut out = Vec::new();
        for c in self.comps(tier).into_iter().chain(self.extras(tier)) {
            let recipe = match c.role {
                ComponentRole::Mind | ComponentRole::Recall => "llamacpp",
                ComponentRole::Speech => "whispercpp",
                ComponentRole::Voice => "kokoro",
            };
            if !out.contains(&recipe) {
                out.push(recipe);
            }
        }
        out
    }

    fn describe_host(&self, ram: u64, free: u64) {
        self.update(|v| {
            v.ram_gib = (ram / GIB) as u32;
            v.free_disk_bytes = free as f64;
            v.options = tier::tier_options(ram, free);
            v.detected = Some(tier::detect(ram, free));
        });
    }

    async fn missing(&self, tier: Tier) -> Result<Vec<Component>, RuntimeError> {
        self.absent(self.comps(tier)).await
    }

    /// The extras of `tier` that are not on disk.
    async fn missing_extras(&self, tier: Tier) -> Result<Vec<Component>, RuntimeError> {
        self.absent(self.extras(tier)).await
    }

    async fn absent(&self, of: Vec<Component>) -> Result<Vec<Component>, RuntimeError> {
        let models = self.client.models().await?;
        Ok(of
            .into_iter()
            .filter(|c| !models.iter().any(|m| m.id == c.model && m.present()))
            .collect())
    }

    async fn check(&self) -> Result<SetupView, RuntimeError> {
        self.update(|v| v.phase = SetupPhase::Checking);
        let (ram, free, _info) = self.host().await;
        self.describe_host(ram, free);
        let detected = tier::detect(ram, free);
        let record = record::read(&self.config_dir);
        let mut next = SetupPhase::Choosing;
        let mut chosen = None;
        let mut paused = None;
        let (mut done, mut total) = (0u64, 0u64);
        if let Some(rec) = record.as_ref().filter(|r| r.tier.is_some()) {
            // A change of tier that never finished does not take away the one that works.
            let working = match rec.working_tier() {
                Some(w) if self.missing(w).await?.is_empty() => Some(w),
                _ => None,
            };
            // Nothing in this launch is fetching the other tier, and nothing starts a download
            // without the person asking again, so the unfinished change is dropped rather than
            // left to read as done while a later resume still aims at it.
            if let Some(w) = working
                && rec.tier != Some(w)
                && !self.ctl().running
            {
                let settled = SetupRecord {
                    tier: Some(w),
                    done: true,
                    completed: Some(w),
                    ..rec.clone()
                };
                match record::write(&self.config_dir, &settled) {
                    Ok(()) => *lock(&self.dropped) = rec.tier,
                    Err(e) => tracing::warn!("could not settle the unfinished change of tier: {e}"),
                }
            }
            let t = working.or(rec.tier).unwrap_or(Tier::Unsupported);
            let missing = self.missing(t).await?;
            total = self.comps(t).iter().map(|c| c.bytes).sum();
            done = total - missing.iter().map(|c| c.bytes).sum::<u64>();
            chosen = Some(t);
            if working.is_some() || (rec.done && missing.is_empty()) {
                next = SetupPhase::Done;
            } else if !rec.done {
                next = SetupPhase::Paused;
                paused = Some(PauseReason::Person);
            }
        }
        let error = if detected == Tier::Unsupported && next == SetupPhase::Choosing {
            if ram < u64::from(tier::min_ram_gib(tier::SMALLEST)) * GIB {
                Some(SetupError::NotEnoughMemory)
            } else {
                Some(SetupError::DiskFull)
            }
        } else {
            None
        };
        let needed = (error == Some(SetupError::DiskFull)).then(|| {
            (tier::download_bytes(tier::SMALLEST) + DISK_HEADROOM).saturating_sub(free) as f64
        });
        self.update(|v| {
            v.phase = next;
            v.chosen = chosen;
            v.paused = paused;
            v.error = error;
            v.needed_bytes = needed;
            v.downloaded_bytes = done as f64;
            v.total_bytes = total as f64;
        });
        Ok(self.view.borrow().clone())
    }

    async fn start(self: &Arc<Self>, tier: Tier) -> Result<(), RuntimeError> {
        if tier == Tier::Unsupported {
            return Err(RuntimeError::Start("this machine cannot run a tier".into()));
        }
        {
            let ctl = self.ctl();
            if ctl.running {
                return if ctl.tier == Some(tier) {
                    Ok(())
                } else {
                    Err(RuntimeError::Busy)
                };
            }
        }
        let (ram, free, info) = self.host().await;
        self.describe_host(ram, free);
        if ram < u64::from(tier::min_ram_gib(tier)) * GIB {
            self.update(|v| {
                v.phase = SetupPhase::Failed;
                v.chosen = Some(tier);
                v.error = Some(SetupError::NotEnoughMemory);
            });
            return Ok(());
        }
        let with_npu = self.decide_npu(tier, &info, ram);
        let models: u64 = self
            .missing(tier)
            .await?
            .iter()
            .chain(&self.missing_extras(tier).await?)
            .map(|c| c.bytes)
            .sum();
        let npu_runtime = if with_npu
            && info.backend_state(npu::RECIPE, npu::BACKEND) != BackendState::Installed
        {
            npu::RUNTIME_BYTES
        } else {
            0
        };
        let backends: u64 = self
            .recipes(tier)
            .into_iter()
            .map(|recipe| {
                let backend = tier::choose_backend(&info, recipe);
                match info.backend_state(recipe, backend) {
                    BackendState::Installed => 0,
                    _ => backend_estimate(recipe, backend),
                }
            })
            .sum::<u64>()
            + npu_runtime;
        let remaining = models + backends;
        if free < remaining + DISK_HEADROOM {
            self.disk_failed(remaining, None);
            return Ok(());
        }
        self.launch(tier)
    }

    /// Starts the run unless one began while the checks above were waiting on the engine.
    fn launch(self: &Arc<Self>, tier: Tier) -> Result<(), RuntimeError> {
        let mut ctl = self.ctl();
        if ctl.running {
            return if ctl.tier == Some(tier) {
                Ok(())
            } else {
                Err(RuntimeError::Busy)
            };
        }
        ctl.running = true;
        ctl.tier = Some(tier);
        ctl.paused = false;
        let inner = self.clone();
        ctl.handle = Some(tokio::spawn(async move {
            let result = inner.run(tier).await;
            if let Err(fail) = result {
                inner.fail(fail).await;
            }
            inner.close_window().await;
            let mut ctl = inner.ctl();
            ctl.running = false;
            ctl.tier = None;
            ctl.job = None;
        }));
        Ok(())
    }

    async fn pause(&self) -> Result<(), RuntimeError> {
        let job = {
            let mut ctl = self.ctl();
            if !ctl.running {
                return Ok(());
            }
            ctl.paused = true;
            ctl.job.clone()
        };
        if let Some(id) = job {
            let _ = self.client.control(&id, Action::Pause).await;
        }
        self.update(|v| {
            v.phase = SetupPhase::Paused;
            v.paused = Some(PauseReason::Person);
        });
        self.wake.notify_one();
        // Nothing may reach the internet for setup while the person has paused; resuming opens
        // the window again.
        self.close_window().await;
        Ok(())
    }

    async fn resume(self: &Arc<Self>) -> Result<(), RuntimeError> {
        let active = {
            let mut ctl = self.ctl();
            if ctl.running {
                ctl.paused = false;
            }
            ctl.running
        };
        if active {
            self.wake.notify_one();
            return Ok(());
        }
        let tier = record::read(&self.config_dir)
            .and_then(|r| r.tier)
            .or(self.view.borrow().chosen)
            .ok_or_else(|| RuntimeError::Start("nothing to resume".into()))?;
        self.start(tier).await
    }

    fn disk_failed(&self, remaining: u64, short: Option<u64>) {
        let free = self.view.borrow().free_disk_bytes as u64;
        let needed = match short {
            Some(short) => short + DISK_HEADROOM,
            None => (remaining + DISK_HEADROOM).saturating_sub(free).max(GIB),
        };
        self.update(|v| {
            v.phase = SetupPhase::Failed;
            v.paused = None;
            v.error = Some(SetupError::DiskFull);
            v.needed_bytes = Some(needed as f64);
        });
    }

    async fn fail(&self, fail: Fail) {
        match fail {
            Fail::Disk { remaining, short } => {
                let (_, free, _) = self.host().await;
                self.update(|v| v.free_disk_bytes = free as f64);
                self.disk_failed(remaining, short);
            }
            Fail::Error(e) => self.update(|v| {
                v.phase = SetupPhase::Failed;
                v.paused = None;
                v.error = Some(e);
            }),
        }
    }

    async fn run(self: &Arc<Self>, tier: Tier) -> Result<(), Fail> {
        let engine = |_: RuntimeError| Fail::Error(SetupError::EngineFailed);
        self.update(|v| {
            v.phase = SetupPhase::Downloading;
            v.chosen = Some(tier);
            v.paused = None;
            v.error = None;
            v.needed_bytes = None;
        });
        self.open_window().await.map_err(engine)?;
        let info = self.client.system_info().await.map_err(engine)?;
        let llama = tier::choose_backend(&info, "llamacpp");
        let whisper = tier::choose_backend(&info, "whispercpp");
        let voice = tier::choose_backend(&info, "kokoro");
        let _ = self
            .control
            .admin()
            .set(&serde_json::json!({
                "llamacpp": {"backend": llama},
                "whispercpp": {"backend": whisper},
            }))
            .await;
        let mut rec = SetupRecord {
            version: 1,
            tier: Some(tier),
            done: false,
            completed: record::read(&self.config_dir).and_then(|r| r.working_tier()),
            backends: Default::default(),
        };
        for (recipe, backend) in [
            ("llamacpp", llama),
            ("whispercpp", whisper),
            ("kokoro", voice),
        ] {
            rec.backends.insert(recipe.to_string(), backend.to_string());
        }
        record::write(&self.config_dir, &rec).map_err(engine)?;

        let steps = self.plan(tier, &info, &rec).await.map_err(Fail::Error)?;
        for step in &steps {
            match self.run_step(step).await {
                // The tier works without an extra: one that cannot be fetched is left out.
                Err(Fail::Error(_)) if self.optional(tier, step) => {
                    tracing::warn!("an optional download could not be fetched; setup goes on");
                }
                other => other?,
            }
        }

        self.update(|v| {
            v.phase = SetupPhase::Installing;
            v.eta_seconds = None;
            v.paused = None;
        });
        if !self.missing(tier).await.map_err(engine)?.is_empty() {
            return Err(Fail::Error(SetupError::EngineFailed));
        }
        let info = self.client.system_info().await.map_err(engine)?;
        for (recipe, backend) in rec
            .backends
            .iter()
            .filter(|(r, _)| self.recipes(tier).contains(&r.as_str()))
        {
            if info.backend_state(recipe, backend) != BackendState::Installed {
                return Err(Fail::Error(SetupError::EngineFailed));
            }
        }
        rec.done = true;
        rec.completed = Some(tier);
        record::write(&self.config_dir, &rec).map_err(engine)?;
        // The new tier's own models, keeping only the finding model the person chose.
        selection::update(&self.config_dir, |_| {}).map_err(engine)?;
        self.close_window().await;
        let total = self
            .comps(tier)
            .iter()
            .chain(&self.extras(tier))
            .map(|c| c.bytes)
            .sum::<u64>() as f64;
        self.update(|v| {
            v.phase = SetupPhase::Done;
            v.downloaded_bytes = total.max(v.downloaded_bytes);
            v.total_bytes = v.total_bytes.max(total);
        });
        Ok(())
    }

    /// What still has to be fetched, and the progress book for all of it.
    async fn plan(
        &self,
        tier: Tier,
        info: &SystemInfo,
        rec: &SetupRecord,
    ) -> Result<Vec<Step>, SetupError> {
        let mut steps = Vec::new();
        for recipe in self.recipes(tier) {
            let backend = rec
                .backends
                .get(recipe)
                .cloned()
                .unwrap_or_else(|| "cpu".into());
            match info.backend_state(recipe, &backend) {
                BackendState::Installed => {}
                BackendState::Unsupported => return Err(SetupError::EngineFailed),
                _ => steps.push(Step::Backend { recipe, backend }),
            }
        }
        if *lock(&self.npu)
            && info.backend_state(npu::RECIPE, npu::BACKEND) != BackendState::Installed
        {
            steps.push(Step::Backend {
                recipe: npu::RECIPE,
                backend: npu::BACKEND.to_string(),
            });
        }
        let mut missing = self
            .missing(tier)
            .await
            .map_err(|_| SetupError::EngineFailed)?;
        missing.extend(
            self.missing_extras(tier)
                .await
                .map_err(|_| SetupError::EngineFailed)?,
        );
        let mut book = Progress::default();
        for c in self.comps(tier).into_iter().chain(self.extras(tier)) {
            let absent = missing.iter().any(|m| m.model == c.model);
            book.steps.push(StepProgress {
                id: format!("model:{}", c.model),
                total: c.bytes,
                done: if absent { 0 } else { c.bytes },
            });
        }
        for step in &steps {
            if matches!(step, Step::Backend { .. }) {
                book.steps.push(StepProgress {
                    id: step.id(),
                    total: 0,
                    done: 0,
                });
            }
        }
        steps.extend(missing.into_iter().map(Step::Model));
        *lock(&self.progress) = book;
        self.publish();
        Ok(steps)
    }

    fn publish(&self) {
        let (done, total, eta) = {
            let mut p = lock(&self.progress);
            let (done, total) = p.totals();
            let now = Instant::now();
            p.samples.push_back((now, done));
            while p
                .samples
                .front()
                .is_some_and(|(t, _)| now.duration_since(*t) > Duration::from_secs(10))
            {
                p.samples.pop_front();
            }
            let eta = match (p.samples.front(), p.samples.back()) {
                (Some((t0, b0)), Some((t1, b1))) if b1 > b0 && t1 > t0 => {
                    let rate = (*b1 - *b0) as f64 / t1.duration_since(*t0).as_secs_f64();
                    Some(((total.saturating_sub(done)) as f64 / rate) as u32)
                }
                _ => None,
            };
            (done, total, eta)
        };
        self.update(|v| {
            v.downloaded_bytes = done as f64;
            v.total_bytes = total as f64;
            v.eta_seconds = eta;
        });
    }

    async fn run_step(&self, step: &Step) -> Result<(), Fail> {
        let id = step.id();
        self.ctl().job = Some(id.clone());
        let mut attempts = 0u32;
        let mut need_start = true;
        let mut last_bytes = 0u64;
        let mut last_move = Instant::now();
        let mut silent_since: Option<Instant> = None;
        let mut last_seen: Option<Download> = None;
        loop {
            if self.gate().await {
                need_start = true;
            }
            if need_start {
                match self.begin(step).await {
                    Ok(Begin::Finished(dl)) => return self.finish(step, dl.or(last_seen)).await,
                    Ok(Begin::Running) => {}
                    Err(Some(fail)) => return Err(fail),
                    Err(None) => {
                        self.mark_offline();
                        self.nap(self.opts.retry_delay).await;
                        continue;
                    }
                }
                need_start = false;
                last_move = Instant::now();
                self.mark_downloading();
            }
            self.nap(self.opts.poll).await;
            if self.ctl().paused {
                continue;
            }
            let jobs = match self.client.downloads().await {
                Ok(jobs) => {
                    silent_since = None;
                    jobs
                }
                Err(_) => {
                    let since = *silent_since.get_or_insert_with(Instant::now);
                    if since.elapsed() > ENGINE_SILENT_LIMIT {
                        return Err(Fail::Error(SetupError::EngineFailed));
                    }
                    self.mark_offline();
                    continue;
                }
            };
            let Some(job) = jobs.into_iter().find(|j| j.id == id) else {
                if self.present(step).await {
                    return self.finish(step, last_seen).await;
                }
                attempts += 1;
                if attempts > 1 {
                    return Err(Fail::Error(SetupError::EngineFailed));
                }
                need_start = true;
                continue;
            };
            if job.finished() {
                return self.finish(step, Some(job)).await;
            }
            self.record(step, &job);
            last_seen = Some(job.clone());
            match job.status.as_str() {
                "error" => {
                    let message = job.error.clone().unwrap_or_default();
                    match classify(&message) {
                        Classified::Disk => {
                            let (done, total) = lock(&self.progress).totals();
                            return Err(Fail::Disk {
                                remaining: total.saturating_sub(done),
                                short: disk_shortfall(&message),
                            });
                        }
                        Classified::Network => {
                            self.mark_offline();
                            let _ = self.client.control(&id, Action::Remove).await;
                            self.nap(self.opts.retry_delay).await;
                            need_start = true;
                        }
                        Classified::Other => {
                            tracing::warn!("download of {} failed: {message}", step.label());
                            attempts += 1;
                            if attempts > 1 {
                                return Err(Fail::Error(SetupError::EngineFailed));
                            }
                            let _ = self.client.control(&id, Action::Remove).await;
                            need_start = true;
                        }
                    }
                }
                "cancelled" => return Err(Fail::Error(SetupError::EngineFailed)),
                "paused" => need_start = true,
                _ => {
                    if job.bytes() > last_bytes {
                        last_bytes = job.bytes();
                        last_move = Instant::now();
                        self.mark_downloading();
                    } else if last_move.elapsed() >= self.opts.offline_after {
                        self.mark_offline();
                    }
                }
            }
        }
    }

    /// Sleeps for `d`, or less when something wants the loop's attention.
    async fn nap(&self, d: Duration) {
        tokio::select! {
            _ = tokio::time::sleep(d) => {}
            _ = self.wake.notified() => {}
        }
    }

    /// Holds while the person has paused. True when it had to wait.
    async fn gate(&self) -> bool {
        let mut waited = false;
        while self.ctl().paused {
            waited = true;
            self.nap(self.opts.poll).await;
        }
        if waited {
            let _ = self.open_window().await;
            self.mark_downloading();
        }
        waited
    }

    fn mark_offline(&self) {
        if self.ctl().paused {
            return;
        }
        self.update(|v| {
            v.phase = SetupPhase::Paused;
            v.paused = Some(PauseReason::Offline);
        });
    }

    fn mark_downloading(&self) {
        if self.ctl().paused {
            return;
        }
        self.update(|v| {
            if v.phase == SetupPhase::Paused || v.paused.is_some() {
                v.phase = SetupPhase::Downloading;
                v.paused = None;
            }
        });
    }

    /// Attaches to the engine's job for `step` or starts one.
    async fn begin(&self, step: &Step) -> Result<Begin, Option<Fail>> {
        let id = step.id();
        let transient = |e: &RuntimeError| {
            matches!(
                e,
                RuntimeError::Unavailable | RuntimeError::Timeout | RuntimeError::Offline
            )
        };
        let jobs = match self.client.downloads().await {
            Ok(j) => j,
            Err(e) if transient(&e) => return Err(None),
            Err(_) => return Err(Some(Fail::Error(SetupError::EngineFailed))),
        };
        if let Some(job) = jobs.into_iter().find(|j| j.id == id) {
            match job.status.as_str() {
                "downloading" | "queued" if !job.finished() => return Ok(Begin::Running),
                _ if job.finished() => return Ok(Begin::Finished(Some(job))),
                _ => self.remove_stopped(&id).await,
            }
        }
        let mut started = self.start_job(step).await;
        if matches!(started, Err(RuntimeError::Rejected { status: 409, .. })) {
            self.nap(self.opts.poll).await;
            started = self.start_job(step).await;
        }
        match started {
            Ok(job) if job.finished() => Ok(Begin::Finished(Some(job))),
            Ok(_) => Ok(Begin::Running),
            Err(RuntimeError::Offline) => {
                if !self.ctl().paused {
                    let _ = self.open_window().await;
                }
                Err(None)
            }
            Err(e) if transient(&e) => Err(None),
            Err(RuntimeError::Server { message, .. } | RuntimeError::Rejected { message, .. })
                if matches!(classify(&message), Classified::Disk) =>
            {
                let (done, total) = lock(&self.progress).totals();
                Err(Some(Fail::Disk {
                    remaining: total.saturating_sub(done),
                    short: disk_shortfall(&message),
                }))
            }
            Err(_) => Err(Some(Fail::Error(SetupError::EngineFailed))),
        }
    }

    async fn start_job(&self, step: &Step) -> Result<Download, RuntimeError> {
        match step {
            Step::Model(c) => self.client.pull(c.model).await,
            Step::Backend { recipe, backend } => self.client.install(recipe, backend).await,
        }
    }

    /// Removes a stopped job once its worker has let go, trying once more on a conflict.
    async fn remove_stopped(&self, id: &str) {
        for _ in 0..200 {
            match self.client.downloads().await {
                Ok(jobs) if jobs.iter().any(|j| j.id == id && j.running) => {
                    self.nap(self.opts.poll).await;
                }
                _ => break,
            }
        }
        for attempt in 0..2 {
            match self.client.control(id, Action::Remove).await {
                Err(RuntimeError::Rejected { status: 409, .. }) if attempt == 0 => {
                    self.nap(self.opts.poll).await;
                }
                _ => break,
            }
        }
    }

    async fn present(&self, step: &Step) -> bool {
        match step {
            Step::Model(c) => self
                .client
                .models()
                .await
                .map(|m| m.iter().any(|x| x.id == c.model && x.present()))
                .unwrap_or(false),
            Step::Backend { recipe, backend } => self
                .client
                .system_info()
                .await
                .map(|i| i.backend_state(recipe, backend) == BackendState::Installed)
                .unwrap_or(false),
        }
    }

    fn record(&self, step: &Step, job: &Download) {
        {
            let mut p = lock(&self.progress);
            if let Some(s) = p.step(&step.id()) {
                if job.total_download_size > 0 {
                    s.total = job.total_download_size;
                }
                s.done = job.bytes().min(s.total.max(job.bytes()));
            }
        }
        self.publish();
    }

    /// The step is done: settle the book, tell the ledger once, tidy the job away.
    async fn finish(&self, step: &Step, last: Option<Download>) -> Result<(), Fail> {
        if !self.present(step).await {
            return Err(Fail::Error(SetupError::EngineFailed));
        }
        let bytes = last
            .as_ref()
            .map(|j| j.bytes().max(j.total_download_size))
            .filter(|b| *b > 0)
            .unwrap_or_else(|| step.estimate());
        let first_time = {
            let mut p = lock(&self.progress);
            if let Some(s) = p.step(&step.id()) {
                s.total = s.total.max(bytes).max(s.done);
                s.done = s.total;
            }
            let id = step.id();
            let fresh = !p.reported.contains(&id);
            if fresh {
                p.reported.push(id);
            }
            fresh
        };
        self.publish();
        if first_time && bytes > 0 {
            (self.on_download)(LedgerKind::Setup, step.label(), bytes);
        }
        let _ = self.client.control(&step.id(), Action::Remove).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// On Windows a resolved path must not carry the verbatim prefix (`\\?\C:\...`), or it is
    /// under no disk's mount point.
    #[test]
    fn the_free_space_of_a_folder_on_a_disk_is_known() {
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(disk_free(here).is_some());
        assert!(disk_free(&here.join("not-there-yet").join("models")).is_some());
    }

    #[test]
    fn reads_the_engines_disk_message() {
        let m = "Insufficient disk space: download requires 2.4 GB but only 0.3 GB is available on /models";
        let short = disk_shortfall(m).unwrap() as f64 / GIB as f64;
        assert!((short - 2.1).abs() < 1e-6, "{short}");
        assert!(matches!(classify(m), Classified::Disk));
        assert_eq!(
            disk_shortfall("download requires 1.0 GB but only 3.0 GB"),
            Some(0)
        );
        assert_eq!(disk_shortfall("No space left on device"), None);
        assert!(matches!(
            classify("No space left on device"),
            Classified::Disk
        ));
        assert!(matches!(
            classify("could not resolve host"),
            Classified::Network
        ));
        assert!(matches!(classify("checksum mismatch"), Classified::Other));
    }
}
