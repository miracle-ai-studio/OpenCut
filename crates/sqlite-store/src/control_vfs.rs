//! SQLite VFS shim that reserves control bytes before native file growth.
//! The default VFS still owns locking, durability and the actual file I/O.
use std::{
    alloc::{Layout, alloc_zeroed, dealloc},
    collections::HashMap,
    ffi::{CStr, CString, c_int, c_void},
    os::unix::ffi::OsStrExt,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    ptr,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicPtr, Ordering},
    },
};

use opencut_contracts::EditorError;
use opencut_storage_budget::{Budget, ConnectionLease, Reservation};
use rusqlite::{Connection, OpenFlags, ffi};

static ORIGINAL: AtomicPtr<ffi::sqlite3_vfs> = AtomicPtr::new(ptr::null_mut());
static REGISTERED: OnceLock<Result<(), &'static str>> = OnceLock::new();
type WorkspaceKey = (PathBuf, String, bool);
static WORKSPACES: OnceLock<Mutex<HashMap<WorkspaceKey, String>>> = OnceLock::new();
const TEMP_DIRECTORY: &str = "sqlite-temp";

struct WorkspaceVfs {
    workspace: PathBuf,
    database_name: String,
    repair: bool,
}

pub(super) fn open(path: &Path) -> Result<Connection, EditorError> {
    open_with_mode(path, false)
}

pub(super) fn open_repair(path: &Path) -> Result<Connection, EditorError> {
    open_with_mode(path, true)
}

fn open_with_mode(path: &Path, repair: bool) -> Result<Connection, EditorError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| EditorError::new("STORAGE_BUDGET_ERROR", "Invalid database file name"))?;
    Budget::new(parent).check_database_control_name(name)?;
    let workspace = parent
        .canonicalize()
        .map_err(|_| EditorError::new("STORAGE_ERROR", "Could not resolve managed workspace"))?;
    let vfs_name = register(&workspace, name, repair)?;
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let flags = if repair {
        flags
    } else {
        flags | OpenFlags::SQLITE_OPEN_CREATE
    };
    Connection::open_with_flags_and_vfs(workspace.join(name), flags, vfs_name.as_str())
        .map_err(super::storage_error)
}

fn initialize_original() -> Result<(), EditorError> {
    REGISTERED
        .get_or_init(|| {
            // SAFETY: SQLite owns the default VFS for the lifetime of the process.
            // The copied shim is leaked because registered VFS pointers must remain live.
            unsafe {
                if ffi::sqlite3_initialize() != ffi::SQLITE_OK {
                    return Err("SQLite initialization failed");
                }
                let original = ffi::sqlite3_vfs_find(ptr::null());
                if original.is_null() || (*original).xOpen.is_none() {
                    return Err("SQLite default VFS unavailable");
                }
                ORIGINAL.store(original, Ordering::Release);
                Ok(())
            }
        })
        .as_ref()
        .map_err(|message| EditorError::new("STORAGE_ERROR", *message))
        .copied()
}

fn register(workspace: &Path, database_name: &str, repair: bool) -> Result<String, EditorError> {
    initialize_original()?;
    let registry = WORKSPACES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut registry = registry
        .lock()
        .map_err(|_| EditorError::new("STORAGE_ERROR", "SQLite VFS registry is unavailable"))?;
    let key = (workspace.to_path_buf(), database_name.to_owned(), repair);
    if let Some(name) = registry.get(&key) {
        return Ok(name.clone());
    }
    let name = format!("opencut-managed-{}", uuid::Uuid::new_v4().simple());
    let name_c = Box::new(
        CString::new(name.as_bytes())
            .map_err(|_| EditorError::new("STORAGE_ERROR", "Invalid SQLite VFS name"))?,
    );
    let mut config = Box::new(WorkspaceVfs {
        workspace: workspace.to_path_buf(),
        database_name: database_name.to_owned(),
        repair,
    });
    // SAFETY: the copied default VFS, name and context are leaked only after
    // registration succeeds; SQLite may call this VFS for process lifetime.
    unsafe {
        let original = ORIGINAL.load(Ordering::Acquire);
        if original.is_null() {
            return Err(EditorError::new("STORAGE_ERROR", "SQLite VFS unavailable"));
        }
        let mut shim = Box::new(*original);
        shim.zName = name_c.as_ptr();
        shim.pAppData = (&mut *config as *mut WorkspaceVfs).cast();
        shim.pNext = ptr::null_mut();
        shim.szOsFile = std::mem::size_of::<GuardedFile>() as c_int;
        shim.xOpen = Some(shim_open);
        if ffi::sqlite3_vfs_register(&mut *shim, 0) != ffi::SQLITE_OK {
            return Err(EditorError::new(
                "STORAGE_ERROR",
                "SQLite VFS registration failed",
            ));
        }
        Box::leak(shim);
        Box::leak(config);
        Box::leak(name_c);
    }
    registry.insert(key, name.clone());
    Ok(name)
}

struct Identity {
    budget: Budget,
    database_name: String,
    database_path: PathBuf,
    repair: bool,
}

/// The first field is the sqlite3_file ABI header. SQLite allocates `szOsFile`
/// bytes and never directly touches the remaining Rust-owned fields.
#[repr(C)]
struct GuardedFile {
    base: ffi::sqlite3_file,
    inner: *mut ffi::sqlite3_file,
    inner_layout: Layout,
    inner_methods: *const ffi::sqlite3_io_methods,
    methods: Box<ffi::sqlite3_io_methods>,
    identity: Option<Identity>,
    _connection_lease: Option<ConnectionLease>,
    reservation: Option<Reservation>,
    file_floor: u64,
    shm_floor: u64,
    file_required: u64,
    shm_required: u64,
}

impl GuardedFile {
    fn desired(&self) -> Option<u64> {
        self.file_required.checked_add(self.shm_required)
    }

    fn ensure(&mut self) -> Result<(), EditorError> {
        let required = self.desired().ok_or_else(admission_storage_error)?;
        if required == 0 {
            return Ok(());
        }
        if self
            .reservation
            .as_ref()
            .is_some_and(|r| required <= r.reserved_bytes())
        {
            return Ok(());
        }
        let identity = self.identity.as_ref().ok_or_else(admission_storage_error)?;
        // Admit bounded growth ahead of SQLite's page writes. Persisting and
        // fsyncing the reservation ledger for every 4 KiB page makes a large
        // edit spend most of its time in admission rather than SQLite. These
        // bytes remain fully reserved; every actual write still holds the
        // repair gate. Near a quota boundary, retry the exact requirement so
        // the optimization cannot turn an otherwise admissible write away.
        const GROWTH_QUANTUM: u64 = 256 * 1024;
        let preferred = if identity.repair {
            required
        } else {
            required
                .checked_add(GROWTH_QUANTUM - 1)
                .map(|n| n / GROWTH_QUANTUM * GROWTH_QUANTUM)
                .filter(|n| *n <= i64::MAX as u64)
                .unwrap_or(required)
        };
        let capacity = |error: &EditorError| {
            matches!(
                error.code.as_str(),
                "DATABASE_CAPACITY_EXHAUSTED" | "STORAGE_CAPACITY_LOW"
            )
        };
        if let Some(reservation) = &mut self.reservation {
            match reservation.ensure(preferred) {
                Err(error) if preferred != required && capacity(&error) => {
                    reservation.ensure(required)
                }
                result => result,
            }
        } else {
            self.reservation = Some(if identity.repair {
                identity
                    .budget
                    .reserve_database_control_repair(&identity.database_name, required)?
            } else {
                match identity
                    .budget
                    .reserve_database_control(&identity.database_name, preferred)
                {
                    Err(error) if preferred != required && capacity(&error) => identity
                        .budget
                        .reserve_database_control(&identity.database_name, required)?,
                    result => result?,
                }
            });
            Ok(())
        }
    }

    fn file_size(&self) -> Result<u64, ()> {
        let mut size = 0_i64;
        // SAFETY: `inner` and its method table remain live until xClose returns.
        let callback = unsafe { (*self.inner_methods).xFileSize.ok_or(())? };
        let result = unsafe { callback(self.inner, &mut size) };
        if result != ffi::SQLITE_OK || size < 0 {
            return Err(());
        }
        Ok(size as u64)
    }

    fn with_write_gate(&self, write: impl FnOnce() -> c_int) -> Result<c_int, EditorError> {
        if let Some(identity) = &self.identity {
            identity
                .budget
                .with_database_write(&identity.database_name, identity.repair, write)
        } else {
            Ok(write())
        }
    }

    fn reserve_file_end(&mut self, end: u64) -> Result<(), EditorError> {
        let current = self.file_size().map_err(|_| admission_storage_error())?;
        if end <= current {
            return Ok(());
        }
        self.file_floor = self.file_floor.min(current);
        self.file_required = self.file_required.max(end.saturating_sub(self.file_floor));
        self.ensure()
    }

    fn reserve_shm_end(&mut self, end: u64) -> Result<(), EditorError> {
        let identity = self.identity.as_ref().ok_or_else(admission_storage_error)?;
        let current = shm_bytes(&identity.database_path).map_err(|_| admission_storage_error())?;
        if end <= current {
            return Ok(());
        }
        self.shm_floor = self.shm_floor.min(current);
        self.shm_required = self.shm_required.max(end.saturating_sub(self.shm_floor));
        self.ensure()
    }
}

fn admission_storage_error() -> EditorError {
    EditorError::new(
        "STORAGE_ERROR",
        "Cannot safely identify or observe a managed SQLite control file",
    )
}

fn admission_sqlite_code(error: &EditorError, other: c_int) -> c_int {
    if std::env::var_os("OPENCUT_TRACE_SQLITE").is_some() {
        eprintln!(
            "OpenCut VFS admission failure: fallback={other} code={} message={}",
            error.code, error.message
        );
    }
    if matches!(
        error.code.as_str(),
        "DATABASE_CAPACITY_EXHAUSTED" | "STORAGE_CAPACITY_LOW"
    ) {
        ffi::SQLITE_FULL
    } else if error.code == "STORAGE_BUDGET_BUSY" {
        // Admission locks can be held by another live Host. Keep this
        // retryable for callers such as Worker heartbeats; IOERR would turn a
        // transient lock into a terminal task failure.
        ffi::SQLITE_BUSY
    } else {
        other
    }
}

fn native_capacity_code(result: c_int, identity: Option<&Identity>) -> c_int {
    if result != ffi::SQLITE_OK
        && result != ffi::SQLITE_NOTFOUND
        && std::env::var_os("OPENCUT_TRACE_SQLITE").is_some()
    {
        eprintln!("OpenCut native VFS failure: result={result}");
    }
    if matches!(result & 0xff, ffi::SQLITE_CANTOPEN | ffi::SQLITE_IOERR)
        && identity.is_some_and(|identity| {
            identity.budget.volume_available_bytes().ok() == Some(0)
                && identity.database_path.is_file()
        })
    {
        ffi::SQLITE_FULL
    } else {
        result
    }
}

fn shm_bytes(database: &Path) -> Result<u64, ()> {
    let mut path = database.as_os_str().to_os_string();
    path.push("-shm");
    let path = PathBuf::from(path);
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        _ => Err(()),
    }
}

unsafe fn identity(
    z_name: *const std::ffi::c_char,
    flags: c_int,
    config: &WorkspaceVfs,
) -> Option<Identity> {
    if z_name.is_null() {
        return None;
    }
    // SAFETY: SQLite passes a NUL-terminated filename to xOpen.
    let bytes = unsafe { CStr::from_ptr(z_name) }.to_bytes();
    let file = PathBuf::from(std::ffi::OsStr::from_bytes(bytes));
    let file_name = file.file_name()?.to_str()?;
    let database_name = if flags & ffi::SQLITE_OPEN_MAIN_DB != 0 {
        file_name.to_owned()
    } else if flags & ffi::SQLITE_OPEN_WAL != 0 {
        file_name.strip_suffix("-wal")?.to_owned()
    } else if flags & ffi::SQLITE_OPEN_MAIN_JOURNAL != 0 {
        file_name.strip_suffix("-journal")?.to_owned()
    } else {
        return None;
    };
    let parent = file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if parent.canonicalize().ok()? != config.workspace || database_name != config.database_name {
        return None;
    }
    let database_path = config.workspace.join(&database_name);
    Some(Identity {
        budget: Budget::new(&config.workspace),
        database_name,
        database_path,
        repair: config.repair,
    })
}

fn is_temporary(flags: c_int) -> bool {
    flags
        & (ffi::SQLITE_OPEN_TEMP_DB
            | ffi::SQLITE_OPEN_TEMP_JOURNAL
            | ffi::SQLITE_OPEN_TRANSIENT_DB
            | ffi::SQLITE_OPEN_SUBJOURNAL)
        != 0
}

fn temporary_name(config: &WorkspaceVfs) -> Result<CString, ()> {
    let path = config
        .workspace
        .join(TEMP_DIRECTORY)
        .join(format!("sqlite-{}", uuid::Uuid::new_v4().simple()));
    CString::new(path.as_os_str().as_bytes()).map_err(|_| ())
}

fn ensure_temporary_directory(config: &WorkspaceVfs) -> Result<(), ()> {
    let directory = config.workspace.join(TEMP_DIRECTORY);
    match std::fs::create_dir(&directory) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(_) => return Err(()),
    }
    if !std::fs::symlink_metadata(&directory)
        .map_err(|_| ())?
        .file_type()
        .is_dir()
    {
        return Err(());
    }
    Ok(())
}

unsafe extern "C" fn shim_open(
    vfs: *mut ffi::sqlite3_vfs,
    z_name: *const std::ffi::c_char,
    file: *mut ffi::sqlite3_file,
    flags: c_int,
    out_flags: *mut c_int,
) -> c_int {
    // A failed xOpen must leave pMethods null: SQLite may otherwise xClose it.
    unsafe { (*file).pMethods = ptr::null() };
    catch_unwind(AssertUnwindSafe(|| unsafe {
        if vfs.is_null() || (*vfs).pAppData.is_null() {
            return ffi::SQLITE_IOERR;
        }
        let config = &*((*vfs).pAppData.cast::<WorkspaceVfs>());
        open_inner(z_name, file, flags, out_flags, config)
    }))
    .unwrap_or(ffi::SQLITE_IOERR)
}

unsafe fn open_inner(
    z_name: *const std::ffi::c_char,
    file: *mut ffi::sqlite3_file,
    flags: c_int,
    out_flags: *mut c_int,
    config: &WorkspaceVfs,
) -> c_int {
    // SAFETY: SQLite owns `file` and `z_name` for the entire xOpen call; the
    // default VFS pointer and callback tables remain registered for process life.
    unsafe {
        let original = ORIGINAL.load(Ordering::Acquire);
        if original.is_null() || (*original).szOsFile <= 0 {
            return ffi::SQLITE_IOERR;
        }
        let Ok(layout) = Layout::from_size_align((*original).szOsFile as usize, 16) else {
            return ffi::SQLITE_IOERR;
        };
        let inner = alloc_zeroed(layout).cast::<ffi::sqlite3_file>();
        if inner.is_null() {
            return ffi::SQLITE_NOMEM;
        }
        let Some(callback) = (*original).xOpen else {
            dealloc(inner.cast(), layout);
            return ffi::SQLITE_IOERR;
        };
        let temp_name = if is_temporary(flags) && z_name.is_null() {
            match temporary_name(config) {
                Ok(name) => Some(name),
                Err(()) => {
                    dealloc(inner.cast(), layout);
                    return ffi::SQLITE_IOERR;
                }
            }
        } else {
            None
        };
        let identity = if temp_name.is_some() {
            Some(Identity {
                budget: Budget::new(&config.workspace),
                database_name: config.database_name.clone(),
                database_path: config.workspace.join(&config.database_name),
                repair: config.repair,
            })
        } else {
            identity(z_name, flags, config)
        };
        if identity.is_none() {
            dealloc(inner.cast(), layout);
            return ffi::SQLITE_IOERR;
        }
        let connection_lease = if let Some(identity) = &identity {
            let result = identity
                .budget
                .database_connection_lease(&identity.database_name, config.repair);
            match result {
                Ok(lease) => Some(lease),
                Err(error) => {
                    dealloc(inner.cast(), layout);
                    return admission_sqlite_code(&error, ffi::SQLITE_IOERR);
                }
            }
        } else {
            None
        };
        if temp_name.is_some() && ensure_temporary_directory(config).is_err() {
            dealloc(inner.cast(), layout);
            return native_capacity_code(ffi::SQLITE_IOERR, identity.as_ref());
        }
        let filename = temp_name.as_ref().map_or(z_name, |name| name.as_ptr());
        let result = callback(original, filename, inner, flags, out_flags);
        if result != ffi::SQLITE_OK || (*inner).pMethods.is_null() {
            if !(*inner).pMethods.is_null()
                && let Some(close) = (*(*inner).pMethods).xClose
            {
                let _ = close(inner);
            }
            dealloc(inner.cast(), layout);
            return if result == ffi::SQLITE_OK {
                ffi::SQLITE_IOERR
            } else {
                native_capacity_code(result, identity.as_ref())
            };
        }
        let inner_methods = (*inner).pMethods;
        let mut size = 0_i64;
        let size_result = (*inner_methods)
            .xFileSize
            .map(|callback| callback(inner, &mut size))
            .unwrap_or(ffi::SQLITE_IOERR);
        let shm_floor = identity
            .as_ref()
            .map(|identity| shm_bytes(&identity.database_path))
            .unwrap_or(Ok(0));
        if size_result != ffi::SQLITE_OK || size < 0 || shm_floor.is_err() {
            if let Some(close) = (*inner_methods).xClose {
                let _ = close(inner);
            }
            dealloc(inner.cast(), layout);
            return ffi::SQLITE_IOERR;
        }
        let methods = wrapped_methods(inner_methods);
        let methods_ptr = &*methods as *const ffi::sqlite3_io_methods;
        ptr::write(
            file.cast::<GuardedFile>(),
            GuardedFile {
                base: ffi::sqlite3_file {
                    pMethods: methods_ptr,
                },
                inner,
                inner_layout: layout,
                inner_methods,
                methods,
                identity,
                _connection_lease: connection_lease,
                reservation: None,
                file_floor: size as u64,
                shm_floor: shm_floor.unwrap_or(0),
                file_required: 0,
                shm_required: 0,
            },
        );
        ffi::SQLITE_OK
    }
}

unsafe fn wrapped_methods(
    original: *const ffi::sqlite3_io_methods,
) -> Box<ffi::sqlite3_io_methods> {
    let mut methods = unsafe { *original };
    methods.xClose = Some(io_close);
    methods.xRead = Some(io_read);
    methods.xWrite = Some(io_write);
    methods.xTruncate = Some(io_truncate);
    methods.xSync = Some(io_sync);
    methods.xFileSize = Some(io_file_size);
    methods.xLock = Some(io_lock);
    methods.xUnlock = Some(io_unlock);
    methods.xCheckReservedLock = Some(io_reserved_lock);
    methods.xFileControl = Some(io_file_control);
    methods.xSectorSize = Some(io_sector_size);
    methods.xDeviceCharacteristics = Some(io_characteristics);
    if methods.iVersion >= 2 {
        methods.xShmMap = Some(io_shm_map);
        methods.xShmLock = Some(io_shm_lock);
        methods.xShmBarrier = Some(io_shm_barrier);
        methods.xShmUnmap = Some(io_shm_unmap);
    }
    if methods.iVersion >= 3 {
        methods.xFetch = Some(io_fetch);
        methods.xUnfetch = Some(io_unfetch);
    }
    Box::new(methods)
}

unsafe extern "C" fn io_close(file: *mut ffi::sqlite3_file) -> c_int {
    let guarded = unsafe { &mut *file.cast::<GuardedFile>() };
    guarded.base.pMethods = ptr::null();
    let result = unsafe { (*guarded.inner_methods).xClose }
        .map(|callback| unsafe { callback(guarded.inner) })
        .unwrap_or(ffi::SQLITE_IOERR);
    let inner = guarded.inner;
    let layout = guarded.inner_layout;
    unsafe { ptr::drop_in_place(guarded) };
    unsafe { dealloc(inner.cast(), layout) };
    result
}

macro_rules! forward_int {
    ($name:ident($($arg:ident: $ty:ty),*) => $field:ident) => {
        unsafe extern "C" fn $name(file: *mut ffi::sqlite3_file, $($arg: $ty),*) -> c_int {
            let guarded = unsafe { &*file.cast::<GuardedFile>() };
            unsafe { (*guarded.inner_methods).$field }
                .map(|callback| unsafe { callback(guarded.inner, $($arg),*) })
                .unwrap_or(ffi::SQLITE_IOERR)
        }
    };
}

forward_int!(io_read(buffer: *mut c_void, amount: c_int, offset: i64) => xRead);
unsafe extern "C" fn io_sync(file: *mut ffi::sqlite3_file, flags: c_int) -> c_int {
    let guarded = unsafe { &mut *file.cast::<GuardedFile>() };
    let result = unsafe { (*guarded.inner_methods).xSync }
        .map(|callback| unsafe { callback(guarded.inner, flags) })
        .unwrap_or(ffi::SQLITE_IOERR);
    if result == ffi::SQLITE_OK {
        let size = guarded.file_size();
        let shm = guarded
            .identity
            .as_ref()
            .map(|identity| shm_bytes(&identity.database_path))
            .unwrap_or(Ok(0));
        if let (Ok(size), Ok(shm)) = (size, shm) {
            // Synced bytes are now covered by physical usage observation.
            // Release unused headroom at this durability boundary so a later
            // transaction cannot consume slack admitted before a policy change.
            guarded.file_floor = size;
            guarded.shm_floor = shm;
            guarded.file_required = 0;
            guarded.shm_required = 0;
            guarded.reservation.take();
        }
    }
    result
}
forward_int!(io_file_size(size: *mut i64) => xFileSize);
forward_int!(io_lock(level: c_int) => xLock);
forward_int!(io_unlock(level: c_int) => xUnlock);
forward_int!(io_reserved_lock(out: *mut c_int) => xCheckReservedLock);
forward_int!(io_shm_lock(offset: c_int, count: c_int, flags: c_int) => xShmLock);
forward_int!(io_shm_unmap(delete: c_int) => xShmUnmap);
forward_int!(io_unfetch(offset: i64, pointer: *mut c_void) => xUnfetch);

unsafe extern "C" fn io_write(
    file: *mut ffi::sqlite3_file,
    buffer: *const c_void,
    amount: c_int,
    offset: i64,
) -> c_int {
    catch_unwind(AssertUnwindSafe(|| unsafe {
        let guarded = &mut *file.cast::<GuardedFile>();
        let Some(end) = u64::try_from(offset)
            .ok()
            .zip(u64::try_from(amount).ok())
            .and_then(|(offset, amount)| offset.checked_add(amount))
        else {
            return ffi::SQLITE_IOERR_WRITE;
        };
        if let Err(error) = guarded.reserve_file_end(end) {
            return admission_sqlite_code(&error, ffi::SQLITE_IOERR_WRITE);
        }
        guarded
            .with_write_gate(|| {
                let result = (*guarded.inner_methods)
                    .xWrite
                    .map(|callback| callback(guarded.inner, buffer, amount, offset))
                    .unwrap_or(ffi::SQLITE_IOERR_WRITE);
                native_capacity_code(result, guarded.identity.as_ref())
            })
            .unwrap_or_else(|error| admission_sqlite_code(&error, ffi::SQLITE_IOERR_WRITE))
    }))
    .unwrap_or(ffi::SQLITE_IOERR_WRITE)
}

unsafe extern "C" fn io_truncate(file: *mut ffi::sqlite3_file, size: i64) -> c_int {
    catch_unwind(AssertUnwindSafe(|| unsafe {
        let guarded = &mut *file.cast::<GuardedFile>();
        let Ok(size) = u64::try_from(size) else {
            return ffi::SQLITE_IOERR_TRUNCATE;
        };
        if let Err(error) = guarded.reserve_file_end(size) {
            return admission_sqlite_code(&error, ffi::SQLITE_IOERR_TRUNCATE);
        }
        let result = guarded
            .with_write_gate(|| {
                let result = (*guarded.inner_methods)
                    .xTruncate
                    .map(|callback| callback(guarded.inner, size as i64))
                    .unwrap_or(ffi::SQLITE_IOERR_TRUNCATE);
                native_capacity_code(result, guarded.identity.as_ref())
            })
            .unwrap_or_else(|error| admission_sqlite_code(&error, ffi::SQLITE_IOERR_TRUNCATE));
        if result == ffi::SQLITE_OK {
            guarded.file_floor = guarded.file_floor.min(size);
        }
        result
    }))
    .unwrap_or(ffi::SQLITE_IOERR_TRUNCATE)
}

unsafe extern "C" fn io_file_control(
    file: *mut ffi::sqlite3_file,
    operation: c_int,
    argument: *mut c_void,
) -> c_int {
    catch_unwind(AssertUnwindSafe(|| unsafe {
        let guarded = &mut *file.cast::<GuardedFile>();
        if operation == ffi::SQLITE_FCNTL_SIZE_HINT {
            if argument.is_null() {
                return ffi::SQLITE_IOERR;
            }
            let size = *(argument.cast::<i64>());
            let Ok(size) = u64::try_from(size) else {
                return ffi::SQLITE_IOERR;
            };
            if let Err(error) = guarded.reserve_file_end(size) {
                return admission_sqlite_code(&error, ffi::SQLITE_IOERR);
            }
        } else if operation == ffi::SQLITE_FCNTL_CHUNK_SIZE && !argument.is_null() {
            // A chunk allocation may exceed a preceding xWrite's requested end.
            if *(argument.cast::<c_int>()) > 0 {
                return ffi::SQLITE_FULL;
            }
        }
        guarded
            .with_write_gate(|| {
                let result = (*guarded.inner_methods)
                    .xFileControl
                    .map(|callback| callback(guarded.inner, operation, argument))
                    .unwrap_or(ffi::SQLITE_NOTFOUND);
                native_capacity_code(result, guarded.identity.as_ref())
            })
            .unwrap_or_else(|error| admission_sqlite_code(&error, ffi::SQLITE_IOERR))
    }))
    .unwrap_or(ffi::SQLITE_IOERR)
}

unsafe extern "C" fn io_shm_map(
    file: *mut ffi::sqlite3_file,
    page: c_int,
    page_size: c_int,
    extend: c_int,
    output: *mut *mut c_void,
) -> c_int {
    catch_unwind(AssertUnwindSafe(|| unsafe {
        let guarded = &mut *file.cast::<GuardedFile>();
        if extend != 0 {
            let Some(end) = u64::try_from(page)
                .ok()
                .and_then(|n| n.checked_add(1))
                .zip(u64::try_from(page_size).ok())
                .and_then(|(page, size)| page.checked_mul(size))
            else {
                return ffi::SQLITE_IOERR_SHMMAP;
            };
            if let Err(error) = guarded.reserve_shm_end(end) {
                return admission_sqlite_code(&error, ffi::SQLITE_IOERR_SHMMAP);
            }
        }
        let mapping = || {
            let result = (*guarded.inner_methods)
                .xShmMap
                .map(|callback| callback(guarded.inner, page, page_size, extend, output))
                .unwrap_or(ffi::SQLITE_IOERR_SHMMAP);
            native_capacity_code(result, guarded.identity.as_ref())
        };
        if extend != 0 {
            guarded
                .with_write_gate(mapping)
                .unwrap_or_else(|error| admission_sqlite_code(&error, ffi::SQLITE_IOERR_SHMMAP))
        } else {
            mapping()
        }
    }))
    .unwrap_or(ffi::SQLITE_IOERR_SHMMAP)
}

unsafe extern "C" fn io_shm_barrier(file: *mut ffi::sqlite3_file) {
    let guarded = unsafe { &*file.cast::<GuardedFile>() };
    if let Some(callback) = unsafe { (*guarded.inner_methods).xShmBarrier } {
        unsafe { callback(guarded.inner) };
    }
}

unsafe extern "C" fn io_sector_size(file: *mut ffi::sqlite3_file) -> c_int {
    let guarded = unsafe { &*file.cast::<GuardedFile>() };
    unsafe { (*guarded.inner_methods).xSectorSize }
        .map(|callback| unsafe { callback(guarded.inner) })
        .unwrap_or(4096)
}

unsafe extern "C" fn io_characteristics(file: *mut ffi::sqlite3_file) -> c_int {
    let guarded = unsafe { &*file.cast::<GuardedFile>() };
    unsafe { (*guarded.inner_methods).xDeviceCharacteristics }
        .map(|callback| unsafe { callback(guarded.inner) })
        .unwrap_or(0)
}

unsafe extern "C" fn io_fetch(
    file: *mut ffi::sqlite3_file,
    offset: i64,
    amount: c_int,
    output: *mut *mut c_void,
) -> c_int {
    let guarded = unsafe { &*file.cast::<GuardedFile>() };
    match unsafe { (*guarded.inner_methods).xFetch } {
        Some(callback) => unsafe { callback(guarded.inner, offset, amount, output) },
        None => {
            unsafe { *output = ptr::null_mut() };
            ffi::SQLITE_OK
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_editor_application::{
        audit::{CallSource, ExecutionContext},
        budget::{BudgetNative, Configure, RepairAction, RepairRequest},
    };

    #[test]
    fn transient_admission_lock_maps_to_retryable_sqlite_busy() {
        let busy = EditorError::new("STORAGE_BUDGET_BUSY", "another live writer holds admission");
        assert_eq!(
            admission_sqlite_code(&busy, ffi::SQLITE_IOERR),
            ffi::SQLITE_BUSY
        );
        let full = EditorError::new("DATABASE_CAPACITY_EXHAUSTED", "no capacity");
        assert_eq!(
            admission_sqlite_code(&full, ffi::SQLITE_IOERR),
            ffi::SQLITE_FULL
        );
    }

    #[test]
    fn managed_sqlite_disk_temp_is_admitted_before_growth() {
        let workspace = tempfile::tempdir().unwrap();
        let database = workspace.path().join("workspace.sqlite");
        let connection = open(&database).unwrap();
        connection
            .execute_batch("PRAGMA temp_store=FILE; CREATE TEMP TABLE sample(value BLOB);")
            .unwrap();
        let budget = Budget::new(workspace.path());
        let observed = budget
            .inspect(false)
            .unwrap()
            .control
            .unwrap()
            .observed_control_bytes
            .parse::<u64>()
            .unwrap();
        let context = ExecutionContext::local(CallSource::Cli);
        budget
            .configure(
                Configure {
                    expected_revision: 0,
                    idempotency_key: "tight-temp".into(),
                    payload_limit_bytes: (64_u64 << 30).to_string(),
                    minimum_available_bytes: "0".into(),
                    media_output_limit_bytes: None,
                    control_limit_bytes: Some((observed + 16 * 1024).to_string()),
                },
                &context,
            )
            .unwrap();
        let error = connection
            .execute("INSERT INTO sample VALUES (zeroblob(4194304))", [])
            .unwrap_err();
        assert_eq!(
            error.sqlite_error_code(),
            Some(rusqlite::ErrorCode::DiskFull)
        );
        budget
            .configure(
                Configure {
                    expected_revision: 1,
                    idempotency_key: "room-temp".into(),
                    payload_limit_bytes: (64_u64 << 30).to_string(),
                    minimum_available_bytes: "0".into(),
                    media_output_limit_bytes: None,
                    control_limit_bytes: Some((observed + 8 * 1024 * 1024).to_string()),
                },
                &context,
            )
            .unwrap();
        assert_eq!(
            connection
                .query_row("SELECT count(*) FROM sample", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        connection
            .execute("INSERT INTO sample VALUES (zeroblob(4194304))", [])
            .unwrap();
        assert_eq!(
            connection
                .query_row("SELECT length(value) FROM sample", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            4_194_304
        );
        let control = budget.inspect(false).unwrap().control.unwrap();
        // SQLite unlinks DELETEONCLOSE temp files while open, so the held
        // reservation accounts for bytes that directory scanning cannot see.
        assert!(control.reserved_control_bytes.parse::<u64>().unwrap() >= 4_194_304);
        drop(connection);
        assert_eq!(
            budget
                .inspect(false)
                .unwrap()
                .control
                .unwrap()
                .reserved_control_bytes,
            "0"
        );
    }

    #[test]
    fn managed_sqlite_temp_directory_symlink_is_rejected() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let database = workspace.path().join("workspace.sqlite");
        let connection = open(&database).unwrap();
        std::os::unix::fs::symlink(outside.path(), workspace.path().join(TEMP_DIRECTORY)).unwrap();
        let error = connection
            .execute_batch("PRAGMA temp_store=FILE; CREATE TEMP TABLE sample(value BLOB); INSERT INTO sample VALUES (zeroblob(4194304));")
            .unwrap_err();
        assert_eq!(
            error.sqlite_error_code(),
            Some(rusqlite::ErrorCode::SystemIoFailure)
        );
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
    }

    #[test]
    fn repair_waits_for_open_connection_and_only_repair_vfs_can_write() {
        let workspace = tempfile::tempdir().unwrap();
        let database = workspace.path().join("workspace.sqlite");
        let connection = open(&database).unwrap();
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL; CREATE TABLE items(value INTEGER); INSERT INTO items VALUES (1);",
            )
            .unwrap();
        let budget = Budget::new(workspace.path());
        let context = ExecutionContext::local(CallSource::Cli);
        let enter = RepairRequest {
            expected_revision: 0,
            idempotency_key: "enter".into(),
        };
        assert_eq!(
            budget
                .repair_transition(RepairAction::Enter, enter.clone(), &context)
                .unwrap_err()
                .code,
            "STORAGE_BUDGET_BUSY"
        );
        drop(connection);
        budget
            .repair_transition(RepairAction::Enter, enter, &context)
            .unwrap();
        assert_eq!(
            open(&database).err().unwrap().code,
            "DATABASE_CAPACITY_EXHAUSTED"
        );
        let repair = open_repair(&database).unwrap();
        repair.execute("UPDATE items SET value = 2", []).unwrap();
        assert_eq!(
            budget
                .repair_transition(
                    RepairAction::Exit,
                    RepairRequest {
                        expected_revision: 1,
                        idempotency_key: "exit".into(),
                    },
                    &context,
                )
                .unwrap_err()
                .code,
            "STORAGE_BUDGET_BUSY"
        );
        drop(repair);
        budget
            .repair_transition(
                RepairAction::Exit,
                RepairRequest {
                    expected_revision: 1,
                    idempotency_key: "exit".into(),
                },
                &context,
            )
            .unwrap();
        let reopened = open(&database).unwrap();
        assert_eq!(
            reopened
                .query_row("SELECT value FROM items", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
    }

    #[test]
    fn wal_growth_is_rejected_before_commit_and_recovers_after_policy_change() {
        let workspace = tempfile::tempdir().unwrap();
        let database = workspace.path().join("workspace.sqlite");
        let connection = open(&database).unwrap();
        connection
            .execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE items(value BLOB NOT NULL);")
            .unwrap();
        let reader = Connection::open(&database).unwrap();
        reader
            .execute_batch("BEGIN; SELECT count(*) FROM items;")
            .unwrap();

        let budget = Budget::new(workspace.path());
        let observed = budget
            .inspect(false)
            .unwrap()
            .control
            .unwrap()
            .observed_control_bytes
            .parse::<u64>()
            .unwrap();
        let context = ExecutionContext::local(CallSource::Cli);
        let limit = observed + 16 * 1024;
        budget
            .configure(
                Configure {
                    expected_revision: 0,
                    idempotency_key: "tight".into(),
                    payload_limit_bytes: (64_u64 << 30).to_string(),
                    minimum_available_bytes: "0".into(),
                    media_output_limit_bytes: None,
                    control_limit_bytes: Some(limit.to_string()),
                },
                &context,
            )
            .unwrap();
        let value = vec![42_u8; 100_000];
        let error = connection
            .execute(
                "INSERT INTO items(value) VALUES (?1)",
                rusqlite::params![value],
            )
            .unwrap_err();
        assert_eq!(
            error.sqlite_error_code(),
            Some(rusqlite::ErrorCode::DiskFull)
        );
        assert_eq!(
            connection
                .query_row("SELECT count(*) FROM items", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );

        let observed = budget
            .inspect(false)
            .unwrap()
            .control
            .unwrap()
            .observed_control_bytes
            .parse::<u64>()
            .unwrap();
        budget
            .configure(
                Configure {
                    expected_revision: 1,
                    idempotency_key: "room".into(),
                    payload_limit_bytes: (64_u64 << 30).to_string(),
                    minimum_available_bytes: "0".into(),
                    media_output_limit_bytes: None,
                    control_limit_bytes: Some((observed + 2 * 1024 * 1024).to_string()),
                },
                &context,
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO items(value) VALUES (?1)",
                rusqlite::params![vec![42_u8; 100_000]],
            )
            .unwrap();
        assert_eq!(
            connection
                .query_row("SELECT count(*) FROM items", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        reader.execute_batch("ROLLBACK").unwrap();
        assert_eq!(
            connection
                .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        drop(connection);
        drop(reader);
        assert_eq!(
            budget
                .inspect(false)
                .unwrap()
                .control
                .unwrap()
                .reserved_control_bytes,
            "0"
        );
    }

    #[test]
    fn another_database_in_the_same_managed_workspace_is_rejected_before_open() {
        let workspace = tempfile::tempdir().unwrap();
        let first = workspace.path().join("first.sqlite");
        let connection = open(&first).unwrap();
        connection
            .execute_batch("CREATE TABLE items(value INTEGER);")
            .unwrap();
        drop(connection);
        let second = workspace.path().join("second.sqlite");
        let error = open(&second).err().unwrap();
        assert_eq!(error.code, "STORAGE_BUDGET_ERROR");
        assert!(!second.exists());
    }

    #[test]
    fn ambiguous_payload_root_is_not_misreported_as_disk_capacity() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let database = workspace.path().join("workspace.sqlite");
        let connection = open(&database).unwrap();
        connection
            .execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE items(value INTEGER);")
            .unwrap();
        drop(connection);
        std::os::unix::fs::symlink(outside.path(), workspace.path().join("artifacts")).unwrap();
        let error = match open(&database) {
            Ok(connection) => connection
                .execute("INSERT INTO items(value) VALUES (1)", [])
                .err()
                .map(crate::storage_error)
                .expect("unsafe root must reject a managed write"),
            Err(error) => error,
        };
        assert_eq!(error.code, "STORAGE_ERROR");
    }
}
