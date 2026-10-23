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
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_string_from_guid(g:*const RtlGuid,dst:*mut super::UnicodeString)->i32 {
 if g.is_null()||dst.is_null(){return STATUS_INVALID_PARAMETER;}let x=unsafe{&*g};let d=unsafe{&mut *dst};let needed=78usize;if d.maximum_length as usize<needed||d.buffer.is_null(){return STATUS_BUFFER_TOO_SMALL;}
 let mut chars=[0u16;39];let fmt=alloc::format!("{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",x.data1,x.data2,x.data3,x.data4[0],x.data4[1],x.data4[2],x.data4[3],x.data4[4],x.data4[5],x.data4[6],x.data4[7]);
 chars[0]=123;for(i,b)in fmt.bytes().enumerate(){chars[i+1]=b as u16;}chars[37]=125;for i in 0..38{unsafe{*d.buffer.add(i)=chars[i];}}d.length=76;STATUS_SUCCESS
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_guid_from_string(s:*const super::UnicodeString,g:*mut RtlGuid)->i32 {
 if s.is_null()||g.is_null(){return STATUS_INVALID_PARAMETER;}let x=unsafe{&*s};if x.length<72||x.length%2!=0||x.buffer.is_null(){return STATUS_INVALID_PARAMETER;}let mut ascii=[0u8;38];let mut n=0;for i in 0..(x.length/2) as usize{let c=unsafe{*x.buffer.add(i)};if c==123||c==125||c==45{continue;}if c>127||n>=32{return STATUS_INVALID_PARAMETER;}ascii[n]=c as u8;n+=1;}if n!=32{return STATUS_INVALID_PARAMETER;}
 fn parse(s:&[u8])->Option<u64>{let mut v=0u64;for c in s{let d=match c{b'0'..=b'9'=>(c-b'0') as u64,b'a'..=b'f'=>(c-b'a'+10) as u64,b'A'..=b'F'=>(c-b'A'+10) as u64,_=>return None};v=(v<<4)|d;}Some(v)}
 let d1=parse(&ascii[0..8]).ok_or(()) ;let d2=parse(&ascii[8..12]).ok_or(());let d3=parse(&ascii[12..16]).ok_or(());let mut tail=[0u8;8];for i in 0..8{tail[i]=parse(&ascii[16+i*2..18+i*2]).ok_or(())? as u8;}
 let (Ok(d1),Ok(d2),Ok(d3))=(d1,d2,d3)else{return STATUS_INVALID_PARAMETER;};unsafe{*g=RtlGuid{data1:d1 as u32,data2:d2 as u16,data3:d3 as u16,data4:tail};}STATUS_SUCCESS
}
