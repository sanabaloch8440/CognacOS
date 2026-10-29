//! US keyboard layout translation core for the Win32 input path.
//! This is a portable layout engine, not yet the Windows KBDTABLES/KbdLayerDescriptor ABI.
#![no_std]

/// Initial x64 KBDTABLES-compatible descriptor layout for the CognacOS US layout.
/// Keep fields pointer-sized and in the Windows keyboard-layout ABI order.
/// This is intentionally a conservative US table (no dead keys or ligatures).
#[repr(C)]
pub struct VkToWchar { pub virtual_key:u8, pub attributes:u8, pub chars:[u16;5] }
#[repr(C)]
pub struct VkToWcharTable { pub entries:*const VkToWchar, pub modification_count:u8, pub entry_size:u8 }
#[repr(C)]
struct VkToWcharTablePointers([*const VkToWcharTable;2]);
unsafe impl Sync for VkToWcharTablePointers {}
#[repr(C)]
pub struct ModifierBit { pub virtual_key:u8, pub modifier_bit:u8 }
#[repr(C)]
pub struct KbdModifiersTable { pub modifier_bits:*const ModifierBit, pub max_mod_bits:u16, pub modifier_number_table:*const u8 }
#[repr(C)]
pub struct ScanVk { pub scan_code:u8, pub virtual_key:u16 }
unsafe impl Sync for VkToWcharTable {}
unsafe impl Sync for KbdModifiersTable {}
unsafe impl Sync for KbdTables {}
#[repr(C)]
pub struct KbdTables {
 pub char_modifiers:*const KbdModifiersTable,
 pub vk_to_wchar_table:*const *const VkToWcharTable,
 pub max_scan_to_vk:u8,
 pub scan_to_vk:*const ScanVk,
 pub locale_flags:u32,
 pub max_dead_key_combinations:u8,
 pub max_ligature:u8,
 pub scan_to_vk_e0:*const u16,
 pub scan_to_vk_e1:*const u16,
 pub dead_keys:*const core::ffi::c_void,
 pub key_names:*const core::ffi::c_void,
 pub key_names_extended:*const core::ffi::c_void,
 pub key_names_dead:*const core::ffi::c_void,
 pub numpad_scan_code:*const u16,
 pub max_mouse_virtual_keys:u8,
 pub mouse_virtual_keys:*const core::ffi::c_void,
}
static VK_CHARS:[VkToWchar;50]=[
 VkToWchar{virtual_key:0x30,attributes:0,chars:[b'0' as u16,b')' as u16,0,0,b'0' as u16]},
 VkToWchar{virtual_key:0x31,attributes:0,chars:[b'1' as u16,b'!' as u16,0,0,b'1' as u16]},
 VkToWchar{virtual_key:0x32,attributes:0,chars:[b'2' as u16,b'@' as u16,0,0,b'2' as u16]},
 VkToWchar{virtual_key:0x33,attributes:0,chars:[b'3' as u16,b'#' as u16,0,0,b'3' as u16]},
 VkToWchar{virtual_key:0x34,attributes:0,chars:[b'4' as u16,b'$' as u16,0,0,b'4' as u16]},
 VkToWchar{virtual_key:0x35,attributes:0,chars:[b'5' as u16,b'%' as u16,0,0,b'5' as u16]},
 VkToWchar{virtual_key:0x36,attributes:0,chars:[b'6' as u16,b'^' as u16,0,0,b'6' as u16]},
 VkToWchar{virtual_key:0x37,attributes:0,chars:[b'7' as u16,b'&' as u16,0,0,b'7' as u16]},
 VkToWchar{virtual_key:0x38,attributes:0,chars:[b'8' as u16,b'*' as u16,0,0,b'8' as u16]},
 VkToWchar{virtual_key:0x39,attributes:0,chars:[b'9' as u16,b'(' as u16,0,0,b'9' as u16]},
 VkToWchar{virtual_key:0x20,attributes:0,chars:[b' ' as u16,b' ' as u16,0,0,b' ' as u16]},
 VkToWchar{virtual_key:0x0d,attributes:0,chars:[b'\r' as u16,b'\r' as u16,0,0,b'\r' as u16]},
 VkToWchar{virtual_key:0x09,attributes:0,chars:[b'\t' as u16,b'\t' as u16,0,0,b'\t' as u16]},
 VkToWchar{virtual_key:0x41,attributes:0,chars:[b'a' as u16,b'A' as u16,0,0,b'A' as u16]},
 VkToWchar{virtual_key:0x42,attributes:0,chars:[b'b' as u16,b'B' as u16,0,0,b'B' as u16]},
 VkToWchar{virtual_key:0x43,attributes:0,chars:[b'c' as u16,b'C' as u16,0,0,b'C' as u16]},
 VkToWchar{virtual_key:0x44,attributes:0,chars:[b'd' as u16,b'D' as u16,0,0,b'D' as u16]},
 VkToWchar{virtual_key:0x45,attributes:0,chars:[b'e' as u16,b'E' as u16,0,0,b'E' as u16]},
 VkToWchar{virtual_key:0x46,attributes:0,chars:[b'f' as u16,b'F' as u16,0,0,b'F' as u16]},
 VkToWchar{virtual_key:0x47,attributes:0,chars:[b'g' as u16,b'G' as u16,0,0,b'G' as u16]},
 VkToWchar{virtual_key:0x48,attributes:0,chars:[b'h' as u16,b'H' as u16,0,0,b'H' as u16]},
 VkToWchar{virtual_key:0x49,attributes:0,chars:[b'i' as u16,b'I' as u16,0,0,b'I' as u16]},
 VkToWchar{virtual_key:0x4a,attributes:0,chars:[b'j' as u16,b'J' as u16,0,0,b'J' as u16]},
 VkToWchar{virtual_key:0x4b,attributes:0,chars:[b'k' as u16,b'K' as u16,0,0,b'K' as u16]},
 VkToWchar{virtual_key:0x4c,attributes:0,chars:[b'l' as u16,b'L' as u16,0,0,b'L' as u16]},
 VkToWchar{virtual_key:0x4d,attributes:0,chars:[b'm' as u16,b'M' as u16,0,0,b'M' as u16]},
 VkToWchar{virtual_key:0x4e,attributes:0,chars:[b'n' as u16,b'N' as u16,0,0,b'N' as u16]},
 VkToWchar{virtual_key:0x4f,attributes:0,chars:[b'o' as u16,b'O' as u16,0,0,b'O' as u16]},
 VkToWchar{virtual_key:0x50,attributes:0,chars:[b'p' as u16,b'P' as u16,0,0,b'P' as u16]},
 VkToWchar{virtual_key:0x51,attributes:0,chars:[b'q' as u16,b'Q' as u16,0,0,b'Q' as u16]},
 VkToWchar{virtual_key:0x52,attributes:0,chars:[b'r' as u16,b'R' as u16,0,0,b'R' as u16]},
 VkToWchar{virtual_key:0x53,attributes:0,chars:[b's' as u16,b'S' as u16,0,0,b'S' as u16]},
 VkToWchar{virtual_key:0x54,attributes:0,chars:[b't' as u16,b'T' as u16,0,0,b'T' as u16]},
 VkToWchar{virtual_key:0x55,attributes:0,chars:[b'u' as u16,b'U' as u16,0,0,b'U' as u16]},
 VkToWchar{virtual_key:0x56,attributes:0,chars:[b'v' as u16,b'V' as u16,0,0,b'V' as u16]},
 VkToWchar{virtual_key:0x57,attributes:0,chars:[b'w' as u16,b'W' as u16,0,0,b'W' as u16]},
 VkToWchar{virtual_key:0x58,attributes:0,chars:[b'x' as u16,b'X' as u16,0,0,b'X' as u16]},
 VkToWchar{virtual_key:0x59,attributes:0,chars:[b'y' as u16,b'Y' as u16,0,0,b'Y' as u16]},
 VkToWchar{virtual_key:0x5a,attributes:0,chars:[b'z' as u16,b'Z' as u16,0,0,b'Z' as u16]},
 VkToWchar{virtual_key:0xbd,attributes:0,chars:[b'-' as u16,b'_' as u16,0,0,b'_' as u16]},
 VkToWchar{virtual_key:0xbb,attributes:0,chars:[b'=' as u16,b'+' as u16,0,0,b'+' as u16]},
 VkToWchar{virtual_key:0xdb,attributes:0,chars:[b'[' as u16,b'{' as u16,0,0,b'{' as u16]},
 VkToWchar{virtual_key:0xdd,attributes:0,chars:[b']' as u16,b'}' as u16,0,0,b'}' as u16]},
 VkToWchar{virtual_key:0xba,attributes:0,chars:[b';' as u16,b':' as u16,0,0,b':' as u16]},
 VkToWchar{virtual_key:0xde,attributes:0,chars:[b'\'' as u16,b'"' as u16,0,0,b'"' as u16]},
 VkToWchar{virtual_key:0xc0,attributes:0,chars:[b'`' as u16,b'~' as u16,0,0,b'~' as u16]},
 VkToWchar{virtual_key:0xbc,attributes:0,chars:[b',' as u16,b'<' as u16,0,0,b'<' as u16]},
 VkToWchar{virtual_key:0xbe,attributes:0,chars:[b'.' as u16,b'>' as u16,0,0,b'>' as u16]},
 VkToWchar{virtual_key:0xbf,attributes:0,chars:[b'/' as u16,b'?' as u16,0,0,b'?' as u16]},
 VkToWchar{virtual_key:0,attributes:0,chars:[0;5]},
];
static VK_TABLE:VkToWcharTable=VkToWcharTable{entries:VK_CHARS.as_ptr(),modification_count:5,entry_size:core::mem::size_of::<VkToWchar>() as u8};
static VK_TABLE_LIST:VkToWcharTablePointers=VkToWcharTablePointers([&VK_TABLE,core::ptr::null()]);
static MODIFIER_BITS:[ModifierBit;4]=[ModifierBit{virtual_key:VK_SHIFT as u8,modifier_bit:1},ModifierBit{virtual_key:VK_CONTROL as u8,modifier_bit:2},ModifierBit{virtual_key:VK_MENU as u8,modifier_bit:4},ModifierBit{virtual_key:0,modifier_bit:0}];
static MODIFIER_NUMBERS:[u8;8]=[0,1,2,3,4,5,6,0];
static CHAR_MODIFIERS:KbdModifiersTable=KbdModifiersTable{modifier_bits:MODIFIER_BITS.as_ptr(),max_mod_bits:3,modifier_number_table:MODIFIER_NUMBERS.as_ptr()};
static SCAN_TO_VK:[ScanVk;89]=[ScanVk{scan_code:0,virtual_key:0},ScanVk{scan_code:1,virtual_key:27},ScanVk{scan_code:2,virtual_key:49},ScanVk{scan_code:3,virtual_key:50},ScanVk{scan_code:4,virtual_key:51},ScanVk{scan_code:5,virtual_key:52},ScanVk{scan_code:6,virtual_key:53},ScanVk{scan_code:7,virtual_key:54},ScanVk{scan_code:8,virtual_key:55},ScanVk{scan_code:9,virtual_key:56},ScanVk{scan_code:10,virtual_key:57},ScanVk{scan_code:11,virtual_key:48},ScanVk{scan_code:12,virtual_key:189},ScanVk{scan_code:13,virtual_key:187},ScanVk{scan_code:14,virtual_key:8},ScanVk{scan_code:15,virtual_key:9},ScanVk{scan_code:16,virtual_key:81},ScanVk{scan_code:17,virtual_key:87},ScanVk{scan_code:18,virtual_key:69},ScanVk{scan_code:19,virtual_key:82},ScanVk{scan_code:20,virtual_key:84},ScanVk{scan_code:21,virtual_key:89},ScanVk{scan_code:22,virtual_key:85},ScanVk{scan_code:23,virtual_key:73},ScanVk{scan_code:24,virtual_key:79},ScanVk{scan_code:25,virtual_key:80},ScanVk{scan_code:26,virtual_key:219},ScanVk{scan_code:27,virtual_key:221},ScanVk{scan_code:28,virtual_key:13},ScanVk{scan_code:29,virtual_key:17},ScanVk{scan_code:30,virtual_key:65},ScanVk{scan_code:31,virtual_key:83},ScanVk{scan_code:32,virtual_key:68},ScanVk{scan_code:33,virtual_key:70},ScanVk{scan_code:34,virtual_key:71},ScanVk{scan_code:35,virtual_key:72},ScanVk{scan_code:36,virtual_key:74},ScanVk{scan_code:37,virtual_key:75},ScanVk{scan_code:38,virtual_key:76},ScanVk{scan_code:39,virtual_key:186},ScanVk{scan_code:40,virtual_key:222},ScanVk{scan_code:41,virtual_key:192},ScanVk{scan_code:42,virtual_key:16},ScanVk{scan_code:43,virtual_key:220},ScanVk{scan_code:44,virtual_key:90},ScanVk{scan_code:45,virtual_key:88},ScanVk{scan_code:46,virtual_key:67},ScanVk{scan_code:47,virtual_key:86},ScanVk{scan_code:48,virtual_key:66},ScanVk{scan_code:49,virtual_key:78},ScanVk{scan_code:50,virtual_key:77},ScanVk{scan_code:51,virtual_key:188},ScanVk{scan_code:52,virtual_key:190},ScanVk{scan_code:53,virtual_key:191},ScanVk{scan_code:54,virtual_key:16},ScanVk{scan_code:55,virtual_key:106},ScanVk{scan_code:56,virtual_key:18},ScanVk{scan_code:57,virtual_key:32},ScanVk{scan_code:58,virtual_key:20},ScanVk{scan_code:59,virtual_key:112},ScanVk{scan_code:60,virtual_key:113},ScanVk{scan_code:61,virtual_key:114},ScanVk{scan_code:62,virtual_key:115},ScanVk{scan_code:63,virtual_key:116},ScanVk{scan_code:64,virtual_key:117},ScanVk{scan_code:65,virtual_key:118},ScanVk{scan_code:66,virtual_key:119},ScanVk{scan_code:67,virtual_key:120},ScanVk{scan_code:68,virtual_key:121},ScanVk{scan_code:69,virtual_key:144},ScanVk{scan_code:70,virtual_key:145},ScanVk{scan_code:71,virtual_key:103},ScanVk{scan_code:72,virtual_key:104},ScanVk{scan_code:73,virtual_key:105},ScanVk{scan_code:74,virtual_key:109},ScanVk{scan_code:75,virtual_key:100},ScanVk{scan_code:76,virtual_key:101},ScanVk{scan_code:77,virtual_key:102},ScanVk{scan_code:78,virtual_key:107},ScanVk{scan_code:79,virtual_key:97},ScanVk{scan_code:80,virtual_key:98},ScanVk{scan_code:81,virtual_key:99},ScanVk{scan_code:82,virtual_key:96},ScanVk{scan_code:83,virtual_key:110},ScanVk{scan_code:84,virtual_key:0},ScanVk{scan_code:85,virtual_key:0},ScanVk{scan_code:86,virtual_key:0},ScanVk{scan_code:87,virtual_key:122},ScanVk{scan_code:88,virtual_key:123}];
static E0:[u16;1]=[0]; static E1:[u16;1]=[0]; static NUMPAD:u16=0x4c;
static KBD_TABLES:KbdTables=KbdTables{
 char_modifiers:&CHAR_MODIFIERS,vk_to_wchar_table:VK_TABLE_LIST.0.as_ptr(),max_scan_to_vk:0x58,
 scan_to_vk:SCAN_TO_VK.as_ptr(),locale_flags:0,max_dead_key_combinations:0,max_ligature:0,
 scan_to_vk_e0:E0.as_ptr(),scan_to_vk_e1:E1.as_ptr(),dead_keys:core::ptr::null(),
 key_names:core::ptr::null(),key_names_extended:core::ptr::null(),key_names_dead:core::ptr::null(),
 numpad_scan_code:&NUMPAD,max_mouse_virtual_keys:0,mouse_virtual_keys:core::ptr::null()
};
#[unsafe(no_mangle)]
pub extern "system" fn KbdLayerDescriptor()->*const KbdTables { &KBD_TABLES }

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

#[cfg(test)]
mod layout_dll_tests {
 use super::*;
 #[test] fn descriptor_is_exported_and_contains_scan_and_character_tables(){
  let p=KbdLayerDescriptor();assert!(!p.is_null());
  let t=unsafe{&*p};assert_eq!(t.max_scan_to_vk,0x58);assert!(!t.scan_to_vk.is_null());
  assert!(!t.char_modifiers.is_null());assert!(!t.vk_to_wchar_table.is_null());
  assert_eq!(unsafe{*t.vk_to_wchar_table},&VK_TABLE as *const VkToWcharTable);
  assert_eq!(SCAN_TO_VK[0x1e].virtual_key,b'A' as u16);
  assert_eq!(SCAN_TO_VK[0x30].virtual_key,b'B' as u16);
 }
}

#[cfg(target_os="windows")]
core::arch::global_asm!(r#"
.text
.p2align 4
.globl DllMain
DllMain:
 movl $1, %eax
 ret
"#,options(att_syntax));
#[cfg(target_os="windows")]
#[panic_handler]
fn panic(_info:&core::panic::PanicInfo<'_>)->!{loop{core::hint::spin_loop();}}
