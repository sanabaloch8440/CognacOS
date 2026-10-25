//! Read-only kernel VFS file objects for staged system assets.
//!
//! This is the initial VFS namespace/provider contract. It deliberately exposes
//! only files explicitly registered by a trusted boot or filesystem provider;
//! arbitrary host paths are never opened. A disk-backed ext4 directory provider
//! can register the same FileSource interface once block-device directory lookup
//! is wired into the kernel.
use crate::hal::IrqSpinLock;
use core::sync::atomic::{AtomicUsize, Ordering};

pub const MAX_FILES: usize = 32;
pub const MAX_HANDLES: usize = 64;
pub const MAX_PATH_BYTES: usize = 260;
pub const STATUS_SUCCESS: i32 = 0;
pub const STATUS_OBJECT_NAME_NOT_FOUND: i32 = 0xC000_0034u32 as i32;
pub const STATUS_INVALID_HANDLE: i32 = 0xC000_0008u32 as i32;
pub const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
pub const STATUS_END_OF_FILE: i32 = 0xC000_0011u32 as i32;
pub const STATUS_BUFFER_TOO_SMALL: i32 = 0xC000_0023u32 as i32;
pub const STATUS_INSUFFICIENT_RESOURCES: i32 = 0xC000_009Au32 as i32;

#[derive(Clone, Copy)]
struct FileRecord { path: [u8; MAX_PATH_BYTES], path_len: usize, address: usize, length: usize, generation: usize, used: bool }
impl FileRecord { const EMPTY: Self = Self { path: [0; MAX_PATH_BYTES], path_len: 0, address: 0, length: 0, generation: 0, used: false }; }
#[derive(Clone, Copy)]
struct HandleRecord { token: usize, owner: u64, file_index: usize, offset: usize, used: bool }
impl HandleRecord { const EMPTY: Self = Self { token: 0, owner: 0, file_index: 0, offset: 0, used: false }; }
struct State { files: [FileRecord; MAX_FILES], handles: [HandleRecord; MAX_HANDLES], next_generation: usize, next_handle: usize }
impl State { const fn new() -> Self { Self { files: [FileRecord::EMPTY; MAX_FILES], handles: [HandleRecord::EMPTY; MAX_HANDLES], next_generation: 1, next_handle: 1 } } }
static STATE: IrqSpinLock<State> = IrqSpinLock::new(State::new());
static INIT: AtomicUsize = AtomicUsize::new(0);

fn fold(b: u8) -> u8 { if (b'A'..=b'Z').contains(&b) { b + (b'a' - b'A') } else { b } }
fn path_equal(a: &[u8], b: &[u8]) -> bool { a.len() == b.len() && a.iter().zip(b).all(|(x,y)| fold(*x)==fold(*y)) }

/// Register immutable bytes under an absolute NT-style path. The provider must
/// keep the byte range mapped and immutable until the kernel shuts down.
pub fn register_static(path: &[u8], bytes: &'static [u8]) -> Result<(), i32> {
    if path.is_empty() || path.len() > MAX_PATH_BYTES || !path.starts_with(b"\\") || bytes.is_empty() {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let mut state = STATE.lock();
    if state.files.iter().any(|f| f.used && path_equal(&f.path[..f.path_len], path)) {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let slot = state.files.iter().position(|f| !f.used).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;
    let generation = state.next_generation;
    state.next_generation = state.next_generation.wrapping_add(1).max(1);
    let mut file = FileRecord::EMPTY;
    file.path[..path.len()].copy_from_slice(path);
    file.path_len = path.len(); file.address = bytes.as_ptr() as usize; file.length = bytes.len();
    file.generation = generation; file.used = true; state.files[slot] = file;
    Ok(())
}

/// Mount the boot-staged NTDLL as a read-only VFS file. Safe to call repeatedly.
pub fn mount_boot_system_files() {
    if INIT.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire).is_err() { return; }
    if let Some(bytes) = crate::boot_ntdll_bytes() {
        let _ = register_static(b"\\SystemRoot\\System32\\ntdll.dll", bytes);
        let _ = register_static(b"\\Windows\\System32\\ntdll.dll", bytes);
    }
    INIT.store(2, Ordering::Release);
}

/// Open a registered file and return an opaque, generation-checked handle.
pub fn open(path: &[u8]) -> Result<usize, i32> { open_for_process(0, path) }
pub fn open_for_process(owner: u64, path: &[u8]) -> Result<usize, i32> {
    if path.is_empty() || path.len() > MAX_PATH_BYTES { return Err(STATUS_INVALID_PARAMETER); }
    mount_boot_system_files();
    let mut state = STATE.lock();
    let file_index = state.files.iter().position(|f| f.used && path_equal(&f.path[..f.path_len], path))
        .ok_or(STATUS_OBJECT_NAME_NOT_FOUND)?;
    let slot = state.handles.iter().position(|h| !h.used).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;
    let token = state.next_handle.wrapping_add(1).max(1);
    state.next_handle = token;
    state.handles[slot] = HandleRecord { token, owner, file_index, offset: 0, used: true };
    Ok((token << 8) | (slot + 1))
}

/// Read from a file at an explicit offset. Returns zero at EOF; no user pointer
/// is accepted here, so syscall wrappers must copy through AddressSpace helpers.
pub fn read_at(handle: usize, offset: usize, out: &mut [u8]) -> Result<usize, i32> { read_at_for_process(0, handle, offset, out) }
pub fn read_at_for_process(owner: u64, handle: usize, offset: usize, out: &mut [u8]) -> Result<usize, i32> {
    let slot = handle & 0xff;
    if slot == 0 || slot > MAX_HANDLES { return Err(STATUS_INVALID_HANDLE); }
    let state = STATE.lock();
    let h = &state.handles[slot - 1];
    if !h.used || h.token != handle >> 8 || h.owner != owner { return Err(STATUS_INVALID_HANDLE); }
    let f = &state.files[h.file_index];
    if !f.used { return Err(STATUS_INVALID_HANDLE); }
    if offset >= f.length { return Ok(0); }
    let count = core::cmp::min(out.len(), f.length - offset);
    // SAFETY: the provider promises the registered static byte range remains valid.
    unsafe { core::ptr::copy_nonoverlapping((f.address + offset) as *const u8, out.as_mut_ptr(), count); }
    Ok(count)
}


/// Read from and advance the current file position, serialized with close.
pub fn read_current(handle: usize, out: &mut [u8]) -> Result<usize, i32> { read_current_for_process(0, handle, out) }
pub fn read_current_for_process(owner: u64, handle: usize, out: &mut [u8]) -> Result<usize, i32> {
    let slot = handle & 0xff;
    if slot == 0 || slot > MAX_HANDLES { return Err(STATUS_INVALID_HANDLE); }
    let mut state = STATE.lock();
    let (file_index, offset) = {
        let h = &state.handles[slot - 1];
        if !h.used || h.token != handle >> 8 || h.owner != owner { return Err(STATUS_INVALID_HANDLE); }
        (h.file_index, h.offset)
    };
    let f = &state.files[file_index];
    if !f.used { return Err(STATUS_INVALID_HANDLE); }
    if offset >= f.length { return Ok(0); }
    let count = core::cmp::min(out.len(), f.length - offset);
    // SAFETY: the provider keeps registered file bytes valid and immutable.
    unsafe { core::ptr::copy_nonoverlapping((f.address + offset) as *const u8, out.as_mut_ptr(), count); }
    state.handles[slot - 1].offset = offset + count;
    Ok(count)
}

pub fn close(handle: usize) -> Result<(), i32> { close_for_process(0, handle) }
pub fn close_for_process(owner: u64, handle: usize) -> Result<(), i32> {
    let slot = handle & 0xff;
    if slot == 0 || slot > MAX_HANDLES { return Err(STATUS_INVALID_HANDLE); }
    let mut state = STATE.lock();
    let h = &mut state.handles[slot - 1];
    if !h.used || h.token != handle >> 8 || h.owner != owner { return Err(STATUS_INVALID_HANDLE); }
    *h = HandleRecord::EMPTY;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    static TEST_BYTES: &[u8] = b"MZ-test-image";
    #[test]
    fn open_read_close_and_stale_handle() {
        let path = b"\\SystemRoot\\Tests\\fixture.dll";
        let _ = register_static(path, TEST_BYTES);
        let handle = open(path).unwrap();
        let mut buf = [0u8; 32];
        let n = read_at(handle, 0, &mut buf).unwrap();
        assert_eq!(&buf[..n], TEST_BYTES);
        assert_eq!(read_at(handle, TEST_BYTES.len(), &mut buf).unwrap(), 0);
        close(handle).unwrap();
        assert_eq!(read_at(handle, 0, &mut buf), Err(STATUS_INVALID_HANDLE));
    }
    #[test]
    fn missing_file_fails_closed() {
        assert_eq!(open(b"\\SystemRoot\\missing.dll"), Err(STATUS_OBJECT_NAME_NOT_FOUND));
    }
    #[test]
    fn handles_are_process_scoped() {
        let path = b"\\SystemRoot\\Tests\\owner-scope.dll";
        let _ = register_static(path, TEST_BYTES);
        let handle = open_for_process(41, path).unwrap();
        let mut out = [0u8; 4];
        assert_eq!(read_at_for_process(42, handle, 0, &mut out), Err(STATUS_INVALID_HANDLE));
        assert_eq!(read_at_for_process(41, handle, 0, &mut out), Ok(4));
        assert_eq!(close_for_process(42, handle), Err(STATUS_INVALID_HANDLE));
        close_for_process(41, handle).unwrap();
    }
}
