//! Native durable admission ledger. File writes remain owned by their adapters.
//! An OS lock is the lifetime of a reservation; expiry clocks never authorize reuse.
use opencut_contracts::EditorError;
use opencut_editor_application::{
    audit::ExecutionContext,
    budget::{self, BudgetNative, Configure, Policy, PolicyChange, Report},
};
use std::path::{Path, PathBuf};

mod child;
pub use child::ReservedChild;
mod attempt;
pub use attempt::{AttemptLease, CleanupLease};

#[derive(Clone, Debug)]
pub struct Budget {
    workspace: PathBuf,
}
impl Budget {
    pub fn new(workspace: impl AsRef<Path>) -> Self {
        Self {
            workspace: workspace.as_ref().to_path_buf(),
        }
    }
    /// Atomically choose and reserve a native output ceiling under current policy.
    pub fn reserve_output(&self, writer: budget::Writer) -> Result<Reservation, EditorError> {
        #[cfg(unix)]
        {
            unix::reserve_inner(self, "media_output", None, Some(writer))
        }
        #[cfg(not(unix))]
        {
            let _ = writer;
            Err(unavailable())
        }
    }
    pub fn reserve_caption(
        &self,
        bytes: u64,
        writer: budget::Writer,
    ) -> Result<Reservation, EditorError> {
        #[cfg(unix)]
        {
            unix::reserve_inner(self, "media_caption", Some(bytes), Some(writer))
        }
        #[cfg(not(unix))]
        {
            let _ = (bytes, writer);
            Err(unavailable())
        }
    }
    pub fn reserve(&self, category: &str, bytes: u64) -> Result<Reservation, EditorError> {
        #[cfg(unix)]
        {
            unix::reserve(self, category, bytes)
        }
        #[cfg(not(unix))]
        {
            let _ = (category, bytes);
            Err(unavailable())
        }
    }
}
impl BudgetNative for Budget {
    fn inspect(&self, recover: bool) -> Result<Report, EditorError> {
        #[cfg(unix)]
        {
            unix::inspect(self, recover)
        }
        #[cfg(not(unix))]
        {
            let _ = recover;
            Err(unavailable())
        }
    }
    fn configure(
        &self,
        input: Configure,
        context: &ExecutionContext,
    ) -> Result<PolicyChange, EditorError> {
        #[cfg(unix)]
        {
            unix::configure(self, input, context)
        }
        #[cfg(not(unix))]
        {
            let _ = (input, context);
            Err(unavailable())
        }
    }
}
#[cfg(not(unix))]
fn unavailable() -> EditorError {
    EditorError::new(
        "CAPABILITY_UNAVAILABLE",
        "Storage admission requires Unix directory and lifetime locks",
    )
}
pub struct Reservation {
    #[cfg(unix)]
    inner: unix::Held,
}
impl Reservation {
    /// Run one cooperating native writer with one writable payload file.
    /// The child inherits this reservation's lifetime lock and a kernel file
    /// size ceiling. This is not an aggregate sandbox for arbitrary commands.
    /// Consume the command so a retained pre_exec hook cannot spawn more writers.
    pub fn spawn_single_file(
        self,
        command: std::process::Command,
    ) -> std::io::Result<ReservedChild> {
        child::spawn(Some(self), command, None)
    }
    pub fn spawn_in_attempt(
        self,
        command: std::process::Command,
        attempt: AttemptLease,
    ) -> std::io::Result<ReservedChild> {
        child::spawn(Some(self), command, Some(attempt))
    }
    pub fn reserved_bytes(&self) -> u64 {
        #[cfg(unix)]
        {
            self.inner.child_lock().1
        }
        #[cfg(not(unix))]
        {
            0
        }
    }
    /// Increase before writing beyond the original reservation. Never shrink while live.
    pub fn ensure(&mut self, bytes: u64) -> Result<(), EditorError> {
        #[cfg(unix)]
        {
            self.inner.ensure(bytes)
        }
        #[cfg(not(unix))]
        {
            let _ = bytes;
            Err(unavailable())
        }
    }
}

#[cfg(unix)]
mod unix {
    use super::*;
    use budget::{MAX_POLICY_CHANGES, MAX_RESERVATIONS};
    use fs2::FileExt;
    use rustix::{
        fd::AsFd,
        fs::{self, AtFlags, FileType, Mode, OFlags},
    };
    use serde::{Deserialize, Serialize};
    use std::{
        collections::{BTreeMap, HashSet},
        ffi::OsStr,
        fs::File,
        io::{Read, Write},
        time::{Duration, Instant},
    };
    const DIR: OFlags = OFlags::RDONLY
        .union(OFlags::DIRECTORY)
        .union(OFlags::NOFOLLOW)
        .union(OFlags::CLOEXEC);
    const CONTROL: &str = ".storage-budget";
    const MAX_LEDGER: u64 = 16 * 1024 * 1024;
    const ROOTS: [&str; 5] = ["asset-store", "media", "uploads", "artifacts", "plugins"];
    fn failure() -> EditorError {
        EditorError::new(
            "STORAGE_BUDGET_ERROR",
            "Cannot safely read or persist storage admission state",
        )
    }
    fn busy() -> EditorError {
        EditorError::new(
            "STORAGE_BUDGET_BUSY",
            "Another admission operation holds the workspace budget lock; retry",
        )
    }
    fn limit() -> EditorError {
        EditorError::new(
            "RESOURCE_TOO_LARGE",
            "Storage admission metadata or observation budget exceeded",
        )
    }
    fn locked(file: &File, exclusive: bool) -> Result<(), EditorError> {
        let started = Instant::now();
        loop {
            let result = if exclusive {
                file.try_lock_exclusive()
            } else {
                FileExt::try_lock_shared(file)
            };
            match result {
                Ok(()) => return Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if started.elapsed() >= Duration::from_secs(5) {
                        return Err(busy());
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(_) => return Err(failure()),
            }
        }
    }
    struct Gate {
        root: File,
        directory: Option<File>,
    }
    impl Gate {
        fn open(budget: &Budget, write: bool) -> Result<Self, EditorError> {
            let root: File = fs::open(&budget.workspace, DIR, Mode::empty())
                .map_err(|_| failure())?
                .into();
            if write {
                match fs::mkdirat(&root, CONTROL, Mode::RUSR | Mode::WUSR | Mode::XUSR) {
                    Ok(()) => {
                        root.sync_all().map_err(|_| failure())?;
                    }
                    Err(rustix::io::Errno::EXIST) => (),
                    Err(_) => return Err(failure()),
                }
            }
            let directory = match fs::openat(&root, CONTROL, DIR, Mode::empty()) {
                Ok(fd) => {
                    let file: File = fd.into();
                    locked(&file, write)?;
                    Some(file)
                }
                Err(rustix::io::Errno::NOENT) if !write => None,
                Err(_) => return Err(failure()),
            };
            Ok(Self { root, directory })
        }
        fn dir(&self) -> Result<&File, EditorError> {
            self.directory.as_ref().ok_or_else(failure)
        }
        fn read(&self) -> Result<Ledger, EditorError> {
            let Some(dir) = &self.directory else {
                return Ok(Ledger::default());
            };
            let fd = match fs::openat(
                dir,
                "ledger.json",
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            ) {
                Ok(fd) => fd,
                Err(rustix::io::Errno::NOENT) => return Ok(Ledger::default()),
                Err(_) => return Err(failure()),
            };
            let stat = fs::fstat(&fd).map_err(|_| failure())?;
            if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
                || stat.st_nlink != 1
                || stat.st_size < 0
                || stat.st_size as u64 > MAX_LEDGER
            {
                return Err(failure());
            }
            let mut text = Vec::new();
            File::from(fd)
                .take(MAX_LEDGER + 1)
                .read_to_end(&mut text)
                .map_err(|_| failure())?;
            if text.len() as u64 > MAX_LEDGER {
                return Err(limit());
            }
            let value: Ledger = serde_json::from_slice(&text).map_err(|_| failure())?;
            value.validate()?;
            Ok(value)
        }
        fn save(&self, value: &Ledger) -> Result<(), EditorError> {
            value.validate()?;
            let data = serde_json::to_vec(value).map_err(|_| failure())?;
            if data.len() as u64 > MAX_LEDGER {
                return Err(limit());
            }
            let dir = self.dir()?;
            // Fixed, private staging name: interrupted saves cannot accumulate files.
            match fs::unlinkat(dir, "ledger.tmp", AtFlags::empty()) {
                Ok(()) | Err(rustix::io::Errno::NOENT) => (),
                Err(_) => return Err(failure()),
            }
            let mut file: File = fs::openat(
                dir,
                "ledger.tmp",
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
            )
            .map_err(|_| failure())?
            .into();
            file.write_all(&data).map_err(|_| failure())?;
            file.sync_all().map_err(|_| failure())?;
            fs::renameat(dir, "ledger.tmp", dir, "ledger.json").map_err(|_| failure())?;
            dir.sync_all().map_err(|_| failure())
        }
    }
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Receipt {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        media_output_limit_bytes: Option<String>,
        input: Configure,
        policy: Policy,
        request_id: String,
        actor: opencut_editor_application::audit::Actor,
    }
    impl Receipt {
        fn view(&self) -> PolicyChange {
            PolicyChange {
                policy: self.policy.clone(),
                media_output_limit_bytes: self.media_output_limit_bytes.clone(),
                request_id: self.request_id.clone(),
                actor: self.actor.clone(),
            }
        }
    }
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Entry {
        category: String,
        bytes: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        writer: Option<budget::Writer>,
    }
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Ledger {
        version: u32,
        #[serde(default = "default_media_output_limit")]
        media_output_limit_bytes: String,
        policy: Policy,
        held: BTreeMap<String, Entry>,
        changes: Vec<Receipt>,
    }
    fn default_media_output_limit() -> String {
        (1_u64 << 30).to_string()
    }
    impl Default for Ledger {
        fn default() -> Self {
            Self {
                version: 1,
                media_output_limit_bytes: default_media_output_limit(),
                policy: Policy::default(),
                held: BTreeMap::new(),
                changes: Vec::new(),
            }
        }
    }
    impl Ledger {
        fn validate(&self) -> Result<(), EditorError> {
            if self.version != 1
                || self.held.len() > MAX_RESERVATIONS
                || self.changes.len() > MAX_POLICY_CHANGES
            {
                return Err(failure());
            }
            budget::decimal(&self.policy.payload_limit_bytes).map_err(|_| failure())?;
            budget::decimal(&self.policy.minimum_available_bytes).map_err(|_| failure())?;
            if budget::decimal(&self.media_output_limit_bytes).map_err(|_| failure())? == 0 {
                return Err(failure());
            }
            for (id, entry) in &self.held {
                if let Some(writer) = &entry.writer {
                    validate_writer(writer)?;
                }
                if matches!(entry.category.as_str(), "media_output" | "media_caption")
                    != entry.writer.is_some()
                {
                    return Err(failure());
                }
                if uuid::Uuid::parse_str(id)
                    .ok()
                    .is_none_or(|u| u.to_string() != *id)
                    || entry.bytes > i64::MAX as u64
                    || !coverage().contains(&entry.category)
                {
                    return Err(failure());
                }
            }
            let mut keys = HashSet::new();
            let mut media_limit = default_media_output_limit();
            for (index, receipt) in self.changes.iter().enumerate() {
                if let Some(limit) = &receipt.input.media_output_limit_bytes {
                    media_limit = limit.clone();
                }
                if receipt
                    .media_output_limit_bytes
                    .as_deref()
                    .unwrap_or(&default_media_output_limit())
                    != media_limit
                {
                    return Err(failure());
                }
                budget::validate(&receipt.input).map_err(|_| failure())?;
                if !keys.insert(&receipt.input.idempotency_key)
                    || receipt.policy.revision != index as u64 + 1
                    || receipt.input.expected_revision != index as u64
                    || receipt.policy.payload_limit_bytes != receipt.input.payload_limit_bytes
                    || receipt.policy.minimum_available_bytes
                        != receipt.input.minimum_available_bytes
                {
                    return Err(failure());
                }
            }
            if media_limit != self.media_output_limit_bytes {
                return Err(failure());
            }
            if self
                .changes
                .last()
                .map(|r| &r.policy)
                .unwrap_or(&Policy::default())
                != &self.policy
            {
                return Err(failure());
            }
            Ok(())
        }
        fn total(&self) -> Result<u64, EditorError> {
            self.held
                .values()
                .try_fold(0u64, |sum, r| sum.checked_add(r.bytes).ok_or_else(failure))
        }
    }
    fn coverage() -> Vec<String> {
        [
            "cas_capture",
            "upload_chunk",
            "upload_assembly",
            "private_asset_copy",
            "media_upload",
            "media_inspection",
            "media_metadata",
            "plugin_package",
            "plugin_grants",
            "artifact_download",
            "media_output",
            "media_caption",
        ]
        .into_iter()
        .map(String::from)
        .collect()
    }
    fn lifetime(gate: &Gate, id: &str, create: bool) -> Result<Option<File>, EditorError> {
        let flags = OFlags::RDWR
            | OFlags::NOFOLLOW
            | OFlags::CLOEXEC
            | if create {
                OFlags::CREATE | OFlags::EXCL
            } else {
                OFlags::empty()
            };
        let fd = match fs::openat(
            gate.dir()?,
            format!("lock-{id}"),
            flags,
            Mode::RUSR | Mode::WUSR,
        ) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) if !create => return Err(failure()),
            Err(_) => return Err(failure()),
        };
        let stat = fs::fstat(&fd).map_err(|_| failure())?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile || stat.st_nlink != 1 {
            return Err(failure());
        }
        let file: File = fd.into();
        match file.try_lock_exclusive() {
            Ok(()) => Ok(Some(file)),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(_) => Err(failure()),
        }
    }
    fn prune_orphan_locks(gate: &Gate, ledger: &Ledger) -> Result<(), EditorError> {
        let mut entries = fs::Dir::read_from(gate.dir()?).map_err(|_| failure())?;
        let mut count = 0;
        for entry in &mut entries {
            count += 1;
            if count > 100_000 {
                return Err(limit());
            }
            let entry = entry.map_err(|_| failure())?;
            let Ok(name) = entry.file_name().to_str() else {
                continue;
            };
            let Some(id) = name.strip_prefix("lock-") else {
                continue;
            };
            if uuid::Uuid::parse_str(id)
                .ok()
                .is_none_or(|u| u.to_string() != id)
            {
                return Err(failure());
            }
            if !ledger.held.contains_key(id) && lifetime(gate, id, false)?.is_some() {
                fs::unlinkat(gate.dir()?, name, AtFlags::empty()).map_err(|_| failure())?;
            }
        }
        gate.dir()?.sync_all().map_err(|_| failure())
    }
    fn recover(gate: &Gate, ledger: &mut Ledger) -> Result<Vec<String>, EditorError> {
        prune_orphan_locks(gate, ledger)?;
        let mut stale = Vec::new();
        for id in ledger.held.keys() {
            if lifetime(gate, id, false)?.is_some() {
                stale.push(id.clone());
            }
        }
        for id in &stale {
            ledger.held.remove(id);
        }
        Ok(stale)
    }
    fn remove_locks(gate: &Gate, ids: &[String]) {
        if let Ok(dir) = gate.dir() {
            for id in ids {
                let _ = fs::unlinkat(dir, format!("lock-{id}"), AtFlags::empty());
            }
            let _ = dir.sync_all();
        }
    }
    pub(super) fn inspect(budget: &Budget, should_recover: bool) -> Result<Report, EditorError> {
        let gate = Gate::open(budget, should_recover)?;
        let mut ledger = gate.read()?;
        let recovered = if should_recover {
            recover(&gate, &mut ledger)?
        } else {
            Vec::new()
        };
        let (used, available) = observe(&gate.root)?;
        if !recovered.is_empty() {
            gate.save(&ledger)?;
            remove_locks(&gate, &recovered);
        }
        let reserved = ledger.total()?;
        let logical = budget::decimal(&ledger.policy.payload_limit_bytes)?
            .saturating_sub(used)
            .saturating_sub(reserved);
        let physical = available
            .saturating_sub(reserved)
            .saturating_sub(budget::decimal(&ledger.policy.minimum_available_bytes)?);
        Ok(Report {
            last_policy_change: ledger.changes.last().map(Receipt::view),
            policy: ledger.policy,
            media_output_limit_bytes: Some(ledger.media_output_limit_bytes),
            observed_payload_bytes: used.to_string(),
            reserved_bytes: reserved.to_string(),
            admission_headroom_bytes: logical.min(physical).to_string(),
            volume_available_bytes: available.to_string(),
            reservations: ledger
                .held
                .into_iter()
                .map(|(id, e)| budget::Reservation {
                    reservation_id: id,
                    category: e.category,
                    reserved_bytes: e.bytes.to_string(),
                    writer: e.writer.clone(),
                })
                .collect(),
            recovered_reservations: recovered.len(),
            coverage: coverage(),
            enforces_all_workspace_writes: false,
        })
    }
    pub(super) fn configure(
        budget: &Budget,
        input: Configure,
        context: &ExecutionContext,
    ) -> Result<PolicyChange, EditorError> {
        budget::validate(&input)?;
        let gate = Gate::open(budget, true)?;
        let mut ledger = gate.read()?;
        if let Some(old) = ledger
            .changes
            .iter()
            .find(|r| r.input.idempotency_key == input.idempotency_key)
        {
            return if old.input == input {
                Ok(old.view())
            } else {
                Err(EditorError::new(
                    "IDEMPOTENCY_KEY_REUSED",
                    "Budget policy key belongs to another request",
                ))
            };
        }
        if input.expected_revision != ledger.policy.revision {
            return Err(EditorError::new(
                "STORAGE_POLICY_CONFLICT",
                "Storage policy changed; refresh and explicitly reconfigure",
            ));
        }
        if ledger.changes.len() == MAX_POLICY_CHANGES {
            return Err(limit());
        }
        let policy = Policy {
            revision: ledger.policy.revision + 1,
            payload_limit_bytes: input.payload_limit_bytes.clone(),
            minimum_available_bytes: input.minimum_available_bytes.clone(),
        };
        ledger.media_output_limit_bytes = input
            .media_output_limit_bytes
            .clone()
            .unwrap_or_else(|| ledger.media_output_limit_bytes.clone());
        ledger.changes.push(Receipt {
            media_output_limit_bytes: Some(ledger.media_output_limit_bytes.clone()),
            input,
            policy: policy.clone(),
            request_id: context.request_id().into(),
            actor: context.actor().clone(),
        });
        ledger.policy = policy.clone();
        gate.save(&ledger)?;
        Ok(ledger.changes.last().expect("just appended policy").view())
    }
    pub(super) fn reserve(
        budget: &Budget,
        category: &str,
        bytes: u64,
    ) -> Result<Reservation, EditorError> {
        reserve_inner(budget, category, Some(bytes), None)
    }
    fn validate_writer(writer: &budget::Writer) -> Result<(), EditorError> {
        if writer.attempt == 0
            || writer.job_id.len() != 36
            || !writer.job_id.starts_with("job_")
            || !writer.job_id[4..]
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(failure());
        }
        Ok(())
    }
    pub(super) fn reserve_inner(
        budget: &Budget,
        category: &str,
        requested: Option<u64>,
        writer: Option<budget::Writer>,
    ) -> Result<Reservation, EditorError> {
        if !coverage().contains(&category.to_string())
            || requested.is_some_and(|n| n > i64::MAX as u64)
        {
            return Err(failure());
        }
        if let Some(writer) = &writer {
            validate_writer(writer)?;
        }
        if matches!(category, "media_output" | "media_caption") != writer.is_some() {
            return Err(failure());
        }

        let gate = Gate::open(budget, true)?;
        let mut ledger = gate.read()?;
        let stale = recover(&gate, &mut ledger)?;
        let (used, available) = observe(&gate.root)?;
        if !stale.is_empty() {
            gate.save(&ledger)?;
            remove_locks(&gate, &stale);
        }
        if ledger.held.len() == MAX_RESERVATIONS {
            return Err(limit());
        }
        let held = ledger.total()?;
        let bytes = if let Some(bytes) = requested {
            bytes
        } else {
            // Reject zero headroom with the appropriate public capacity reason.
            budget::admit(&ledger.policy, used, held, 1, available)?;
            budget::decimal(&ledger.media_output_limit_bytes)?
                .min(
                    budget::decimal(&ledger.policy.payload_limit_bytes)?
                        .saturating_sub(used)
                        .saturating_sub(held),
                )
                .min(
                    available
                        .saturating_sub(held)
                        .saturating_sub(budget::decimal(&ledger.policy.minimum_available_bytes)?),
                )
        };
        budget::admit(&ledger.policy, used, held, bytes, available)?;
        let id = uuid::Uuid::new_v4().to_string();
        let lock = lifetime(&gate, &id, true)?.ok_or_else(failure)?;
        ledger.held.insert(
            id.clone(),
            Entry {
                category: category.into(),
                bytes,
                writer,
            },
        );
        gate.save(&ledger)?;
        Ok(Reservation {
            inner: Held {
                budget: budget.clone(),
                id,
                bytes,
                lock: Some(lock),
            },
        })
    }
    pub(super) struct Held {
        budget: Budget,
        id: String,
        bytes: u64,
        lock: Option<File>,
    }
    impl Held {
        pub(super) fn child_lock(&self) -> (&File, u64) {
            (self.lock.as_ref().expect("live reservation"), self.bytes)
        }
        pub(super) fn ensure(&mut self, bytes: u64) -> Result<(), EditorError> {
            if bytes <= self.bytes {
                return Ok(());
            }
            if bytes > i64::MAX as u64 {
                return Err(limit());
            }
            let gate = Gate::open(&self.budget, true)?;
            let mut ledger = gate.read()?;
            let (used, available) = observe(&gate.root)?;
            budget::admit(
                &ledger.policy,
                used,
                ledger.total()?,
                bytes - self.bytes,
                available,
            )?;
            let entry = ledger.held.get_mut(&self.id).ok_or_else(failure)?;
            if entry.bytes != self.bytes {
                return Err(failure());
            }
            entry.bytes = bytes;
            gate.save(&ledger)?;
            self.bytes = bytes;
            Ok(())
        }
    }
    impl Drop for Held {
        fn drop(&mut self) {
            // If durable release fails, keep the conservative record. Recovery only
            // releases it after this process no longer holds the lifetime lock.
            let result = (|| -> Result<(), EditorError> {
                let gate = Gate::open(&self.budget, true)?;
                // Do not explicitly unlock: fork/dup children reference the
                // same flock. Close our reference, then independently prove the
                // lock is free before removing the durable record.
                drop(self.lock.take());
                let Some(_unowned) = lifetime(&gate, &self.id, false)? else {
                    return Ok(());
                };
                let mut ledger = gate.read()?;
                ledger.held.remove(&self.id);
                gate.save(&ledger)?;
                remove_locks(&gate, std::slice::from_ref(&self.id));
                Ok(())
            })();
            let _ = result;
        }
    }
    #[allow(clippy::unnecessary_cast)]
    fn observe(root: &File) -> Result<(u64, u64), EditorError> {
        struct Scan {
            device: u64,
            seen: HashSet<(u64, u64)>,
            count: usize,
            bytes: u64,
            start: Instant,
        }
        impl Scan {
            fn visit(
                &mut self,
                parent: &impl AsFd,
                name: &OsStr,
                depth: usize,
            ) -> Result<(), EditorError> {
                self.count += 1;
                if self.count > 100_000
                    || depth > 64
                    || self.start.elapsed() > Duration::from_secs(5)
                {
                    return Err(limit());
                }
                let stat = match fs::statat(parent, name, AtFlags::SYMLINK_NOFOLLOW) {
                    Ok(s) => s,
                    Err(rustix::io::Errno::NOENT) => return Ok(()),
                    Err(_) => return Err(failure()),
                };
                if stat.st_dev as u64 != self.device {
                    return Err(failure());
                }
                match FileType::from_raw_mode(stat.st_mode) {
                    FileType::RegularFile => {
                        if stat.st_size < 0 {
                            return Err(failure());
                        }
                        if self.seen.insert((stat.st_dev as u64, stat.st_ino as u64)) {
                            self.bytes = self
                                .bytes
                                .checked_add(stat.st_size as u64)
                                .ok_or_else(limit)?;
                        }
                    }
                    FileType::Directory => {
                        let fd =
                            fs::openat(parent, name, DIR, Mode::empty()).map_err(|_| failure())?;
                        let opened = fs::fstat(&fd).map_err(|_| failure())?;
                        if opened.st_dev != stat.st_dev || opened.st_ino != stat.st_ino {
                            return Err(failure());
                        }
                        let mut dir = fs::Dir::read_from(&fd).map_err(|_| failure())?;
                        for entry in &mut dir {
                            let entry = entry.map_err(|_| failure())?;
                            let name = entry.file_name();
                            if name.to_bytes() == b"." || name.to_bytes() == b".." {
                                continue;
                            }
                            use std::os::unix::ffi::OsStrExt;
                            self.visit(&fd, OsStr::from_bytes(name.to_bytes()), depth + 1)?;
                        }
                    }
                    _ => return Err(failure()),
                }
                Ok(())
            }
        }
        let stat = fs::fstat(root).map_err(|_| failure())?;
        let mut scan = Scan {
            device: stat.st_dev as u64,
            seen: HashSet::new(),
            count: 0,
            bytes: 0,
            start: Instant::now(),
        };
        for name in ROOTS {
            scan.visit(root, OsStr::new(name), 0)?;
        }
        let volume = fs::fstatvfs(root).map_err(|_| failure())?;
        let available = (volume.f_bavail as u64)
            .checked_mul(volume.f_frsize as u64)
            .ok_or_else(failure)?;
        Ok((scan.bytes, available))
    }
}
