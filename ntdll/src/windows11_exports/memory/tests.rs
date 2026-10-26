#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_temporal_copy_copies_exact_range() {
        let source = [0x10u8, 0x20, 0x30, 0x40, 0x50];
        let mut destination = [0u8; 7];
        unsafe {
            RtlCopyMemoryNonTemporal(destination.as_mut_ptr().cast(), source.as_ptr().cast(), source.len());
        }
        assert_eq!(&destination[..5], &source);
        assert_eq!(&destination[5..], &[0, 0]);
    }

    #[test]
    fn non_temporal_fill_fills_exact_range() {
        let mut destination = [0u8; 6];
        unsafe { RtlFillMemoryNonTemporal(destination.as_mut_ptr().cast(), 4, 0xA5); }
        assert_eq!(destination, [0xA5, 0xA5, 0xA5, 0, 0, 0]);
    }

    #[test]
    fn non_temporal_helpers_accept_zero_length_null_ranges() {
        unsafe {
            RtlCopyMemoryNonTemporal(core::ptr::null_mut(), core::ptr::null(), 0);
            RtlFillMemoryNonTemporal(core::ptr::null_mut(), 0, 0xFF);
        }
    }

    #[test]
    fn memory_stream_initializes_reads_seeks_and_reports_size() {
        let mut stream = core::mem::MaybeUninit::<RTL_MEMORY_STREAM>::uninit();
        let mut source = *b"cognacos";
        unsafe {
            RtlInitMemoryStream(stream.as_mut_ptr());
            let stream = &mut *stream.as_mut_ptr();
            stream.start = source.as_mut_ptr().cast();
            stream.current = source.as_mut_ptr().cast();
            stream.end = source.as_mut_ptr().add(source.len()).cast();
            assert_eq!(RtlAddRefMemoryStream((stream as *mut RTL_MEMORY_STREAM).cast()), 1);
            let mut out = [0u8; 3];
            let mut read = 0;
            assert_eq!(RtlReadMemoryStream((stream as *mut RTL_MEMORY_STREAM).cast(), out.as_mut_ptr().cast(), 3, &mut read), S_OK);
            assert_eq!(read, 3);
            assert_eq!(&out, b"cog");
            let mut position = 0u64;
            assert_eq!(RtlSeekMemoryStream((stream as *mut RTL_MEMORY_STREAM).cast(), 2, STREAM_SEEK_SET, &mut position), S_OK);
            assert_eq!(position, 2);
            let input = *b"XYZ";
            let mut written = 0;
            assert_eq!(RtlWriteMemoryStream((stream as *mut RTL_MEMORY_STREAM).cast(), input.as_ptr().cast(), input.len() as ULONG, &mut written), S_OK);
            assert_eq!(written, 3);
            assert_eq!(&source[2..5], b"XYZ");
            let mut stat = core::mem::MaybeUninit::<StatStg>::zeroed();
            assert_eq!(RtlStatMemoryStream((stream as *mut RTL_MEMORY_STREAM).cast(), stat.as_mut_ptr().cast(), 1), S_OK);
            assert_eq!((*stat.as_ptr()).size, source.len() as u64);
        }
    }

    #[test]
    fn memory_zone_and_lookaside_abi_layouts_are_stable() {
        assert_eq!(core::mem::offset_of!(RTL_MEMORY_STREAM, current), 16);
        assert_eq!(core::mem::offset_of!(RTL_MEMORY_STREAM, start), 24);
        assert_eq!(core::mem::offset_of!(RTL_MEMORY_STREAM, end), 32);
        assert_eq!(core::mem::size_of::<RTL_MEMORY_STREAM>(), 56);
    }

    #[test]
    fn cm_memory_resource_descriptor_encodes_and_decodes() {
        let mut descriptor = core::mem::MaybeUninit::<CM_PARTIAL_RESOURCE_DESCRIPTOR>::zeroed();
        let mut start = 0u64;
        unsafe {
            assert_eq!(RtlCmEncodeMemIoResource(descriptor.as_mut_ptr().cast(), 4, 0x2000, 0x1234_0000), STATUS_SUCCESS);
            assert_eq!(RtlCmDecodeMemIoResource(descriptor.as_mut_ptr().cast(), &mut start), 0x2000);
            assert_eq!(start, 0x1234_0000);
            assert_eq!(RtlIoDecodeMemIoResource(descriptor.as_mut_ptr().cast(), &mut start), 0x2000);
            assert_eq!(RtlCmEncodeMemIoResource(descriptor.as_mut_ptr().cast(), 7, 0x2000, 0x1_0000_0000), STATUS_SUCCESS);
            assert_eq!(RtlCmDecodeMemIoResource(descriptor.as_mut_ptr().cast(), &mut start), 0x2000);
            assert_eq!(start, 0x1_0000_0000);
            assert_eq!(RtlCmEncodeMemIoResource(descriptor.as_mut_ptr().cast(), 7, 0x1_0000_0000, 0x1_0000_0000), STATUS_UNSUCCESSFUL);
            assert_eq!(RtlCmEncodeMemIoResource(descriptor.as_mut_ptr().cast(), 7, 0x1001, 0x1234), STATUS_UNSUCCESSFUL);
        }
    }

    #[test]
    fn secure_cache_callbacks_register_deregister_and_notify() {
        static CALLS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
        unsafe extern "system" fn callback(_base: PVOID, length: SIZE_T) {
            if length == 17 { CALLS.fetch_add(1, Ordering::SeqCst); }
        }
        let callback_ptr = callback as *const () as PVOID;
        unsafe {
            assert_eq!(RtlRegisterSecureMemoryCacheCallback(callback_ptr), STATUS_SUCCESS);
            assert_eq!(RtlRegisterSecureMemoryCacheCallback(callback_ptr), STATUS_SUCCESS);
            assert_eq!(RtlFlushSecureMemoryCache(1usize as PVOID, 17), 1);
            assert_eq!(RtlDeregisterSecureMemoryCacheCallback(callback_ptr), STATUS_SUCCESS);
            assert_eq!(RtlDeregisterSecureMemoryCacheCallback(callback_ptr), STATUS_INVALID_PARAMETER);
        }
        assert!(CALLS.load(Ordering::SeqCst) >= 1);
    }

    #[test]
    fn memory_basic_information_matches_windows_x64_layout() {
        assert_eq!(core::mem::size_of::<MEMORY_BASIC_INFORMATION>(), 48);
        assert_eq!(core::mem::offset_of!(MEMORY_BASIC_INFORMATION, region_size), 24);
        assert_eq!(core::mem::offset_of!(MEMORY_BASIC_INFORMATION, state), 32);
        assert_eq!(core::mem::offset_of!(MEMORY_BASIC_INFORMATION, kind), 40);
    }

    #[test]
    fn unsupported_vm_operations_fail_explicitly() {
        assert_eq!(
            unsafe { NtFlushVirtualMemory(core::ptr::null_mut(), core::ptr::null_mut(), core::ptr::null_mut(), core::ptr::null_mut()) },
            STATUS_NOT_IMPLEMENTED
        );
        assert_eq!(
            unsafe { NtLockVirtualMemory(core::ptr::null_mut(), core::ptr::null_mut(), core::ptr::null_mut(), 0) },
            STATUS_INVALID_PARAMETER
        );
    }
}
