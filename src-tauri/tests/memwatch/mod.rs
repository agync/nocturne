//! Memory and processor use of this process and every process under it (the engine and the
//! model servers it starts), sampled from `/proc` while the run moves through named phases.
//!
//! Memory is proportional set size (shared pages split between the processes that map them)
//! plus graphics memory the processes hold, read from each DRM file's `fdinfo`. On a machine
//! with unified memory, such as a Mac, that graphics memory is ordinary memory too, so the sum
//! is the figure to compare against a memory budget. Where `/proc` is missing every reading is
//! zero. On macOS the figure for each process is the larger of its physical footprint (from
//! `/usr/bin/footprint`, which includes what the graphics processor uses) and its resident set
//! (from `ps`): the footprint leaves out model files the engine maps and reads in place, which
//! still occupy memory while a model is loaded.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

#[cfg(not(target_os = "macos"))]
const EVERY: Duration = Duration::from_millis(500);
#[cfg(target_os = "macos")]
const EVERY: Duration = Duration::from_secs(2);
const TICKS_PER_SECOND: f64 = 100.0;

#[derive(Default, Clone)]
struct Reading {
    pss_kib: u64,
    rss_kib: u64,
    gpu_kib: u64,
    ticks: u64,
    by_name: BTreeMap<String, u64>,
    ticks_by_name: BTreeMap<String, u64>,
}

impl Reading {
    /// On Linux the proportional set plus graphics memory. On macOS `pss_kib` already holds
    /// the larger of footprint and resident set, and `gpu_kib` the footprint alone.
    fn total_kib(&self) -> u64 {
        if cfg!(target_os = "macos") {
            self.pss_kib
        } else {
            self.pss_kib + self.gpu_kib
        }
    }
}

#[derive(Default)]
struct Phase {
    samples: u64,
    peak: Reading,
    sum_total_kib: u64,
    first_ticks: Option<(Instant, u64)>,
    last_ticks: Option<(Instant, u64)>,
    first_by_name: BTreeMap<String, u64>,
    last_by_name: BTreeMap<String, u64>,
    /// Each process's own highest reading in the phase, whenever it came.
    highest_by_name: BTreeMap<String, u64>,
}

#[derive(Default)]
struct Shared {
    phase: String,
    phases: Vec<(String, Phase)>,
}

pub struct MemWatch {
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl MemWatch {
    pub fn start(first_phase: &str) -> MemWatch {
        let shared = Arc::new(Mutex::new(Shared {
            phase: first_phase.to_string(),
            phases: vec![(first_phase.to_string(), Phase::default())],
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let (s, st) = (shared.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            let root = std::process::id();
            while !st.load(Ordering::Relaxed) {
                let r = read_tree(root);
                let now = Instant::now();
                let mut g = s.lock().unwrap();
                let name = g.phase.clone();
                if let Some((_, p)) = g.phases.iter_mut().rev().find(|(n, _)| *n == name) {
                    p.samples += 1;
                    p.sum_total_kib += r.total_kib();
                    if p.first_ticks.is_none() {
                        p.first_ticks = Some((now, r.ticks));
                        p.first_by_name = r.ticks_by_name.clone();
                    }
                    p.last_ticks = Some((now, r.ticks));
                    p.last_by_name = r.ticks_by_name.clone();
                    for (k, v) in &r.by_name {
                        let top = p.highest_by_name.entry(k.clone()).or_default();
                        *top = (*top).max(*v);
                    }
                    if r.total_kib() > p.peak.total_kib() {
                        p.peak = r;
                    }
                }
                drop(g);
                std::thread::sleep(EVERY);
            }
        });
        MemWatch {
            shared,
            stop,
            thread: Some(thread),
        }
    }

    /// Starts a new phase; samples from now on count towards it.
    pub fn phase(&self, name: &str) {
        let mut g = self.shared.lock().unwrap();
        if g.phase == name {
            return;
        }
        g.phase = name.to_string();
        g.phases.push((name.to_string(), Phase::default()));
    }

    /// Every phase's peak and mean, and the processor use across it, as JSON.
    pub fn report(&self) -> Value {
        let g = self.shared.lock().unwrap();
        let mut out = Vec::new();
        for (name, p) in &g.phases {
            if p.samples == 0 {
                continue;
            }
            let secs = match (p.first_ticks, p.last_ticks) {
                (Some((t0, _)), Some((t1, _))) if t1 > t0 => t1.duration_since(t0).as_secs_f64(),
                _ => 0.0,
            };
            let percent = |a: u64, b: u64| {
                if secs > 0.0 {
                    json!(
                        (b.saturating_sub(a) as f64 / TICKS_PER_SECOND / secs * 1000.0).round()
                            / 10.0
                    )
                } else {
                    Value::Null
                }
            };
            let cpu = match (p.first_ticks, p.last_ticks) {
                (Some((_, a)), Some((_, b))) => percent(a, b),
                _ => Value::Null,
            };
            let cpu_by_process: serde_json::Map<String, Value> = p
                .last_by_name
                .iter()
                .map(|(k, b)| {
                    (
                        k.clone(),
                        percent(p.first_by_name.get(k).copied().unwrap_or(*b), *b),
                    )
                })
                .collect();
            out.push(json!({
                "phase": name,
                "samples": p.samples,
                "peak_mib": mib(p.peak.total_kib()),
                "peak_pss_mib": mib(p.peak.pss_kib),
                "peak_gpu_mib": mib(p.peak.gpu_kib),
                "peak_rss_mib": mib(p.peak.rss_kib),
                "mean_mib": mib(p.sum_total_kib / p.samples),
                "cpu_percent_of_one_core": cpu,
                "cpu_by_process": cpu_by_process,
                "highest_by_process_mib": p.highest_by_name.iter()
                    .map(|(k, v)| (k.clone(), json!(mib(*v)))).collect::<serde_json::Map<_, _>>(),
                "at_peak_by_process_mib": p.peak.by_name.iter()
                    .map(|(k, v)| (k.clone(), json!(mib(*v)))).collect::<serde_json::Map<_, _>>(),
            }));
        }
        Value::Array(out)
    }
}

impl Drop for MemWatch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn mib(kib: u64) -> u64 {
    kib / 1024
}

#[cfg(target_os = "macos")]
fn read_tree(root: u32) -> Reading {
    let Ok(out) = std::process::Command::new("/bin/ps")
        .args(["-axo", "pid=,ppid=,rss=,time=,comm="])
        .env("LC_ALL", "C")
        .output()
    else {
        return Reading::default();
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut parent: HashMap<u32, u32> = HashMap::new();
    let mut comm: HashMap<u32, String> = HashMap::new();
    let mut ticks: HashMap<u32, u64> = HashMap::new();
    let mut rss: HashMap<u32, u64> = HashMap::new();
    for line in text.lines() {
        let mut f = line.split_whitespace();
        let (Some(pid), Some(ppid), Some(resident), Some(time)) =
            (f.next(), f.next(), f.next(), f.next())
        else {
            continue;
        };
        let (Ok(pid), Ok(ppid)) = (pid.parse::<u32>(), ppid.parse::<u32>()) else {
            continue;
        };
        let name: String = f.collect::<Vec<_>>().join(" ");
        let name = name.rsplit('/').next().unwrap_or("").to_string();
        // [[hh:]mm:]ss.cc
        let secs = time.split(':').fold(0.0, |acc, part| {
            acc * 60.0 + part.parse::<f64>().unwrap_or(0.0)
        });
        parent.insert(pid, ppid);
        comm.insert(pid, name);
        ticks.insert(pid, (secs * TICKS_PER_SECOND) as u64);
        rss.insert(pid, resident.parse().unwrap_or(0));
    }
    let mut members: HashSet<u32> = HashSet::from([root]);
    loop {
        let before = members.len();
        for (pid, ppid) in &parent {
            if members.contains(ppid) {
                members.insert(*pid);
            }
        }
        if members.len() == before {
            break;
        }
    }
    // The sampler's own `ps` and `footprint` are children of this process too.
    members.retain(|pid| !matches!(comm.get(pid).map(String::as_str), Some("ps" | "footprint")));
    let mut cmd = std::process::Command::new("/usr/bin/footprint");
    cmd.args(["--noCategories", "-f", "bytes"]);
    for pid in &members {
        cmd.args(["-p", &pid.to_string()]);
    }
    let mut footprint: HashMap<u32, u64> = HashMap::new();
    if let Ok(out) = cmd.output() {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            // "name [pid]: 64-bit    Footprint: 27427488 B (16384 bytes per page)"
            let (Some(open), Some(close), Some(at)) =
                (line.rfind('['), line.rfind("]:"), line.find("Footprint: "))
            else {
                continue;
            };
            let pid = line[open + 1..close].parse::<u32>().ok();
            let bytes = line[at + 11..]
                .split_whitespace()
                .next()
                .and_then(|n| n.parse::<u64>().ok());
            if let (Some(pid), Some(bytes)) = (pid, bytes) {
                footprint.insert(pid, bytes / 1024);
            }
        }
    }
    let mut r = Reading::default();
    for pid in members {
        let foot = footprint.get(&pid).copied().unwrap_or(0);
        let resident = rss.get(&pid).copied().unwrap_or(0);
        let kib = foot.max(resident);
        let t = ticks.get(&pid).copied().unwrap_or(0);
        r.pss_kib += kib;
        r.gpu_kib += foot;
        r.rss_kib += resident;
        r.ticks += t;
        let name = if pid == root {
            "app".to_string()
        } else {
            comm.get(&pid).cloned().unwrap_or_default()
        };
        *r.by_name.entry(name.clone()).or_default() += kib;
        *r.ticks_by_name.entry(name).or_default() += t;
    }
    r
}

#[cfg(not(target_os = "macos"))]
fn read_tree(root: u32) -> Reading {
    let mut parent: HashMap<u32, u32> = HashMap::new();
    let mut comm: HashMap<u32, String> = HashMap::new();
    let mut ticks: HashMap<u32, u64> = HashMap::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Reading::default();
    };
    for e in entries.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(e.path().join("stat")) else {
            continue;
        };
        let (Some(open), Some(close)) = (stat.find('('), stat.rfind(')')) else {
            continue;
        };
        let name = stat[open + 1..close].to_string();
        let fields: Vec<&str> = stat[close + 1..].split_whitespace().collect();
        let ppid = fields.get(1).and_then(|f| f.parse().ok()).unwrap_or(0);
        let ut: u64 = fields.get(11).and_then(|f| f.parse().ok()).unwrap_or(0);
        let st: u64 = fields.get(12).and_then(|f| f.parse().ok()).unwrap_or(0);
        parent.insert(pid, ppid);
        comm.insert(pid, name);
        ticks.insert(pid, ut + st);
    }
    let mut members: HashSet<u32> = HashSet::from([root]);
    loop {
        let before = members.len();
        for (pid, ppid) in &parent {
            if members.contains(ppid) {
                members.insert(*pid);
            }
        }
        if members.len() == before {
            break;
        }
    }
    let mut r = Reading::default();
    for pid in members {
        let (pss, rss) = smaps(pid);
        let gpu = gpu_kib(pid);
        r.pss_kib += pss;
        r.rss_kib += rss;
        r.gpu_kib += gpu;
        let mut t = ticks.get(&pid).copied().unwrap_or(0);
        if pid == root {
            // The sampler is a thread of this process; its own reading of /proc is not the app's.
            t = t.saturating_sub(own_ticks());
        }
        r.ticks += t;
        let name = if pid == root {
            "app".to_string()
        } else {
            comm.get(&pid).cloned().unwrap_or_default()
        };
        *r.by_name.entry(name.clone()).or_default() += pss + gpu;
        *r.ticks_by_name.entry(name).or_default() += t;
    }
    r
}

#[cfg(not(target_os = "macos"))]
/// Processor ticks the calling thread has used.
fn own_ticks() -> u64 {
    let Ok(stat) = std::fs::read_to_string("/proc/thread-self/stat") else {
        return 0;
    };
    let Some(close) = stat.rfind(')') else {
        return 0;
    };
    let fields: Vec<&str> = stat[close + 1..].split_whitespace().collect();
    let ut: u64 = fields.get(11).and_then(|f| f.parse().ok()).unwrap_or(0);
    let st: u64 = fields.get(12).and_then(|f| f.parse().ok()).unwrap_or(0);
    ut + st
}

#[cfg(not(target_os = "macos"))]
fn smaps(pid: u32) -> (u64, u64) {
    let Ok(text) = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")) else {
        return (0, 0);
    };
    let field = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key))
            .and_then(|r| r.split_whitespace().next())
            .and_then(|n| n.parse::<u64>().ok())
            .unwrap_or(0)
    };
    (field("Pss:"), field("Rss:"))
}

#[cfg(not(target_os = "macos"))]
/// Graphics memory the process holds, once per DRM client, in KiB.
fn gpu_kib(pid: u32) -> u64 {
    let Ok(entries) = std::fs::read_dir(format!("/proc/{pid}/fdinfo")) else {
        return 0;
    };
    let mut clients: HashMap<String, u64> = HashMap::new();
    for e in entries.flatten() {
        let Ok(text) = std::fs::read_to_string(e.path()) else {
            continue;
        };
        let Some(id) = text
            .lines()
            .find_map(|l| l.strip_prefix("drm-client-id:"))
            .map(|s| s.trim().to_string())
        else {
            continue;
        };
        let mut total = 0;
        let mut resident = 0;
        let mut legacy = 0;
        for line in text.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let kib = kib_of(value);
            if key.starts_with("drm-resident-") {
                resident += kib;
            } else if key.starts_with("drm-total-") {
                total += kib;
            } else if key.starts_with("drm-memory-") {
                legacy += kib;
            }
        }
        let held = if resident > 0 {
            resident
        } else if total > 0 {
            total
        } else {
            legacy
        };
        clients.insert(id, held);
    }
    clients.values().sum()
}

#[cfg(not(target_os = "macos"))]
fn kib_of(value: &str) -> u64 {
    let mut parts = value.split_whitespace();
    let n: u64 = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
    match parts.next() {
        Some("MiB") => n * 1024,
        Some("GiB") => n * 1024 * 1024,
        Some("KiB") => n,
        _ => n / 1024,
    }
}
