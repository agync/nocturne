//! Programs started and waited for, without a console window on Windows. `nocturne-lemonade`
//! re-exports [`command`] beside its builders for programs that run on.

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A program to run and wait for, or to leave running unsupervised.
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut cmd, CREATE_NO_WINDOW);
    cmd
}
