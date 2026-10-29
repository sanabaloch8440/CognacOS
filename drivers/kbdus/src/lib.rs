//! US keyboard layout translation core for the Win32 input path.
//! This is a portable layout engine, not yet the Windows KBDTABLES/KbdLayerDescriptor ABI.
#![no_std]
#[cfg(test)] extern crate std;
use cognacos_kbdclass::{KeyboardInputData, KEY_BREAK, KEY_E0};

pub const VK_BACK:u16=0x08;
pub const VK_TAB:u16=0x09;
pub const VK_RETURN:u16=0x0d;
pub const VK_SHIFT:u16=0x10;
pub const VK_CONTROL:u16=0x11;
pub const VK_MENU:u16=0x12;
pub const VK_CAPITAL:u16=0x14;
pub const VK_ESCAPE:u16=0x1b;
pub const VK_SPACE:u16=0x20;
pub const VK_PRIOR:u16=0x21;
pub const VK_NEXT:u16=0x22;
pub const VK_END:u16=0x23;
pub const VK_HOME:u16=0x24;
pub const VK_LEFT:u16=0x25;
pub const VK_UP:u16=0x26;
pub const VK_RIGHT:u16=0x27;
pub const VK_DOWN:u16=0x28;
pub const VK_INSERT:u16=0x2d;
pub const VK_DELETE:u16=0x2e;
pub const VK_LWIN:u16=0x5b;
pub const VK_RWIN:u16=0x5c;
pub const VK_NUMPAD0:u16=0x60;
pub const VK_F1:u16=0x70;
pub const VK_NUMLOCK:u16=0x90;
pub const VK_SCROLL:u16=0x91;

#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct KeyEvent { pub virtual_key:u16, pub scan_code:u16, pub unicode:u16, pub key_up:bool, pub extended:bool }
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct Modifiers { pub shift:bool, pub caps_lock:bool, pub control:bool, pub alt:bool }

/// Map a Set-1 scan code (without the break bit) to a Windows virtual-key code.
/// Extended keys are explicitly distinguished so keypad Enter and navigation keys
/// don't alias their non-extended counterparts.
pub const fn scan_to_vk(scan:u16,extended:bool)->Option<u16>{
 if extended { return match scan {
  0x1c=>Some(VK_RETURN),0x1d=>Some(VK_CONTROL),0x35=>Some(VK_MENU),
  0x38=>Some(VK_MENU),0x47=>Some(VK_HOME),0x48=>Some(VK_UP),0x49=>Some(VK_PRIOR),
  0x4b=>Some(VK_LEFT),0x4d=>Some(VK_RIGHT),0x4f=>Some(VK_END),0x50=>Some(VK_DOWN),
  0x51=>Some(VK_NEXT),0x52=>Some(VK_INSERT),0x53=>Some(VK_DELETE),
  0x5b=>Some(VK_LWIN),0x5c=>Some(VK_RWIN),_=>None
 } }
 match scan {
  0x01=>Some(VK_ESCAPE),0x0e=>Some(VK_BACK),0x0f=>Some(VK_TAB),0x1c=>Some(VK_RETURN),
  0x1d=>Some(VK_CONTROL),0x2a|0x36=>Some(VK_SHIFT),0x38=>Some(VK_MENU),
  0x39=>Some(VK_SPACE),0x3a=>Some(VK_CAPITAL),0x45=>Some(VK_NUMLOCK),0x46=>Some(VK_SCROLL),
  0x3b..=0x44=>Some(VK_F1+(scan-0x3b)),0x57=>Some(0x7a),0x58=>Some(0x7b),
  0x47=>Some(VK_NUMPAD0+7),0x48=>Some(VK_NUMPAD0+8),0x49=>Some(VK_NUMPAD0+9),
  0x4b=>Some(VK_NUMPAD0+4),0x4c=>Some(VK_NUMPAD0+5),0x4d=>Some(VK_NUMPAD0+6),
  0x4f=>Some(VK_NUMPAD0+1),0x50=>Some(VK_NUMPAD0+2),0x51=>Some(VK_NUMPAD0+3),
  0x52=>Some(VK_NUMPAD0),0x53=>Some(0x6e),
  0x10=>Some(b'Q' as u16),0x11=>Some(b'W' as u16),0x12=>Some(b'E' as u16),
  0x13=>Some(b'R' as u16),0x14=>Some(b'T' as u16),0x15=>Some(b'Y' as u16),
  0x16=>Some(b'U' as u16),0x17=>Some(b'I' as u16),0x18=>Some(b'O' as u16),0x19=>Some(b'P' as u16),
  0x1e=>Some(b'A' as u16),0x1f=>Some(b'S' as u16),0x20=>Some(b'D' as u16),
  0x21=>Some(b'F' as u16),0x22=>Some(b'G' as u16),0x23=>Some(b'H' as u16),
  0x24=>Some(b'J' as u16),0x25=>Some(b'K' as u16),0x26=>Some(b'L' as u16),
  0x2c=>Some(b'Z' as u16),0x2d=>Some(b'X' as u16),0x2e=>Some(b'C' as u16),
  0x2f=>Some(b'V' as u16),0x30=>Some(b'B' as u16),0x31=>Some(b'N' as u16),0x32=>Some(b'M' as u16),
  0x02..=0x0a=>Some((b'1' as u16)+(scan-0x02)),0x0b=>Some(b'0' as u16),
  0x0c=>Some(0xbd),0x0d=>Some(0xbb),0x1a=>Some(0xdb),0x1b=>Some(0xdd),
  0x27=>Some(0xba),0x28=>Some(0xde),0x29=>Some(0xc0),0x2b=>Some(0xdc),
  0x33=>Some(0xbc),0x34=>Some(0xbe),0x35=>Some(0xbf),_=>None
 }
}

/// Translate a key packet to a UTF-16 code unit for the US layout.
/// Control combinations and key-up events do not generate text; callers should
/// dispatch virtual-key transitions separately to the Win32 input queue.
pub const fn translate_text(vk:u16,mods:Modifiers,key_up:bool)->u16{
 if key_up||mods.control||mods.alt{return 0}
 if vk>=b'A' as u16&&vk<=b'Z' as u16 {
  let upper=mods.shift^mods.caps_lock;
  return if upper{vk}else{vk+32}
 }
 if vk>=b'0' as u16&&vk<=b'9' as u16 {
  if !mods.shift{return vk}
  return match vk {48=>b')' as u16,49=>b'!' as u16,50=>b'@' as u16,51=>b'#' as u16,52=>b'$' as u16,53=>b'%' as u16,54=>b'^' as u16,55=>b'&' as u16,56=>b'*' as u16,57=>b'(' as u16,_=>0}
 }
 match vk {
  VK_SPACE=>b' ' as u16,VK_TAB=>b'\t' as u16,VK_RETURN=>b'\r' as u16,
  0xbd=>if mods.shift{b'_' as u16}else{b'-' as u16},
  0xbb=>if mods.shift{b'+' as u16}else{b'=' as u16},
  0xdb=>if mods.shift{b'{' as u16}else{b'[' as u16},
  0xdd=>if mods.shift{b'}' as u16}else{b']' as u16},
  0xba=>if mods.shift{b':' as u16}else{b';' as u16},
  0xde=>if mods.shift{b'"' as u16}else{b'\'' as u16},
  0xc0=>if mods.shift{b'~' as u16}else{b'`' as u16},
  0xdc=>if mods.shift{b'|' as u16}else{b'\\' as u16},
  0xbc=>if mods.shift{b'<' as u16}else{b',' as u16},
  0xbe=>if mods.shift{b'>' as u16}else{b'.' as u16},
  0xbf=>if mods.shift{b'?' as u16}else{b'/' as u16},
  _=>0
 }
}

/// Convert the common class packet flags into a layout event without interpreting
/// the scan code's high bit as a break bit (KBDCLASS carries break in Flags).
pub fn translate_packet(packet:KeyboardInputData,mods:Modifiers)->Option<KeyEvent>{
 let extended=packet.flags&KEY_E0!=0;
 let key_up=packet.flags&KEY_BREAK!=0;
 let vk=scan_to_vk(packet.make_code,extended)?;
 let unicode=translate_text(vk,mods,key_up);
 Some(KeyEvent{virtual_key:vk,scan_code:packet.make_code,unicode,key_up,extended})
}

#[cfg(test)]
mod tests{
 use super::*;
 #[test] fn us_qwerty_and_shift_caps_rules(){
  assert_eq!(scan_to_vk(0x1e,false),Some(b'A' as u16));
  assert_eq!(translate_text(b'A' as u16,Modifiers::default(),false),b'a' as u16);
  assert_eq!(translate_text(b'A' as u16,Modifiers{shift:true,..Modifiers::default()},false),b'A' as u16);
  assert_eq!(translate_text(b'A' as u16,Modifiers{caps_lock:true,..Modifiers::default()},false),b'A' as u16);
  assert_eq!(translate_text(b'A' as u16,Modifiers{shift:true,caps_lock:true,..Modifiers::default()},false),b'a' as u16);
 }
 #[test] fn shifted_digits_and_punctuation(){
  let m=Modifiers{shift:true,..Modifiers::default()};
  assert_eq!(translate_text(b'1' as u16,m,false),b'!' as u16);
  assert_eq!(translate_text(0xbd,m,false),b'_' as u16);
  assert_eq!(translate_text(0xbf,m,false),b'?' as u16);
 }
 #[test] fn extended_navigation_and_keyup(){
  assert_eq!(scan_to_vk(0x4b,true),Some(VK_LEFT));
  let p=KeyboardInputData{make_code:0x1e,flags:KEY_BREAK,..Default::default()};
  let e=translate_packet(p,Modifiers::default()).unwrap();
  assert!(e.key_up);assert_eq!(e.unicode,0);
 }
 #[test] fn control_and_alt_suppress_text(){assert_eq!(translate_text(b'C' as u16,Modifiers{control:true,..Modifiers::default()},false),0);}
}
