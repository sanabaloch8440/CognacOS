//! E820 reservation clipping for PCI host-bridge CRS apertures.
//!
//! Apply this once to decoded host-bridge windows before publishing them to
//! the PCI resource arbiter. Never apply it to arbitrary BAR allocations.
//! A platform quirk may disable clipping when firmware reserves an entire
//! CRS window even though that window is needed for hot-plug capacity.
#![no_std]
use crate::resource_manager::Aperture;
use crate::resources::BarKind;
pub const MAX_E820_RANGES: usize = 256;
pub const MAX_CLIPPED_APERTURES: usize = 64;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct E820Range { pub start: u64, pub length: u64 }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipError { InvalidRange, Overflow, Capacity }
fn end(start:u64,length:u64)->Result<u64,ClipError>{
 if length==0{return Err(ClipError::InvalidRange)}
 start.checked_add(length).ok_or(ClipError::Overflow)
}
/// Subtract reserved E820 memory ranges from host-bridge memory apertures.
/// I/O apertures pass through unchanged. Output is staged and committed only
/// after every input range has been validated.
pub fn clip_host_apertures(input:&[Aperture],reserved:&[E820Range],output:&mut[Aperture])->Result<usize,ClipError>{
 if input.len()>MAX_CLIPPED_APERTURES||reserved.len()>MAX_E820_RANGES{return Err(ClipError::Capacity)}
 let empty=Aperture{start:0,length:0,kind:BarKind::Io,prefetchable:false};
 let mut staged=[empty;MAX_CLIPPED_APERTURES];let mut count=0usize;
 for aperture in input.iter().copied(){
  end(aperture.start,aperture.length)?;
  if aperture.kind==BarKind::Io{if count==staged.len(){return Err(ClipError::Capacity)}staged[count]=aperture;count+=1;continue}
  let mut fragments=[E820Range{start:0,length:0};MAX_CLIPPED_APERTURES];let mut n=1usize;
  fragments[0]=E820Range{start:aperture.start,length:aperture.length};
  for r in reserved.iter().copied(){
   let re=end(r.start,r.length)?;
   let mut next=[E820Range{start:0,length:0};MAX_CLIPPED_APERTURES];let mut nn=0usize;
   for f in fragments[..n].iter().copied(){
    let fe=end(f.start,f.length)?;
    if r.start>=fe||re<=f.start{if nn==next.len(){return Err(ClipError::Capacity)}next[nn]=f;nn+=1;continue}
    if r.start>f.start{if nn==next.len(){return Err(ClipError::Capacity)}next[nn]=E820Range{start:f.start,length:r.start-f.start};nn+=1}
    if re<fe{if nn==next.len(){return Err(ClipError::Capacity)}next[nn]=E820Range{start:re,length:fe-re};nn+=1}
   }
   fragments=next;n=nn;if n==0{break}
  }
  for f in fragments[..n].iter().copied(){if count==staged.len(){return Err(ClipError::Capacity)}staged[count]=Aperture{start:f.start,length:f.length,kind:aperture.kind,prefetchable:aperture.prefetchable};count+=1}
 }
 if output.len()<count{return Err(ClipError::Capacity)}
 output[..count].copy_from_slice(&staged[..count]);Ok(count)
}
/// Policy-aware root-window preparation. ignore_e820 must come only from the
/// validated immutable platform quirk policy.
pub fn prepare_host_apertures(input:&[Aperture],reserved:&[E820Range],ignore_e820:bool,output:&mut[Aperture])->Result<usize,ClipError>{
 if ignore_e820{if output.len()<input.len(){return Err(ClipError::Capacity)}output[..input.len()].copy_from_slice(input);return Ok(input.len())}
 clip_host_apertures(input,reserved,output)
}
#[cfg(test)]
mod tests{
 use super::*;
 fn ap(start:u64,length:u64)->Aperture{Aperture{start,length,kind:BarKind::Memory32,prefetchable:false}}
 #[test]fn clips_intersection_and_preserves_both_sides(){let input=[ap(0x8000,0x8000)];let r=[E820Range{start:0x9000,length:0x2000}];let mut out=[ap(0,0);4];assert_eq!(clip_host_apertures(&input,&r,&mut out),Ok(2));assert_eq!(out[0],ap(0x8000,0x1000));assert_eq!(out[1],ap(0xb000,0x5000))}
 #[test]fn full_window_reservation_and_opt_out(){let input=[ap(0x8000,0x4000)];let r=[E820Range{start:0x7000,length:0x6000}];let mut out=[ap(0,0);2];assert_eq!(clip_host_apertures(&input,&r,&mut out),Ok(0));assert_eq!(prepare_host_apertures(&input,&r,true,&mut out),Ok(1));assert_eq!(out[0],input[0])}
 #[test]fn io_windows_are_not_clipped(){let a=Aperture{start:0x1000,length:0x100,kind:BarKind::Io,prefetchable:false};let r=[E820Range{start:0x1000,length:0x100}];let mut out=[a;1];assert_eq!(clip_host_apertures(&[a],&r,&mut out),Ok(1));assert_eq!(out[0],a)}
 #[test]fn invalid_reservation_does_not_mutate_output(){let input=[ap(0x8000,0x1000)];let r=[E820Range{start:0x9000,length:0}];let mut out=[ap(0xdead,0xbeef);2];let before=out;assert_eq!(clip_host_apertures(&input,&r,&mut out),Err(ClipError::InvalidRange));assert_eq!(out,before)}
}
