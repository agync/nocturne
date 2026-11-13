//! Whether a model fits this computer for a job: the memory budget, the busiest moment of the
//! chosen set, the engine's own size cap and the free disk.

use nocturne_contract::{ComponentRole, Fit, FitReason, Job, ModelChoice, Tier};

use crate::client::{BackendState, ModelInfo, SystemInfo};
use crate::fit_table::{self, PRESET_PEAKS};
use crate::residency;
use crate::selection::{self, Picks, Selection};
use crate::tier::{self, DISK_HEADROOM};

/// The limits a choice is judged against, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub ram: u64,
    pub budget: u64,
    pub disk_free: u64,
    /// The largest model the engine accepts: max(largest pool, 0.8 x RAM).
    pub engine_cap: u64,
}

/// Why a catalogue model is not offered for a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exclusion {
    /// The job needs another kind of engine.
    Recipe,
    /// The model lacks a label the job needs (for answering, `chat` and `vision`).
    Label,
    /// It thinks out loud first, which Nocturne does not turn off reliably.
    Reasoning,
    /// Not from the catalogue (`user.*`).
    UserModel,
    /// Served by a provider, not this computer.
    Cloud,
    /// It cannot take the context Nocturne loads it with.
    ShortContext,
}

const GIB: u64 = 1 << 30;

/// The limits for a computer with `ram` bytes. The memory budget is
/// `min(0.55 x RAM + 1.7 GiB, RAM - 1.0 GiB)`: every tier preset fits at its own minimum, with
/// room left for the system and the app (plan 13, Decisions, "Fit budget").
pub fn budget(ram: u64, largest_pool: u64, disk_free: u64) -> Budget {
    let budget = (ram / 100 * 55 + GIB / 10 * 17).min(ram.saturating_sub(GIB));
    Budget {
        ram,
        budget,
        disk_free,
        engine_cap: largest_pool.max(ram / 10 * 8),
    }
}

/// How `need` bytes at the busiest moment sit in `b`: Fits at most 0.85 of the budget, Tight at
/// most the budget, Won't fit above it.
pub fn fit_for(need: u64, b: &Budget) -> Fit {
    if need <= b.budget / 100 * 85 {
        Fit::Fits
    } else if need <= b.budget {
        Fit::Tight
    } else {
        Fit::WontFit
    }
}

/// The memory nothing loaded still costs, so an idle moment never decides a fit.
const IDLE: u64 = 500_000_000;
/// The context every job's model must take.
const MIN_CONTEXT: u32 = 8192;

/// The limits for this computer: `ram` and `disk_free` in bytes, the largest pool read from the
/// engine's view of the graphics.
pub fn for_host(info: &SystemInfo, ram: u64, disk_free: u64) -> Budget {
    let pool = info
        .devices
        .amd_gpu
        .iter()
        .filter(|g| g.available && !g.integrated)
        .map(|g| (g.vram_gb * 1e9) as u64)
        .max()
        .unwrap_or(0);
    budget(ram, pool, disk_free)
}

/// The memory this computer has, in bytes.
pub fn ram_bytes() -> u64 {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    sys.total_memory()
}

/// The component role that does `job`.
pub fn role_of(job: Job) -> ComponentRole {
    match job {
        Job::Answering => ComponentRole::Mind,
        Job::Recordings => ComponentRole::Speech,
        Job::Aloud => ComponentRole::Voice,
        Job::Finding => ComponentRole::Recall,
    }
}

/// The job a component role does.
pub fn job_of(role: ComponentRole) -> Job {
    match role {
        ComponentRole::Mind => Job::Answering,
        ComponentRole::Speech => Job::Recordings,
        ComponentRole::Voice => Job::Aloud,
        ComponentRole::Recall => Job::Finding,
    }
}

/// Memory `model` uses while loaded: the table's figure, else the estimate from its catalogue
/// entry, else nothing known.
pub fn memory_of(model: &str, info: Option<&ModelInfo>) -> u64 {
    if let Some(e) = fit_table::lookup(model) {
        return e.memory;
    }
    info.map_or(0, |m| fit_table::estimate(gb(m.size), &m.labels))
}

/// Bytes from the catalogue's gigabytes.
pub fn gb(size: f64) -> u64 {
    (size.max(0.0) * 1e9).round() as u64
}

/// The memory the busiest moment of `picks` needs, from the table alone.
pub fn peak(picks: &Picks, alone_voice: bool) -> u64 {
    peak_with(picks, alone_voice, |m| memory_of(m, None))
}

/// The busiest of three moments: reading (the mind, recall and speech together; a tier preset's
/// measured figure where the three are its own), speaking (the voice alone when it runs alone,
/// else beside the mind and recall) and idle.
pub fn peak_with(picks: &Picks, alone_voice: bool, mem: impl Fn(&str) -> u64) -> u64 {
    let (mind, voice, recall) = (mem(&picks.mind), mem(&picks.voice), mem(&picks.recall));
    let read = read_set_with(picks, &mem);
    let speak = if alone_voice {
        voice
    } else {
        voice + mind + recall
    };
    read.max(speak).max(IDLE)
}

/// Reading: the mind, recall and speech together, or a tier preset's measured figure where the
/// three are its own and the figure is larger.
pub fn read_set_with(picks: &Picks, mem: impl Fn(&str) -> u64) -> u64 {
    let read = mem(&picks.mind) + mem(&picks.recall) + mem(&picks.speech);
    PRESET_PEAKS
        .iter()
        .find(|p| reads_like(p.tier, picks))
        .map_or(read, |p| read.max(p.busiest))
}

fn reads_like(tier: Tier, picks: &Picks) -> bool {
    [
        ComponentRole::Mind,
        ComponentRole::Speech,
        ComponentRole::Recall,
    ]
    .into_iter()
    .all(|r| tier::model_for(tier, r) == Some(picks.get(r)))
}

/// Whether `m` may be offered for `job` at all: the engine and labels the job needs, no
/// reasoning models, nothing outside the catalogue, nothing served online, enough context.
pub fn qualifies(job: Job, m: &ModelInfo) -> Result<(), Exclusion> {
    if m.id.starts_with("user.") {
        return Err(Exclusion::UserModel);
    }
    if m.cloud_provider.is_some() || m.recipe == "cloud" {
        return Err(Exclusion::Cloud);
    }
    let (recipe, labels): (&str, &[&str]) = match job {
        Job::Answering => ("llamacpp", &["chat", "vision"]),
        Job::Recordings => ("whispercpp", &["transcription"]),
        Job::Aloud => ("kokoro", &["tts"]),
        Job::Finding => ("llamacpp", &["embeddings"]),
    };
    if m.recipe != recipe {
        return Err(Exclusion::Recipe);
    }
    if !labels.iter().all(|l| m.labels.iter().any(|x| x == l)) {
        return Err(Exclusion::Label);
    }
    if m.labels.iter().any(|l| l == "reasoning") {
        return Err(Exclusion::Reasoning);
    }
    let short = m.max_context_window.is_some_and(|w| w < MIN_CONTEXT)
        || fit_table::lookup(&m.id).is_some_and(|e| !e.ctx_ok);
    if short && job != Job::Aloud && job != Job::Recordings {
        return Err(Exclusion::ShortContext);
    }
    Ok(())
}

/// The models offered for `job`, each with its fit: the current one first, then those on this
/// computer, then the rest; within each, those that can be used before those that cannot.
pub fn choices(
    job: Job,
    catalogue: &[ModelInfo],
    info: &SystemInfo,
    current: &Selection,
    b: &Budget,
) -> Vec<ModelChoice> {
    let mut out: Vec<ModelChoice> = catalogue
        .iter()
        .filter(|m| qualifies(job, m).is_ok())
        .map(|m| judge(job, m, catalogue, info, current, b))
        .collect();
    let usable = |c: &ModelChoice| matches!(c.fit, Fit::Fits | Fit::Tight);
    out.sort_by(|a, b| {
        (!a.current, !a.on_disk, !usable(a))
            .cmp(&(!b.current, !b.on_disk, !usable(b)))
            .then(if usable(a) {
                b.memory_bytes.total_cmp(&a.memory_bytes)
            } else {
                a.memory_bytes.total_cmp(&b.memory_bytes)
            })
            .then(a.id.cmp(&b.id))
    });
    out
}

/// The name a person sees: the catalogue id without the file format and the release words.
pub fn display_name(id: &str) -> String {
    let mut name = id.strip_suffix("-GGUF").unwrap_or(id);
    if let Some(i) = name.find("-Instruct") {
        name = &name[..i];
    }
    name.strip_suffix("-it").unwrap_or(name).to_string()
}

/// `current` with `model` doing `role`: a custom choice, or none when it is the tier's own.
pub fn with_choice(current: &Selection, role: ComponentRole, model: &str) -> Selection {
    let mut s = current.clone();
    if tier::model_for(s.tier, role) == Some(model) {
        s.custom.remove(&role);
    } else {
        s.custom.insert(role, model.to_string());
    }
    s
}

/// The busiest moment `s` needs on `b`, with `catalogue` for models the table does not know.
pub fn busiest(s: &Selection, catalogue: &[ModelInfo], b: &Budget) -> u64 {
    let mem = |id: &str| memory_of(id, catalogue.iter().find(|m| m.id == id));
    peak_with(
        &selection::picks(s),
        residency::alone_for(s, b)
            .iter()
            .any(|v| *v == selection::picks(s).voice),
        mem,
    )
}

/// One catalogue model judged for `job` on this computer, with the person's other choices as
/// they are.
pub fn judge(
    job: Job,
    m: &ModelInfo,
    catalogue: &[ModelInfo],
    info: &SystemInfo,
    current: &Selection,
    b: &Budget,
) -> ModelChoice {
    let role = role_of(job);
    let mem = |id: &str| memory_of(id, catalogue.iter().find(|c| c.id == id));
    let picks = selection::picks(current);
    let memory = mem(&m.id);
    let size = gb(m.size);
    let on_disk = m.present();
    let (fit, reason) = fit_of(job, m, catalogue, info, current, b);
    ModelChoice {
        id: m.id.clone(),
        name: display_name(&m.id),
        job,
        size_bytes: size as f64,
        memory_bytes: memory as f64,
        on_disk,
        current: picks.get(role) == m.id,
        recommended: tier::model_for(current.tier, role) == Some(m.id.as_str()),
        tested: fit_table::lookup(&m.id).is_some_and(|e| e.tested),
        fit,
        reason,
        smaller: memory < mem(picks.get(role)),
    }
}

fn fit_of(
    job: Job,
    m: &ModelInfo,
    catalogue: &[ModelInfo],
    info: &SystemInfo,
    current: &Selection,
    b: &Budget,
) -> (Fit, Option<FitReason>) {
    let backend = tier::choose_backend(info, &m.recipe);
    let size = gb(m.size);
    if info.backend_state(&m.recipe, backend) == BackendState::Unsupported || size > b.engine_cap {
        return (Fit::CantUse, Some(FitReason::NotAvailable));
    }
    if !m.present() && size + DISK_HEADROOM > b.disk_free {
        let short = size + DISK_HEADROOM - b.disk_free;
        return (
            Fit::CantUse,
            Some(FitReason::Disk {
                short_bytes: short as f64,
            }),
        );
    }
    let role = role_of(job);
    let chosen = with_choice(current, role, &m.id);
    let fit = fit_for(busiest(&chosen, catalogue, b), b);
    if fit != Fit::WontFit {
        return (fit, None);
    }
    (fit, Some(memory_reason(role, &chosen, catalogue, b)))
}

/// Which other job, if any, keeps a choice that does not fit from fitting: one the person
/// changed that would make room by going back to the tier's own (`Memory` naming it), or one
/// whose smallest model would make room (`NeedsOtherJob`). `Memory` without a limiter when the
/// choice is too big whatever the others use.
fn memory_reason(
    role: ComponentRole,
    chosen: &Selection,
    catalogue: &[ModelInfo],
    b: &Budget,
) -> FitReason {
    let others = [
        ComponentRole::Mind,
        ComponentRole::Recall,
        ComponentRole::Speech,
        ComponentRole::Voice,
    ]
    .into_iter()
    .filter(|r| *r != role);
    let fits = |s: &Selection| busiest(s, catalogue, b) <= b.budget;
    let mut smallest = chosen.clone();
    for other in others.clone() {
        if let Some(m) = smallest_for(job_of(other), catalogue) {
            smallest = with_choice(&smallest, other, &m);
        }
    }
    if !fits(&smallest) {
        return FitReason::Memory { limiter: None };
    }
    for other in others.clone() {
        if let Some(preset) = tier::model_for(chosen.tier, other)
            && chosen.custom.contains_key(&other)
            && fits(&with_choice(chosen, other, preset))
        {
            return FitReason::Memory {
                limiter: Some(job_of(other)),
            };
        }
    }
    let picks = selection::picks(chosen);
    let mem = |id: &str| memory_of(id, catalogue.iter().find(|c| c.id == id));
    let largest = others
        .filter(|r| {
            smallest_for(job_of(*r), catalogue).is_some_and(|m| fits(&with_choice(chosen, *r, &m)))
        })
        .max_by_key(|r| mem(picks.get(*r)));
    match largest {
        Some(r) => FitReason::NeedsOtherJob { job: job_of(r) },
        None => FitReason::Memory { limiter: None },
    }
}

fn smallest_for(job: Job, catalogue: &[ModelInfo]) -> Option<String> {
    catalogue
        .iter()
        .filter(|m| qualifies(job, m).is_ok())
        .min_by_key(|m| memory_of(&m.id, Some(m)))
        .map(|m| m.id.clone())
}
