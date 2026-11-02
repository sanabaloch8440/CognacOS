//! Bounded, integer-only AML method execution core.
//! This constrained interpreter executes a safe scalar subset and fails closed
//! on unsupported AML terms and address-space access.
#![no_std]
use crate::aml::{AmlNamespace,AmlObjectType,decode_package_length};
pub const MAX_AML_STEPS:usize=100_000;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum ExecError{InvalidMethod,BadArgumentCount,Truncated,UnsupportedOpcode,StepLimit,BadTarget,BadPackage,LoopControl}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum AmlValue{Integer(u64)}
impl AmlValue{pub const fn integer(self)->u64{match self{Self::Integer(v)=>v}}}
#[derive(Clone,Copy)]
struct Reader<'a>{bytes:&'a [u8],pos:usize,end:usize}
impl<'a> Reader<'a>{
 fn byte(&mut self)->Result<u8,ExecError>{if self.pos>=self.end{return Err(ExecError::Truncated)}let v=self.bytes[self.pos];self.pos+=1;Ok(v)}
 fn take(&mut self,n:usize)->Result<&'a [u8],ExecError>{let e=self.pos.checked_add(n).ok_or(ExecError::Truncated)?;if e>self.end{return Err(ExecError::Truncated)}let s=&self.bytes[self.pos..e];self.pos=e;Ok(s)}
 fn pkg_end(&mut self)->Result<usize,ExecError>{let (_,e)=decode_package_length(self.bytes,self.pos,self.end).map_err(|_|ExecError::BadPackage)?;Ok(e)}
}
#[derive(Clone,Copy)]
enum Target{Null,Local(usize),Arg(usize)}
struct Machine{locals:[u64;8],args:[u64;7],steps:usize}
impl Machine{
 fn step(&mut self)->Result<(),ExecError>{self.steps+=1;if self.steps>MAX_AML_STEPS{Err(ExecError::StepLimit)}else{Ok(())}}
 fn target(r:&mut Reader<'_>)->Result<Target,ExecError>{let op=r.byte()?;match op{0x00=>Ok(Target::Null),0x60..=0x67=>Ok(Target::Local((op-0x60)as usize)),0x68..=0x6e=>Ok(Target::Arg((op-0x68)as usize)),_=>Err(ExecError::BadTarget)}}
 fn store(&mut self,t:Target,v:u64){match t{Target::Null=>{},Target::Local(i)=>self.locals[i]=v,Target::Arg(i)=>self.args[i]=v}}
 fn term(&mut self,r:&mut Reader<'_>)->Result<u64,ExecError>{
  self.step()?;let op=r.byte()?;
  match op{
   0x00=>Ok(0),0x01=>Ok(1),0xff=>Ok(u64::MAX),0x0a=>Ok(r.byte()? as u64),
   0x0b=>{let b=r.take(2)?;Ok(u16::from_le_bytes([b[0],b[1]])as u64)},
   0x0c=>{let b=r.take(4)?;Ok(u32::from_le_bytes([b[0],b[1],b[2],b[3]])as u64)},
   0x0e=>{let b=r.take(8)?;Ok(u64::from_le_bytes(b.try_into().map_err(|_|ExecError::Truncated)?))},
   0x60..=0x67=>Ok(self.locals[(op-0x60)as usize]),0x68..=0x6e=>Ok(self.args[(op-0x68)as usize]),
   0x72|0x74|0x77|0x79|0x7a|0x7b|0x7d|0x7f=>{
    let a=self.term(r)?;let b=self.term(r)?;
    let v=match op{0x72=>a.wrapping_add(b),0x74=>a.wrapping_sub(b),0x77=>a.wrapping_mul(b),0x79=>a.wrapping_shl((b&63)as u32),0x7a=>a.wrapping_shr((b&63)as u32),0x7b=>a&b,0x7d=>a|b,_=>a^b};
    let t=Self::target(r)?;self.store(t,v);Ok(v)
   }
   0x80=>{let v=!self.term(r)?;let t=Self::target(r)?;self.store(t,v);Ok(v)},
   0x90=>{let a=self.term(r)?;let b=self.term(r)?;Ok((a!=0&&b!=0)as u64)},
   0x91=>{let a=self.term(r)?;let b=self.term(r)?;Ok((a!=0||b!=0)as u64)},
   0x92=>Ok((self.term(r)?==0)as u64),
   0x93|0x94|0x95=>{let a=self.term(r)?;let b=self.term(r)?;Ok(match op{0x93=>(a==b)as u64,0x94=>(a>b)as u64,_=>(a<b)as u64})}
   _=>Err(ExecError::UnsupportedOpcode)
  }
 }
 fn execute(&mut self,r:&mut Reader<'_>,depth:u8)->Result<Option<u64>,ExecError>{
  if depth>16{return Err(ExecError::LoopControl)}
  while r.pos<r.end{
   self.step()?;let op=r.byte()?;
   match op{
    0xa3=>{},
    0xa4=>return Ok(Some(self.term(r)?)),
    0x70=>{let v=self.term(r)?;let t=Self::target(r)?;self.store(t,v);},
    0xa0|0xa2=>{
     let end=r.pkg_end()?;let plen=1+((r.bytes[r.pos]>>6)as usize);let begin=r.pos+plen;
     if begin>end{return Err(ExecError::BadPackage)}
     let mut body=Reader{bytes:r.bytes,pos:begin,end};
     let mut predicate=true;
     if op==0xa0{predicate=self.term(&mut body)?!=0;if predicate{if let Some(v)=self.execute(&mut body,depth+1)?{return Ok(Some(v));}}}
     else{
      let loop_start=body.pos;
      loop{self.step()?;let mut cond=Reader{bytes:body.bytes,pos:loop_start,end:body.end};if self.term(&mut cond)?==0{break}
       let mut iteration=Reader{bytes:body.bytes,pos:cond.pos,end:body.end};if let Some(v)=self.execute(&mut iteration,depth+1)?{return Ok(Some(v))}
       if iteration.pos<=loop_start{return Err(ExecError::LoopControl)}
      }
     }
     r.pos=end;
     if op==0xa0 && r.pos<r.end && r.bytes[r.pos]==0xa1 {
      r.pos+=1;let else_end=r.pkg_end()?;let else_plen=1+((r.bytes[r.pos]>>6)as usize);let else_begin=r.pos+else_plen;
      if else_begin>else_end{return Err(ExecError::BadPackage)}
      if !predicate{let mut alternate=Reader{bytes:r.bytes,pos:else_begin,end:else_end};if let Some(v)=self.execute(&mut alternate,depth+1)?{return Ok(Some(v));}}
      r.pos=else_end;
     }
    }
    0xa1=>return Err(ExecError::UnsupportedOpcode),
    0x9f|0xa5=>return Err(ExecError::LoopControl),
    _=>{r.pos-=1;let _=self.term(r)?;}
   }
  }
  Ok(None)
 }
}
/// Execute one loaded method against the exact AML definition-block byte slice.
/// Only integer arguments and scalar operations are currently supported.
pub fn execute_method(aml:&[u8],namespace:&AmlNamespace,method_index:u16,args:&[u64])->Result<AmlValue,ExecError>{
 let node=namespace.nodes.get(method_index as usize).ok_or(ExecError::InvalidMethod)?;
 if node.object_type!=AmlObjectType::Method{return Err(ExecError::InvalidMethod)}
 let expected=(node.method_flags&7)as usize;if args.len()!=expected||args.len()>7{return Err(ExecError::BadArgumentCount)}
 let start=node.body_start as usize;let end=start.checked_add(node.body_length as usize).ok_or(ExecError::InvalidMethod)?;
 if end>aml.len(){return Err(ExecError::InvalidMethod)}
 let mut m=Machine{locals:[0;8],args:[0;7],steps:0};m.args[..args.len()].copy_from_slice(args);
 let mut r=Reader{bytes:aml,pos:start,end};let v=m.execute(&mut r,0)?.unwrap_or(0);Ok(AmlValue::Integer(v))
}
#[cfg(test)]
mod tests{
 use super::*;use crate::aml::{AmlNamespace,load_definition_block};
 #[test]fn executes_integer_method_with_arg_and_add(){
  let aml=[0x14,0x0d,b'T',b'E',b'S',b'T',1,0xa4,0x72,0x68,0x0a,2,0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[40]).unwrap(),AmlValue::Integer(42));
 }
 #[test]fn executes_if_else_and_comparison_predicate(){
  // If (LEqual (Arg0, One)) Return (7); Else Return (9)
  let aml=[0x14,0x13,b'T',b'E',b'S',b'T',1,0xa0,0x07,0x93,0x68,0x01,0xa4,0x0a,7,0xa1,0x04,0xa4,0x0a,9];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[1]).unwrap(),AmlValue::Integer(7));
  assert_eq!(execute_method(&aml,&ns,id,&[0]).unwrap(),AmlValue::Integer(9));
 }
 #[test]fn rejects_wrong_argument_count(){
  let aml=[0x14,0x0b,b'T',b'E',b'S',b'T',0,0xa4,0x0a,7];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[1]),Err(ExecError::BadArgumentCount));
 }
}
