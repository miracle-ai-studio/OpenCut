//! Native one-output writer ownership. The kernel ceiling survives supervisor
//! death and the inherited flock prevents recovery while the writer still lives.
use crate::{AttemptLease, Reservation};
use std::{
    io,
    process::{Child, Command},
};

#[must_use = "Monitor and reap the admitted child; dropping this handle terminates it"]
pub struct ReservedChild {
    child: Child,
    _reservation: Option<Reservation>,
    _attempt: Option<AttemptLease>,
}
impl Drop for ReservedChild {
    fn drop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        // The reservation field drops afterwards. A killed supervisor cannot
        // run this destructor, so the child's inherited lock remains essential.
    }
}
impl ReservedChild {
    pub fn process(&mut self) -> &mut Child {
        &mut self.child
    }
}

#[cfg(unix)]
pub(super) fn spawn(
    reservation: Option<Reservation>,
    mut command: Command,
    attempt: Option<AttemptLease>,
) -> io::Result<ReservedChild> {
    use std::{
        fs::File,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::process::CommandExt,
        },
    };
    let (lock, bytes) = reservation
        .as_ref()
        .map(|r| {
            let (file, bytes) = r.inner.child_lock();
            (Some(file), bytes)
        })
        .unwrap_or((None, 0));
    // Never borrow stdio descriptor numbers. Only this child's pre_exec clears
    // CLOEXEC, so concurrent unrelated spawns cannot inherit the reservation.
    // SAFETY: lock is live; fcntl creates a distinct owned descriptor on success.
    let mut inherited = Vec::new();
    for lock in lock
        .into_iter()
        .chain(attempt.as_ref().map(|a| a.0.as_ref()))
    {
        let fd = unsafe { libc::fcntl(lock.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: fd was just allocated and has exactly one Rust owner.
        inherited.push(unsafe { File::from_raw_fd(fd) });
    }
    #[allow(clippy::useless_conversion)] // rlim_t width is a native ABI property
    let limit: libc::rlim_t = bytes.try_into().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Output ceiling does not fit native limits",
        )
    })?;
    // SAFETY: after fork the callback only reads captured scalar/descriptor
    // values and invokes native descriptor/resource-limit syscalls. It performs
    // no allocation, logging, lock acquisition or destructor work. The capture
    // owns the descriptor until spawn returns; exec keeps only its child copy.
    unsafe {
        command.pre_exec(move || {
            for file in &inherited {
                if libc::fcntl(file.as_raw_fd(), libc::F_SETFD, 0) == -1 {
                    return Err(io::Error::last_os_error());
                }
            }
            let mut current = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            if libc::getrlimit(libc::RLIMIT_FSIZE, &mut current) == -1 {
                return Err(io::Error::last_os_error());
            }
            let bounded = libc::rlimit {
                rlim_cur: current.rlim_cur.min(limit),
                rlim_max: current.rlim_max.min(limit),
            };
            if libc::setrlimit(libc::RLIMIT_FSIZE, &bounded) == -1 {
                return Err(io::Error::last_os_error());
            }
            // A size-limit signal must not create an unreserved crash dump.
            let no_core = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            if libc::setrlimit(libc::RLIMIT_CORE, &no_core) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command.spawn()?;
    // Drop the parent's duplicate before returning; the original remains in
    // reservation. The child owns its inherited reference until it closes/exits.
    drop(command);
    Ok(ReservedChild {
        child,
        _reservation: reservation,
        _attempt: attempt,
    })
}

#[cfg(not(unix))]
pub(super) fn spawn(
    _: Option<Reservation>,
    _: Command,
    _: Option<AttemptLease>,
) -> io::Result<ReservedChild> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Inherited output admission requires Unix",
    ))
}
