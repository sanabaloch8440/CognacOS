use core::ffi::c_void;
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_compare_memory(a:*const u8,b:*const u8,n:u64)->u64 {
    if n==0{return 0;}if a.is_null()||b.is_null(){return 0;}let mut i=0;while i<n&&unsafe{*a.add(i as usize)==*b.add(i as usize)}{i+=1;}i
}

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_compare_memory_ulong(a:*const u32,n:u64,value:u32)->u64 {
    if n==0{return 0;}if a.is_null(){return 0;}let mut i=0;while i<n/4&&unsafe{*a.add(i as usize)==value}{i+=1;}i*4
}

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_compute_crc32(initial:u32,buffer:*const u8,length:i32)->u32 {
    if buffer.is_null()||length<=0{return initial;}let mut crc=!initial;for i in 0..length as usize{crc^=unsafe{*buffer.add(i)} as u32;for _ in 0..8{crc=if crc&1!=0{(crc>>1)^0xedb8_8320}else{crc>>1};}}!crc
}

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_crc32(initial:u32,buffer:*const u8,length:i32)->u32 { unsafe{rtl_compute_crc32(initial,buffer,length)} }

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_copy_memory(dst:*mut c_void,src:*const c_void,n:u64) { if n>0&&!dst.is_null()&&!src.is_null(){unsafe{core::ptr::copy(src as *const u8,dst as *mut u8,n as usize);}} }

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_fill_memory(dst:*mut c_void,n:u64,value:u8) { if n>0&&!dst.is_null(){unsafe{core::ptr::write_bytes(dst as *mut u8,value,n as usize);}} }

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_copy_memory_non_temporal(dst:*mut c_void,src:*const c_void,n:u64) { unsafe { rtl_copy_memory(dst,src,n) } }

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_fill_memory_non_temporal(dst:*mut c_void,n:u64,value:u8) { unsafe { rtl_fill_memory(dst,n,value) } }

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_move_memory(dst:*mut c_void,src:*const c_void,n:u64) { if n>0&&!dst.is_null()&&!src.is_null(){unsafe{core::ptr::copy(src as *const u8,dst as *mut u8,n as usize);}} }

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_zero_memory(dst:*mut c_void,n:u64) { if n>0&&!dst.is_null(){unsafe{core::ptr::write_bytes(dst as *mut u8,0,n as usize);}} }