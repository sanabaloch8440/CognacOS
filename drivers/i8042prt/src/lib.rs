//! Legacy 8042 PS/2 keyboard port core. Hardware access is injected by the HAL backend.
#![no_std]
pub use cognacos_nt_abi as nt_abi;
#[cfg(test)] extern crate std;
use cognacos_kbdclass::{KeyboardInputData, KEY_BREAK, KEY_E0};

pub const STATUS_PORT:u16=0x64; pub const DATA_PORT:u16=0x60;
pub const STATUS_OUTPUT_FULL:u8=1; pub const STATUS_INPUT_FULL:u8=2;
pub const CMD_READ_CONFIG:u8=0x20; pub const CMD_WRITE_CONFIG:u8=0x60; pub const CMD_DISABLE_KEYBOARD:u8=0xad;
pub const CMD_ENABLE_KEYBOARD:u8=0xae; pub const CMD_SELF_TEST:u8=0xaa; pub const CMD_TEST_KEYBOARD:u8=0xab;
pub const CMD_WRITE_KEYBOARD:u8=0xd4;
pub const KEYBOARD_ACK:u8=0xfa; pub const KEYBOARD_RESEND:u8=0xfe;

pub trait PortIo { fn read8(&mut self,port:u16)->u8; fn write8(&mut self,port:u16,value:u8); }
#[derive(Clone,Copy,Debug,PartialEq,Eq)] pub enum PortError{Timeout,ControllerFailure,KeyboardFailure,UnexpectedResponse}
/// All controller polling is bounded; never spin forever on broken or absent emulated hardware.
pub fn wait_input_empty<I:PortIo>(io:&mut I,limit:u32)->Result<(),PortError>{for _ in 0..limit{if io.read8(STATUS_PORT)&STATUS_INPUT_FULL==0{return Ok(())}}Err(PortError::Timeout)}
pub fn wait_output_full<I:PortIo>(io:&mut I,limit:u32)->Result<u8,PortError>{for _ in 0..limit{if io.read8(STATUS_PORT)&STATUS_OUTPUT_FULL!=0{return Ok(io.read8(DATA_PORT))}}Err(PortError::Timeout)}
pub fn controller_command<I:PortIo>(io:&mut I,cmd:u8,limit:u32)->Result<(),PortError>{wait_input_empty(io,limit)?;io.write8(STATUS_PORT,cmd);Ok(())}
pub fn keyboard_command<I:PortIo>(io:&mut I,cmd:u8,limit:u32)->Result<(),PortError>{
 for _ in 0..3 {wait_input_empty(io,limit)?;io.write8(DATA_PORT,cmd);match wait_output_full(io,limit)?{KEYBOARD_ACK=>return Ok(()),KEYBOARD_RESEND=>continue,_=>return Err(PortError::UnexpectedResponse)}}Err(PortError::KeyboardFailure)
}

/// Set-1 scan-code decoder. Prefix state is explicit so callers can keep one state per device.
pub struct ScanDecoder{extended:bool,break_next:bool}
impl ScanDecoder{pub const fn new()->Self{Self{extended:false,break_next:false}}
 pub fn feed(&mut self,byte:u8)->Option<KeyboardInputData>{
  if byte==0xe0{self.extended=true;return None} if byte==0xe1{self.extended=false;self.break_next=false;return None}
  let extended=self.extended;self.extended=false;let breaking=byte&0x80!=0;
  Some(KeyboardInputData{make_code:(byte&0x7f) as u16,flags:(if breaking{KEY_BREAK}else{0})|(if extended{KEY_E0}else{0}),..Default::default()})
 }}
impl Default for ScanDecoder{fn default()->Self{Self::new()}}

#[cfg(test)] mod tests{use super::*;#[test]fn decodes_make_break_and_extended(){let mut d=ScanDecoder::new();assert_eq!(d.feed(0x1e).unwrap().make_code,0x1e);assert_eq!(d.feed(0x9e).unwrap().flags&KEY_BREAK,KEY_BREAK);assert_eq!(d.feed(0xe0),None);let p=d.feed(0x4b).unwrap();assert_eq!(p.flags&KEY_E0,KEY_E0);assert_eq!(p.make_code,0x4b);}struct Fake{status:u8,data:u8}impl PortIo for Fake{fn read8(&mut self,p:u16)->u8{if p==STATUS_PORT{self.status}else{self.data}}fn write8(&mut self,_:u16,_:u8){}}#[test]fn wait_is_bounded(){let mut f=Fake{status:STATUS_INPUT_FULL,data:0};assert_eq!(wait_input_empty(&mut f,3),Err(PortError::Timeout));}}
