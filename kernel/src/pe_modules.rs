//! Bounded per-process PE module lifetime registry.
//!
//! This tracks module identity, dependency edges, reference ownership and
//! initialization transitions. Mapping/unmapping remains the responsibility of
//! the owning process address-space manager; callers must not free an image until
//! begin_unload returns its unique unload ticket.
use crate::hal::IrqSpinLock;

pub const MAX_MODULES: usize = 32;
pub const MAX_NAME_BYTES: usize = 96;
pub const STATUS_SUCCESS: i32 = 0;
pub const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
pub const STATUS_OBJECT_NAME_COLLISION: i32 = 0xC000_0035u32 as i32;
pub const STATUS_DLL_NOT_FOUND: i32 = 0xC000_0135u32 as i32;
pub const STATUS_RESOURCE_IN_USE: i32 = 0xC000_0708u32 as i32;
pub const STATUS_INSUFFICIENT_RESOURCES: i32 = 0xC000_009Au32 as i32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleState { Empty, Reserved, Mapped, Initializing, Initialized, Unloading }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleSnapshot {
    pub process_id: u64, pub base: u64, pub size: u32, pub name: [u8; MAX_NAME_BYTES], pub name_len: u8,
    pub references: u32, pub dependencies: u32, pub state: ModuleState, pub generation: u32,
}
impl ModuleSnapshot {
    const EMPTY: Self = Self { process_id: 0, base: 0, size: 0, name: [0; MAX_NAME_BYTES], name_len: 0,
        references: 0, dependencies: 0, state: ModuleState::Empty, generation: 0 };
}
struct State { entries: [ModuleSnapshot; MAX_MODULES], next_generation: u32 }
impl State { const fn new() -> Self { Self { entries: [ModuleSnapshot::EMPTY; MAX_MODULES], next_generation: 1 } } }
static STATE: IrqSpinLock<State> = IrqSpinLock::new(State::new());

fn fold(b: u8) -> u8 { if (b'A'..=b'Z').contains(&b) { b + 32 } else { b } }
fn same_name(a: &ModuleSnapshot, name: &[u8]) -> bool {
    a.state != ModuleState::Empty && a.name_len as usize == name.len()
        && a.name[..name.len()].iter().zip(name).all(|(x,y)| fold(*x)==fold(*y))
}
fn valid_name(name: &[u8]) -> bool {
    !name.is_empty() && name.len() < MAX_NAME_BYTES
        && name.iter().all(|b| *b != 0 && *b != b'/' && *b != b'\\')
}

/// Reserve a module identity before mapping. Duplicate loads return the existing
/// slot with STATUS_OBJECT_NAME_COLLISION so callers can add a reference instead.
pub fn reserve(name: &[u8]) -> Result<(usize, u32), i32> { reserve_for_process(0, name) }
pub fn reserve_for_process(process_id: u64, name: &[u8]) -> Result<(usize, u32), i32> {
    if !valid_name(name) { return Err(STATUS_INVALID_PARAMETER); }
    let mut s = STATE.lock();
    if s.entries.iter().any(|e| e.process_id == process_id && same_name(e, name)) { return Err(STATUS_OBJECT_NAME_COLLISION); }
    let slot = s.entries.iter().position(|e| e.state == ModuleState::Empty)
        .ok_or(STATUS_INSUFFICIENT_RESOURCES)?;
    let generation = s.next_generation;
    s.next_generation = s.next_generation.wrapping_add(1).max(1);
    let mut e = ModuleSnapshot::EMPTY;
    e.process_id = process_id; e.name[..name.len()].copy_from_slice(name); e.name_len = name.len() as u8;
    e.references = 1; e.state = ModuleState::Reserved; e.generation = generation;
    s.entries[slot] = e;
    Ok((slot, generation))
}

/// Publish an already mapped PE image. Invalid sizes or stale reservation tokens
/// fail without altering another module's metadata.
pub fn publish_mapping(slot: usize, generation: u32, base: u64, size: u32) -> Result<(), i32> {
    if base == 0 || size == 0 || base.checked_add(size as u64).is_none() { return Err(STATUS_INVALID_PARAMETER); }
    let mut s = STATE.lock();
    let e = s.entries.get_mut(slot).ok_or(STATUS_INVALID_PARAMETER)?;
    if e.state != ModuleState::Reserved || e.generation != generation { return Err(STATUS_INVALID_PARAMETER); }
    e.base = base; e.size = size; e.state = ModuleState::Mapped; Ok(())
}

/// Register the result of the existing PE32+ mapper in the lifetime table.
pub fn register_loaded_image(process_id: u64, name: &[u8], image: crate::pe::LoadedImage) -> Result<(usize, u32), i32> {
    if !image.is_dll { return Err(STATUS_INVALID_PARAMETER); }
    let (slot, generation) = reserve_for_process(process_id, name)?;
    if let Err(status) = publish_mapping(slot, generation, image.base, image.image_size) {
        let _ = rollback(slot, generation);
        return Err(status);
    }
    Ok((slot, generation))
}

pub fn begin_initialization(slot: usize, generation: u32) -> Result<(), i32> {
    let mut s = STATE.lock(); let e = s.entries.get_mut(slot).ok_or(STATUS_INVALID_PARAMETER)?;
    if e.generation != generation || e.state != ModuleState::Mapped { return Err(STATUS_INVALID_PARAMETER); }
    e.state = ModuleState::Initializing; Ok(())
}
pub fn finish_initialization(slot: usize, generation: u32, success: bool) -> Result<(), i32> {
    let mut s = STATE.lock(); let e = s.entries.get_mut(slot).ok_or(STATUS_INVALID_PARAMETER)?;
    if e.generation != generation || e.state != ModuleState::Initializing { return Err(STATUS_INVALID_PARAMETER); }
    if success { e.state = ModuleState::Initialized; Ok(()) }
    else {
        let dependencies = e.dependencies;
        for dependency in 0..MAX_MODULES {
            if dependencies & (1u32 << dependency) != 0 && s.entries[dependency].state != ModuleState::Empty {
                s.entries[dependency].references = s.entries[dependency].references.saturating_sub(1);
            }
        }
        s.entries[slot] = ModuleSnapshot::EMPTY;
        Err(STATUS_DLL_NOT_FOUND)
    }
}
pub fn add_reference(slot: usize, generation: u32) -> Result<u32, i32> {
    let mut s = STATE.lock(); let e = s.entries.get_mut(slot).ok_or(STATUS_INVALID_PARAMETER)?;
    if e.generation != generation || !matches!(e.state, ModuleState::Mapped | ModuleState::Initializing | ModuleState::Initialized) {
        return Err(STATUS_INVALID_PARAMETER);
    }
    e.references = e.references.checked_add(1).ok_or(STATUS_RESOURCE_IN_USE)?; Ok(e.references)
}

/// Record a dependency edge and take exactly one reference on the dependency.
pub fn add_dependency(parent: usize, parent_generation: u32, dependency: usize, dependency_generation: u32) -> Result<(), i32> {
    if parent == dependency || parent >= MAX_MODULES || dependency >= MAX_MODULES { return Err(STATUS_INVALID_PARAMETER); }
    let mut s = STATE.lock();
    let parent_entry = s.entries[parent];
    let dep_entry = s.entries[dependency];
    if parent_entry.generation != parent_generation || parent_entry.state == ModuleState::Empty
        || dep_entry.generation != dependency_generation || parent_entry.process_id != dep_entry.process_id
        || !matches!(dep_entry.state, ModuleState::Mapped | ModuleState::Initializing | ModuleState::Initialized) {
        return Err(STATUS_DLL_NOT_FOUND);
    }
    let bit = 1u32 << dependency;
    if s.entries[parent].dependencies & bit != 0 { return Ok(()); }
    // Check overflow before publishing the edge so failure is transactional.
    let references = s.entries[dependency].references.checked_add(1).ok_or(STATUS_RESOURCE_IN_USE)?;
    s.entries[dependency].references = references;
    s.entries[parent].dependencies |= bit;
    Ok(())
}

/// Drop one caller reference. Returns Some(base,size) only when this was the
/// final reference and no other module depends on it; Ok(None) means the
/// reference was released but the mapping must remain alive.
pub fn begin_unload(slot: usize, generation: u32) -> Result<Option<(u64, u32)>, i32> {
    let mut s = STATE.lock();
    let e = s.entries.get(slot).copied().ok_or(STATUS_INVALID_PARAMETER)?;
    if e.generation != generation || e.state == ModuleState::Empty || e.state == ModuleState::Reserved
        || e.state == ModuleState::Unloading || e.references == 0 { return Err(STATUS_INVALID_PARAMETER); }
    if e.references > 1 {
        s.entries[slot].references -= 1;
        return Ok(None);
    }
    if s.entries.iter().any(|other| other.state != ModuleState::Empty && other.dependencies & (1u32 << slot) != 0) {
        // Dependency edges each own a reference. Seeing a live dependency with
        // only one total reference means metadata is inconsistent; do not lose
        // the last unloadable reference or strand a zero-ref mapped image.
        return Err(STATUS_RESOURCE_IN_USE);
    }
    s.entries[slot].references = 0;
    s.entries[slot].state = ModuleState::Unloading;
    Ok(Some((e.base, e.size)))
}

/// Find a module whose last caller/dependency reference was released.
/// The loader should repeatedly drain this queue after finishing a parent unload.
pub fn find_unreferenced_for_process(process_id: u64) -> Option<(usize, ModuleSnapshot)> {
    let s = STATE.lock();
    s.entries.iter().enumerate().find(|(slot, e)| {
        e.process_id == process_id
            && matches!(e.state, ModuleState::Mapped | ModuleState::Initialized)
            && e.references == 0
            && !s.entries.iter().any(|parent| parent.state != ModuleState::Empty
                && parent.dependencies & (1u32 << *slot) != 0)
    }).map(|(slot, entry)| (slot, *entry))
}

/// Begin unloading a module made unreferenced when a parent dependency edge was
/// removed. This is separate from begin_unload because it consumes no caller ref.
pub fn begin_unreferenced_unload(slot: usize, generation: u32) -> Result<(u64, u32), i32> {
    let mut s = STATE.lock();
    let e = s.entries.get(slot).copied().ok_or(STATUS_INVALID_PARAMETER)?;
    if e.generation != generation || !matches!(e.state, ModuleState::Mapped | ModuleState::Initialized)
        || e.references != 0
        || s.entries.iter().any(|parent| parent.state != ModuleState::Empty
            && parent.dependencies & (1u32 << slot) != 0) {
        return Err(STATUS_RESOURCE_IN_USE);
    }
    s.entries[slot].state = ModuleState::Unloading;
    Ok((e.base, e.size))
}

/// Complete unmapping after the address-space manager has removed the image.
pub fn finish_unload(slot: usize, generation: u32) -> Result<(), i32> {
    let mut s = STATE.lock();
    let e = s.entries.get(slot).copied().ok_or(STATUS_INVALID_PARAMETER)?;
    if e.generation != generation || e.state != ModuleState::Unloading { return Err(STATUS_INVALID_PARAMETER); }
    let dependencies = e.dependencies;
    for dependency in 0..MAX_MODULES {
        if dependencies & (1u32 << dependency) != 0 && s.entries[dependency].state != ModuleState::Empty {
            s.entries[dependency].references = s.entries[dependency].references.saturating_sub(1);
        }
    }
    s.entries[slot] = ModuleSnapshot::EMPTY; Ok(())
}

/// Roll back a reservation or failed mapping before it is visible to callers.
pub fn rollback(slot: usize, generation: u32) -> Result<(), i32> {
    let mut s = STATE.lock(); let e = s.entries.get_mut(slot).ok_or(STATUS_INVALID_PARAMETER)?;
    if e.generation != generation || !matches!(e.state, ModuleState::Reserved | ModuleState::Mapped) {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let dependencies = e.dependencies;
    for dependency in 0..MAX_MODULES {
        if dependencies & (1u32 << dependency) != 0 && s.entries[dependency].state != ModuleState::Empty {
            s.entries[dependency].references = s.entries[dependency].references.saturating_sub(1);
        }
    }
    s.entries[slot] = ModuleSnapshot::EMPTY;
    Ok(())
}
pub const STATUS_INVALID_IMAGE_FORMAT: i32 = 0xC000_007Bu32 as i32;
pub const STATUS_BUFFER_TOO_SMALL: i32 = 0xC000_0023u32 as i32;
pub const STATUS_OBJECT_NAME_NOT_FOUND: i32 = 0xC000_0034u32 as i32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleLoadResult {
    pub slot: usize,
    pub generation: u32,
    pub base: u64,
    pub size: u32,
    /// False when an existing module was found and its reference was incremented.
    pub newly_mapped: bool,
}

/// Kernel-side VFS -> PE mapper -> module-registry transaction. The caller must
/// supply the current process's live address space and a bounded scratch buffer.
/// This is the core service the future NtMapViewOfSection/NTDLL bridge should call;
/// it deliberately does not execute DllMain or publish PEB loader-list entries.
pub unsafe fn load_dll_from_vfs(
    process_id: u64,
    space: &mut crate::ntoskrnl::mm::address_space::AddressSpace,
    path: &[u8],
    module_name: &[u8],
    file_buffer: &mut [u8],
    resolver: Option<crate::pe::ImportResolver>,
) -> Result<ModuleLoadResult, i32> {
    if path.is_empty() || module_name.is_empty() || file_buffer.is_empty() {
        return Err(STATUS_INVALID_PARAMETER);
    }
    if let Some((slot, snapshot)) = find_for_process(process_id, module_name) {
        if !matches!(snapshot.state, ModuleState::Mapped | ModuleState::Initializing | ModuleState::Initialized) {
            return Err(STATUS_RESOURCE_IN_USE);
        }
        let references = add_reference(slot, snapshot.generation)?;
        let _ = references;
        return Ok(ModuleLoadResult { slot, generation: snapshot.generation, base: snapshot.base, size: snapshot.size, newly_mapped: false });
    }

    let (slot, generation) = match reserve_for_process(process_id, module_name) {
        Ok(token) => token,
        Err(status) if status == STATUS_OBJECT_NAME_COLLISION => {
            let (slot, snapshot) = find_for_process(process_id, module_name).ok_or(status)?;
            add_reference(slot, snapshot.generation)?;
            return Ok(ModuleLoadResult { slot, generation: snapshot.generation, base: snapshot.base, size: snapshot.size, newly_mapped: false });
        }
        Err(status) => return Err(status),
    };

    let handle = match crate::file_services::open_for_process(process_id, path) {
        Ok(handle) => handle,
        Err(status) => { let _ = rollback(slot, generation); return Err(status); }
    };
    let mut total = 0usize;
    let read_result = loop {
        if total == file_buffer.len() {
            let mut probe = [0u8; 1];
            match crate::file_services::read_at_for_process(process_id, handle, total, &mut probe) {
                Ok(0) => break Ok(()),
                Ok(_) => break Err(STATUS_BUFFER_TOO_SMALL),
                Err(status) => break Err(status),
            }
        }
        let end = core::cmp::min(file_buffer.len(), total.saturating_add(4096));
        match crate::file_services::read_at_for_process(process_id, handle, total, &mut file_buffer[total..end]) {
            Ok(0) => break Ok(()),
            Ok(count) => {
                if count > end - total { break Err(STATUS_INVALID_IMAGE_FORMAT); }
                total += count;
            }
            Err(status) => break Err(status),
        }
    };
    let _ = crate::file_services::close_for_process(process_id, handle);
    if let Err(status) = read_result {
        let _ = rollback(slot, generation);
        return Err(status);
    }
    if total == 0 {
        let _ = rollback(slot, generation);
        return Err(STATUS_INVALID_IMAGE_FORMAT);
    }

    let image = match unsafe { crate::pe::load_dll(space, &file_buffer[..total], module_name, resolver) } {
        Ok(image) => image,
        Err(error) => {
            let _ = rollback(slot, generation);
            return Err(match error {
                crate::pe::PeError::OutOfMemory => STATUS_INSUFFICIENT_RESOURCES,
                _ => STATUS_INVALID_IMAGE_FORMAT,
            });
        }
    };
    if let Err(status) = publish_mapping(slot, generation, image.base, image.image_size) {
        let _ = unsafe { crate::pe::unload_dll(space, image, module_name) };
        let _ = rollback(slot, generation);
        return Err(status);
    }
    Ok(ModuleLoadResult { slot, generation, base: image.base, size: image.image_size, newly_mapped: true })
}

pub fn find(name: &[u8]) -> Option<(usize, ModuleSnapshot)> { find_for_process(0, name) }
pub fn find_for_process(process_id: u64, name: &[u8]) -> Option<(usize, ModuleSnapshot)> {
    let s = STATE.lock();
    s.entries.iter().enumerate().find(|(_,e)| e.process_id == process_id && same_name(e,name)).map(|(i,e)|(i,*e))
}
pub fn release_reference(slot: usize, generation: u32) -> Result<u32, i32> {
    let mut s = STATE.lock(); let e = s.entries.get_mut(slot).ok_or(STATUS_INVALID_PARAMETER)?;
    if e.generation != generation || e.state == ModuleState::Empty || e.references <= 1 { return Err(STATUS_RESOURCE_IN_USE); }
    e.references -= 1; Ok(e.references)
}

/// Deterministic boot-time exercise of the metadata lifecycle (no PE code is executed).
pub fn self_test() -> bool {
    let name = b"cognacos-kernel-lifetime-smoke.dll";
    let Ok((slot, generation)) = reserve_for_process(0xffff_fffe, name) else { return false; };
    if reserve_for_process(0xffff_fffe, name).is_ok() { let _ = rollback(slot, generation); return false; }
    if publish_mapping(slot, generation, 0x0000_4000_0000, 0x3000).is_err()
        || begin_initialization(slot, generation).is_err()
        || finish_initialization(slot, generation, true).is_err()
        || add_reference(slot, generation) != Ok(2)
        || release_reference(slot, generation) != Ok(1) {
        let _ = rollback(slot, generation);
        return false;
    }
    match begin_unload(slot, generation) {
        Ok(Some((0x0000_4000_0000, 0x3000))) => finish_unload(slot, generation).is_ok(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_load_and_reference_lifetime() {
        let name = b"cognac-test-duplicate.dll";
        let (slot, generation) = reserve(name).unwrap();
        assert_eq!(reserve(name), Err(STATUS_OBJECT_NAME_COLLISION));
        publish_mapping(slot, generation, 0x4000_0000, 0x3000).unwrap();
        begin_initialization(slot, generation).unwrap();
        finish_initialization(slot, generation, true).unwrap();
        assert_eq!(add_reference(slot, generation), Ok(2));
        assert_eq!(begin_unload(slot, generation), Ok(None));
        assert_eq!(find(name).unwrap().1.references, 1);
        assert_eq!(begin_unload(slot, generation), Ok(Some((0x4000_0000, 0x3000))));
        finish_unload(slot, generation).unwrap();
        assert!(find(name).is_none());
    }
    #[test]
    fn failed_initialization_rolls_back_module() {
        let name = b"cognac-test-init-failure.dll";
        let (slot, generation) = reserve(name).unwrap();
        publish_mapping(slot, generation, 0x5000_0000, 0x2000).unwrap();
        begin_initialization(slot, generation).unwrap();
        assert_eq!(finish_initialization(slot, generation, false), Err(STATUS_DLL_NOT_FOUND));
        assert!(find(name).is_none());
    }
    #[test]
    fn dependency_reference_releases_after_parent_unload() {
        let (parent, pg) = reserve_for_process(0xffff_fffd, b"cognac-test-parent-lifetime.dll").unwrap();
        let (dependency, dg) = reserve_for_process(0xffff_fffd, b"cognac-test-dependency-lifetime.dll").unwrap();
        publish_mapping(parent, pg, 0x6100_0000, 0x2000).unwrap();
        publish_mapping(dependency, dg, 0x6200_0000, 0x2000).unwrap();
        begin_initialization(parent, pg).unwrap();
        finish_initialization(parent, pg, true).unwrap();
        begin_initialization(dependency, dg).unwrap();
        finish_initialization(dependency, dg, true).unwrap();
        add_dependency(parent, pg, dependency, dg).unwrap();
        assert_eq!(find_for_process(0xffff_fffd, b"cognac-test-dependency-lifetime.dll").unwrap().1.references, 2);
        assert_eq!(begin_unload(parent, pg), Ok(Some((0x6100_0000, 0x2000))));
        finish_unload(parent, pg).unwrap();
        let (candidate, snapshot) = find_unreferenced_for_process(0xffff_fffd).unwrap();
        assert_eq!(candidate, dependency);
        assert_eq!(snapshot.references, 0);
        assert_eq!(begin_unreferenced_unload(dependency, dg), Ok((0x6200_0000, 0x2000)));
        finish_unload(dependency, dg).unwrap();
    }

    #[test]
    fn unresolved_dependency_is_rejected() {
        let (parent, pg) = reserve(b"cognac-test-parent.dll").unwrap();
        publish_mapping(parent, pg, 0x6000_0000, 0x1000).unwrap();
        assert_eq!(add_dependency(parent, pg, 31, 99), Err(STATUS_DLL_NOT_FOUND));
    }
}
