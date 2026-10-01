use std::fs::File;
use std::io;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result};
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

use crate::error::UserError;

/// Result of running an external program to completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Everything tabmon needs from the operating system, so commands can be tested with a fake.
pub trait System {
    fn run(&self, program: &str, args: &[&str]) -> Result<Output>;
    /// Starts a program in its own process group, with stdout and stderr going to `log`.
    fn spawn_detached(&self, program: &str, args: &[&str], log: &Path) -> Result<u32>;
    /// Name of a live process; `None` if it does not exist or is a zombie.
    fn process_name(&self, pid: u32) -> Option<String>;
    fn terminate(&self, pid: u32) -> Result<()>;
    fn module_loaded(&self, module: &str) -> bool;
    fn sleep(&self, duration: Duration);
}

pub struct RealSystem {
    pub verbose: bool,
}

impl RealSystem {
    fn trace(&self, program: &str, args: &[&str]) {
        if self.verbose {
            eprintln!("+ {program} {}", args.join(" "));
        }
    }
}

impl System for RealSystem {
    fn run(&self, program: &str, args: &[&str]) -> Result<Output> {
        self.trace(program, args);
        let out = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| spawn_error(program, e))?;
        Ok(Output {
            success: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }

    fn spawn_detached(&self, program: &str, args: &[&str], log: &Path) -> Result<u32> {
        self.trace(program, args);
        let file =
            File::create(log).with_context(|| format!("could not create {}", log.display()))?;
        let child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(file.try_clone()?)
            .stderr(file)
            .process_group(0)
            .spawn()
            .map_err(|e| spawn_error(program, e))?;
        Ok(child.id())
    }

    fn process_name(&self, pid: u32) -> Option<String> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        parse_stat(&stat)
    }

    fn terminate(&self, pid: u32) -> Result<()> {
        let raw = i32::try_from(pid).context("invalid pid")?;
        kill(Pid::from_raw(raw), Signal::SIGTERM)
            .with_context(|| format!("could not stop process {pid}"))
    }

    fn module_loaded(&self, module: &str) -> bool {
        Path::new("/sys/module").join(module).exists()
    }

    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// Process name from the contents of `/proc/<pid>/stat`; `None` for zombies or unreadable data.
pub fn parse_stat(stat: &str) -> Option<String> {
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    let name = stat.get(open + 1..close)?;
    let state = stat.get(close + 1..)?.trim_start().chars().next()?;
    if state == 'Z' || state == 'X' {
        None
    } else {
        Some(name.to_string())
    }
}

/// Arch package that provides a program tabmon calls.
pub fn package_for(program: &str) -> Option<&'static str> {
    match program {
        "niri" => Some("niri"),
        "wayvnc" => Some("wayvnc"),
        "adb" => Some("android-tools"),
        "notify-send" => Some("libnotify"),
        _ => None,
    }
}

fn spawn_error(program: &str, err: io::Error) -> anyhow::Error {
    if err.kind() != io::ErrorKind::NotFound {
        return anyhow::Error::new(err).context(format!("could not run {program}"));
    }
    let e = UserError::new(format!("{program} not found"));
    match package_for(program) {
        Some(pkg) => e.hint(format!("pacman -S {pkg}")).into(),
        None => e.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::render;
    use std::time::Instant;

    #[test]
    fn parse_stat_reads_the_name() {
        assert_eq!(
            parse_stat("1234 (wayvnc) S 1 1234 1234 0"),
            Some("wayvnc".to_string())
        );
    }

    #[test]
    fn parse_stat_handles_parentheses_in_the_name() {
        assert_eq!(
            parse_stat("7 (my (odd) name) R 1 7"),
            Some("my (odd) name".to_string())
        );
    }

    #[test]
    fn parse_stat_treats_zombies_as_dead() {
        assert_eq!(parse_stat("1234 (wayvnc) Z 1 1234 1234 0"), None);
        assert_eq!(parse_stat("1234 (wayvnc) X 1 1234 1234 0"), None);
    }

    #[test]
    fn parse_stat_rejects_garbage() {
        assert_eq!(parse_stat(""), None);
        assert_eq!(parse_stat("no parentheses here"), None);
    }

    #[test]
    fn missing_program_gets_pacman_hint() {
        let err = spawn_error("adb", io::Error::from(io::ErrorKind::NotFound));
        assert_eq!(
            render(&err),
            "error: adb not found\nhint: pacman -S android-tools"
        );
    }

    #[test]
    fn missing_unknown_program_has_no_hint() {
        let err = spawn_error("frobnicate", io::Error::from(io::ErrorKind::NotFound));
        assert_eq!(render(&err), "error: frobnicate not found");
    }

    #[test]
    fn run_captures_output_and_status() {
        let sys = RealSystem { verbose: false };
        let out = sys
            .run("sh", &["-c", "echo hi; echo oops >&2; exit 3"])
            .unwrap();
        assert!(!out.success);
        assert_eq!(out.stdout, "hi\n");
        assert_eq!(out.stderr, "oops\n");
    }

    #[test]
    fn run_reports_missing_programs() {
        let sys = RealSystem { verbose: false };
        let err = sys.run("tabmon-no-such-program", &[]).unwrap_err();
        assert_eq!(render(&err), "error: tabmon-no-such-program not found");
    }

    #[test]
    fn own_process_is_alive() {
        let sys = RealSystem { verbose: false };
        assert!(sys.process_name(std::process::id()).is_some());
    }

    #[test]
    fn spawned_process_can_be_terminated() {
        let sys = RealSystem { verbose: false };
        let dir = tempfile::tempdir().unwrap();
        let pid = sys
            .spawn_detached("sleep", &["30"], &dir.path().join("log"))
            .unwrap();
        // spawn() can return before the kernel renames the child in execve, so for a moment
        // /proc still shows the parent thread's name. Wait for the rename instead of racing it.
        let deadline = Instant::now() + Duration::from_secs(2);
        while sys.process_name(pid).as_deref() != Some("sleep") && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(sys.process_name(pid).as_deref(), Some("sleep"));

        sys.terminate(pid).unwrap();
        // The child stays a zombie until this test process exits; it must still count as dead.
        let deadline = Instant::now() + Duration::from_secs(2);
        while sys.process_name(pid).is_some() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(sys.process_name(pid), None);
    }

    #[test]
    fn spawned_process_writes_to_the_log() {
        let sys = RealSystem { verbose: false };
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log");
        sys.spawn_detached("sh", &["-c", "echo out; echo err >&2"], &log)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut text = String::new();
        while Instant::now() < deadline {
            text = std::fs::read_to_string(&log).unwrap_or_default();
            if text.contains("out") && text.contains("err") {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            text.contains("out") && text.contains("err"),
            "log was: {text:?}"
        );
    }
}
