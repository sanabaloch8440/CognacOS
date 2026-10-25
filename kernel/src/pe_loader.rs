//! Transactional bridge between the staged VFS, PE32+ mapper, and per-process
//! module lifetime registry. This is a kernel-side prerequisite for the native
//! NTDLL loader; native section/map syscalls must call these APIs while holding
//! the owning process's address-space mutation/rundown contract.
use crate::ntoskrnl::mm::address_space::AddressSpace;
use crate::hal::IrqSpinLock;
use crate::pe::{ImportResolver, ImportSymbol, LoadedImage, PeError};

pub const STATUS_SUCCESS: i32 = 0;
pub const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
pub const STATUS_INVALID_IMAGE_FORMAT: i32 = 0xC000_007Bu32 as i32;
pub const STATUS_DLL_NOT_FOUND: i32 = 0xC000_0135u32 as i32;
pub const STATUS_BUFFER_TOO_SMALL: i32 = 0xC000_0023u32 as i32;
pub const STATUS_RESOURCE_IN_USE: i32 = 0xC000_0708u32 as i32;
pub const STATUS_DLL_INIT_FAILED: i32 = 0xC000_0142u32 as i32;

const FILE_CHUNK: usize = 4096;
static LOAD_UNLOAD_LOCK: IrqSpinLock<()> = IrqSpinLock::new(());
#[derive(Clone, Copy)]
struct ImportContext {
    active: bool,
    process_id: u64,
    parent_slot: usize,
    parent_generation: u32,
    resolver: Option<ImportResolver>,
}
impl ImportContext {
    const EMPTY: Self = Self { active: false, process_id: 0, parent_slot: 0, parent_generation: 0, resolver: None };
}
static IMPORT_CONTEXT: IrqSpinLock<ImportContext> = IrqSpinLock::new(ImportContext::EMPTY);

fn transaction_import_resolver(dll: &[u8], symbol: ImportSymbol<'_>) -> Option<u64> {
    let context = *IMPORT_CONTEXT.lock();
    if !context.active { return None; }
    let dependency_name = crate::winabi::api_set_host(dll).unwrap_or(dll);
    let (dependency_slot, dependency) = crate::pe_modules::find_for_process(context.process_id, dependency_name)?;
    if !matches!(dependency.state, crate::pe_modules::ModuleState::Mapped
        | crate::pe_modules::ModuleState::Initializing
        | crate::pe_modules::ModuleState::Initialized) { return None; }
    // Resolve against the exact module identity selected above (including
    // API-set host normalization), not the original contract string. Otherwise
    // an API-set import could accidentally bind an export from a different
    // module with a matching symbol name.
    let resolve = context.resolver.unwrap_or(crate::winabi::resolve_import);
    let loaded_name = &dependency.name[..dependency.name_len as usize];
    let address = resolve(loaded_name, symbol)?;
    crate::pe_modules::add_dependency(
        context.parent_slot, context.parent_generation, dependency_slot, dependency.generation,
    ).ok()?;
    Some(address)
}

#[derive(Clone, Copy, Debug)]
pub struct LoadResult {
    pub slot: usize,
    pub generation: u32,
    pub image: LoadedImage,
    pub already_loaded: bool,
}

/// Read and validate the entry-point RVA from a mapped AMD64 PE image. The
/// image header is copied through the owning address space rather than directly
/// dereferenced from a user virtual address.
fn mapped_entrypoint(space: &AddressSpace, base: u64, image_size: u32) -> Result<u64, i32> {
    const HEADER_BYTES: usize = 0x1000;
    if base == 0 || image_size < 0x1000 { return Err(STATUS_INVALID_IMAGE_FORMAT); }
    let mut header = [0u8; HEADER_BYTES];
    unsafe { space.copy_from_user(base, &mut header) }.map_err(|_| STATUS_INVALID_IMAGE_FORMAT)?;
    let u16_at = |offset: usize| -> Option<u16> {
        Some(u16::from_le_bytes(header.get(offset..offset.checked_add(2)?)?.try_into().ok()?))
    };
    let u32_at = |offset: usize| -> Option<u32> {
        Some(u32::from_le_bytes(header.get(offset..offset.checked_add(4)?)?.try_into().ok()?))
    };
    if u16_at(0) != Some(0x5a4d) { return Err(STATUS_INVALID_IMAGE_FORMAT); }
    let nt = u32_at(0x3c).ok_or(STATUS_INVALID_IMAGE_FORMAT)? as usize;
    if nt.checked_add(24 + 112).is_none_or(|end| end > header.len())
        || u32_at(nt) != Some(0x0000_4550)
        || u16_at(nt + 4) != Some(0x8664)
        || u16_at(nt + 24) != Some(0x20b) {
        return Err(STATUS_INVALID_IMAGE_FORMAT);
    }
    let optional_size = u16_at(nt + 4 + 16).ok_or(STATUS_INVALID_IMAGE_FORMAT)? as usize;
    if optional_size < 112 || nt.checked_add(24 + optional_size).is_none_or(|end| end > header.len()) {
        return Err(STATUS_INVALID_IMAGE_FORMAT);
    }
    let entry_rva = u32_at(nt + 24 + 16).ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
    if entry_rva == 0 { return Ok(0); }
    if entry_rva >= image_size { return Err(STATUS_INVALID_IMAGE_FORMAT); }
    base.checked_add(entry_rva as u64).ok_or(STATUS_INVALID_IMAGE_FORMAT)
}

/// Transition a mapped DLL to Initializing and return its validated entry point.
/// The caller must arrange a user-mode callout (DLL_PROCESS_ATTACH) on a thread
/// in this process; kernel code must never invoke the user entry point directly.
pub fn prepare_module_initialization(
    space: &AddressSpace,
    process_id: u64,
    module_name: &[u8],
) -> Result<(usize, u32, u64), i32> {
    if process_id == 0 || module_name.is_empty() { return Err(STATUS_INVALID_PARAMETER); }
    let (slot, snapshot) = crate::pe_modules::find_for_process(process_id, module_name)
        .ok_or(STATUS_DLL_NOT_FOUND)?;
    if snapshot.state != crate::pe_modules::ModuleState::Mapped {
        return Err(STATUS_RESOURCE_IN_USE);
    }
    let entry = mapped_entrypoint(space, snapshot.base, snapshot.size)?;
    crate::pe_modules::begin_initialization(slot, snapshot.generation)?;
    Ok((slot, snapshot.generation, entry))
}

/// Complete the user-mode DLL initialization callout. On failure, the mapped
/// image and its export/runtime registration are removed before the module
/// registry releases dependency references. On success, the module becomes
/// eligible for normal import resolution and unload rundown.
pub unsafe fn complete_module_initialization(
    space: &mut AddressSpace,
    process_id: u64,
    module_name: &[u8],
    slot: usize,
    generation: u32,
    success: bool,
) -> Result<(), i32> {
    let _transaction = LOAD_UNLOAD_LOCK.lock();
    let snapshot = crate::pe_modules::find_for_process(process_id, module_name)
        .map(|(_, entry)| entry).ok_or(STATUS_DLL_NOT_FOUND)?;
    if snapshot.generation != generation || snapshot.state != crate::pe_modules::ModuleState::Initializing {
        return Err(STATUS_INVALID_PARAMETER);
    }
    if success {
        return crate::pe_modules::finish_initialization(slot, generation, true);
    }
    let image = LoadedImage { base: snapshot.base, entry: 0, image_size: snapshot.size, is_dll: true };
    unsafe { crate::pe::unload_dll(space, image, module_name) }.map_err(pe_status)?;
    let _ = crate::pe_modules::finish_initialization(slot, generation, false);
    Err(STATUS_DLL_INIT_FAILED)
}

fn pe_status(_error: PeError) -> i32 { STATUS_INVALID_IMAGE_FORMAT }

/// Read an explicitly registered VFS file and load its PE32+ DLL into this
/// process. The caller supplies a bounded scratch buffer; no untrusted file size
/// can trigger an unbounded kernel allocation. Imports must be resolved by the
/// supplied resolver, which should bind only validated loaded-module exports.
pub unsafe fn load_dll_from_vfs(
    space: &mut AddressSpace,
    process_id: u64,
    path: &[u8],
    module_name: &[u8],
    scratch: &mut [u8],
    resolver: Option<ImportResolver>,
) -> Result<LoadResult, i32> {
    let _transaction = LOAD_UNLOAD_LOCK.lock();
    if process_id == 0 || path.is_empty() || module_name.is_empty() || scratch.is_empty() {
        return Err(STATUS_INVALID_PARAMETER);
    }
    if let Some((slot, existing)) = crate::pe_modules::find_for_process(process_id, module_name) {
        if matches!(existing.state, crate::pe_modules::ModuleState::Mapped
            | crate::pe_modules::ModuleState::Initializing
            | crate::pe_modules::ModuleState::Initialized) {
            crate::pe_modules::add_reference(slot, existing.generation)?;
            return Ok(LoadResult {
                slot, generation: existing.generation,
                image: LoadedImage { base: existing.base, entry: 0, image_size: existing.size, is_dll: true },
                already_loaded: true,
            });
        }
        return Err(STATUS_RESOURCE_IN_USE);
    }

    let handle = crate::file_services::open_for_process(process_id, path)?;
    let mut total = 0usize;
    let read_result = loop {
        if total == scratch.len() {
            let mut probe = [0u8; 1];
            match crate::file_services::read_at_for_process(process_id, handle, total, &mut probe) {
                Ok(0) => break Ok(()),
                Ok(_) => break Err(STATUS_BUFFER_TOO_SMALL),
                Err(status) => break Err(status),
            }
        }
        let amount = core::cmp::min(FILE_CHUNK, scratch.len() - total);
        match crate::file_services::read_at_for_process(process_id, handle, total, &mut scratch[total..total + amount]) {
            Ok(0) => break Ok(()),
            Ok(n) => {
                let Some(next) = total.checked_add(n) else { break Err(STATUS_INVALID_PARAMETER); };
                total = next;
            }
            Err(status) => break Err(status),
        }
    };
    let close_result = crate::file_services::close_for_process(process_id, handle);
    read_result?;
    close_result?;
    if total == 0 { return Err(STATUS_INVALID_IMAGE_FORMAT); }

    let (slot, generation) = crate::pe_modules::reserve_for_process(process_id, module_name)?;
    *IMPORT_CONTEXT.lock() = ImportContext {
        active: true, process_id, parent_slot: slot, parent_generation: generation, resolver,
    };
    let mapped = unsafe { crate::pe::load_dll(space, &scratch[..total], module_name, Some(transaction_import_resolver)) };
    *IMPORT_CONTEXT.lock() = ImportContext::EMPTY;
    let image = match mapped {
        Ok(image) => image,
        Err(error) => {
            let _ = crate::pe_modules::rollback(slot, generation);
            return Err(pe_status(error));
        }
    };
    if let Err(status) = crate::pe_modules::publish_mapping(slot, generation, image.base, image.image_size) {
        if unsafe { crate::pe::unload_dll(space, image, module_name) }.is_ok() {
            let _ = crate::pe_modules::rollback(slot, generation);
        }
        return Err(status);
    }
    Ok(LoadResult { slot, generation, image, already_loaded: false })
}

/// Drop one caller reference and unmap the image only when the module registry
/// issues the final unload ticket. Dependencies orphaned by this unload are then
/// drained using their own generation-checked tickets. This never invokes user
/// detach callbacks; those must finish before calling this kernel primitive.
pub unsafe fn unload_dll(
    space: &mut AddressSpace,
    process_id: u64,
    module_name: &[u8],
) -> Result<bool, i32> {
    let _transaction = LOAD_UNLOAD_LOCK.lock();
    let (slot, snapshot) = crate::pe_modules::find_for_process(process_id, module_name)
        .ok_or(STATUS_DLL_NOT_FOUND)?;
    let Some((base, size)) = crate::pe_modules::begin_unload(slot, snapshot.generation)? else {
        return Ok(false);
    };
    let image = LoadedImage { base, entry: 0, image_size: size, is_dll: true };
    unsafe { crate::pe::unload_dll(space, image, module_name) }.map_err(pe_status)?;
    crate::pe_modules::finish_unload(slot, snapshot.generation)?;

    for _ in 0..crate::pe_modules::MAX_MODULES {
        let Some((dependency_slot, dependency)) = crate::pe_modules::find_unreferenced_for_process(process_id) else { break; };
        let (dep_base, dep_size) = crate::pe_modules::begin_unreferenced_unload(dependency_slot, dependency.generation)?;
        let name = &dependency.name[..dependency.name_len as usize];
        let dep_image = LoadedImage { base: dep_base, entry: 0, image_size: dep_size, is_dll: true };
        unsafe { crate::pe::unload_dll(space, dep_image, name) }.map_err(pe_status)?;
        crate::pe_modules::finish_unload(dependency_slot, dependency.generation)?;
    }
    Ok(true)
}
