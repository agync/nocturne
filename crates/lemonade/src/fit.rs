//! Whether a model fits this computer for a job: the memory budget, the busiest moment of the
//! chosen set, the engine's own size cap and the free disk.

use nocturne_contract::{ComponentRole, Fit, FitReason, Job, ModelChoice, Tier};

use crate::client::{BackendState, ModelInfo, SystemInfo};
use crate::decision_models::{D1_OMNI_2B, D3_LITE_FLM, DecisionModel};
use crate::fit_table::{self, PRESET_PEAKS};
use crate::npu;
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
    budget(ram, info.devices.largest_pool(), disk_free)
}

/// The memory this computer has, in bytes.
pub fn ram_bytes() -> u64 {
    sysinfo::System::new()
        .map(|mut sys| {
            sys.refresh_memory();
            sys.total_memory()
        })
        .unwrap_or(0)
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

/// Reading: the mind, recall and speech together, plus what the end-to-end runs measured above
/// the table's figures for the preset whose mind this is. That excess is the mind's working
/// memory beside the others, so it follows the mind whatever speech and recall are chosen.
pub fn read_set_with(picks: &Picks, mem: impl Fn(&str) -> u64) -> u64 {
    mem(&picks.mind) + mem(&picks.recall) + mem(&picks.speech) + measured_excess(&picks.mind)
}

/// How much more a measured preset needed at its busiest than its models' table figures add up
/// to, for the preset whose mind is `mind`.
pub fn measured_excess(mind: &str) -> u64 {
    PRESET_PEAKS
        .iter()
        .filter(|p| p.measured && tier::model_for(p.tier, ComponentRole::Mind) == Some(mind))
        .map(|p| {
            let own = Picks {
                mind: mind.to_string(),
                speech: tier::model_for(p.tier, ComponentRole::Speech)
                    .unwrap_or_default()
                    .to_string(),
                voice: String::new(),
                recall: tier::model_for(p.tier, ComponentRole::Recall)
                    .unwrap_or_default()
                    .to_string(),
            };
            let sum: u64 = [&own.mind, &own.speech, &own.recall]
                .into_iter()
                .map(|m| memory_of(m, None))
                .sum();
            p.busiest.saturating_sub(sum)
        })
        .max()
        .unwrap_or(0)
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

/// The model that answers decisions on this computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeciderPick {
    /// The mind already loaded answers them.
    Mind,
    Model(&'static DecisionModel),
}

/// The decider that suits this computer: `ram` bytes of memory, `vram` bytes on its largest
/// graphics card of its own (0 for graphics that share the memory) and the NPU as `npu` reads it.
///
/// In order: `d3-lite-FLM` when the NPU is [`Readiness::Ready`]; else `d1-Omni-2B` when the engine
/// gives llama.cpp a graphics backend (never the CPU); else the mind. An Unsupported tier always
/// gets the mind, and a mind the person chose changes nothing, since the models are picked by the
/// hardware.
///
/// A decision model loads in the engine's `classification` pool, so it does not evict the mind
/// under `max_loaded_models`, but its memory still counts. Deciding runs while items are read, so
/// it is added to the reading moment of `s`, where the NPU's mind is counted as
/// [`npu::placement`] places it (beside the GPU mind, or in its place). A model is picked only
/// when the busiest moment with it still Fits the budget. On a card of its own the GPU model must
/// also fit on the card beside the mind and recall, and then costs the shared memory nothing;
/// when it does not fit the card, the choice falls to the next in the list. Pure: the same inputs
/// give the same pick.
pub fn decider_for(
    info: &SystemInfo,
    s: &Selection,
    ram: u64,
    vram: u64,
    npu: &npu::Readiness,
) -> DeciderPick {
    let mut picks = selection::picks(s);
    if s.tier == Tier::Unsupported || picks.mind.is_empty() {
        return DeciderPick::Mind;
    }
    let mem = |id: &str| memory_of(id, None);
    let b = budget(ram, info.devices.largest_pool(), u64::MAX);
    let alone_voice = !residency::alone_for(s, &b).is_empty();
    let ready = *npu == npu::Readiness::Ready;
    let mut extra = 0;
    if ready && !s.custom.contains_key(&ComponentRole::Speech) && npu::fits(s, ram) {
        picks.speech = npu::SPEECH.model.to_string();
    }
    if ready {
        match npu::placement(s, ram, vram) {
            npu::Placement::Gpu => {}
            npu::Placement::Split => extra = mem(npu::MIND.model),
            npu::Placement::NpuOnly => picks.mind = npu::MIND.model.to_string(),
        }
    }
    let own = selection::picks(s);
    let on_card = mem(&own.mind) + mem(&own.recall);
    let card_holds = vram > 0 && on_card <= vram;
    // The busiest moment with `model` added to reading. When the card holds the mind and recall
    // they are not in the shared memory, as `npu::placement` counts it.
    let busiest_with = |model: &DecisionModel| {
        if card_holds {
            return mem(&picks.speech) + mem(&picks.voice) + extra + mem(model.id);
        }
        let read = read_set_with(&picks, mem) + extra + mem(model.id);
        (extra + peak_with(&picks, alone_voice, mem)).max(read)
    };
    let fits = |need: u64| fit_for(need, &b) == Fit::Fits;

    if ready && fits(busiest_with(&D3_LITE_FLM)) {
        return DeciderPick::Model(&D3_LITE_FLM);
    }
    if tier::choose_backend(info, D1_OMNI_2B.recipe) == "cpu" {
        return DeciderPick::Mind;
    }
    // On the card the model sits beside the mind and recall and costs the shared memory nothing.
    let ok = if card_holds {
        on_card + mem(D1_OMNI_2B.id) <= vram && fits(busiest_with(&D1_OMNI_2B) - mem(D1_OMNI_2B.id))
    } else {
        vram == 0 && fits(busiest_with(&D1_OMNI_2B))
    };
    if ok {
        return DeciderPick::Model(&D1_OMNI_2B);
    }
    DeciderPick::Mind
}
