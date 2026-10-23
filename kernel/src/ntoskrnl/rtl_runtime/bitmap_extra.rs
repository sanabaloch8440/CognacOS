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

#[repr(C)]
pub struct RtlBitmapRun { pub starting_index:u32, pub number_of_bits:u32 }
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_extract_bit_map(dst:*mut RtlBitmap,src:*const RtlBitmap,start:u32,count:u32) {
 if dst.is_null()||src.is_null(){return;}
 let n=count.min(unsafe{(*dst).size_of_bitmap});
 for i in 0..n {let v=start.checked_add(i).is_some_and(|s|s<unsafe{(*src).size_of_bitmap}&&unsafe{read_bit(src,s)});unsafe{write_bit(dst,i,v);}}
 for i in n..unsafe{(*dst).size_of_bitmap}{unsafe{write_bit(dst,i,false);}}
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_find_clear_runs(bitmap:*const RtlBitmap,runs:*mut RtlBitmapRun,capacity:u32,locate_longest:u8)->u32 {
 if bitmap.is_null()||(capacity!=0&&runs.is_null()){return 0;}
 let size=unsafe{(*bitmap).size_of_bitmap};let mut found=0u32;let mut i=0u32;
 while i<size {if unsafe{read_bit(bitmap,i)}{i+=1;continue;}let start=i;while i<size&&!unsafe{read_bit(bitmap,i)}{i+=1;}
 if found<capacity{unsafe{*runs.add(found as usize)=RtlBitmapRun{starting_index:start,number_of_bits:i-start};found+=1;}}
 }
 if locate_longest!=0&&capacity>0 {for i in 0..found {for j in i+1..found {if unsafe{(*runs.add(j as usize)).number_of_bits>(*runs.add(i as usize)).number_of_bits}{unsafe{core::ptr::swap(runs.add(i as usize),runs.add(j as usize));}}}}}
 found
}
