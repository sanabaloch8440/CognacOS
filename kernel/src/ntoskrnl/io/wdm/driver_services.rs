// Opaque-pointer adapters keep the driver-facing ABI independent of the
// kernel's private Rust wrapper types while forwarding to the live WDM manager.
unsafe extern "win64" fn kernel_io_complete_irp(
    irp:*mut cognacos_nt_abi::io::IRP,priority_boost:i8
) { unsafe { crate::wdm::io_complete_request(irp.cast(),priority_boost); } }
unsafe extern "win64" fn kernel_io_create_device(
    driver:*mut core::ffi::c_void,extension_size:u32,
    name:*mut cognacos_nt_abi::object::UNICODE_STRING,device_type:u32,
    characteristics:u32,exclusive:u8,output:*mut *mut core::ffi::c_void
)->i32 {
    if output.is_null(){return cognacos_nt_abi::ntstatus::STATUS_INVALID_PARAMETER}
    let mut device=core::ptr::null_mut();
    let status=unsafe{crate::wdm::io_create_device(driver.cast(),extension_size,name,device_type,characteristics,exclusive,&mut device)};
    unsafe{output.write(device.cast())};status
}
unsafe extern "win64" fn kernel_io_delete_device(device:*mut core::ffi::c_void) {
    unsafe{crate::wdm::io_delete_device(device.cast());}
}
unsafe extern "win64" fn kernel_io_call_driver(device:*mut core::ffi::c_void,irp:*mut core::ffi::c_void)->i32 {
    unsafe{crate::wdm::io_call_driver(device.cast(),irp.cast())}
}
unsafe extern "win64" fn kernel_io_allocate_irp(stack_size:i8,charge_quota:u8)->*mut core::ffi::c_void {
    unsafe{crate::wdm::io_allocate_irp(stack_size,charge_quota).cast()}
}
unsafe extern "win64" fn kernel_io_current_stack(irp:*mut core::ffi::c_void)->*mut core::ffi::c_void {
    unsafe{crate::wdm::io_get_current_irp_stack_location(irp.cast()).cast()}
}
unsafe extern "win64" fn kernel_io_next_stack(irp:*mut core::ffi::c_void)->*mut core::ffi::c_void {
    unsafe{crate::wdm::io_get_next_irp_stack_location(irp.cast()).cast()}
}
unsafe extern "win64" fn kernel_io_complete_status(
    irp:*mut core::ffi::c_void,status:i32,information:usize,priority_boost:i8
) {
    unsafe{crate::wdm::io_complete_irp_status(irp.cast(),status,information,priority_boost);}
}
unsafe extern "win64" fn kernel_io_pin_buffer(
    buffer:*mut core::ffi::c_void,length:u32,write_access:u8,
    mdl_out:*mut *mut core::ffi::c_void,system_address_out:*mut *mut core::ffi::c_void
)->i32 {
    use cognacos_nt_abi::ntstatus::{STATUS_ACCESS_VIOLATION,STATUS_INSUFFICIENT_RESOURCES,STATUS_INVALID_PARAMETER,STATUS_SUCCESS};
    if buffer.is_null()||length==0||write_access>1||mdl_out.is_null()||system_address_out.is_null(){return STATUS_INVALID_PARAMETER}
    unsafe{mdl_out.write(core::ptr::null_mut());system_address_out.write(core::ptr::null_mut());}
    let mdl=unsafe{crate::wdm::io_allocate_mdl(buffer,length,0,0,core::ptr::null_mut())};
    if mdl.is_null(){return STATUS_INSUFFICIENT_RESOURCES}
    unsafe{crate::ntoskrnl::mm::runtime::mm_probe_and_lock_pages(mdl,0,if write_access!=0{1}else{0});}
    let flags=unsafe{core::ptr::read_unaligned(mdl.cast::<u8>().add(0x0a).cast::<i16>())};
    if flags&(0x0002|0x0004)==0 {
        unsafe{crate::wdm::io_free_mdl(mdl);}
        return STATUS_ACCESS_VIOLATION
    }
    let mapped=unsafe{crate::ntoskrnl::mm::runtime::mm_map_locked_pages_specify_cache(mdl,0,1,core::ptr::null_mut(),0,0)};
    if mapped.is_null(){
        unsafe{crate::ntoskrnl::mm::runtime::mm_unlock_pages(mdl);crate::wdm::io_free_mdl(mdl);}
        return STATUS_INSUFFICIENT_RESOURCES
    }
    unsafe{mdl_out.write(mdl);system_address_out.write(mapped);}
    STATUS_SUCCESS
}
unsafe extern "win64" fn kernel_io_unpin_buffer(mdl:*mut core::ffi::c_void,system_address:*mut core::ffi::c_void){
    if mdl.is_null(){return}
    unsafe{
        if !system_address.is_null(){crate::ntoskrnl::mm::runtime::mm_unmap_locked_pages(system_address,mdl);}
        let flags=core::ptr::read_unaligned(mdl.cast::<u8>().add(0x0a).cast::<i16>());
        if flags&0x0004==0{crate::ntoskrnl::mm::runtime::mm_unlock_pages(mdl);}
        crate::wdm::io_free_mdl(mdl);
    }
}

unsafe extern "win64" fn kernel_io_attach_device(source:*mut core::ffi::c_void,target:*mut core::ffi::c_void)->*mut core::ffi::c_void {
    unsafe{crate::wdm::io_attach_device_to_device_stack(source.cast(),target.cast()).cast()}
}
unsafe extern "win64" fn kernel_io_detach_device(target:*mut core::ffi::c_void) {
    unsafe{crate::wdm::io_detach_device(target.cast());}
}
#[repr(C)]
struct KernelDeviceRelations{
    count:u32,
    objects:[*mut crate::wdm::DeviceObject;64],
}
unsafe extern "win64" fn kernel_io_publish_bus_relations(
    bus:*mut core::ffi::c_void,children:*const *mut core::ffi::c_void,count:u32
)->i32 {
    if bus.is_null()||count>64||(count!=0&&children.is_null()){return cognacos_nt_abi::ntstatus::STATUS_INVALID_PARAMETER}
    let mut relations=KernelDeviceRelations{count,objects:[core::ptr::null_mut();64]};
    for i in 0..count as usize{
        let child=unsafe{core::ptr::read_unaligned(children.add(i))};
        if child.is_null(){return cognacos_nt_abi::ntstatus::STATUS_INVALID_PARAMETER}
        relations.objects[i]=child.cast();
    }
    unsafe{crate::wdm::IoPnPProcessBusRelations(bus.cast(),(&relations as *const KernelDeviceRelations).cast())}
}

unsafe extern "win64" fn kernel_io_set_completion(
    irp:*mut core::ffi::c_void,
    routine:Option<cognacos_hal_contract::IoCompletionRoutine>,
    context:*mut core::ffi::c_void,success:u8,error:u8,cancel:u8
){
    let callback:Option<crate::wdm::IoCompletionRoutine>=routine.map(|f|unsafe{
        core::mem::transmute::<cognacos_hal_contract::IoCompletionRoutine,crate::wdm::IoCompletionRoutine>(f)
    });
    unsafe{crate::wdm::io_set_completion_routine(irp.cast(),callback,context,success,error,cancel);}
}
unsafe extern "win64" fn kernel_io_set_cancel(
    irp:*mut core::ffi::c_void,
    routine:Option<cognacos_hal_contract::IoCancelRoutine>
)->Option<cognacos_hal_contract::IoCancelRoutine>{
    let callback:Option<crate::wdm::DriverCancelRoutine>=routine.map(|f|unsafe{
        core::mem::transmute::<cognacos_hal_contract::IoCancelRoutine,crate::wdm::DriverCancelRoutine>(f)
    });
    let old=unsafe{crate::wdm::io_set_cancel_routine(irp.cast(),callback)};
    old.map(|f|unsafe{core::mem::transmute::<crate::wdm::DriverCancelRoutine,cognacos_hal_contract::IoCancelRoutine>(f)})
}
unsafe extern "win64" fn kernel_io_cancel_irp(irp:*mut core::ffi::c_void)->u8{
    unsafe{crate::wdm::io_cancel_irp(irp.cast())}
}
unsafe extern "win64" fn kernel_io_mark_bus_pdo(device:*mut core::ffi::c_void)->i32{
    unsafe{crate::wdm::io_mark_bus_enumerated_pdo(device.cast())}
}
unsafe extern "win64" fn kernel_io_skip_current_stack(irp:*mut core::ffi::c_void) {
    unsafe{crate::wdm::io_skip_current_irp_stack_location(irp.cast());}
}
unsafe extern "win64" fn kernel_io_free_irp(irp:*mut core::ffi::c_void) {
    unsafe{crate::wdm::io_free_irp(irp.cast());}
}

/// Live WDM service table for native drivers. Every callback reaches the same
/// DEVICE_OBJECT/IRP tables used by the kernel's IoCallDriver/PnP manager.
pub fn driver_kernel_io_service_table()->cognacos_hal_contract::KernelIoServiceTable {
    cognacos_hal_contract::KernelIoServiceTable{
        size:core::mem::size_of::<cognacos_hal_contract::KernelIoServiceTable>() as u32,
        version:cognacos_hal_contract::KERNEL_IO_SERVICES_VERSION,
        complete_irp:Some(kernel_io_complete_irp),
        create_device:Some(kernel_io_create_device),
        delete_device:Some(kernel_io_delete_device),
        call_driver:Some(kernel_io_call_driver),
        allocate_irp:Some(kernel_io_allocate_irp),
        current_stack_location:Some(kernel_io_current_stack),
        next_stack_location:Some(kernel_io_next_stack),
        complete_irp_status:Some(kernel_io_complete_status),
        pin_buffer:Some(kernel_io_pin_buffer),
        unpin_buffer:Some(kernel_io_unpin_buffer),
        attach_device:Some(kernel_io_attach_device),
        detach_device:Some(kernel_io_detach_device),
        publish_bus_relations:Some(kernel_io_publish_bus_relations),
        skip_current_stack:Some(kernel_io_skip_current_stack),
        free_irp:Some(kernel_io_free_irp),
        mark_bus_enumerated_pdo:Some(kernel_io_mark_bus_pdo),
        set_completion_routine:Some(kernel_io_set_completion),
        set_cancel_routine:Some(kernel_io_set_cancel),
        cancel_irp:Some(kernel_io_cancel_irp),
    }
}
