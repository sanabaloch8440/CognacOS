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


pub const CMD_ENABLE_SCANNING:u8=0xf4;
pub const CMD_DISABLE_SCANNING:u8=0xf5;
pub const CONFIG_KEYBOARD_IRQ:u8=1<<0;
pub const CONFIG_TRANSLATION:u8=1<<6;
pub trait PowerAwarePortIo:PortIo{
 fn set_keyboard_irq_enabled(&mut self,enabled:bool)->Result<(),PortError>;
 fn synchronize_keyboard_irq(&mut self)->Result<(),PortError>;
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum KeyboardPortState{Offline,Running,Suspended,Failed}
/// Owns PS/2 startup and D0 transitions. IRQ delivery is masked and synchronized
/// before touching the controller; it is restored only after scanning is enabled.
pub struct Ps2KeyboardPort{state:KeyboardPortState,decoder:ScanDecoder}
impl Ps2KeyboardPort{
 pub const fn new()->Self{Self{state:KeyboardPortState::Offline,decoder:ScanDecoder::new()}}
 pub const fn state(&self)->KeyboardPortState{self.state}
 fn controller_response<I:PortIo>(io:&mut I,cmd:u8,limit:u32)->Result<u8,PortError>{controller_command(io,cmd,limit)?;wait_output_full(io,limit)}
 fn write_config<I:PortIo>(io:&mut I,value:u8,limit:u32)->Result<(),PortError>{controller_command(io,CMD_WRITE_CONFIG,limit)?;wait_input_empty(io,limit)?;io.write8(DATA_PORT,value);Ok(())}
 fn configure<I:PortIo>(io:&mut I,limit:u32)->Result<(),PortError>{
  if Self::controller_response(io,CMD_SELF_TEST,limit)?!=0x55{return Err(PortError::ControllerFailure)}
  controller_command(io,CMD_DISABLE_KEYBOARD,limit)?;
  let config=Self::controller_response(io,CMD_READ_CONFIG,limit)?;
  Self::write_config(io,config|CONFIG_KEYBOARD_IRQ|CONFIG_TRANSLATION,limit)?;
  if Self::controller_response(io,CMD_TEST_KEYBOARD,limit)?!=0{return Err(PortError::KeyboardFailure)}
  controller_command(io,CMD_ENABLE_KEYBOARD,limit)?;
  keyboard_command(io,CMD_ENABLE_SCANNING,limit)?;
  Ok(())
 }
 pub fn start<I:PowerAwarePortIo>(&mut self,io:&mut I,limit:u32)->Result<(),PortError>{
  io.set_keyboard_irq_enabled(false)?;io.synchronize_keyboard_irq()?;
  self.state=KeyboardPortState::Offline;
  if let Err(e)=Self::configure(io,limit){self.state=KeyboardPortState::Failed;let _=io.set_keyboard_irq_enabled(false);return Err(e)}
  if let Err(e)=io.set_keyboard_irq_enabled(true){self.state=KeyboardPortState::Failed;return Err(e)}
  self.state=KeyboardPortState::Running;Ok(())
 }
 pub fn suspend<I:PowerAwarePortIo>(&mut self,io:&mut I,limit:u32)->Result<(),PortError>{
  if self.state!=KeyboardPortState::Running{return Err(PortError::ControllerFailure)}
  io.set_keyboard_irq_enabled(false)?;io.synchronize_keyboard_irq()?;
  if let Err(e)=keyboard_command(io,CMD_DISABLE_SCANNING,limit).and_then(|_|controller_command(io,CMD_DISABLE_KEYBOARD,limit)){
   self.state=KeyboardPortState::Failed;return Err(e)
  }
  self.state=KeyboardPortState::Suspended;Ok(())
 }
 pub fn resume<I:PowerAwarePortIo>(&mut self,io:&mut I,limit:u32)->Result<(),PortError>{
  if self.state!=KeyboardPortState::Suspended{return Err(PortError::ControllerFailure)}
  self.start(io,limit)
 }
 pub fn feed_scancode(&mut self,byte:u8)->Option<KeyboardInputData>{if self.state==KeyboardPortState::Running{self.decoder.feed(byte)}else{None}}
}
impl Default for Ps2KeyboardPort{fn default()->Self{Self::new()}}

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

#[cfg(test)]
mod power_lifecycle_tests {
 use super::*;
 use std::vec::Vec;
 struct Fake { status:u8, config:u8, expect_config:bool, responses:Vec<u8>, irq:bool, syncs:u32 }
 impl Fake { fn new()->Self{Self{status:0,config:CONFIG_TRANSLATION,expect_config:false,responses:Vec::new(),irq:false,syncs:0}} }
 impl PortIo for Fake {
  fn read8(&mut self,p:u16)->u8{
   if p==STATUS_PORT { if self.responses.is_empty(){self.status&=!STATUS_OUTPUT_FULL}else{self.status|=STATUS_OUTPUT_FULL};self.status }
   else { if self.responses.is_empty(){0}else{self.responses.remove(0)} }
  }
  fn write8(&mut self,p:u16,v:u8){
   if p==STATUS_PORT { match v { CMD_SELF_TEST=>self.responses.push(0x55), CMD_TEST_KEYBOARD=>self.responses.push(0), CMD_READ_CONFIG=>self.responses.push(self.config), CMD_WRITE_CONFIG=>self.expect_config=true, _=>{} } }
   else if self.expect_config { self.config=v;self.expect_config=false; }
   else if v==CMD_ENABLE_SCANNING||v==CMD_DISABLE_SCANNING { self.responses.push(KEYBOARD_ACK); }
  }
 }
 impl PowerAwarePortIo for Fake {
  fn set_keyboard_irq_enabled(&mut self,e:bool)->Result<(),PortError>{self.irq=e;Ok(())}
  fn synchronize_keyboard_irq(&mut self)->Result<(),PortError>{self.syncs+=1;Ok(())}
 }
 #[test] fn keyboard_input_is_gated_during_suspend_and_restored_after_resume(){
  let mut io=Fake::new();let mut keyboard=Ps2KeyboardPort::new();
  assert_eq!(keyboard.start(&mut io,32),Ok(()));assert!(io.irq);assert_eq!(keyboard.state(),KeyboardPortState::Running);
  assert!(keyboard.feed_scancode(0x1e).is_some());
  assert_eq!(keyboard.suspend(&mut io,32),Ok(()));assert!(!io.irq);assert_eq!(keyboard.state(),KeyboardPortState::Suspended);
  assert!(keyboard.feed_scancode(0x30).is_none());
  assert_eq!(keyboard.resume(&mut io,32),Ok(()));assert!(io.irq);assert_eq!(keyboard.state(),KeyboardPortState::Running);
  assert!(keyboard.feed_scancode(0x2e).is_some());assert!(io.syncs>=3);
 }
 #[test] fn failed_initialization_never_enables_irq(){
  struct Broken(Fake);
  impl PortIo for Broken{fn read8(&mut self,p:u16)->u8{if p==STATUS_PORT{STATUS_OUTPUT_FULL}else{0}}fn write8(&mut self,_:u16,_:u8){}}
  impl PowerAwarePortIo for Broken{fn set_keyboard_irq_enabled(&mut self,e:bool)->Result<(),PortError>{self.0.irq=e;Ok(())}fn synchronize_keyboard_irq(&mut self)->Result<(),PortError>{Ok(())}}
  let mut io=Broken(Fake::new());let mut keyboard=Ps2KeyboardPort::new();
  assert_eq!(keyboard.start(&mut io,2),Err(PortError::ControllerFailure));assert!(!io.0.irq);assert_eq!(keyboard.state(),KeyboardPortState::Failed);
 }
}
