const STATUS_SUCCESS:i32=0;const STATUS_INVALID_PARAMETER:i32=0xC000_000Du32 as i32;const STATUS_BUFFER_TOO_SMALL:i32=0xC000_0023u32 as i32;
#[repr(C)] #[derive(Clone,Copy)] pub struct RtlGuid {pub data1:u32,pub data2:u16,pub data3:u16,pub data4:[u8;8]}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_ipv4_address_to_string_a(address:*const u32,dst:*mut u8)->*mut u8 {
 if address.is_null()||dst.is_null(){return core::ptr::null_mut();}let p=address as *const u8;let mut out=dst;for i in 0..4{let mut v=unsafe{*p.add(i)};let mut tmp=[0u8;3];let mut n=0;loop{tmp[n]=b'0'+v%10;n+=1;v/=10;if v==0{break;}}for j in (0..n).rev(){unsafe{*out=tmp[j];}out=unsafe{out.add(1)};}if i!=3{unsafe{*out=b'.';}out=unsafe{out.add(1)};}}unsafe{*out=0;}out
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_ipv4_address_to_string_w(address:*const u32,dst:*mut u16)->*mut u16 {
 if address.is_null()||dst.is_null(){return core::ptr::null_mut();}let p=address as *const u8;let mut out=dst;for i in 0..4{let mut v=unsafe{*p.add(i)};let mut tmp=[0u16;3];let mut n=0;loop{tmp[n]=(b'0'+v%10) as u16;n+=1;v/=10;if v==0{break;}}for j in (0..n).rev(){unsafe{*out=tmp[j];}out=unsafe{out.add(1)};}if i!=3{unsafe{*out=46;}out=unsafe{out.add(1)};}}unsafe{*out=0;}out
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_ethernet_address_to_string_a(address:*const u8,dst:*mut u8)->*mut u8 {
 if address.is_null()||dst.is_null(){return core::ptr::null_mut();}let hex=b"0123456789ABCDEF";let mut p=dst;for i in 0..6{let v=unsafe{*address.add(i)};unsafe{*p=hex[(v>>4) as usize];*p.add(1)=hex[(v&15) as usize];}p=unsafe{p.add(2)};if i!=5{unsafe{*p=b'-';}p=unsafe{p.add(1)};}}unsafe{*p=0;}p
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_ethernet_address_to_string_w(address:*const u8,dst:*mut u16)->*mut u16 {
 if address.is_null()||dst.is_null(){return core::ptr::null_mut();}let hex=b"0123456789ABCDEF";let mut p=dst;for i in 0..6{let v=unsafe{*address.add(i)};unsafe{*p=hex[(v>>4) as usize] as u16;*p.add(1)=hex[(v&15) as usize] as u16;}p=unsafe{p.add(2)};if i!=5{unsafe{*p=45;}p=unsafe{p.add(1)};}}unsafe{*p=0;}p
}
fn hex_digit(v:u8)->u16 { if v<10 { (b'0'+v) as u16 } else { (b'A'+v-10) as u16 } }
unsafe fn put_hex(dst:*mut u16,offset:usize,value:u64,digits:usize) { for i in 0..digits { let shift=(digits-1-i)*4; unsafe{*dst.add(offset+i)=hex_digit(((value>>shift)&15) as u8);} } }
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_string_from_guid(g:*const RtlGuid,dst:*mut super::UnicodeString)->i32 {
 if g.is_null()||dst.is_null(){return STATUS_INVALID_PARAMETER;}let x=unsafe{&*g};let d=unsafe{&mut *dst};if (d.maximum_length as usize)<78||d.buffer.is_null(){return STATUS_BUFFER_TOO_SMALL;}
 unsafe{*d.buffer=123;put_hex(d.buffer,1,x.data1 as u64,8);*d.buffer.add(9)=45;put_hex(d.buffer,10,x.data2 as u64,4);*d.buffer.add(14)=45;put_hex(d.buffer,15,x.data3 as u64,4);*d.buffer.add(19)=45;put_hex(d.buffer,20,x.data4[0] as u64,2);put_hex(d.buffer,22,x.data4[1] as u64,2);*d.buffer.add(24)=45;for i in 0..6{put_hex(d.buffer,25+i*2,x.data4[i+2] as u64,2);}*d.buffer.add(37)=125;*d.buffer.add(38)=0;}d.length=76;STATUS_SUCCESS
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_guid_from_string(s:*const super::UnicodeString,g:*mut RtlGuid)->i32 {
 if s.is_null()||g.is_null(){return STATUS_INVALID_PARAMETER;}let x=unsafe{&*s};if x.length<76||x.length%2!=0||x.buffer.is_null(){return STATUS_INVALID_PARAMETER;}
 let mut ascii=[0u8;32];let mut n=0usize;for i in 0..(x.length/2) as usize{let c=unsafe{*x.buffer.add(i)};if c==123||c==125||c==45{continue;}if c>127||n>=32{return STATUS_INVALID_PARAMETER;}ascii[n]=c as u8;n+=1;}if n!=32{return STATUS_INVALID_PARAMETER;}
 fn parse(s:&[u8])->Option<u64>{let mut v=0u64;for c in s{let d=match c{b'0'..=b'9'=>(c-b'0') as u64,b'a'..=b'f'=>(c-b'a'+10) as u64,b'A'..=b'F'=>(c-b'A'+10) as u64,_=>return None};v=(v<<4)|d;}Some(v)}
 let (Some(d1),Some(d2),Some(d3))=(parse(&ascii[0..8]),parse(&ascii[8..12]),parse(&ascii[12..16])) else{return STATUS_INVALID_PARAMETER;};
 let mut tail=[0u8;8];for i in 0..8{let Some(v)=parse(&ascii[16+i*2..18+i*2]) else{return STATUS_INVALID_PARAMETER;};tail[i]=v as u8;}
 unsafe{*g=RtlGuid{data1:d1 as u32,data2:d2 as u16,data3:d3 as u16,data4:tail};}STATUS_SUCCESS
}
