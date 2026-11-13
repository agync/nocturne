//! The flags a helper accepts, read from the help text captured from the pinned binary
//! (`fixtures/help/<kind>/<version>/<command>.txt`, written by `scripts/helpers-pin.sh`).
//! Nocturne passes a flag only when the fixture lists it.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use crate::HelperKind;

/// `(kind, command, help text)`. A command is a file stem: `tunnel` for cloudflared, `help` for
/// nocturne-tailnet.
const FIXTURES: &[(HelperKind, &str, &str)] = &[
    (
        HelperKind::Cloudflared,
        "tunnel",
        include_str!("../fixtures/help/cloudflared/2024.3.0/tunnel.txt"),
    ),
    (
        HelperKind::Tailnet,
        "help",
        include_str!("../fixtures/help/tailnet/1.104.6/help.txt"),
    ),
];

type Parsed = Vec<(HelperKind, &'static str, Vec<&'static str>)>;

fn parsed() -> &'static Parsed {
    static PARSED: OnceLock<Parsed> = OnceLock::new();
    PARSED.get_or_init(|| {
        FIXTURES
            .iter()
            .map(|(kind, command, text)| {
                let flags: Vec<&'static str> = parse_flags(text)
                    .into_iter()
                    .map(|f| &*Box::leak(f.into_boxed_str()))
                    .collect();
                (*kind, *command, flags)
            })
            .collect()
    })
}

/// The flags listed by one command's help text, each as `--long` or `-s`.
///
/// Both help styles are read: `--flag value   description` (cloudflared) and Go's
/// `-flag string` followed by an indented description (nocturne-tailnet). Lines starting with `#` are
/// provenance notes, not help.
pub fn parse_flags(help: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in help.lines() {
        let line = line.trim();
        if !line.starts_with('-') {
            continue;
        }
        let head = line.split("  ").next().unwrap_or(line);
        for part in head.split(',') {
            let word = part.split_whitespace().next().unwrap_or("");
            let name = word.trim_start_matches('-');
            let name = name.split('=').next().unwrap_or("");
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            {
                continue;
            }
            out.insert(if name.len() == 1 {
                format!("-{name}")
            } else {
                format!("--{name}")
            });
        }
    }
    out
}

/// Every flag any captured command of this helper lists.
pub fn allowed_flags(kind: HelperKind) -> &'static [&'static str] {
    static ALL: OnceLock<Vec<(HelperKind, Vec<&'static str>)>> = OnceLock::new();
    let all = ALL.get_or_init(|| {
        [HelperKind::Cloudflared, HelperKind::Tailnet]
            .into_iter()
            .map(|kind| {
                let set: BTreeSet<&'static str> = parsed()
                    .iter()
                    .filter(|(k, _, _)| *k == kind)
                    .flat_map(|(_, _, flags)| flags.iter().copied())
                    .collect();
                (kind, set.into_iter().collect())
            })
            .collect()
    });
    all.iter()
        .find(|(k, _)| *k == kind)
        .map_or(&[], |(_, flags)| flags.as_slice())
}

/// The flags one command's captured help lists; empty for a command with no fixture.
pub fn allowed_flags_for(kind: HelperKind, command: &str) -> &'static [&'static str] {
    parsed()
        .iter()
        .find(|(k, c, _)| *k == kind && *c == command)
        .map_or(&[], |(_, _, flags)| flags.as_slice())
}

/// The commands that have a captured fixture.
#[cfg(test)]
pub fn commands(kind: HelperKind) -> Vec<&'static str> {
    FIXTURES
        .iter()
        .filter(|(k, _, _)| *k == kind)
        .map(|(_, c, _)| *c)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_help_styles_parse() {
        let go = "Usage of x:\n  -bird-socket string\n    \tpath of the bird unix socket\n  -cleanup\n    \tclean up\n";
        let flags = parse_flags(go);
        assert!(flags.contains("--bird-socket") && flags.contains("--cleanup"));
        let cobra = "OPTIONS:\n   --name value, -n value   Stable name\n  --accept-dns, --accept-dns=false\n    \taccept\n";
        let flags = parse_flags(cobra);
        for f in ["--name", "-n", "--accept-dns"] {
            assert!(flags.contains(f), "{f} in {flags:?}");
        }
        assert!(parse_flags("# note\nplain text\n").is_empty());
    }

    #[test]
    fn captured_help_lists_what_nocturne_needs() {
        let tunnel = allowed_flags_for(HelperKind::Cloudflared, "tunnel");
        for f in [
            "--no-autoupdate",
            "--metrics",
            "--loglevel",
            "--unix-socket",
            "--url",
        ] {
            assert!(tunnel.contains(&f), "{f}");
        }
        let tailnet = allowed_flags_for(HelperKind::Tailnet, "help");
        assert_eq!(tailnet, ["--hostname", "--state-dir", "--target"]);
        assert!(allowed_flags(HelperKind::Tailnet).contains(&"--hostname"));
        assert!(allowed_flags_for(HelperKind::Tailnet, "nonsense").is_empty());
    }

    #[test]
    fn every_pinned_version_has_fixtures() {
        let lock =
            crate::lock::HelpersLock::parse(crate::lock::HelpersLock::embedded_text()).unwrap();
        for (kind, dir) in [
            (HelperKind::Cloudflared, "cloudflared"),
            (HelperKind::Tailnet, "tailnet"),
        ] {
            let version = &lock.helper(kind).unwrap().version;
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures/help")
                .join(dir)
                .join(version);
            for command in commands(kind) {
                assert!(root.join(format!("{command}.txt")).is_file(), "{command}");
            }
        }
    }
}
