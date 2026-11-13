//! Opening the system's sound settings, where the microphone is allowed.
//!
//! Each candidate is started directly with fixed arguments, never through a shell, and the
//! first one that starts wins. A desktop with none of them says so, and the notice keeps its
//! sentence.

use std::io;

/// A program and its fixed arguments.
pub type Candidate = (&'static str, &'static [&'static str]);

#[cfg(windows)]
pub const CANDIDATES: &[Candidate] = &[("explorer.exe", &["ms-settings:privacy-microphone"])];

#[cfg(not(windows))]
pub const CANDIDATES: &[Candidate] = &[
    ("gnome-control-center", &["sound"]),
    ("systemsettings", &["kcm_pulseaudio"]),
    ("pavucontrol", &[]),
];

/// Try each candidate in order with `start`; the first that starts is the answer.
pub fn open_with(
    candidates: &[Candidate],
    start: &dyn Fn(&str, &[&str]) -> io::Result<()>,
) -> Result<&'static str, io::Error> {
    for (program, args) in candidates {
        match start(program, args) {
            Ok(()) => return Ok(program),
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "no sound settings program found",
    ))
}

/// Start a program and leave it running; a thread collects its exit so it does not linger.
fn spawn(program: &str, args: &[&str]) -> io::Result<()> {
    let mut child = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Open the sound settings on this computer.
pub fn open() -> Result<&'static str, io::Error> {
    open_with(CANDIDATES, &spawn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const LIST: &[Candidate] = &[("first", &["a"]), ("second", &["b", "c"]), ("third", &[])];

    #[test]
    fn the_first_program_that_starts_wins() {
        let tried = RefCell::new(Vec::new());
        let start = |p: &str, args: &[&str]| {
            tried.borrow_mut().push((p.to_string(), args.len()));
            if p == "first" {
                Err(io::Error::from(io::ErrorKind::NotFound))
            } else {
                Ok(())
            }
        };
        assert_eq!(open_with(LIST, &start).unwrap(), "second");
        assert_eq!(
            *tried.borrow(),
            [("first".to_string(), 1), ("second".to_string(), 2)],
            "later ones are not tried"
        );
    }

    #[test]
    fn none_found_is_not_found_and_a_real_failure_stops_the_search() {
        let missing = |_: &str, _: &[&str]| Err(io::Error::from(io::ErrorKind::NotFound));
        let err = open_with(LIST, &missing).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        let denied = |_: &str, _: &[&str]| Err(io::Error::from(io::ErrorKind::PermissionDenied));
        assert_eq!(
            open_with(LIST, &denied).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
    }

    #[test]
    fn the_candidates_are_fixed_programs_without_a_shell() {
        for (program, args) in CANDIDATES {
            assert!(
                !program.contains(' ') && !program.contains('/'),
                "{program}"
            );
            for a in *args {
                assert!(
                    !a.contains(';') && !a.contains('|') && !a.contains('$'),
                    "{a}"
                );
            }
        }
        assert!(
            !CANDIDATES
                .iter()
                .any(|(p, _)| matches!(*p, "sh" | "bash" | "cmd"))
        );
    }
}
