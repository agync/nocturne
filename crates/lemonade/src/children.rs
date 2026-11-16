//! What the engine is doing when it does not answer in time: whether it works or waits, and
//! which programs it has started. Only names are kept, never a folder or a command line.

/// Said when the processes cannot be read at all.
pub(crate) const UNREADABLE: &str = "the processes could not be read";

/// One running process, as much of it as is needed to describe the engine.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Proc {
    pid: u32,
    parent: Option<u32>,
    /// Seconds since the Unix epoch.
    started: u64,
    name: String,
    /// Processor time used so far, in milliseconds.
    cpu_ms: u64,
}

/// The processes started, directly or not, by `root`. A parent counts only when it started no
/// later than its child, so a number the system gave out again links nothing.
fn under(procs: &[Proc], root: u32) -> Vec<&Proc> {
    let Some(top) = procs.iter().find(|p| p.pid == root) else {
        return Vec::new();
    };
    let by_pid = |pid: u32| procs.iter().find(|p| p.pid == pid);
    procs
        .iter()
        .filter(|p| p.pid != root && p.started >= top.started)
        .filter(|p| {
            let mut at = *p;
            // A chain can only be as long as the list; a longer one is a loop.
            for _ in 0..procs.len() {
                let Some(parent) = at.parent else {
                    return false;
                };
                if parent == root {
                    return true;
                }
                match by_pid(parent) {
                    Some(next) if next.started <= at.started => at = next,
                    _ => return false,
                }
            }
            false
        })
        .collect()
}

/// A program's own file name, never a folder.
fn plain_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    base.chars().take(64).collect()
}

fn procs() -> Option<Vec<Proc>> {
    let mut sys = sysinfo::System::new().ok()?;
    sys.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        sysinfo::ProcessRefreshKind::nothing()
            .without_tasks()
            .with_cpu(),
    );
    Some(
        sys.processes()
            .values()
            .filter(|p| p.thread_kind().is_none())
            .map(|p| Proc {
                pid: p.pid().as_u32(),
                parent: p.parent().map(|q| q.as_u32()),
                started: p.start_time(),
                name: plain_name(&p.name().to_string_lossy()),
                cpu_ms: p.accumulated_cpu_time(),
            })
            .collect(),
    )
}

/// One line on `root` for the log: the processor time it has used, which tells a busy engine
/// from one that waits, and the names of the programs it started that still run.
pub(crate) fn describe(root: u32) -> String {
    match procs() {
        Some(all) => describe_in(&all, root),
        None => UNREADABLE.into(),
    }
}

fn describe_in(all: &[Proc], root: u32) -> String {
    let Some(top) = all.iter().find(|p| p.pid == root) else {
        return format!(
            "the engine is not among the {} processes that could be read",
            all.len()
        );
    };
    let mut names: Vec<&str> = under(all, root).iter().map(|p| p.name.as_str()).collect();
    names.sort_unstable();
    format!(
        "the engine is running and has used {:.1} s of processor time; programs it started that still run: {names:?}",
        top.cpu_ms as f64 / 1000.0
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, parent: Option<u32>, started: u64, name: &str) -> Proc {
        Proc {
            pid,
            parent,
            started,
            name: name.into(),
            cpu_ms: 0,
        }
    }

    fn pids(found: Vec<&Proc>) -> Vec<u32> {
        let mut out: Vec<u32> = found.iter().map(|p| p.pid).collect();
        out.sort();
        out
    }

    fn tree() -> Vec<Proc> {
        vec![
            p(1, None, 0, "system"),
            p(10, Some(1), 100, "nocturne.exe"),
            Proc {
                cpu_ms: 2340,
                ..p(20, Some(10), 110, "lemond.exe")
            },
            p(30, Some(20), 112, "conhost.exe"),
            p(31, Some(30), 113, "child.exe"),
            p(40, Some(10), 111, "other.exe"),
        ]
    }

    #[test]
    fn everything_the_engine_started_is_under_it_and_nothing_else() {
        let all = tree();
        assert_eq!(pids(under(&all, 20)), [30, 31]);
        assert!(under(&all, 99).is_empty());
    }

    #[test]
    fn a_reused_number_links_nothing() {
        let mut all = tree();
        // Its parent was a process numbered 20 that ended before the engine had that number.
        all.push(p(50, Some(20), 105, "old.exe"));
        // A process whose recorded parent started after it.
        all.push(p(51, Some(30), 111, "older.exe"));
        assert_eq!(pids(under(&all, 20)), [30, 31]);
    }

    #[test]
    fn a_loop_of_parents_ends() {
        let all = vec![
            p(20, None, 100, "lemond.exe"),
            p(60, Some(61), 100, "a"),
            p(61, Some(60), 100, "b"),
        ];
        assert!(under(&all, 20).is_empty());
    }

    #[test]
    fn the_description_has_the_time_and_the_names_only() {
        let mut all = tree();
        all[3].name = plain_name(r"\??\C:\WINDOWS\system32\conhost.exe");
        assert_eq!(
            describe_in(&all, 20),
            "the engine is running and has used 2.3 s of processor time; programs it started \
             that still run: [\"child.exe\", \"conhost.exe\"]"
        );
        assert_eq!(
            describe_in(&all, 99),
            "the engine is not among the 6 processes that could be read"
        );
    }

    #[test]
    fn a_name_is_its_file_name_only() {
        assert_eq!(plain_name(r"C:\Users\someone\tools\hook.exe"), "hook.exe");
        assert_eq!(plain_name("/opt/tools/bin/x"), "x");
        assert_eq!(plain_name("cmd.exe"), "cmd.exe");
        assert_eq!(plain_name(&"a".repeat(100)).len(), 64);
    }

    #[test]
    fn a_running_child_is_named_under_its_parent() {
        #[cfg(unix)]
        let (program, args, name) = ("sleep", vec!["30"], "sleep");
        #[cfg(windows)]
        let (program, args, name) = ("ping", vec!["-n", "30", "127.0.0.1"], "PING.EXE");
        let mut child = crate::process::command(program)
            .args(args)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let said = describe(std::process::id());
        let _ = child.kill();
        let _ = child.wait();
        assert!(
            said.to_ascii_lowercase()
                .contains(&format!("\"{}\"", name.to_ascii_lowercase())),
            "{said}"
        );
        assert!(said.starts_with("the engine is running"), "{said}");
    }
}
