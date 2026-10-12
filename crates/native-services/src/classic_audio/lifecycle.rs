//! Native process-group termination and parent exit acknowledgement.
use std::{
    process::Child,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

#[cfg(unix)]
pub(crate) fn kill_group(pid: u32) {
    if pid > 0 && pid <= i32::MAX as u32 {
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
}
#[cfg(unix)]
pub(crate) fn group_exists(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    unsafe {
        libc::kill(-(pid as i32), 0) == 0
            || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }
}
pub(crate) fn wait_group_gone(pid: u32, timeout: Duration) -> bool {
    let start = Instant::now();
    loop {
        if !group_exists(pid) {
            return true;
        }
        if start.elapsed() >= timeout {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
pub(crate) struct ProcessGuard {
    pub(crate) child: Child,
    pub(crate) browser_pid: Arc<AtomicU32>,
    pub(crate) done: Arc<AtomicBool>,
}
impl ProcessGuard {
    pub(crate) fn terminate(&mut self) {
        #[cfg(unix)]
        {
            kill_group(self.browser_pid.load(Ordering::Acquire));
            signal_unreaped_node(&mut self.child, kill_group);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Drop for ProcessGuard {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Release);
        if self.child.try_wait().ok().flatten().is_none() {
            self.terminate();
        }
    }
}

/// An exited child may already have been reaped by try_wait. Never signal its
/// cached numeric process group after that point; no other worker reaps it.
fn signal_unreaped_node(child: &mut Child, signal: impl FnOnce(u32)) {
    if matches!(child.try_wait(), Ok(None)) {
        signal(child.id());
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    #[test]
    fn node_group_signal_is_only_issued_while_the_child_is_unreaped() {
        let mut child = Command::new("/bin/cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let mut signalled = None;
        let expected = child.id();
        signal_unreaped_node(&mut child, |pid| signalled = Some(pid));
        assert_eq!(signalled, Some(expected));
        child.kill().unwrap();
        child.wait().unwrap();
        signal_unreaped_node(&mut child, |_| panic!("reaped PID must never be signalled"));
        // Repeated failed cleanup remains safe with the Child's cached status.
        signal_unreaped_node(&mut child, |_| panic!("repeated reaped PID signal"));
    }
}
