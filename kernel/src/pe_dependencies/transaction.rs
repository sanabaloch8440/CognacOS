//! Iterative, bounded PE dependency transaction engine.
//!
//! The engine owns no heap memory and never recursively calls itself. The
//! backend adapts process VFS reads, existing-module lookup, PE mapping, edge
//! publication, and rollback to kernel services. Each DFS depth has a separate
//! caller-provided file buffer. Newly mapped modules are journaled and rolled
//! back in reverse order if any later step fails.
use super::{discover, DependencyPlan, MAX_MODULE_NAME};

pub const MAX_LOAD_DEPTH: usize = 16;
pub const MAX_TRANSACTION_MODULES: usize = 32;
pub const STATUS_DLL_NOT_FOUND: i32 = 0xC000_0135u32 as i32;
pub const STATUS_STACK_OVERFLOW: i32 = 0xC000_00FDu32 as i32;
pub const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
pub const STATUS_RESOURCE_IN_USE: i32 = 0xC000_0708u32 as i32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleName {
    bytes: [u8; MAX_MODULE_NAME],
    len: u8,
}
impl ModuleName {
    pub fn new(bytes: &[u8]) -> Result<Self, i32> {
        if bytes.is_empty() || bytes.len() >= MAX_MODULE_NAME
            || !bytes.iter().all(|b| b.is_ascii_alphanumeric() || matches!(*b, b'.' | b'_' | b'-' | b'+')) {
            return Err(STATUS_INVALID_PARAMETER);
        }
        let mut name = Self { bytes: [0; MAX_MODULE_NAME], len: bytes.len() as u8 };
        name.bytes[..bytes.len()].copy_from_slice(bytes);
        Ok(name)
    }
    pub fn as_bytes(&self) -> &[u8] { &self.bytes[..self.len as usize] }
    fn equals(&self, other: &Self) -> bool { self.as_bytes().eq_ignore_ascii_case(other.as_bytes()) }
}
#[derive(Clone, Copy)]
struct Frame {
    name: ModuleName,
    plan: DependencyPlan,
    next: usize,
    file_len: usize,
    loaded_handle: Option<u64>,
    waiting_child: Option<ModuleName>,
}
impl Frame {
    const EMPTY: Self = Self {
        name: ModuleName { bytes: [0; MAX_MODULE_NAME], len: 0 },
        plan: DependencyPlan::new(), next: 0, file_len: 0,
        loaded_handle: None, waiting_child: None,
    };
}
#[derive(Clone, Copy)]
struct JournalEntry { name: ModuleName, handle: u64 }
const EMPTY_JOURNAL: JournalEntry = JournalEntry {
    name: ModuleName { bytes: [0; MAX_MODULE_NAME], len: 0 }, handle: 0,
};

/// Platform adapter for the transaction engine. `map` is called only after
/// every dependency has been found or mapped. `link` must add the dependency
/// reference atomically; `rollback` must unmap and unregister one module and
/// release its outgoing dependency references. Existing modules are never
/// journaled and therefore are never rolled back by this transaction.
pub trait Backend {
    fn lookup(&mut self, name: &[u8]) -> Result<Option<u64>, i32>;
    fn read_file(&mut self, name: &[u8], output: &mut [u8]) -> Result<usize, i32>;
    fn map(&mut self, name: &[u8], file: &[u8]) -> Result<u64, i32>;
    fn link(&mut self, parent: u64, dependency: u64) -> Result<(), i32>;
    fn rollback(&mut self, name: &[u8], handle: u64);
}

fn rollback<B: Backend>(backend: &mut B, journal: &[JournalEntry], count: usize) {
    for entry in journal[..count].iter().rev() {
        backend.rollback(entry.name.as_bytes(), entry.handle);
    }
}

/// Load a root image after recursively resolving its dependency graph.
///
/// `root_file` is the already-read root image. `depth_buffers` must contain
/// one independent buffer per allowed DFS depth; no single buffer is reused
/// while its contents are still needed. The operation is all-or-nothing for
/// modules first mapped by this call. The returned handle is the root mapping.
pub fn load_transaction<B: Backend>(
    backend: &mut B,
    root_name: &[u8],
    root_file: &[u8],
    depth_buffers: &mut [&mut [u8]],
) -> Result<u64, i32> {
    if depth_buffers.is_empty() || depth_buffers.len() > MAX_LOAD_DEPTH {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let root = ModuleName::new(root_name)?;
    let root_plan = discover(root_file)?;
    let mut frames = [Frame::EMPTY; MAX_LOAD_DEPTH];
    let mut journal = [EMPTY_JOURNAL; MAX_TRANSACTION_MODULES];
    let mut journal_len = 0usize;
    let mut depth = 0usize;
    frames[0] = Frame { name: root, plan: root_plan, next: 0, file_len: root_file.len(), loaded_handle: None, waiting_child: None };
    let mut result = None;

    loop {
        let frame = frames[depth];
        if frame.next < frame.plan.len() {
            let dep = frame.plan.get(frame.next).ok_or(STATUS_INVALID_PARAMETER)?;
            let child = ModuleName::new(dep.as_bytes())?;
            frames[depth].next += 1;
            match backend.lookup(child.as_bytes()) {
                Ok(Some(_)) => continue,
                Ok(None) => {}
                Err(e) => {
                    rollback(backend, &journal, journal_len);
                    return Err(e);
                }
            }

            // An import that names an ancestor is a cycle, not a request to
            // recursively map another copy of the same DLL.
            if frames[..=depth].iter().any(|f| f.name.len != 0 && f.name.equals(&child)) {
                rollback(backend, &journal, journal_len);
                return Err(STATUS_RESOURCE_IN_USE);
            }
            if depth + 1 >= depth_buffers.len() {
                rollback(backend, &journal, journal_len);
                return Err(STATUS_STACK_OVERFLOW);
            }
            let next_depth = depth + 1;
            let file_len = match backend.read_file(child.as_bytes(), depth_buffers[next_depth]) {
                Ok(n) if n > 0 && n <= depth_buffers[next_depth].len() => n,
                Ok(_) => { rollback(backend, &journal, journal_len); return Err(STATUS_DLL_NOT_FOUND); }
                Err(e) => { rollback(backend, &journal, journal_len); return Err(e); }
            };
            let plan = match discover(&depth_buffers[next_depth][..file_len]) {
                Ok(p) => p,
                Err(e) => { rollback(backend, &journal, journal_len); return Err(e); }
            };
            frames[next_depth] = Frame { name: child, plan, next: 0, file_len, loaded_handle: None, waiting_child: Some(frame.name) };
            depth = next_depth;
            continue;
        }

        // Dependencies have been processed. Map this image only now.
        let name = frames[depth].name;
        let file: &[u8] = if depth == 0 { root_file } else {
            &depth_buffers[depth][..frames[depth].file_len]
        };
        let handle = match backend.map(name.as_bytes(), file) {
            Ok(h) => h,
            Err(e) => { rollback(backend, &journal, journal_len); return Err(e); }
        };
        if journal_len == journal.len() {
            backend.rollback(name.as_bytes(), handle);
            rollback(backend, &journal, journal_len);
            return Err(super::STATUS_BUFFER_TOO_SMALL);
        }
        journal[journal_len] = JournalEntry { name, handle };
        journal_len += 1;

        // All imports of this image are now mapped. Publish dependency edges
        // only after mapping succeeds, so a failed edge publication triggers
        // rollback of the whole newly-created graph.
        for i in 0..frames[depth].plan.len() {
            let dep = frames[depth].plan.get(i).ok_or(STATUS_INVALID_PARAMETER)?;
            let dependency_handle = match backend.lookup(dep.as_bytes()) {
                Ok(Some(h)) => h,
                Ok(None) => {
                    rollback(backend, &journal, journal_len);
                    return Err(STATUS_DLL_NOT_FOUND);
                }
                Err(e) => {
                    rollback(backend, &journal, journal_len);
                    return Err(e);
                }
            };
            if let Err(e) = backend.link(handle, dependency_handle) {
                rollback(backend, &journal, journal_len);
                return Err(e);
            }
        }

        if depth == 0 {
            result = Some(handle);
            break;
        }
        depth -= 1;
    }
    result.ok_or(STATUS_DLL_NOT_FOUND)
}
