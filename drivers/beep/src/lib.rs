//! Windows x64 Beep.sys protocol and PC-speaker backend core.
#![no_std]

pub const FILE_DEVICE_BEEP:u32=0x0000_0001;
pub const IOCTL_BEEP_SET:u32=(FILE_DEVICE_BEEP<<16)|(0<<2)|0; // METHOD_BUFFERED, FILE_ANY_ACCESS
pub const BEEP_DEVICE_NAME:&[u16]=&[b'\\' as u16,b'D' as u16,b'e' as u16,b'v' as u16,b'i' as u16,b'c' as u16,b'e' as u16,b'\\' as u16,b'B' as u16,b'e' as u16,b'e' as u16,b'p' as u16,0];
pub const BEEP_DOS_NAME:&[u16]=&[b'\\' as u16,b'D' as u16,b'o' as u16,b's' as u16,b'D' as u16,b'e' as u16,b'v' as u16,b'i' as u16,b'c' as u16,b'e' as u16,b's' as u16,b'\\' as u16,b'B' as u16,b'e' as u16,b'e' as u16,b'p' as u16,0];
pub const STATUS_SUCCESS:i32=0;
pub const STATUS_INVALID_PARAMETER:i32=0xC000000D_u32 as i32;
pub const STATUS_INVALID_DEVICE_REQUEST:i32=0xC0000010_u32 as i32;
pub const STATUS_BUFFER_TOO_SMALL:i32=0xC0000023_u32 as i32;
pub const STATUS_DEVICE_NOT_READY:i32=0xC00000A3_u32 as i32;
pub const BEEP_MIN_FREQUENCY:u32=37;
pub const BEEP_MAX_FREQUENCY:u32=32767;
pub const BEEP_MAX_DURATION_MS:u32=10_000;
pub const PIT_INPUT_HZ:u32=1_193_182;
pub const SPEAKER_CONTROL_PORT:u16=0x61;
pub const PIT_CHANNEL2_PORT:u16=0x42;
pub const PIT_COMMAND_PORT:u16=0x43;
pub const SPEAKER_GATE:u8=1;
pub const SPEAKER_DATA:u8=2;

/// Exact Windows BEEP_SET_PARAMETERS wire layout: two ULONGs, 8 bytes.
#[repr(C)]
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct BeepSetParameters { pub frequency:u32, pub duration_ms:u32 }
const _:()=assert!(core::mem::size_of::<BeepSetParameters>()==8);

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum BeepError { InvalidFrequency,InvalidDuration,IoFailure,NotRunning }
pub fn validate(p:BeepSetParameters)->Result<(),BeepError>{
 if p.frequency!=0 && !(BEEP_MIN_FREQUENCY..=BEEP_MAX_FREQUENCY).contains(&p.frequency){return Err(BeepError::InvalidFrequency)}
 if p.duration_ms>BEEP_MAX_DURATION_MS{return Err(BeepError::InvalidDuration)}
 Ok(())
}
pub const fn pit_divisor(frequency:u32)->Option<u16>{
 if frequency==0{return None}
 let d=(PIT_INPUT_HZ as u64+frequency as u64/2)/frequency as u64;
 if d<1||d>65535{None}else{Some(d as u16)}
}
/// HAL adapter. Implementations must serialize port access and run only at a
/// permitted IRQL; the driver core never assumes arbitrary port I/O is safe.
pub trait SpeakerIo {
 fn read_port_u8(&mut self,port:u16)->Result<u8,BeepError>;
 fn write_port_u8(&mut self,port:u16,value:u8)->Result<(),BeepError>;
}
pub struct Speaker { active:bool }
impl Speaker {
 pub const fn new()->Self{Self{active:false}}
 pub const fn is_active(&self)->bool{self.active}
 pub fn stop<I:SpeakerIo>(&mut self,io:&mut I)->Result<(),BeepError>{
  let v=io.read_port_u8(SPEAKER_CONTROL_PORT)?;
  io.write_port_u8(SPEAKER_CONTROL_PORT,v&!(SPEAKER_GATE|SPEAKER_DATA))?;
  self.active=false;Ok(())
 }
 pub fn start<I:SpeakerIo>(&mut self,io:&mut I,p:BeepSetParameters)->Result<Option<u16>,BeepError>{
  validate(p)?;
  if p.frequency==0||p.duration_ms==0{self.stop(io)?;return Ok(None)}
  let divisor=pit_divisor(p.frequency).ok_or(BeepError::InvalidFrequency)?;
  // Program PIT channel 2, low byte then high byte, mode 3 square wave.
  io.write_port_u8(PIT_COMMAND_PORT,0xb6)?;
  io.write_port_u8(PIT_CHANNEL2_PORT,divisor as u8)?;
  io.write_port_u8(PIT_CHANNEL2_PORT,(divisor>>8) as u8)?;
  let control=io.read_port_u8(SPEAKER_CONTROL_PORT)?;
  io.write_port_u8(SPEAKER_CONTROL_PORT,control|SPEAKER_GATE|SPEAKER_DATA)?;
  self.active=true;Ok(Some(divisor))
 }
}
impl Default for Speaker{fn default()->Self{Self::new()}}

/// Generation ticket for timer-driven speaker shutdown. A kernel timer/DPC
/// stores the ticket returned by arm(); its callback must call expire(ticket).
/// cancel() invalidates queued callbacks before the device is stopped or unloaded.
/// This prevents an old timer callback from stopping a newer tone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpeakerDeadline { generation: u64, armed: bool }
impl SpeakerDeadline {
 pub const fn new()->Self { Self { generation: 0, armed: false } }
 /// Begin a new timed tone and return the callback ticket to associate with it.
 pub fn arm(&mut self)->u64 {
  self.generation=self.generation.wrapping_add(1);
  if self.generation==0 { self.generation=1; }
  self.armed=true;
  self.generation
 }
 /// Invalidate the outstanding timer/DPC ticket. Call KeCancelTimer and
 /// remove/flush its DPC in the WDM adapter as appropriate for its IRQL.
 pub fn cancel(&mut self) {
  self.generation=self.generation.wrapping_add(1);
  if self.generation==0 { self.generation=1; }
  self.armed=false;
 }
 /// Returns true exactly once for the currently armed ticket.
 pub fn expire(&mut self,ticket:u64)->bool {
  if !self.armed || ticket==0 || ticket!=self.generation { return false; }
  self.armed=false;
  true
 }
 pub const fn is_armed(&self)->bool { self.armed }
}

/// Validates the buffered IOCTL contract without dereferencing an untrusted
/// user pointer. The I/O manager must first capture METHOD_BUFFERED SystemBuffer.
pub fn decode_ioctl(code:u32,input:&[u8])->Result<BeepSetParameters,i32>{
 if code!=IOCTL_BEEP_SET{return Err(STATUS_INVALID_DEVICE_REQUEST)}
 if input.len()<8{return Err(STATUS_BUFFER_TOO_SMALL)}
 let p=BeepSetParameters{frequency:u32::from_le_bytes(input[0..4].try_into().unwrap()),duration_ms:u32::from_le_bytes(input[4..8].try_into().unwrap())};
 validate(p).map_err(|_|STATUS_INVALID_PARAMETER)?;Ok(p)
}

#[cfg(test)]
mod tests{
 use super::*;
 #[derive(Default)]struct Fake{control:u8,writes:std::vec::Vec<(u16,u8)>}
 impl SpeakerIo for Fake{
  fn read_port_u8(&mut self,p:u16)->Result<u8,BeepError>{assert_eq!(p,SPEAKER_CONTROL_PORT);Ok(self.control)}
  fn write_port_u8(&mut self,p:u16,v:u8)->Result<(),BeepError>{self.writes.push((p,v));if p==SPEAKER_CONTROL_PORT{self.control=v}Ok(())}
 }
 extern crate std;
 #[test]fn ioctl_and_wire_layout_match_wdm_contract(){assert_eq!(IOCTL_BEEP_SET,0x10000);assert_eq!(core::mem::size_of::<BeepSetParameters>(),8);let mut b=[0u8;8];b[..4].copy_from_slice(&440u32.to_le_bytes());b[4..].copy_from_slice(&100u32.to_le_bytes());assert_eq!(decode_ioctl(IOCTL_BEEP_SET,&b).unwrap(),BeepSetParameters{frequency:440,duration_ms:100});}
 #[test]fn rejects_unknown_ioctl_short_buffer_and_invalid_values(){assert_eq!(decode_ioctl(0,&[0;8]),Err(STATUS_INVALID_DEVICE_REQUEST));assert_eq!(decode_ioctl(IOCTL_BEEP_SET,&[0;4]),Err(STATUS_BUFFER_TOO_SMALL));assert_eq!(validate(BeepSetParameters{frequency:36,duration_ms:1}),Err(BeepError::InvalidFrequency));assert_eq!(validate(BeepSetParameters{frequency:440,duration_ms:10_001}),Err(BeepError::InvalidDuration));}
 #[test]fn stale_timer_ticket_cannot_stop_a_replacement_tone(){
  let mut timer=SpeakerDeadline::new();
  let first=timer.arm();
  timer.cancel();
  let second=timer.arm();
  assert_ne!(first,second);
  assert!(!timer.expire(first));
  assert!(timer.is_armed());
  assert!(timer.expire(second));
  assert!(!timer.expire(second));
  assert!(!timer.is_armed());
 }
 #[test]fn pit_divisor_is_rounded_and_bounded(){assert_eq!(pit_divisor(0),None);assert_eq!(pit_divisor(440),Some(2712));assert_eq!(pit_divisor(37),Some(32248));assert_eq!(pit_divisor(32767),Some(36));}
 #[test]fn programs_channel_two_and_toggles_speaker_gate(){let mut io=Fake::default();let mut s=Speaker::new();assert_eq!(s.start(&mut io,BeepSetParameters{frequency:440,duration_ms:100}).unwrap(),Some(2712));assert!(s.is_active());assert!(io.writes.contains(&(PIT_COMMAND_PORT,0xb6)));assert_eq!(s.stop(&mut io),Ok(()));assert!(!s.is_active());assert_eq!(io.control&3,0);}
}
