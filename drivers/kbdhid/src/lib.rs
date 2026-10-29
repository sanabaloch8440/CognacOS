//! USB/Bluetooth HID boot-keyboard report mapper. Transport and HIDClass own report delivery.
#![no_std]
pub use cognacos_nt_abi as nt_abi;
#[cfg(test)] extern crate std;
use cognacos_kbdclass::{KeyboardInputData, KEY_BREAK, KEY_E0};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HidError { ShortReport, ErrorRollOver, InvalidUsage, OutputFull }

/// Map the standard USB HID keyboard usages to set-1 scan codes. This intentionally covers
/// the common boot-keyboard range; unmapped consumer/vendor usages are rejected, not guessed.
pub const fn usage_to_set1(usage: u8) -> Option<(u16, bool)> {
 match usage {
  0x04=>Some((0x1e,false)),0x05=>Some((0x30,false)),0x06=>Some((0x2e,false)),0x07=>Some((0x20,false)),
  0x08=>Some((0x12,false)),0x09=>Some((0x21,false)),0x0a=>Some((0x22,false)),0x0b=>Some((0x23,false)),
  0x0c=>Some((0x17,false)),0x0d=>Some((0x24,false)),0x0e=>Some((0x25,false)),0x0f=>Some((0x26,false)),
  0x10=>Some((0x32,false)),0x11=>Some((0x31,false)),0x12=>Some((0x18,false)),0x13=>Some((0x19,false)),
  0x14=>Some((0x10,false)),0x15=>Some((0x13,false)),0x16=>Some((0x1f,false)),0x17=>Some((0x14,false)),
  0x18=>Some((0x16,false)),0x19=>Some((0x2f,false)),0x1a=>Some((0x11,false)),0x1b=>Some((0x2d,false)),
  0x1c=>Some((0x15,false)),0x1d=>Some((0x2c,false)),0x1e=>Some((0x02,false)),0x1f=>Some((0x03,false)),
  0x20=>Some((0x04,false)),0x21=>Some((0x05,false)),0x22=>Some((0x06,false)),0x23=>Some((0x07,false)),
  0x24=>Some((0x08,false)),0x25=>Some((0x09,false)),0x26=>Some((0x0a,false)),0x27=>Some((0x0b,false)),
  0x28=>Some((0x1c,false)),0x29=>Some((0x01,false)),0x2a=>Some((0x0e,false)),0x2b=>Some((0x0f,false)),
  0x2c=>Some((0x39,false)),0x2d=>Some((0x0c,false)),0x2e=>Some((0x0d,false)),0x2f=>Some((0x1a,false)),
  0x30=>Some((0x1b,false)),0x31=>Some((0x2b,false)),0x32=>Some((0x2b,false)),0x33=>Some((0x27,false)),
  0x34=>Some((0x28,false)),0x35=>Some((0x29,false)),0x36=>Some((0x33,false)),0x37=>Some((0x34,false)),
  0x38=>Some((0x35,false)),0x39=>Some((0x3a,false)),0x3a=>Some((0x3b,false)),0x3b=>Some((0x3c,false)),
  0x3c=>Some((0x3d,false)),0x3d=>Some((0x3e,false)),0x3e=>Some((0x3f,false)),0x3f=>Some((0x40,false)),
  0x40=>Some((0x41,false)),0x41=>Some((0x42,false)),0x42=>Some((0x43,false)),0x43=>Some((0x44,false)),
  0x44=>Some((0x57,false)),0x45=>Some((0x58,false)),0x49=>Some((0x52,true)),0x4a=>Some((0x47,true)),
  0x4b=>Some((0x49,true)),0x4c=>Some((0x53,true)),0x4d=>Some((0x4f,true)),0x4e=>Some((0x51,true)),
  0x4f=>Some((0x4d,true)),0x50=>Some((0x4b,true)),0x51=>Some((0x50,true)),0x52=>Some((0x48,true)),
  0x4c=>Some((0x53,true)),0x00=>None,_=>None
 }
}

/// Decode an 8-byte HID boot keyboard report into make/break transitions against previous usages.
/// The caller supplies output storage; no allocation occurs. Modifier bits map to left/right Ctrl,
/// Shift, Alt and GUI scan codes. ErrorRollOver is reported without generating synthetic releases.
pub struct KeyboardState { pub previous_keys: [u8;6], pub previous_modifiers: u8 }
impl KeyboardState { pub const fn new()->Self { Self { previous_keys:[0;6], previous_modifiers:0 } } }
impl Default for KeyboardState { fn default()->Self { Self::new() } }

pub fn decode_boot_report(report:&[u8], state:&mut KeyboardState, out:&mut [KeyboardInputData])->Result<usize,HidError>{
 if report.len()<8{return Err(HidError::ShortReport)}
 let keys=&report[2..8];
 if keys.contains(&1){return Err(HidError::ErrorRollOver)}
 let mut n=0;
 let modifiers=report[0];
 for bit in 0..8 { let usage=match bit {0=>0xe0,1=>0xe1,2=>0xe2,3=>0xe3,4=>0xe4,5=>0xe5,6=>0xe6,_=>0xe7};
  let was=state.previous_modifiers&(1<<bit)!=0; let now=modifiers&(1<<bit)!=0;
  if was!=now { if n>=out.len(){return Err(HidError::OutputFull)} let (scan,ext)=modifier_scan(usage); out[n]=KeyboardInputData{make_code:scan,flags:(if !now{KEY_BREAK}else{0})|(if ext{KEY_E0}else{0}),..Default::default()};n+=1; }
 }
 for &u in state.previous_keys.iter() { if u!=0&&!keys.contains(&u) { if let Some((scan,ext))=usage_to_set1(u){if n>=out.len(){return Err(HidError::OutputFull)}out[n]=KeyboardInputData{make_code:scan,flags:KEY_BREAK|(if ext{KEY_E0}else{0}),..Default::default()};n+=1;} } }
 for &u in keys { if u!=0&&!state.previous_keys.contains(&u) { if let Some((scan,ext))=usage_to_set1(u){if n>=out.len(){return Err(HidError::OutputFull)}out[n]=KeyboardInputData{make_code:scan,flags:if ext{KEY_E0}else{0},..Default::default()};n+=1;} } }
 state.previous_keys=[keys[0],keys[1],keys[2],keys[3],keys[4],keys[5]]; state.previous_modifiers=modifiers;
 Ok(n)
}
const fn modifier_scan(u:u8)->(u16,bool){match u{0xe0=>(0x1d,false),0xe1=>(0x2a,false),0xe2=>(0x38,false),0xe3=>(0x5b,true),0xe4=>(0x1d,true),0xe5=>(0x36,false),0xe6=>(0x38,true),_=>(0x5c,true)}}

#[cfg(test)] mod tests { use super::*; #[test] fn maps_and_diffs_boot_reports(){let mut state=KeyboardState::new();let mut out=[KeyboardInputData::default();16];let mut r=[0;8];r[2]=0x04;assert_eq!(decode_boot_report(&r,&mut state,&mut out),Ok(1));assert_eq!(out[0].make_code,0x1e);r[2]=0;assert_eq!(decode_boot_report(&r,&mut state,&mut out),Ok(1));assert_ne!(out[0].flags&KEY_BREAK,0);} #[test] fn rollover_does_not_release_keys(){let mut state=KeyboardState{previous_keys:[4,0,0,0,0,0],previous_modifiers:0};let mut out=[KeyboardInputData::default();16];let mut r=[0;8];r[2]=1;assert_eq!(decode_boot_report(&r,&mut state,&mut out),Err(HidError::ErrorRollOver));assert_eq!(state.previous_keys[0],4);} }
