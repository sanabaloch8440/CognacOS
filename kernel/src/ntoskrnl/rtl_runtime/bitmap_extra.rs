use super::RtlBitmap;
#[inline] unsafe fn read_bit(b:*const RtlBitmap,i:u32)->bool {if b.is_null()||i>=unsafe{(*b).size_of_bitmap}||unsafe{(*b).buffer.is_null()}{false}else{unsafe{(*(*b).buffer.add((i/32) as usize)&(1u32<<(i%32)))!=0}}}
#[inline] unsafe fn write_bit(b:*mut RtlBitmap,i:u32,v:bool){if b.is_null()||i>=unsafe{(*b).size_of_bitmap}||unsafe{(*b).buffer.is_null()}{return;}let p=unsafe{(*b).buffer.add((i/32) as usize)};let m=1u32<<(i%32);unsafe{if v{*p|=m}else{*p&=!m}}}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_copy_bit_map(dst:*mut RtlBitmap,src:*const RtlBitmap) {
 if dst.is_null()||src.is_null(){return;}let n=unsafe{(*dst).size_of_bitmap.min((*src).size_of_bitmap)};for i in 0..n{let v=unsafe{read_bit(src,i)};unsafe{write_bit(dst,i,v);}}
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_intersect_bit_maps(dst:*mut RtlBitmap,a:*const RtlBitmap,b:*const RtlBitmap) {
 if dst.is_null()||a.is_null()||b.is_null(){return;}let n=unsafe{(*dst).size_of_bitmap.min((*a).size_of_bitmap).min((*b).size_of_bitmap)};for i in 0..n{let v=unsafe{read_bit(a,i)&&read_bit(b,i)};unsafe{write_bit(dst,i,v);}}
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_merge_bit_maps(dst:*mut RtlBitmap,a:*const RtlBitmap,b:*const RtlBitmap,merge_type:u32) {
 if dst.is_null()||a.is_null()||b.is_null(){return;}let n=unsafe{(*dst).size_of_bitmap.min((*a).size_of_bitmap).min((*b).size_of_bitmap)};for i in 0..n{let x=unsafe{read_bit(a,i)};let y=unsafe{read_bit(b,i)};let v=match merge_type{0=>x|y,1=>x^y,2=>x&!y,3=>x&y,_=>x|y};unsafe{write_bit(dst,i,v);}}
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_shift_left_bit_map(bitmap:*mut RtlBitmap,shift:u32) {
 if bitmap.is_null(){return;}let n=unsafe{(*bitmap).size_of_bitmap};if n==0{return;}if shift>=n{for i in 0..n{unsafe{write_bit(bitmap,i,false);}}return;}for i in (0..n).rev(){let v=if i>=shift{unsafe{read_bit(bitmap,i-shift)}}else{false};unsafe{write_bit(bitmap,i,v);}}
}

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_copy_bit_map_ex(dst:*mut RtlBitmap,src:*const RtlBitmap,_start:u32,_count:u32) {
 if dst.is_null()||src.is_null(){return;}let n=unsafe{(*dst).size_of_bitmap.min((*src).size_of_bitmap)};for i in 0..n{let v=unsafe{read_bit(src,i)};unsafe{write_bit(dst,i,v);}}
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_extract_bit_map_ex(dst:*mut super::RtlBitmapEx,src:*const super::RtlBitmapEx,start:usize,count:usize) {
 if dst.is_null()||src.is_null(){return;}let n=count.min(unsafe{(*dst).size_of_bitmap});
 for i in 0..n {if unsafe{(*dst).buffer.is_null()}{return;}let idx=start.checked_add(i);let v=idx.is_some_and(|s|s<unsafe{(*src).size_of_bitmap}&&!unsafe{(*src).buffer.is_null()}&&unsafe{*(*src).buffer.add(s/32)&(1u32<<(s%32))!=0});let p=unsafe{(*dst).buffer.add(i/32)};let m=1u32<<(i%32);unsafe{if v{*p|=m}else{*p&=!m}}}
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_extract_bit_map(
    dst: *mut RtlBitmap,
    src: *const RtlBitmap,
    start: u32,
    count: u32,
) {
    if dst.is_null() || src.is_null() {
        return;
    }
    let dst_size = unsafe { (*dst).size_of_bitmap };
    let src_size = unsafe { (*src).size_of_bitmap };
    let n = count.min(dst_size);
    if n > 0 && (unsafe { (*dst).buffer.is_null() } || unsafe { (*src).buffer.is_null() }) {
        return;
    }
    for i in 0..n {
        let source_index = start.checked_add(i);
        let value = source_index.is_some_and(|index| {
            index < src_size && unsafe { read_bit(src, index) }
        });
        unsafe { write_bit(dst, i, value); }
    }
}
