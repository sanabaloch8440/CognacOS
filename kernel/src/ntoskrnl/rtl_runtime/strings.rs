use super::{AnsiString, UnicodeString};
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_downcase_unicode_char(c:u16)->u16 { if (b'A' as u16..=b'Z' as u16).contains(&c){c+32}else{c} }

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_upcase_unicode_char(c:u16)->u16 { if (b'a' as u16..=b'z' as u16).contains(&c){c-32}else{c} }

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_equal_string(a:*const AnsiString,b:*const AnsiString,case_insensitive:u8)->u8 {
    if a.is_null()||b.is_null(){return 0;}let x=unsafe{&*a};let y=unsafe{&*b};if x.length!=y.length{return 0;}if x.length==0{return 1;}if x.buffer.is_null()||y.buffer.is_null(){return 0;}
    for i in 0..x.length as usize{let mut l=unsafe{*x.buffer.add(i)};let mut r=unsafe{*y.buffer.add(i)};if case_insensitive!=0{if l.is_ascii_uppercase(){l+=32;}if r.is_ascii_uppercase(){r+=32;}}if l!=r{return 0;}}1
}

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_compare_string(a:*const AnsiString,b:*const AnsiString,case_insensitive:u8)->i32 {
    if a.is_null()||b.is_null(){return 0;}let x=unsafe{&*a};let y=unsafe{&*b};if (x.length!=0&&x.buffer.is_null())||(y.length!=0&&y.buffer.is_null()){return 0;}
    let n=(x.length.min(y.length)) as usize;for i in 0..n{let mut l=unsafe{*x.buffer.add(i)};let mut r=unsafe{*y.buffer.add(i)};if case_insensitive!=0{l=l.to_ascii_lowercase();r=r.to_ascii_lowercase();}if l!=r{return l as i32-r as i32;}}x.length as i32-y.length as i32
}

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_equal_unicode_string(a:*const UnicodeString,b:*const UnicodeString,case_insensitive:u8)->u8 {
    if a.is_null()||b.is_null(){return 0;}let x=unsafe{&*a};let y=unsafe{&*b};if x.length!=y.length||x.length%2!=0{return 0;}if x.length==0{return 1;}if x.buffer.is_null()||y.buffer.is_null(){return 0;}
    for i in 0..(x.length/2) as usize{let mut l=unsafe{*x.buffer.add(i)};let mut r=unsafe{*y.buffer.add(i)};if case_insensitive!=0{l=unsafe{rtl_downcase_unicode_char(l)};r=unsafe{rtl_downcase_unicode_char(r)};}if l!=r{return 0;}}1
}

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_compare_unicode_string(a:*const UnicodeString,b:*const UnicodeString,case_insensitive:u8)->i32 {
    if a.is_null()||b.is_null(){return 0;}let x=unsafe{&*a};let y=unsafe{&*b};if x.length%2!=0||y.length%2!=0{return 0;}if (x.length>0&&x.buffer.is_null())||(y.length>0&&y.buffer.is_null()){return 0;}
    let n=(x.length.min(y.length)/2) as usize;for i in 0..n{let mut l=unsafe{*x.buffer.add(i)};let mut r=unsafe{*y.buffer.add(i)};if case_insensitive!=0{l=unsafe{rtl_downcase_unicode_char(l)};r=unsafe{rtl_downcase_unicode_char(r)};}if l!=r{return l as i32-r as i32;}}(x.length as i32)-(y.length as i32)
}

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_copy_string(dst:*mut AnsiString,src:*const AnsiString) {
    if dst.is_null(){return;}let d=unsafe{&mut *dst};if d.buffer.is_null()||d.maximum_length==0{d.length=0;return;}let n=if src.is_null(){0}else{unsafe{(*src).length.min(d.maximum_length)}};if n>0&&!unsafe{(*src).buffer.is_null()}{unsafe{core::ptr::copy((*src).buffer,d.buffer,n as usize);}}d.length=n;
}

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_copy_unicode_string(dst:*mut UnicodeString,src:*const UnicodeString) {
    if dst.is_null(){return;}let d=unsafe{&mut *dst};if d.buffer.is_null()||d.maximum_length<2{d.length=0;return;}let mut n=if src.is_null(){0}else{unsafe{(*src).length.min(d.maximum_length-2)}};n&=!1;if n>0&&!unsafe{(*src).buffer.is_null()}{unsafe{core::ptr::copy((*src).buffer,d.buffer,(n/2) as usize);}}unsafe{*d.buffer.add((n/2) as usize)=0;}d.length=n;
}

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_init_ansi_string(dst:*mut AnsiString,src:*const u8) {
    if dst.is_null(){return;}let d=unsafe{&mut *dst};d.buffer=src as *mut u8;if src.is_null(){d.length=0;d.maximum_length=0;return;}
    let mut n=0usize;while n<u16::MAX as usize-1&&unsafe{*src.add(n)}!=0{n+=1;}d.length=n as u16;d.maximum_length=(n+1) as u16;
}

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_init_string(dst:*mut AnsiString,src:*const u8) { unsafe{rtl_init_ansi_string(dst,src)} }

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_init_unicode_string(dst:*mut UnicodeString,src:*const u16) {
    if dst.is_null(){return;}let d=unsafe{&mut *dst};d.buffer=src as *mut u16;if src.is_null(){d.length=0;d.maximum_length=0;return;}
    let mut n=0usize;while n<(u16::MAX as usize/2)-1&&unsafe{*src.add(n)}!=0{n+=1;}d.length=(n*2) as u16;d.maximum_length=((n+1)*2) as u16;
}

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_prefix_unicode_string(prefix:*const UnicodeString,string:*const UnicodeString,case_insensitive:u8)->u8 {
    if prefix.is_null()||string.is_null(){return 0;}let p=unsafe{&*prefix};let s=unsafe{&*string};if p.length>s.length||p.length%2!=0||s.length%2!=0{return 0;}
    if p.length==0{return 1;}if p.buffer.is_null()||s.buffer.is_null(){return 0;}
    for i in 0..(p.length/2) as usize{let mut a=unsafe{*p.buffer.add(i)};let mut b=unsafe{*s.buffer.add(i)};if case_insensitive!=0{a=unsafe{rtl_downcase_unicode_char(a)};b=unsafe{rtl_downcase_unicode_char(b)};}if a!=b{return 0;}}1
}

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_append_unicode_string_to_string(dst:*mut UnicodeString,src:*const UnicodeString)->i32 {
    const STATUS_SUCCESS:i32=0;const STATUS_BUFFER_TOO_SMALL:i32=0xC000_0023u32 as i32;
    if dst.is_null(){return STATUS_BUFFER_TOO_SMALL;}let d=unsafe{&mut *dst};if src.is_null(){return STATUS_SUCCESS;}let s=unsafe{&*src};
    if s.length==0{return STATUS_SUCCESS;}if s.buffer.is_null()||d.buffer.is_null()||d.length%2!=0||s.length%2!=0{return STATUS_BUFFER_TOO_SMALL;}
    let Some(end)=d.length.checked_add(s.length) else{return STATUS_BUFFER_TOO_SMALL;};if end>d.maximum_length{return STATUS_BUFFER_TOO_SMALL;}
    unsafe{core::ptr::copy(s.buffer,d.buffer.add((d.length/2) as usize),(s.length/2) as usize);}d.length=end;STATUS_SUCCESS
}

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_append_unicode_to_string(dst:*mut UnicodeString,src:*const u16)->i32 {
    const STATUS_SUCCESS:i32=0;const STATUS_BUFFER_TOO_SMALL:i32=0xC000_0023u32 as i32;
    if dst.is_null(){return STATUS_BUFFER_TOO_SMALL;}let d=unsafe{&mut *dst};if src.is_null(){return STATUS_SUCCESS;}if d.buffer.is_null()||d.length%2!=0{return STATUS_BUFFER_TOO_SMALL;}
    let mut n=0usize;while n<(u16::MAX as usize/2)&&unsafe{*src.add(n)}!=0{n+=1;}let bytes=n.saturating_mul(2);let Some(end)=d.length.checked_add(bytes as u16) else{return STATUS_BUFFER_TOO_SMALL;};if end>=d.maximum_length{return STATUS_BUFFER_TOO_SMALL;}
    unsafe{core::ptr::copy(src,d.buffer.add((d.length/2) as usize),n);*d.buffer.add((end/2) as usize)=0;}d.length=end;STATUS_SUCCESS
}