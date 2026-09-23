//! Native durable admission ledger. File writes remain owned by their adapters.
//! An OS lock is the lifetime of a reservation; expiry clocks never authorize reuse.
use opencut_contracts::EditorError;
use opencut_editor_application::{
    audit::ExecutionContext,
    budget::{
        self, BudgetNative, Configure, Policy, PolicyChange, RepairAction, RepairChange,
        RepairRequest, Report,
    },
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
    /// Reserve database growth in a separate control account. `database_name`
    /// is a single file name in this workspace, never a caller-selected path.
    pub fn reserve_database_control(
        &self,
        database_name: &str,
        bytes: u64,
    ) -> Result<Reservation, EditorError> {
        #[cfg(unix)]
        {
            unix::reserve_control(self, database_name, bytes)
        }
        #[cfg(not(unix))]
        {
            let _ = (database_name, bytes);
            Err(unavailable())
        }
    }
    /// Only the dedicated local repair VFS may spend released repair capacity.
    pub fn reserve_database_control_repair(
        &self,
        database_name: &str,
        bytes: u64,
    ) -> Result<Reservation, EditorError> {
        #[cfg(unix)]
        {
            unix::reserve_control_repair(self, database_name, bytes)
        }
        #[cfg(not(unix))]
        {
            let _ = (database_name, bytes);
            Err(unavailable())
        }
    }
    /// Reject a different database in an already-bound managed workspace
    /// before SQLite creates files or maps a binding error to SQLITE_FULL.
    pub fn check_database_control_name(&self, database_name: &str) -> Result<(), EditorError> {
        #[cfg(unix)]
        {
            unix::check_database_control_name(self, database_name)
        }
        #[cfg(not(unix))]
        {
            let _ = database_name;
            Err(unavailable())
        }
    }
    /// Keep the shared admission gate locked through an actual SQLite VFS
    /// mutation, so entering repair cannot race an already-authorized write.
    pub fn with_database_write<T>(
        &self,
        database_name: &str,
        repair: bool,
        write: impl FnOnce() -> T,
    ) -> Result<T, EditorError> {
        #[cfg(unix)]
        {
            unix::with_database_write(self, database_name, repair, write)
        }
        #[cfg(not(unix))]
        {
            let _ = (database_name, repair, write);
            Err(unavailable())
        }
    }
    /// A read-only OS lease held for each managed SQLite file handle. It does
    /// not add a ledger entry or consume disk when an existing volume is full.
    pub fn database_connection_lease(
        &self,
        database_name: &str,
        repair: bool,
    ) -> Result<ConnectionLease, EditorError> {
        #[cfg(unix)]
        {
            unix::database_connection_lease(self, database_name, repair)
        }
        #[cfg(not(unix))]
        {
            let _ = (database_name, repair);
            Err(unavailable())
        }
    }
    pub fn volume_available_bytes(&self) -> Result<u64, EditorError> {
        #[cfg(unix)]
        {
            unix::volume_available_bytes(self)
        }
        #[cfg(not(unix))]
        {
            Err(unavailable())
        }
    }
}
pub struct ConnectionLease {
    #[cfg(unix)]
    _inner: unix::ConnectionLease,
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
    fn repair_transition(
        &self,
        action: RepairAction,
        input: RepairRequest,
        context: &ExecutionContext,
    ) -> Result<RepairChange, EditorError> {
        #[cfg(unix)]
        {
            unix::repair_transition(self, action, input, context)
        }
        #[cfg(not(unix))]
        {
            let _ = (action, input, context);
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
    const DEFAULT_CONTROL_LIMIT: u64 = 4 * 1024 * 1024 * 1024;
    const REPAIR_FILE: &str = "repair.reserve";
    const CONNECTION_LOCK: &str = "connections.lock";
    const REPAIR_BYTES: u64 = 8 * 1024 * 1024;
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
                Err(rustix::io::Errno::NOENT) => {
                    let mut entries = fs::Dir::read_from(dir).map_err(|_| failure())?;
                    for entry in &mut entries {
                        let entry = entry.map_err(|_| failure())?;
                        let name = entry.file_name().to_bytes();
                        if name != b"." && name != b".." {
                            return Err(failure());
                        }
                    }
                    return Ok(Ledger::default());
                }
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
    fn repair_stats(gate: &Gate) -> Result<(u64, u64), EditorError> {
        let fd = match fs::openat(
            gate.dir()?,
            REPAIR_FILE,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) => return Ok((0, 0)),
            Err(_) => return Err(failure()),
        };
        let stat = fs::fstat(&fd).map_err(|_| failure())?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
            || stat.st_nlink != 1
            || stat.st_size < 0
            || stat.st_blocks < 0
        {
            return Err(failure());
        }
        Ok((
            stat.st_size as u64,
            (stat.st_blocks as u64)
                .checked_mul(512)
                .ok_or_else(failure)?,
        ))
    }
    fn repair_file_ready(gate: &Gate) -> Result<bool, EditorError> {
        let (size, blocks) = repair_stats(gate)?;
        Ok(size == REPAIR_BYTES && blocks >= REPAIR_BYTES)
    }
    fn connection_lock_file(gate: &Gate, create: bool) -> Result<File, EditorError> {
        let flags = OFlags::RDWR
            | OFlags::NOFOLLOW
            | OFlags::CLOEXEC
            | if create {
                OFlags::CREATE
            } else {
                OFlags::empty()
            };
        let file: File = fs::openat(gate.dir()?, CONNECTION_LOCK, flags, Mode::RUSR | Mode::WUSR)
            .map_err(|_| failure())?
            .into();
        let stat = fs::fstat(&file).map_err(|_| failure())?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
            || stat.st_nlink != 1
            || stat.st_size != 0
        {
            return Err(failure());
        }
        if create {
            file.sync_all().map_err(|_| failure())?;
            gate.dir()?.sync_all().map_err(|_| failure())?;
        }
        Ok(file)
    }
    fn capacity() -> EditorError {
        EditorError::new(
            "STORAGE_CAPACITY_LOW",
            "Volume cannot allocate and synchronize the physical repair reserve",
        )
    }
    fn fill_repair_file(gate: &Gate) -> Result<(), EditorError> {
        if repair_file_ready(gate)? {
            return Ok(());
        }
        let file: File = fs::openat(
            gate.dir()?,
            REPAIR_FILE,
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(|_| failure())?
        .into();
        let stat = fs::fstat(&file).map_err(|_| failure())?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile || stat.st_nlink != 1 {
            return Err(failure());
        }
        file.set_len(0).map_err(|_| capacity())?;
        let mut writer = file;
        let mut random = File::open("/dev/urandom").map_err(|_| failure())?;
        let mut chunk = [0_u8; 64 * 1024];
        for _ in 0..(REPAIR_BYTES / chunk.len() as u64) {
            random.read_exact(&mut chunk).map_err(|_| failure())?;
            writer.write_all(&chunk).map_err(|_| capacity())?;
        }
        writer.sync_all().map_err(|_| capacity())?;
        gate.dir()?.sync_all().map_err(|_| capacity())?;
        if !repair_file_ready(gate)? {
            return Err(failure());
        }
        Ok(())
    }
    fn provision_repair(gate: &Gate, ledger: &mut Ledger) -> Result<(), EditorError> {
        if ledger.version == 3 {
            return Ok(());
        }
        let (_, available) = observe(&gate.root)?;
        let already_ready = repair_file_ready(gate)?;
        let held = ledger
            .payload_total()?
            .checked_add(ledger.control_total()?)
            .ok_or_else(capacity)?;
        let required = budget::decimal(&ledger.policy.minimum_available_bytes)?
            .checked_add(held)
            .and_then(|n| n.checked_add(if already_ready { 0 } else { REPAIR_BYTES }))
            .ok_or_else(capacity)?;
        if available < required {
            return Err(capacity());
        }
        fill_repair_file(gate)?;
        let _connection_lock = connection_lock_file(gate, true)?;
        ledger.version = 3;
        if ledger.control_limit_bytes.is_none() {
            ledger.control_limit_bytes = Some(DEFAULT_CONTROL_LIMIT.to_string());
        }
        ledger.repair = Some(RepairRecord {
            target_bytes: REPAIR_BYTES.to_string(),
            revision: 0,
            state: budget::RepairState::Ready,
            baseline_control_bytes: None,
        });
        gate.save(ledger)
    }
    fn require_normal_repair(
        gate: &Gate,
        ledger: &Ledger,
        control: bool,
    ) -> Result<(), EditorError> {
        let Some(repair) = &ledger.repair else {
            return Err(failure());
        };
        if repair.state != budget::RepairState::Ready {
            return Err(EditorError::new(
                if control {
                    "DATABASE_CAPACITY_EXHAUSTED"
                } else {
                    "STORAGE_CAPACITY_LOW"
                },
                "Normal writes are paused until the repair reserve is replenished",
            ));
        }
        if !repair_file_ready(gate)? {
            return Err(failure());
        }
        let _lock = connection_lock_file(gate, false)?;
        Ok(())
    }
    fn require_repair_active(gate: &Gate, ledger: &Ledger) -> Result<(), EditorError> {
        if !ledger
            .repair
            .as_ref()
            .is_some_and(|repair| repair.state == budget::RepairState::Active)
        {
            return Err(EditorError::new(
                "DATABASE_CAPACITY_EXHAUSTED",
                "Database repair writes require an active owner repair session",
            ));
        }
        if repair_stats(gate)?.0 != 0 {
            return Err(failure());
        }
        let _lock = connection_lock_file(gate, false)?;
        Ok(())
    }
    pub(super) struct ConnectionLease {
        _file: File,
    }
    pub(super) fn database_connection_lease(
        budget: &Budget,
        database_name: &str,
        repair: bool,
    ) -> Result<super::ConnectionLease, EditorError> {
        validate_database_name(database_name)?;
        let gate = Gate::open(budget, true)?;
        let mut ledger = gate.read()?;
        if ledger
            .control_database_name
            .as_deref()
            .is_some_and(|bound| bound != database_name)
        {
            return Err(failure());
        }
        if repair {
            require_repair_active(&gate, &ledger)?;
            if ledger.control_database_name.as_deref() != Some(database_name) {
                return Err(failure());
            }
        } else {
            provision_repair(&gate, &mut ledger)?;
            require_normal_repair(&gate, &ledger, true)?;
            if ledger.control_database_name.is_none() {
                ledger.control_database_name = Some(database_name.to_owned());
                gate.save(&ledger)?;
            }
        }
        let file = connection_lock_file(&gate, false)?;
        match FileExt::try_lock_shared(&file) {
            Ok(()) => Ok(super::ConnectionLease {
                _inner: ConnectionLease { _file: file },
            }),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Err(busy()),
            Err(_) => Err(failure()),
        }
    }
    fn admit_repair_control(
        ledger: &Ledger,
        observed_control: u64,
        bytes: u64,
        available: u64,
    ) -> Result<(), EditorError> {
        let repair = ledger.repair.as_ref().ok_or_else(failure)?;
        let baseline = budget::decimal(
            repair
                .baseline_control_bytes
                .as_deref()
                .ok_or_else(failure)?,
        )?;
        budget::admit_repair(
            budget::decimal(&repair.target_bytes)?,
            observed_control.saturating_sub(baseline),
            ledger.control_total()?,
            bytes,
            available,
        )
    }
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Receipt {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        media_output_limit_bytes: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        control_limit_bytes: Option<String>,
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
                control_limit_bytes: self.control_limit_bytes.clone(),
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
    struct RepairRecord {
        target_bytes: String,
        revision: u64,
        state: budget::RepairState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        baseline_control_bytes: Option<String>,
    }
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Ledger {
        version: u32,
        #[serde(default = "default_media_output_limit")]
        media_output_limit_bytes: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        control_limit_bytes: Option<String>,
        policy: Policy,
        held: BTreeMap<String, Entry>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        control_database_name: Option<String>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        control_held: BTreeMap<String, Entry>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        repair: Option<RepairRecord>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        repair_changes: Vec<RepairChange>,
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
                control_limit_bytes: None,
                policy: Policy::default(),
                held: BTreeMap::new(),
                control_database_name: None,
                control_held: BTreeMap::new(),
                repair: None,
                repair_changes: Vec::new(),
                changes: Vec::new(),
            }
        }
    }
    impl Ledger {
        fn validate(&self) -> Result<(), EditorError> {
            if !matches!(self.version, 1..=3)
                || self.held.len() + self.control_held.len() > MAX_RESERVATIONS
                || self.changes.len() > MAX_POLICY_CHANGES
                || self.repair_changes.len() > MAX_POLICY_CHANGES
            {
                return Err(failure());
            }
            if self.version == 1
                && (self.control_limit_bytes.is_some()
                    || self.control_database_name.is_some()
                    || !self.control_held.is_empty())
            {
                return Err(failure());
            }
            if self.version != 3 && (self.repair.is_some() || !self.repair_changes.is_empty()) {
                return Err(failure());
            }
            if self.version >= 2
                && self
                    .control_limit_bytes
                    .as_deref()
                    .and_then(|s| budget::decimal(s).ok())
                    .is_none_or(|n| n == 0)
            {
                return Err(failure());
            }
            if self.version == 3 {
                let repair = self.repair.as_ref().ok_or_else(failure)?;
                if repair.target_bytes != REPAIR_BYTES.to_string()
                    || repair.revision as usize != self.repair_changes.len()
                    || matches!(repair.state, budget::RepairState::Degraded)
                    || (matches!(repair.state, budget::RepairState::Ready)
                        != repair.baseline_control_bytes.is_none())
                {
                    return Err(failure());
                }
                if let Some(baseline) = &repair.baseline_control_bytes {
                    budget::decimal(baseline).map_err(|_| failure())?;
                }
                let mut state = budget::RepairState::Ready;
                let mut keys = HashSet::new();
                for (index, change) in self.repair_changes.iter().enumerate() {
                    budget::validate_repair(&change.input).map_err(|_| failure())?;
                    let next = match (state, change.action) {
                        (budget::RepairState::Ready, RepairAction::Enter) => {
                            budget::RepairState::Active
                        }
                        (budget::RepairState::Active, RepairAction::Exit) => {
                            budget::RepairState::Ready
                        }
                        _ => return Err(failure()),
                    };
                    if !keys.insert(&change.input.idempotency_key)
                        || change.input.expected_revision != index as u64
                        || change.revision != index as u64 + 1
                        || change.state != next
                    {
                        return Err(failure());
                    }
                    state = next;
                }
                if state != repair.state {
                    return Err(failure());
                }
            }
            if let Some(name) = &self.control_database_name {
                validate_database_name(name)?;
            } else if !self.control_held.is_empty() {
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
            for (id, entry) in &self.control_held {
                if self.held.contains_key(id)
                    || uuid::Uuid::parse_str(id)
                        .ok()
                        .is_none_or(|u| u.to_string() != *id)
                    || !matches!(entry.category.as_str(), "sqlite_control" | "sqlite_repair")
                    || entry.writer.is_some()
                    || entry.bytes > i64::MAX as u64
                {
                    return Err(failure());
                }
                if entry.category == "sqlite_repair"
                    && !self
                        .repair
                        .as_ref()
                        .is_some_and(|repair| matches!(repair.state, budget::RepairState::Active))
                {
                    return Err(failure());
                }
            }
            let mut keys = HashSet::new();
            let mut media_limit = default_media_output_limit();
            let mut control_limit: Option<String> = None;
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
                if let Some(limit) = &receipt.input.control_limit_bytes {
                    control_limit = Some(limit.clone());
                } else if control_limit.is_none()
                    && receipt.control_limit_bytes.as_deref()
                        == Some(&DEFAULT_CONTROL_LIMIT.to_string())
                {
                    control_limit = Some(DEFAULT_CONTROL_LIMIT.to_string());
                }
                if receipt.control_limit_bytes != control_limit {
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
            if let Some(limit) = control_limit {
                if self.control_limit_bytes.as_deref() != Some(limit.as_str()) {
                    return Err(failure());
                }
            } else if self.control_limit_bytes.is_some()
                && self.control_limit_bytes.as_deref() != Some(&DEFAULT_CONTROL_LIMIT.to_string())
            {
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
        fn payload_total(&self) -> Result<u64, EditorError> {
            self.held
                .values()
                .try_fold(0u64, |sum, r| sum.checked_add(r.bytes).ok_or_else(failure))
        }
        fn control_total(&self) -> Result<u64, EditorError> {
            self.control_held
                .values()
                .try_fold(0u64, |sum, r| sum.checked_add(r.bytes).ok_or_else(failure))
        }
        fn control_limit(&self) -> Result<u64, EditorError> {
            self.control_limit_bytes
                .as_deref()
                .map(budget::decimal)
                .transpose()
                .map(|n| n.unwrap_or(DEFAULT_CONTROL_LIMIT))
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
    fn validate_database_name(name: &str) -> Result<(), EditorError> {
        if name.is_empty()
            || name.len() > 255
            || name == "."
            || name == ".."
            || name.starts_with('.')
            || name.contains('/')
            || name.contains('\\')
            || name.chars().any(char::is_control)
            || ROOTS.contains(&name)
            || name == CONTROL
            || name == "sqlite-temp"
            || ["-wal", "-shm", "-journal"]
                .iter()
                .any(|suffix| name.ends_with(suffix))
        {
            return Err(failure());
        }
        Ok(())
    }
    pub(super) fn check_database_control_name(
        budget: &Budget,
        database_name: &str,
    ) -> Result<(), EditorError> {
        validate_database_name(database_name)?;
        let gate = Gate::open(budget, false)?;
        if gate
            .read()?
            .control_database_name
            .as_deref()
            .is_some_and(|bound| bound != database_name)
        {
            return Err(EditorError::new(
                "STORAGE_BUDGET_ERROR",
                "Managed workspace is already bound to another SQLite database",
            ));
        }
        Ok(())
    }
    pub(super) fn with_database_write<T>(
        budget: &Budget,
        database_name: &str,
        repair: bool,
        write: impl FnOnce() -> T,
    ) -> Result<T, EditorError> {
        validate_database_name(database_name)?;
        let mut gate = Gate::open(budget, false)?;
        let mut ledger = gate.read()?;
        if !repair && (ledger.version < 3 || ledger.control_database_name.is_none()) {
            drop(gate);
            let exclusive = Gate::open(budget, true)?;
            let mut current = exclusive.read()?;
            if current
                .control_database_name
                .as_deref()
                .is_some_and(|bound| bound != database_name)
            {
                return Err(failure());
            }
            provision_repair(&exclusive, &mut current)?;
            if current.control_database_name.is_none() {
                current.control_database_name = Some(database_name.to_owned());
                exclusive.save(&current)?;
            }
            drop(exclusive);
            gate = Gate::open(budget, false)?;
            ledger = gate.read()?;
        }
        if ledger.control_database_name.as_deref() != Some(database_name) {
            return Err(failure());
        }
        if repair {
            require_repair_active(&gate, &ledger)?;
        } else {
            require_normal_repair(&gate, &ledger, true)?;
        }
        // Keep `gate` alive while the default VFS performs its actual write.
        let result = write();
        drop(gate);
        Ok(result)
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
            if !ledger.held.contains_key(id)
                && !ledger.control_held.contains_key(id)
                && lifetime(gate, id, false)?.is_some()
            {
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
        for id in ledger.control_held.keys() {
            if lifetime(gate, id, false)?.is_some() {
                stale.push(id.clone());
            }
        }
        for id in &stale {
            ledger.held.remove(id);
            ledger.control_held.remove(id);
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
        let reserved = ledger.payload_total()?;
        let control_reserved = ledger.control_total()?;
        let logical = budget::decimal(&ledger.policy.payload_limit_bytes)?
            .saturating_sub(used)
            .saturating_sub(reserved);
        let physical = available
            .saturating_sub(reserved)
            .saturating_sub(control_reserved)
            .saturating_sub(budget::decimal(&ledger.policy.minimum_available_bytes)?);
        let control = if ledger.version >= 2 {
            let observed = observe_control(&gate.root, &ledger)?;
            let control_limit = ledger.control_limit()?;
            let repair = if let Some(record) = &ledger.repair {
                let (size, blocks) = repair_stats(&gate)?;
                let lock_ready = connection_lock_file(&gate, false).is_ok();
                Some(budget::RepairReport {
                    revision: record.revision,
                    state: if !lock_ready
                        || (record.state == budget::RepairState::Ready
                            && (size != REPAIR_BYTES || blocks < REPAIR_BYTES))
                    {
                        budget::RepairState::Degraded
                    } else {
                        record.state
                    },
                    target_bytes: record.target_bytes.clone(),
                    allocated_file_bytes: size.to_string(),
                    allocated_blocks_bytes: blocks.to_string(),
                    last_change: ledger.repair_changes.last().cloned(),
                })
            } else {
                None
            };
            let normal_ready = repair
                .as_ref()
                .is_some_and(|r| r.state == budget::RepairState::Ready);
            Some(budget::ControlReport {
                database_name: ledger.control_database_name.clone(),
                control_limit_bytes: control_limit.to_string(),
                observed_control_bytes: observed.to_string(),
                reserved_control_bytes: control_reserved.to_string(),
                admission_headroom_bytes: ledger.control_database_name.as_ref().map(|_| {
                    if normal_ready || repair.is_none() {
                        control_limit
                            .saturating_sub(observed)
                            .saturating_sub(control_reserved)
                            .min(physical)
                            .to_string()
                    } else {
                        "0".into()
                    }
                }),
                reservations: ledger
                    .control_held
                    .iter()
                    .map(|(id, entry)| budget::Reservation {
                        reservation_id: id.clone(),
                        category: entry.category.clone(),
                        reserved_bytes: entry.bytes.to_string(),
                        writer: None,
                    })
                    .collect(),
                write_interception_enabled: false,
                repair,
            })
        } else {
            None
        };
        Ok(Report {
            last_policy_change: ledger.changes.last().map(Receipt::view),
            policy: ledger.policy,
            media_output_limit_bytes: Some(ledger.media_output_limit_bytes),
            observed_payload_bytes: used.to_string(),
            reserved_bytes: reserved.to_string(),
            admission_headroom_bytes: if control
                .as_ref()
                .and_then(|value| value.repair.as_ref())
                .is_some_and(|repair| repair.state != budget::RepairState::Ready)
            {
                "0".into()
            } else {
                logical.min(physical).to_string()
            },
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
            control,
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
        if let Some(limit) = &input.control_limit_bytes {
            ledger.control_limit_bytes = Some(limit.clone());
            ledger.version = ledger.version.max(2);
        }
        ledger.changes.push(Receipt {
            media_output_limit_bytes: Some(ledger.media_output_limit_bytes.clone()),
            control_limit_bytes: ledger.control_limit_bytes.clone(),
            input,
            policy: policy.clone(),
            request_id: context.request_id().into(),
            actor: context.actor().clone(),
        });
        ledger.policy = policy.clone();
        gate.save(&ledger)?;
        Ok(ledger.changes.last().expect("just appended policy").view())
    }
    pub(super) fn repair_transition(
        budget: &Budget,
        action: RepairAction,
        input: RepairRequest,
        context: &ExecutionContext,
    ) -> Result<RepairChange, EditorError> {
        budget::validate_repair(&input)?;
        let gate = Gate::open(budget, true)?;
        let mut ledger = gate.read()?;
        if ledger.version < 3 {
            provision_repair(&gate, &mut ledger)?;
        }
        if let Some(old) = ledger
            .repair_changes
            .iter()
            .find(|change| change.input.idempotency_key == input.idempotency_key)
        {
            return if old.input == input && old.action == action {
                Ok(old.clone())
            } else {
                Err(EditorError::new(
                    "IDEMPOTENCY_KEY_REUSED",
                    "Repair key belongs to another transition",
                ))
            };
        }
        let repair = ledger.repair.as_ref().ok_or_else(failure)?;
        if input.expected_revision != repair.revision {
            return Err(EditorError::new(
                "STORAGE_POLICY_CONFLICT",
                "Repair state changed; inspect and retry with its current revision",
            ));
        }
        if ledger.repair_changes.len() == MAX_POLICY_CHANGES {
            return Err(limit());
        }
        let expected = match action {
            RepairAction::Enter => budget::RepairState::Ready,
            RepairAction::Exit => budget::RepairState::Active,
        };
        if repair.state != expected {
            return Err(EditorError::new(
                "STORAGE_POLICY_CONFLICT",
                "Repair transition does not match the current state",
            ));
        }
        let stale = recover(&gate, &mut ledger)?;
        let connection_guard = connection_lock_file(&gate, false)?;
        match connection_guard.try_lock_exclusive() {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Err(busy()),
            Err(_) => return Err(failure()),
        }
        if !ledger.held.is_empty() || !ledger.control_held.is_empty() {
            return Err(busy());
        }
        let next = match action {
            RepairAction::Enter => {
                let fd = fs::openat(
                    gate.dir()?,
                    REPAIR_FILE,
                    OFlags::RDWR | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )
                .map_err(|_| failure())?;
                let file: File = fd.into();
                let stat = fs::fstat(&file).map_err(|_| failure())?;
                if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
                    || stat.st_nlink != 1
                    || stat.st_size < 0
                    || stat.st_size as u64 > REPAIR_BYTES
                {
                    return Err(failure());
                }
                file.set_len(0).map_err(|_| failure())?;
                file.sync_all().map_err(|_| failure())?;
                gate.dir()?.sync_all().map_err(|_| failure())?;
                let (_, available) = observe(&gate.root)?;
                if available < REPAIR_BYTES - (512 * 1024) {
                    return Err(capacity());
                }
                let baseline = observe_control(&gate.root, &ledger)?;
                ledger
                    .repair
                    .as_mut()
                    .ok_or_else(failure)?
                    .baseline_control_bytes = Some(baseline.to_string());
                budget::RepairState::Active
            }
            RepairAction::Exit => {
                let (_, available) = observe(&gate.root)?;
                let (_, existing_blocks) = repair_stats(&gate)?;
                if available
                    .checked_add(existing_blocks)
                    .is_none_or(|n| n < REPAIR_BYTES + 512 * 1024)
                {
                    return Err(capacity());
                }
                fill_repair_file(&gate)?;
                ledger
                    .repair
                    .as_mut()
                    .ok_or_else(failure)?
                    .baseline_control_bytes = None;
                budget::RepairState::Ready
            }
        };
        let repair = ledger.repair.as_mut().ok_or_else(failure)?;
        repair.revision += 1;
        repair.state = next;
        let receipt = RepairChange {
            action,
            input,
            revision: repair.revision,
            state: next,
            request_id: context.request_id().into(),
            actor: context.actor().clone(),
        };
        ledger.repair_changes.push(receipt.clone());
        gate.save(&ledger)?;
        remove_locks(&gate, &stale);
        Ok(receipt)
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
        if !stale.is_empty() {
            gate.save(&ledger)?;
            remove_locks(&gate, &stale);
        }
        provision_repair(&gate, &mut ledger)?;
        require_normal_repair(&gate, &ledger, false)?;
        let (used, available) = observe(&gate.root)?;
        if ledger.held.len() + ledger.control_held.len() == MAX_RESERVATIONS {
            return Err(limit());
        }
        let held = ledger.payload_total()?;
        let control_held = ledger.control_total()?;
        let bytes = if let Some(bytes) = requested {
            bytes
        } else {
            // Reject zero headroom with the appropriate public capacity reason.
            budget::admit_payload(&ledger.policy, used, held, control_held, 1, available)?;
            budget::decimal(&ledger.media_output_limit_bytes)?
                .min(
                    budget::decimal(&ledger.policy.payload_limit_bytes)?
                        .saturating_sub(used)
                        .saturating_sub(held),
                )
                .min(
                    available
                        .saturating_sub(held)
                        .saturating_sub(control_held)
                        .saturating_sub(budget::decimal(&ledger.policy.minimum_available_bytes)?),
                )
        };
        budget::admit_payload(&ledger.policy, used, held, control_held, bytes, available)?;
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
                kind: Kind::Payload,
                lock: Some(lock),
            },
        })
    }
    pub(super) fn reserve_control(
        budget: &Budget,
        database_name: &str,
        bytes: u64,
    ) -> Result<Reservation, EditorError> {
        reserve_control_inner(budget, database_name, bytes, false)
    }
    pub(super) fn reserve_control_repair(
        budget: &Budget,
        database_name: &str,
        bytes: u64,
    ) -> Result<Reservation, EditorError> {
        reserve_control_inner(budget, database_name, bytes, true)
    }
    fn reserve_control_inner(
        budget: &Budget,
        database_name: &str,
        bytes: u64,
        repair_mode: bool,
    ) -> Result<Reservation, EditorError> {
        validate_database_name(database_name)?;
        if bytes > i64::MAX as u64 {
            return Err(limit());
        }
        let gate = Gate::open(budget, true)?;
        let mut ledger = gate.read()?;
        if ledger
            .control_database_name
            .as_deref()
            .is_some_and(|name| name != database_name)
        {
            return Err(failure());
        }
        let stale = recover(&gate, &mut ledger)?;
        if !stale.is_empty() {
            gate.save(&ledger)?;
            remove_locks(&gate, &stale);
        }
        if repair_mode {
            require_repair_active(&gate, &ledger)?;
        } else {
            provision_repair(&gate, &mut ledger)?;
            require_normal_repair(&gate, &ledger, true)?;
        }
        let (_, available) = observe(&gate.root)?;
        if ledger.held.len() + ledger.control_held.len() == MAX_RESERVATIONS {
            return Err(limit());
        }
        ledger.control_database_name = Some(database_name.to_owned());
        let used = observe_control(&gate.root, &ledger)?;
        if repair_mode {
            admit_repair_control(&ledger, used, bytes, available)?;
        } else {
            budget::admit_control(
                ledger.control_limit()?,
                used,
                ledger.control_total()?,
                ledger.payload_total()?,
                bytes,
                budget::decimal(&ledger.policy.minimum_available_bytes)?,
                available,
            )?;
        }
        let id = uuid::Uuid::new_v4().to_string();
        let lock = lifetime(&gate, &id, true)?.ok_or_else(failure)?;
        ledger.version = ledger.version.max(2);
        if ledger.control_limit_bytes.is_none() {
            ledger.control_limit_bytes = Some(DEFAULT_CONTROL_LIMIT.to_string());
        }
        ledger.control_held.insert(
            id.clone(),
            Entry {
                category: if repair_mode {
                    "sqlite_repair".into()
                } else {
                    "sqlite_control".into()
                },
                bytes,
                writer: None,
            },
        );
        gate.save(&ledger)?;
        Ok(Reservation {
            inner: Held {
                budget: budget.clone(),
                id,
                bytes,
                kind: if repair_mode {
                    Kind::RepairControl
                } else {
                    Kind::Control
                },
                lock: Some(lock),
            },
        })
    }
    pub(super) struct Held {
        budget: Budget,
        id: String,
        bytes: u64,
        kind: Kind,
        lock: Option<File>,
    }
    #[derive(Clone, Copy)]
    enum Kind {
        Payload,
        Control,
        RepairControl,
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
            provision_repair(&gate, &mut ledger)?;
            if !matches!(self.kind, Kind::RepairControl) {
                require_normal_repair(&gate, &ledger, matches!(self.kind, Kind::Control))?;
            }
            let (used, available) = observe(&gate.root)?;
            match self.kind {
                Kind::Payload => budget::admit_payload(
                    &ledger.policy,
                    used,
                    ledger.payload_total()?,
                    ledger.control_total()?,
                    bytes - self.bytes,
                    available,
                )?,
                Kind::Control => budget::admit_control(
                    ledger.control_limit()?,
                    observe_control(&gate.root, &ledger)?,
                    ledger.control_total()?,
                    ledger.payload_total()?,
                    bytes - self.bytes,
                    budget::decimal(&ledger.policy.minimum_available_bytes)?,
                    available,
                )?,
                Kind::RepairControl => {
                    require_repair_active(&gate, &ledger)?;
                    admit_repair_control(
                        &ledger,
                        observe_control(&gate.root, &ledger)?,
                        bytes - self.bytes,
                        available,
                    )?
                }
            }
            let entry = match self.kind {
                Kind::Payload => ledger.held.get_mut(&self.id),
                Kind::Control | Kind::RepairControl => ledger.control_held.get_mut(&self.id),
            }
            .ok_or_else(failure)?;
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
                match self.kind {
                    Kind::Payload => ledger.held.remove(&self.id),
                    Kind::Control | Kind::RepairControl => ledger.control_held.remove(&self.id),
                };
                gate.save(&ledger)?;
                remove_locks(&gate, std::slice::from_ref(&self.id));
                Ok(())
            })();
            let _ = result;
        }
    }
    #[allow(clippy::unnecessary_cast)]
    fn scan_roots(root: &File, names: &[&str]) -> Result<u64, EditorError> {
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
        for name in names {
            scan.visit(root, OsStr::new(name), 0)?;
        }
        Ok(scan.bytes)
    }
    fn observe_control(root: &File, ledger: &Ledger) -> Result<u64, EditorError> {
        let total = if let Some(database_name) = &ledger.control_database_name {
            let wal = format!("{database_name}-wal");
            let shm = format!("{database_name}-shm");
            let journal = format!("{database_name}-journal");
            scan_roots(
                root,
                &[CONTROL, database_name, &wal, &shm, &journal, "sqlite-temp"],
            )?
        } else {
            scan_roots(root, &[CONTROL, "sqlite-temp"])?
        };
        // Repair capacity is a separate physical allocation, not database
        // control usage. The file remains visible to the directory scanner.
        let file = match fs::openat(root, CONTROL, DIR, Mode::empty()) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) => return Ok(total),
            Err(_) => return Err(failure()),
        };
        let fd = match fs::openat(
            &file,
            REPAIR_FILE,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(fd) => Some(fd),
            Err(rustix::io::Errno::NOENT) => None,
            Err(_) => return Err(failure()),
        };
        let repair_bytes = if let Some(fd) = fd {
            let stat = fs::fstat(&fd).map_err(|_| failure())?;
            if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
                || stat.st_nlink != 1
                || stat.st_size < 0
            {
                return Err(failure());
            }
            stat.st_size as u64
        } else {
            0
        };
        total.checked_sub(repair_bytes).ok_or_else(failure)
    }
    fn observe(root: &File) -> Result<(u64, u64), EditorError> {
        let used = scan_roots(root, &ROOTS)?;
        let volume = fs::fstatvfs(root).map_err(|_| failure())?;
        let available = (volume.f_bavail as u64)
            .checked_mul(volume.f_frsize as u64)
            .ok_or_else(failure)?;
        Ok((used, available))
    }
    pub(super) fn volume_available_bytes(budget: &Budget) -> Result<u64, EditorError> {
        let root: File = fs::open(&budget.workspace, DIR, Mode::empty())
            .map_err(|_| failure())?
            .into();
        let volume = fs::fstatvfs(&root).map_err(|_| failure())?;
        (volume.f_bavail as u64)
            .checked_mul(volume.f_frsize as u64)
            .ok_or_else(failure)
    }
}
