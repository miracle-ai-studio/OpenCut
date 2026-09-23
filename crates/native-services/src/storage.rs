//! Descriptor-relative inventory. Never traverses a discovered symbolic link.
use opencut_contracts::EditorError;
use opencut_editor_application::storage::{InspectInput, StorageInspector, UsageReport};
use std::path::Path;

pub struct NativeStorage<'a>(pub &'a Path);
impl StorageInspector for NativeStorage<'_> {
    fn inspect(&self, input: &InspectInput) -> Result<UsageReport, EditorError> {
        if self.0 == Path::new(":memory:") {
            return Err(EditorError::new(
                "CAPABILITY_UNAVAILABLE",
                "Storage observation requires a file-backed workspace",
            ));
        }
        #[cfg(unix)]
        {
            unix::inspect(self.0, input)
        }
        #[cfg(not(unix))]
        {
            let _ = input;
            Err(EditorError::new(
                "CAPABILITY_UNAVAILABLE",
                "Descriptor-based storage observation is unavailable on this platform",
            ))
        }
    }
}

// Native stat field widths differ between Unix targets.
#[cfg(unix)]
#[allow(clippy::unnecessary_cast)]
mod unix {
    use super::*;
    use opencut_api_contracts::storage::{Category, Omissions, StopReason, Usage};
    use opencut_editor_application::storage;
    use rustix::{
        fd::AsFd,
        fs::{self, AtFlags, FileType, Mode, OFlags, Stat},
    };
    use std::{
        collections::HashSet,
        ffi::OsStr,
        os::unix::ffi::OsStrExt,
        sync::atomic::{AtomicUsize, Ordering},
        time::{Duration, Instant},
    };

    static ACTIVE: AtomicUsize = AtomicUsize::new(0);
    struct Permit;
    impl Drop for Permit {
        fn drop(&mut self) {
            ACTIVE.fetch_sub(1, Ordering::Release);
        }
    }
    const DIRECTORY: OFlags = OFlags::RDONLY
        .union(OFlags::DIRECTORY)
        .union(OFlags::NOFOLLOW)
        .union(OFlags::CLOEXEC);
    fn failed() -> EditorError {
        EditorError::new("STORAGE_ERROR", "Cannot observe workspace storage")
    }
    fn usage(category: Category) -> Usage {
        Usage {
            category,
            present: false,
            complete: true,
            regular_files: 0,
            directories: 0,
            hardlink_aliases: 0,
            logical_bytes: "0".into(),
            allocated_bytes: "0".into(),
            omissions: Omissions::default(),
        }
    }
    struct Scan {
        device: u64,
        seen: HashSet<(u64, u64)>,
        started: Instant,
        max_entries: usize,
        visited: usize,
        stop: Option<StopReason>,
    }
    impl Scan {
        fn visit(&mut self, u: &mut Usage) -> bool {
            if self.stop.is_none() {
                if self.visited >= self.max_entries {
                    self.stop = Some(StopReason::EntryLimit);
                } else if self.started.elapsed() >= Duration::from_millis(storage::TIME_LIMIT_MS) {
                    self.stop = Some(StopReason::TimeLimit);
                }
            }
            if self.stop.is_some() {
                u.complete = false;
                return false;
            }
            self.visited += 1;
            true
        }
        fn error(&self, u: &mut Usage) {
            u.complete = false;
            u.omissions.io_errors += 1;
        }
        fn node(
            &mut self,
            parent: &impl AsFd,
            name: &OsStr,
            depth: usize,
            missing_ok: bool,
            expected_file: bool,
            u: &mut Usage,
        ) {
            if !self.visit(u) {
                return;
            }
            let stat = match fs::statat(parent, name, AtFlags::SYMLINK_NOFOLLOW) {
                Ok(s) => s,
                Err(rustix::io::Errno::NOENT) if missing_ok => return,
                Err(_) => {
                    self.error(u);
                    return;
                }
            };
            u.present = true;
            if stat.st_dev as u64 != self.device {
                u.complete = false;
                u.omissions.other_devices += 1;
                return;
            }
            let kind = FileType::from_raw_mode(stat.st_mode);
            if kind == FileType::Symlink {
                u.complete = false;
                u.omissions.symlinks += 1;
                return;
            }
            if kind != FileType::RegularFile && (kind != FileType::Directory || expected_file) {
                u.complete = false;
                u.omissions.other_file_types += 1;
                return;
            }
            if kind == FileType::RegularFile {
                self.count(&stat, false, u);
                return;
            }
            if depth >= storage::MAX_DEPTH {
                u.complete = false;
                u.omissions.depth_limited += 1;
                return;
            }
            // An entry can be swapped after statat: openat refuses links; fstat verifies identity.
            let fd = match fs::openat(parent, name, DIRECTORY, Mode::empty()) {
                Ok(fd) => fd,
                Err(_) => {
                    self.error(u);
                    return;
                }
            };
            let opened = match fs::fstat(&fd) {
                Ok(s) if s.st_dev == stat.st_dev && s.st_ino == stat.st_ino => s,
                _ => {
                    self.error(u);
                    return;
                }
            };
            if !self.count(&opened, true, u) {
                return;
            }
            let mut dir = match fs::Dir::read_from(&fd) {
                Ok(d) => d,
                Err(_) => {
                    self.error(u);
                    return;
                }
            };
            while let Some(entry) = dir.read() {
                let entry = match entry {
                    Ok(e) => e,
                    Err(_) => {
                        self.error(u);
                        break;
                    }
                };
                let name = entry.file_name().to_bytes();
                if name == b"." || name == b".." {
                    continue;
                }
                self.node(&fd, OsStr::from_bytes(name), depth + 1, false, false, u);
                if self.stop.is_some() {
                    break;
                }
            }
        }
        fn count(&mut self, s: &Stat, directory: bool, u: &mut Usage) -> bool {
            if !self.seen.insert((s.st_dev as u64, s.st_ino as u64)) {
                u.hardlink_aliases += 1;
                return false;
            }
            let allocated = u128::try_from(s.st_blocks).unwrap_or(0) * 512;
            u.allocated_bytes = (u.allocated_bytes.parse::<u128>().expect("internal bytes")
                + allocated)
                .to_string();
            if directory {
                u.directories += 1;
            } else {
                u.regular_files += 1;
                u.logical_bytes = (u.logical_bytes.parse::<u128>().expect("internal bytes")
                    + u128::try_from(s.st_size).unwrap_or(0))
                .to_string();
            }
            true
        }
    }
    pub fn inspect(database: &Path, input: &InspectInput) -> Result<UsageReport, EditorError> {
        ACTIVE
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < storage::MAX_CONCURRENT).then_some(n + 1)
            })
            .map_err(|_| {
                EditorError::new(
                    "STORAGE_SCAN_BUSY",
                    "Storage observation concurrency limit reached; retry later",
                )
            })?;
        let _permit = Permit;
        let root = database
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let basename = database.file_name().ok_or_else(failed)?;
        let fd = fs::open(root, DIRECTORY, Mode::empty()).map_err(|_| failed())?;
        let device = fs::fstat(&fd).map_err(|_| failed())?.st_dev as u64;
        let volume = fs::fstatvfs(&fd).ok();
        let mut scan = Scan {
            device,
            seen: HashSet::new(),
            started: Instant::now(),
            max_entries: input.max_entries,
            visited: 0,
            stop: None,
        };
        let mut categories = Vec::new();
        let mut db = usage(Category::Database);
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let mut name = basename.to_os_string();
            name.push(suffix);
            scan.node(&fd, &name, 0, true, true, &mut db);
        }
        scan.node(&fd, OsStr::new(".storage-budget"), 0, true, false, &mut db);
        categories.push(db);
        for (category, name) in [
            (Category::AssetStore, "asset-store"),
            (Category::Media, "media"),
            (Category::Uploads, "uploads"),
            (Category::Artifacts, "artifacts"),
            (Category::Plugins, "plugins"),
        ] {
            let mut u = usage(category);
            scan.node(&fd, OsStr::new(name), 0, true, false, &mut u);
            categories.push(u);
        }
        let sum = |allocated: bool| {
            categories
                .iter()
                .map(|u| {
                    if allocated {
                        &u.allocated_bytes
                    } else {
                        &u.logical_bytes
                    }
                })
                .map(|s| s.parse::<u128>().expect("internal bytes"))
                .sum::<u128>()
                .to_string()
        };
        Ok(UsageReport {
            schema_version: 1,
            consistency: "live_non_atomic".into(),
            complete: categories.iter().all(|u| u.complete),
            visited_entries: scan.visited,
            stop_reason: scan.stop,
            logical_bytes: sum(false),
            allocated_bytes: sum(true),
            volume_available_bytes: volume
                .as_ref()
                .map(|v| (u128::from(v.f_bavail) * u128::from(v.f_frsize)).to_string()),
            volume_capacity_bytes: volume
                .as_ref()
                .map(|v| (u128::from(v.f_blocks) * u128::from(v.f_frsize)).to_string()),
            volume_observed: volume.is_some(),
            categories,
        })
    }
}
