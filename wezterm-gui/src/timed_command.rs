use std::io;
#[cfg(unix)]
use std::io::Read;
#[cfg(unix)]
use std::process::Stdio;
use std::process::{Command, Output};
use std::time::Duration;
#[cfg(unix)]
use std::time::Instant;

/// On Unix, bound both process and pipe waits and terminate the process group
/// on timeout. Other platforms retain the standard, unbounded `Command::output`.
pub fn output_with_timeout(command: &mut Command, timeout: Duration) -> io::Result<Output> {
    #[cfg(unix)]
    {
        return unix_output_with_timeout(command, timeout);
    }
    #[cfg(not(unix))]
    {
        let _ = timeout;
        command.output()
    }
}

#[cfg(unix)]
fn unix_output_with_timeout(command: &mut Command, timeout: Duration) -> io::Result<Output> {
    use std::os::fd::AsRawFd;
    use std::os::unix::process::CommandExt;

    let start = Instant::now();
    let mut child = command
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let process_group = child.id() as libc::pid_t;
    let result = (|| {
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        for fd in [stdout.as_raw_fd(), stderr.as_raw_fd()] {
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags == -1
                || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1
            {
                return Err(io::Error::last_os_error());
            }
        }
        // Nonblocking reads keep pipe cleanup synchronous even if a descendant
        // exits the process group and retains a pipe: no reader threads survive.
        let mut pipes: [Box<dyn Read>; 2] = [Box::new(stdout), Box::new(stderr)];
        let mut streams = [Vec::new(), Vec::new()];
        let mut closed = [false; 2];
        let mut status = None;
        let mut buffer = [0; 8192];
        loop {
            if status.is_none() {
                status = child.try_wait()?;
            }
            for index in 0..2 {
                while !closed[index] && start.elapsed() < timeout {
                    match pipes[index].read(&mut buffer) {
                        Ok(0) => closed[index] = true,
                        Ok(count) => streams[index].extend_from_slice(&buffer[..count]),
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                        Err(error) => return Err(error),
                    }
                }
            }
            if let Some(status) = status {
                if closed.iter().all(|closed| *closed) {
                    let [stdout, stderr] = streams;
                    return Ok(Output {
                        status,
                        stdout,
                        stderr,
                    });
                }
            }
            if start.elapsed() >= timeout {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "metadata command timed out",
                ));
            }
            std::thread::sleep(
                Duration::from_millis(10).min(timeout.saturating_sub(start.elapsed())),
            );
        }
    })();
    if result.is_err() {
        // The direct child may already have exited while descendants hold pipes.
        // Kill the group in either case, then reap the direct child if necessary.
        unsafe {
            libc::kill(-process_group, libc::SIGKILL);
        }
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn terminates_descendants_after_the_direct_child_exits() {
        let temp_dir = std::env::var_os("TMPDIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let marker = temp_dir.join(format!(
            "wezmux-timed-command-descendant-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&marker);
        let error = output_with_timeout(
            Command::new("sh")
                .args(["-c", "(sleep 0.2; printf survived > \"$1\") &", "sh"])
                .arg(&marker),
            Duration::from_millis(20),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        std::thread::sleep(Duration::from_millis(300));
        let survived = marker.exists();
        let _ = std::fs::remove_file(marker);
        assert!(!survived, "descendant survived the timeout");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn timeout_does_not_leave_pipe_reader_threads() {
        // Isolate the thread count from other tests running concurrently.
        const ISOLATED: &str = "WEZMUX_TIMED_COMMAND_THREAD_TEST";
        if std::env::var_os(ISOLATED).is_none() {
            let module = module_path!().split_once("::").unwrap().1;
            let test = format!("{module}::timeout_does_not_leave_pipe_reader_threads");
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", &test, "--test-threads=1"])
                .env(ISOLATED, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout).contains("1 passed"),
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let thread_count = || std::fs::read_dir("/proc/self/task").unwrap().count();
        let before = thread_count();
        for _ in 0..3 {
            let error = output_with_timeout(
                Command::new("sh").args(["-c", "sleep 2 &"]),
                Duration::from_millis(20),
            )
            .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        }
        assert_eq!(thread_count(), before, "pipe readers survived the timeout");
    }

    #[test]
    fn captures_stdout_stderr_and_failure_status() {
        let output = output_with_timeout(
            Command::new("sh").args(["-c", "printf result; printf diagnostic >&2; exit 7"]),
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(output.stdout, b"result");
        assert_eq!(output.stderr, b"diagnostic");
        assert_eq!(output.status.code(), Some(7));
    }

    #[test]
    fn terminates_a_stalled_command() {
        let start = std::time::Instant::now();
        let error = output_with_timeout(Command::new("sleep").arg("10"), Duration::from_millis(50))
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn reports_missing_executable() {
        let error = output_with_timeout(
            &mut Command::new("/nonexistent/wezmux-test-command"),
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }
}
