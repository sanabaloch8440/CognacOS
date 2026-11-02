//! Bounded, integer-only AML method execution core.
//! This constrained interpreter executes a safe scalar subset and fails closed
//! on unsupported AML terms and address-space access.
#![no_std]
use crate::aml::{AmlNamespace,AmlObjectType,decode_package_length};
pub const MAX_AML_STEPS:usize=100_000;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum ExecError{InvalidMethod,BadArgumentCount,Truncated,UnsupportedOpcode,StepLimit,BadTarget,BadPackage,LoopControl,ExecutionBusy,DivideByZero,NestingLimit}
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
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum ExecFlow{Continue,Return(u64),Break,ContinueLoop}
struct Machine{locals:[u64;8],args:[u64;7],steps:usize}
impl Machine{
 fn step(&mut self)->Result<(),ExecError>{self.steps+=1;if self.steps>MAX_AML_STEPS{Err(ExecError::StepLimit)}else{Ok(())}}
 fn target(r:&mut Reader<'_>)->Result<Target,ExecError>{let op=r.byte()?;match op{0x00=>Ok(Target::Null),0x60..=0x67=>Ok(Target::Local((op-0x60)as usize)),0x68..=0x6e=>Ok(Target::Arg((op-0x68)as usize)),_=>Err(ExecError::BadTarget)}}
 fn store(&mut self,t:Target,v:u64){match t{Target::Null=>{},Target::Local(i)=>self.locals[i]=v,Target::Arg(i)=>self.args[i]=v}}
 fn term(&mut self,r:&mut Reader<'_>)->Result<u64,ExecError>{self.term_depth(r,0)}
 fn term_depth(&mut self,r:&mut Reader<'_>,depth:u8)->Result<u64,ExecError>{
  if depth>32{return Err(ExecError::NestingLimit)}
  self.step()?;let op=r.byte()?;
  match op{
   0x00=>Ok(0),0x01=>Ok(1),0xff=>Ok(u64::MAX),0x0a=>Ok(r.byte()? as u64),
   0x0b=>{let b=r.take(2)?;Ok(u16::from_le_bytes([b[0],b[1]])as u64)},
   0x0c=>{let b=r.take(4)?;Ok(u32::from_le_bytes([b[0],b[1],b[2],b[3]])as u64)},
   0x0e=>{let b=r.take(8)?;Ok(u64::from_le_bytes(b.try_into().map_err(|_|ExecError::Truncated)?))},
   0x60..=0x67=>Ok(self.locals[(op-0x60)as usize]),0x68..=0x6e=>Ok(self.args[(op-0x68)as usize]),
   0x72|0x74|0x77|0x79|0x7a|0x7b|0x7c|0x7d|0x7e|0x7f|0x85=>{
    let a=self.term_depth(r,depth+1)?;let b=self.term_depth(r,depth+1)?;
    let v=match op{
     0x72=>a.wrapping_add(b),0x74=>a.wrapping_sub(b),0x77=>a.wrapping_mul(b),
     0x79=>if b>=64{0}else{a<<b},0x7a=>if b>=64{0}else{a>>b},
     0x7b=>a&b,0x7c=>!(a&b),0x7d=>a|b,0x7e=>!(a|b),0x85=>{if b==0{return Err(ExecError::DivideByZero)}a%b},_=>a^b
    };
    let t=Self::target(r)?;self.store(t,v);Ok(v)
   }
   0x81|0x82=>{
    let value=self.term_depth(r,depth+1)?;
    let result=if value==0{0}else if op==0x81{64-value.leading_zeros() as u64}else{value.trailing_zeros() as u64+1};
    let t=Self::target(r)?;self.store(t,result);Ok(result)
   }
   0x75|0x76=>{
    let t=Self::target(r)?;
    let current=match t{Target::Null=>0,Target::Local(i)=>self.locals[i],Target::Arg(i)=>self.args[i]};
    let value=if op==0x75{current.wrapping_add(1)}else{current.wrapping_sub(1)};
    self.store(t,value);Ok(value)
   }
   0x78=>{
    let dividend=self.term_depth(r,depth+1)?;let divisor=self.term_depth(r,depth+1)?;
    if divisor==0{return Err(ExecError::DivideByZero)}
    let remainder=dividend%divisor;let quotient=dividend/divisor;
    // AML Divide operand order is Dividend, Divisor, Remainder, Quotient.
    let rem=Self::target(r)?;let q=Self::target(r)?;
    self.store(rem,remainder);self.store(q,quotient);Ok(quotient)
   }
   0x80=>{let v=!self.term_depth(r,depth+1)?;let t=Self::target(r)?;self.store(t,v);Ok(v)},
   0x90=>{let a=self.term_depth(r,depth+1)?;let b=self.term_depth(r,depth+1)?;Ok((a!=0&&b!=0)as u64)},
   0x91=>{let a=self.term_depth(r,depth+1)?;let b=self.term_depth(r,depth+1)?;Ok((a!=0||b!=0)as u64)},
   0x92=>Ok((self.term_depth(r,depth+1)?==0)as u64),
   0x93|0x94|0x95=>{let a=self.term_depth(r,depth+1)?;let b=self.term_depth(r,depth+1)?;Ok(match op{0x93=>(a==b)as u64,0x94=>(a>b)as u64,_=>(a<b)as u64})}
   _=>Err(ExecError::UnsupportedOpcode)
  }
 }
 fn execute(&mut self,r:&mut Reader<'_>,depth:u8)->Result<ExecFlow,ExecError>{
  if depth>16{return Err(ExecError::LoopControl)}
  while r.pos<r.end{
   self.step()?;let op=r.byte()?;
   match op{
    0xa3=>{},
    0xa4=>return Ok(ExecFlow::Return(self.term(r)?)),
    0x70=>{let v=self.term(r)?;let t=Self::target(r)?;self.store(t,v);},
    0xa0|0xa2=>{
     let end=r.pkg_end()?;let plen=1+((r.bytes[r.pos]>>6)as usize);let begin=r.pos+plen;
     if begin>end{return Err(ExecError::BadPackage)}
     let mut body=Reader{bytes:r.bytes,pos:begin,end};
     let mut predicate=true;
     if op==0xa0{predicate=self.term(&mut body)?!=0;if predicate{let flow=self.execute(&mut body,depth+1)?;if flow!=ExecFlow::Continue{return Ok(flow);}}}
     else{
      let loop_start=body.pos;
      loop{self.step()?;let mut cond=Reader{bytes:body.bytes,pos:loop_start,end:body.end};if self.term(&mut cond)?==0{break}
       let mut iteration=Reader{bytes:body.bytes,pos:cond.pos,end:body.end};
       let flow=self.execute(&mut iteration,depth+1)?;
       if iteration.pos<=loop_start{return Err(ExecError::LoopControl)}
       match flow{ExecFlow::Continue=>{},ExecFlow::Return(v)=>return Ok(ExecFlow::Return(v)),ExecFlow::Break=>break,ExecFlow::ContinueLoop=>continue}
      }
     }
     r.pos=end;
     if op==0xa0 && r.pos<r.end && r.bytes[r.pos]==0xa1 {
      r.pos+=1;let else_end=r.pkg_end()?;let else_plen=1+((r.bytes[r.pos]>>6)as usize);let else_begin=r.pos+else_plen;
      if else_begin>else_end{return Err(ExecError::BadPackage)}
      if !predicate{let mut alternate=Reader{bytes:r.bytes,pos:else_begin,end:else_end};let flow=self.execute(&mut alternate,depth+1)?;if flow!=ExecFlow::Continue{return Ok(flow);}}
      r.pos=else_end;
     }
    }
    0xa1=>return Err(ExecError::UnsupportedOpcode),
    0x9f=>return Ok(ExecFlow::Break),
    0xa5=>return Ok(ExecFlow::ContinueLoop),
    _=>{r.pos-=1;let _=self.term(r)?;}
   }
  }
  Ok(ExecFlow::Continue)
 }
}

/// Evaluate one bounded TermArg without executing a method body. Returns the
/// value and number of bytes consumed, allowing typed object decoders to
/// evaluate BufferSize expressions without guessing their encoded length.
pub fn evaluate_term_argument(bytes:&[u8])->Result<(u64,usize),ExecError>{
 let mut machine=Machine{locals:[0;8],args:[0;7],steps:0};
 let mut reader=Reader{bytes,pos:0,end:bytes.len()};
 let value=machine.term(&mut reader)?;
 Ok((value,reader.pos))
}

/// Execute one loaded method against the exact AML definition-block byte slice.
/// Only integer arguments and scalar operations are currently supported.
pub fn execute_method(aml:&[u8],namespace:&AmlNamespace,method_index:u16,args:&[u64])->Result<AmlValue,ExecError>{
 let index=method_index as usize;
 if index>=namespace.count{return Err(ExecError::InvalidMethod)}
 let node=namespace.nodes.get(index).ok_or(ExecError::InvalidMethod)?;
 if node.object_type!=AmlObjectType::Method{return Err(ExecError::InvalidMethod)}
 let expected=(node.method_flags&7)as usize;if args.len()!=expected||args.len()>7{return Err(ExecError::BadArgumentCount)}
 let start=node.body_start as usize;let end=start.checked_add(node.body_length as usize).ok_or(ExecError::InvalidMethod)?;
 if end>aml.len(){return Err(ExecError::InvalidMethod)}
 let mut m=Machine{locals:[0;8],args:[0;7],steps:0};m.args[..args.len()].copy_from_slice(args);
 let mut r=Reader{bytes:aml,pos:start,end};let value=match m.execute(&mut r,0)?{ExecFlow::Continue=>0,ExecFlow::Return(v)=>v,ExecFlow::Break|ExecFlow::ContinueLoop=>return Err(ExecError::LoopControl)};Ok(AmlValue::Integer(value))
}
#[cfg(test)]
mod tests{
 use super::*;use crate::aml::{AmlNamespace,load_definition_block};
 #[test]fn executes_integer_method_with_arg_and_add(){
  let aml=[0x14,0x0c,b'T',b'E',b'S',b'T',1,0xa4,0x72,0x68,0x0a,2,0x00];
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
 #[test]fn executes_divide_and_stores_quotient_and_remainder(){
  // Divide stores remainder in Local0 and quotient in Local1; return their sum.
  let aml=[0x14,0x12,b'T',b'E',b'S',b'T',0,0x78,0x0a,23,0x0a,5,0x60,0x61,
           0xa4,0x72,0x60,0x61,0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]).unwrap(),AmlValue::Integer(7));
 }
 #[test]fn executes_modulo_and_rejects_zero_divisor(){
  let aml=[0x14,0x0e,b'T',b'E',b'S',b'T',0,0xa4,0x85,0x0a,23,0x0a,5,0x60,0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]).unwrap(),AmlValue::Integer(3));
  let zero=[0x14,0x0b,b'T',b'E',b'S',b'T',0,0xa4,0x85,0x0a,23,0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&zero,&mut ns).unwrap();let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&zero,&ns,id,&[]),Err(ExecError::DivideByZero));
 }
 #[test]fn executes_find_set_bit_operations_with_aml_bit_positions(){
  // FindSetLeftBit(0x8000) => 16; FindSetRightBit(0x8000) => 16.
  let left=[0x14,0x0e,b'T',b'E',b'S',b'T',0,0xa4,0x81,0x0c,0x00,0x80,0,0x00,0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&left,&mut ns).unwrap();let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&left,&ns,id,&[]).unwrap(),AmlValue::Integer(16));
  let right=[0x14,0x0e,b'T',b'E',b'S',b'T',0,0xa4,0x82,0x0c,0x00,0x80,0,0x00,0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&right,&mut ns).unwrap();let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&right,&ns,id,&[]).unwrap(),AmlValue::Integer(16));
  let zero=[0x14,0x0a,b'T',b'E',b'S',b'T',0,0xa4,0x81,0x00,0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&zero,&mut ns).unwrap();let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&zero,&ns,id,&[]).unwrap(),AmlValue::Integer(0));
 }
 #[test]fn rejects_divide_by_zero(){
  let aml=[0x14,0x0d,b'T',b'E',b'S',b'T',0,0xa4,0x78,0x0a,1,0x00,0x60,0x61];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]),Err(ExecError::DivideByZero));
 }
 #[test]fn shift_counts_at_or_above_integer_width_yield_zero(){
  let aml=[0x14,0x0d,b'T',b'E',b'S',b'T',0,0xa4,0x79,0x0a,1,0x0a,64,0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]).unwrap(),AmlValue::Integer(0));
 }
 #[test]fn executes_increment_decrement_and_bitwise_complements(){
  let aml=[0x14,0x13,b'T',b'E',b'S',b'T',0,0x70,0x0a,7,0x60,0x75,0x60,0x76,0x60,0xa4,0x7e,0x60,0x0a,0];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]).unwrap(),AmlValue::Integer(!7u64));
 }
 #[test]fn rejects_namespace_slots_outside_the_live_node_count(){
  let aml=[0x14,0x09,b'T',b'E',b'S',b'T',0,0xa4,0x0a,7];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  assert_eq!(execute_method(&aml,&ns,1,&[]),Err(ExecError::InvalidMethod));
  assert_eq!(execute_method(&aml,&ns,u16::MAX,&[]),Err(ExecError::InvalidMethod));
 }
 #[test]fn bounds_non_terminating_aml_while_loops(){
  // While (One) { Noop } must stop at the interpreter step budget.
  let aml=[0x14,0x09,b'T',b'E',b'S',b'T',0,0xa2,0x03,0x01,0xa3];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]),Err(ExecError::StepLimit));
 }
 #[test]fn rejects_truncated_term_arguments_at_runtime(){
  // The parser retains method bodies, so the executor must reject a
  // truncated DWordConst when the method is invoked.
  let aml=[0x14,0x0a,b'T',b'E',b'S',b'T',0,0xa4,0x0c,0x11,0x22];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]),Err(ExecError::Truncated));
 }
 #[test]fn evaluates_remaining_scalar_bitwise_and_shift_operations(){
  assert_eq!(evaluate_term_argument(&[0x7f,0x0a,0xf0,0x0a,0x0f,0x00]),Ok((0xff,6)));
  assert_eq!(evaluate_term_argument(&[0x7a,0x0a,0x80,0x0a,4,0x00]),Ok((8,6)));
  assert_eq!(evaluate_term_argument(&[0x7b,0x0a,0xf0,0x0a,0x0f,0x00]),Ok((0,6)));
  assert_eq!(evaluate_term_argument(&[0x7d,0x0a,0xf0,0x0a,0x0f,0x00]),Ok((0xff,6)));
  assert_eq!(evaluate_term_argument(&[0x77,0x0a,7,0x0a,6,0x00]),Ok((42,6)));
 }
 #[test]fn term_argument_decoder_reports_truncation_and_unsupported_names(){
  assert_eq!(evaluate_term_argument(&[0x0c,0x11,0x22]),Err(ExecError::Truncated));
  assert_eq!(evaluate_term_argument(&[b'V',b'A',b'L',b'0']),Err(ExecError::UnsupportedOpcode));
 }
 #[test]fn bounds_recursive_term_argument_nesting(){
  let mut nested=std::vec![0x92;34];
  nested.push(0x01);
  assert_eq!(evaluate_term_argument(&nested),Err(ExecError::NestingLimit));
  let mut allowed=std::vec![0x92;32];
  allowed.push(0x01);
  assert_eq!(evaluate_term_argument(&allowed),Ok((1,33)));
 }
 #[test]fn break_exits_the_nearest_while_loop(){
  let aml=[0x14,0x0f,b'T',b'E',b'S',b'T',0,
   0x70,0x01,0x60,
   0xa2,0x03,0x01,0x9f,
   0xa4,0x60];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]).unwrap(),AmlValue::Integer(1));
 }
 #[test]fn continue_rechecks_while_predicate_after_advancing_state(){
  let aml=[0x14,0x14,b'T',b'E',b'S',b'T',0,
   0x70,0x0a,2,0x60,
   0xa2,0x07,0x94,0x60,0x00,0x76,0x60,0xa5,
   0xa4,0x60];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]).unwrap(),AmlValue::Integer(0));
 }
 #[test]fn break_propagates_through_nested_if(){
  let aml=[0x14,0x10,b'T',b'E',b'S',b'T',0,
   0xa2,0x06,0x01,0xa0,0x03,0x01,0x9f,
   0xa4,0x0a,9];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]).unwrap(),AmlValue::Integer(9));
 }
 #[test]fn continue_propagates_through_nested_if(){
  let aml=[0x14,0x17,b'T',b'E',b'S',b'T',0,
   0x70,0x0a,2,0x60,
   0xa2,0x0a,0x94,0x60,0x00,0xa0,0x05,0x01,0x76,0x60,0xa5,
   0xa4,0x60];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]).unwrap(),AmlValue::Integer(0));
 }
 #[test]fn rejects_wrong_argument_count(){
  let aml=[0x14,0x09,b'T',b'E',b'S',b'T',0,0xa4,0x0a,7];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[1]),Err(ExecError::BadArgumentCount));
 }
}
