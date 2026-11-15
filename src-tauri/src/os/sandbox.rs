//! Whether the app runs inside a Flatpak. Inside one, paths such as `/app/bin/nocturne` exist
//! only in the sandbox, and anything the desktop runs has to go through `flatpak run`.

use std::sync::OnceLock;

/// The file Flatpak puts at the root of every sandbox.
const INFO: &str = "/.flatpak-info";

/// The app id from the sandbox's `/.flatpak-info` (its `[Application]` `name`), else, when that
/// file has no name, from `FLATPAK_ID`. `None` outside a Flatpak, which is when there is no info
/// file, and when the id is not a well-formed app id (so it can go into a command line as it is).
pub fn flatpak_app_id(info: Option<&str>, env_id: Option<&str>) -> Option<String> {
    let info = info?;
    let mut section = "";
    let mut name = None;
    for line in info.lines().map(str::trim) {
        if let Some(s) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = s;
        } else if section == "Application"
            && let Some(v) = line.strip_prefix("name=")
        {
            name = Some(v.trim());
        }
    }
    let id = name.or(env_id)?;
    is_app_id(id).then(|| id.to_string())
}

/// A Flatpak app id: at least two dot-separated parts of ASCII letters, digits, `_` and `-`,
/// not starting with `-` (`flatpak run` would read that as an option).
fn is_app_id(id: &str) -> bool {
    let parts: Vec<_> = id.split('.').collect();
    parts.len() >= 2
        && !id.starts_with('-')
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
}

/// This process's Flatpak app id, read once.
pub fn flatpak_app_id_now() -> Option<&'static str> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    static ID: OnceLock<Option<String>> = OnceLock::new();
    ID.get_or_init(|| {
        flatpak_app_id(
            std::fs::read_to_string(INFO).ok().as_deref(),
            std::env::var("FLATPAK_ID").ok().as_deref(),
        )
    })
    .as_deref()
}

#[cfg(test)]
mod tests {
    use super::*;

    const INFO: &str = "[Application]\nname=is.nocturne.Nocturne\nruntime=runtime/org.gnome.Platform/x86_64/51\n\n[Instance]\ninstance-id=123\n";

    #[test]
    fn the_info_file_names_the_app() {
        assert_eq!(
            flatpak_app_id(Some(INFO), None).as_deref(),
            Some("is.nocturne.Nocturne")
        );
        assert_eq!(
            flatpak_app_id(Some(INFO), Some("other.App")).as_deref(),
            Some("is.nocturne.Nocturne"),
            "the file wins over the variable"
        );
    }

    #[test]
    fn without_the_info_file_it_is_not_a_flatpak() {
        assert_eq!(flatpak_app_id(None, Some("is.nocturne.Nocturne")), None);
        assert_eq!(flatpak_app_id(None, None), None);
    }

    #[test]
    fn the_variable_stands_in_for_a_file_without_a_name() {
        assert_eq!(
            flatpak_app_id(Some("[Instance]\nname=x.y\n"), Some("is.nocturne.Nocturne")).as_deref(),
            Some("is.nocturne.Nocturne"),
            "a name in another section is not the app's"
        );
        assert_eq!(flatpak_app_id(Some(""), None), None);
    }

    #[test]
    fn an_id_that_is_not_an_app_id_is_refused() {
        for bad in [
            "nocturne",
            "is..Nocturne",
            "is.nocturne.Noc turne",
            "is.nocturne.N;rm",
            "is.nocturne.N$(x)",
            "-x.y",
        ] {
            let info = format!("[Application]\nname={bad}\n");
            assert_eq!(flatpak_app_id(Some(&info), None), None, "{bad}");
        }
    }
}
