//! Typed accessors for the public x64 IRP and IO_STACK_LOCATION fields used
//! by the I/O manager. Keep ABI offsets confined to this file; dispatchers
//! should consume typed values rather than reaching into opaque IRP storage.

use core::ptr;

const IRP_IO_STATUS_STATUS_OFFSET: usize = 0x30;
const IRP_IO_STATUS_INFORMATION_OFFSET: usize = 0x38;
const IRP_USER_BUFFER_OFFSET: usize = 0x70;

/// Read IRP.IoStatus.Status from the x64 WDM packet.
pub unsafe fn io_get_irp_status(irp: *const Irp) -> i32 {
    if irp.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe {
        ptr::read_unaligned(
            ptr::addr_of!((*irp).bytes).cast::<u8>()
                .add(IRP_IO_STATUS_STATUS_OFFSET).cast::<i32>()
        )
    }
}

/// Read IRP.IoStatus.Information from the x64 WDM packet.
pub unsafe fn io_get_irp_information(irp: *const Irp) -> usize {
    if irp.is_null() { return 0; }
    unsafe {
        ptr::read_unaligned(
            ptr::addr_of!((*irp).bytes).cast::<u8>()
                .add(IRP_IO_STATUS_INFORMATION_OFFSET).cast::<usize>()
        )
    }
}

/// Return IRP.UserBuffer without exposing the opaque packet layout.
pub unsafe fn io_get_irp_user_buffer(irp: *const Irp) -> *mut c_void {
    if irp.is_null() { return ptr::null_mut(); }
    unsafe {
        ptr::read_unaligned(
            ptr::addr_of!((*irp).bytes).cast::<u8>()
                .add(IRP_USER_BUFFER_OFFSET).cast::<*mut c_void>()
        )
    }
}

/// Read a 32-bit member of the IO_STACK_LOCATION.Parameters union.
pub unsafe fn io_read_stack_u32(stack: *const IoStackLocation, offset: usize) -> u32 {
    if stack.is_null() || offset.checked_add(core::mem::size_of::<u32>()).map_or(true, |end| end > 0x20) {
        return 0;
    }
    unsafe { ptr::read_unaligned((*stack).parameters.as_ptr().add(offset).cast::<u32>()) }
}

/// Read a signed 64-bit member of the IO_STACK_LOCATION.Parameters union.
pub unsafe fn io_read_stack_i64(stack: *const IoStackLocation, offset: usize) -> i64 {
    if stack.is_null() || offset.checked_add(core::mem::size_of::<i64>()).is_none_or(|end| end > 0x20) {
        return 0;
    }
    unsafe { ptr::read_unaligned((*stack).parameters.as_ptr().add(offset).cast::<i64>()) }
}

/// Write a 32-bit member of the IO_STACK_LOCATION.Parameters union.
pub unsafe fn io_write_stack_u32(stack: *mut IoStackLocation, offset: usize, value: u32) -> bool {
    if stack.is_null() || offset.checked_add(core::mem::size_of::<u32>()).is_none_or(|end| end > 0x20) {
        return false;
    }
    unsafe { ptr::write_unaligned((*stack).parameters.as_mut_ptr().add(offset).cast::<u32>(), value); }
    true
}

/// Write a signed 64-bit member of the IO_STACK_LOCATION.Parameters union.
pub unsafe fn io_write_stack_i64(stack: *mut IoStackLocation, offset: usize, value: i64) -> bool {
    if stack.is_null() || offset.checked_add(core::mem::size_of::<i64>()).is_none_or(|end| end > 0x20) {
        return false;
    }
    unsafe { ptr::write_unaligned((*stack).parameters.as_mut_ptr().add(offset).cast::<i64>(), value); }
    true
}

/// Set IRP.UserBuffer without exposing the opaque packet layout.
pub unsafe fn io_set_irp_user_buffer(irp: *mut Irp, buffer: *mut c_void) -> bool {
    if irp.is_null() { return false; }
    unsafe {
        ptr::write_unaligned(
            ptr::addr_of_mut!((*irp).bytes).cast::<u8>()
                .add(IRP_USER_BUFFER_OFFSET).cast::<*mut c_void>(),
            buffer
        );
    }
    true
}
