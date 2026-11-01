#[unsafe(export_name = "IoGetRequestorSessionId")]
pub unsafe extern "win64" fn io_get_requestor_session_id(irp: *mut Irp, session_id: *mut u32) -> i32 {
    const STATUS_INVALID_PARAMETER: i32 = 0xC000_000D_u32 as i32;
    const STATUS_NOT_FOUND: i32 = 0xC000_0225_u32 as i32;
    if irp.is_null() || session_id.is_null() { return STATUS_INVALID_PARAMETER; }
    let pid = {
        let state = WDM.lock();
        let Some(slot) = state.irps.iter().find(|slot| slot.used && ptr::eq(&slot.object, irp)) else {
            return STATUS_NOT_FOUND;
        };
        slot.requestor_pid
    };
    if pid == 0 { return STATUS_NOT_FOUND; }
    let Some(session) = crate::process::session_id_for_pid(pid) else { return STATUS_NOT_FOUND; };
    unsafe { session_id.write(session); }
    STATUS_SUCCESS
}
