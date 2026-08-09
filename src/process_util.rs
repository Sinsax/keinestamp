#[cfg(windows)]
pub fn hide_console(mut cmd: std::process::Command) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    cmd
}

#[cfg(not(windows))]
pub fn hide_console(cmd: std::process::Command) -> std::process::Command {
    cmd
}
