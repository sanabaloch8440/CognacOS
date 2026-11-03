//! Bounded AML method execution with typed scalar/data operands and target stores.
//! This constrained interpreter fails closed on unsupported AML terms and hardware access.
/*
 * FIXME(ACPI-PRODUCTION) — AML method execution
 * FIXME(ACPI-PRODUCTION): This is a constrained AML executor, not yet a full production interpreter. Intended completion: exact operand and target semantics, control flow, references, method arguments/locals and serialization, mutexes/events/Global Lock, namespace mutation, OperationRegion/Field handlers, module-level execution, _OSI policy, budgets/timeouts, and cleanup on every exit path. Keep hardware access behind validated region handlers; never execute AML in interrupt context. Add differential tests against ACPICA for every supported opcode.
 */

#![no_std]
use crate::aml::{AmlNamespace,AmlObjectType,decode_package_length,parse_name_string,resolve_name_string};
use crate::aml_objects::{decode_namespace_name,AmlObjectArena,AmlRuntimeValue,AmlValueKind};
use crate::aml_runtime::{to_integer_implicit,AmlBindings,AmlReference,RuntimeError,StoreMode,dereference,store_reference_with_mode,make_object_reference,reference_from_value};
pub const MAX_AML_STEPS:usize=100_000;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum ExecError{InvalidMethod,BadArgumentCount,Truncated,UnsupportedOpcode,StepLimit,BadTarget,BadPackage,LoopControl,ExecutionBusy,DivideByZero,NestingLimit,InvalidName,NameNotFound,NonIntegerName,CallDepthLimit,TargetTypeMismatch,UnsupportedReturnType}
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
enum Target{Null,Local(usize),Arg(usize),Reference(AmlReference)}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum ExecFlow{Continue,Return(AmlRuntimeValue),Break,ContinueLoop}
struct Machine<'a,'b>{locals:[AmlRuntimeValue;8],args:[AmlRuntimeValue;7],arena:&'b mut AmlObjectArena,bindings:&'b mut AmlBindings,integer_width_bits:u8,steps:usize,aml:&'a [u8],namespace:&'a AmlNamespace,scope:u16,call_depth:u8}
impl<'a,'b> Machine<'a,'b>{
 fn step(&mut self)->Result<(),ExecError>{self.steps+=1;if self.steps>MAX_AML_STEPS{Err(ExecError::StepLimit)}else{Ok(())}}
 fn name_reference(&mut self,node:u16)->Result<AmlReference,ExecError>{
  let reference=self.bindings.reference_node(self.namespace,node).map_err(|_|ExecError::BadTarget)?;
  match dereference(reference,&self.bindings,&self.arena){
   Ok(_)=>Ok(reference),
   Err(RuntimeError::UnboundNode)=>{
    let value=decode_namespace_name(self.aml,self.namespace,node,&mut self.arena)
        .map_err(|_|ExecError::NonIntegerName)?;
    self.bindings.bind_checked(self.namespace,node,value,&self.arena)
        .map_err(|_|ExecError::NonIntegerName)?;
    Ok(reference)
   }
   Err(_)=>Err(ExecError::NonIntegerName),
  }
 }
 fn target(&mut self,r:&mut Reader<'_>)->Result<Target,ExecError>{
  let op=r.byte()?;
  match op{
   0x00=>Ok(Target::Null),
   0x60..=0x67=>Ok(Target::Local((op-0x60)as usize)),
   0x68..=0x6e=>Ok(Target::Arg((op-0x68)as usize)),
   _=>{
    r.pos-=1;
    let (name,next)=parse_name_string(r.bytes,r.pos,r.end).map_err(|_|ExecError::BadTarget)?;
    r.pos=next;
    let node=resolve_name_string(self.namespace,self.scope,&name).map_err(|_|ExecError::BadTarget)?;
    let canonical=self.namespace.resolve_alias(node).map_err(|_|ExecError::BadTarget)?;
    let entry=self.namespace.nodes.get(canonical as usize)
        .filter(|_|(canonical as usize)<self.namespace.count).ok_or(ExecError::BadTarget)?;
    if entry.object_type!=AmlObjectType::Name{return Err(ExecError::BadTarget)}
    Ok(Target::Reference(self.name_reference(canonical)?))
   }
  }
 }
 fn integer_value(value:u64)->AmlRuntimeValue{let mut out=AmlRuntimeValue::EMPTY;out.kind=AmlValueKind::Integer;out.integer=value;out}
 fn uninitialized_value()->AmlRuntimeValue{let mut out=AmlRuntimeValue::EMPTY;out.kind=AmlValueKind::Uninitialized;out}
 fn integer_mask(&self)->u64{if self.integer_width_bits==32{u32::MAX as u64}else{u64::MAX}}
 fn normalize(&self,value:u64)->u64{value&self.integer_mask()}
 fn operand_integer(&self,value:&AmlRuntimeValue)->Result<u64,ExecError>{to_integer_implicit(value,self.integer_width_bits).map_err(|_|ExecError::NonIntegerName)}
 fn store(&mut self,t:Target,v:u64)->Result<(),ExecError>{
  self.store_value(t,Self::integer_value(self.normalize(v)),StoreMode::Store)
 }
 fn term(&mut self,r:&mut Reader<'_>)->Result<u64,ExecError>{self.term_depth(r,0)}
 fn store_reference_value(&mut self,reference:AmlReference,value:AmlRuntimeValue,
    mode:StoreMode)->Result<(),ExecError>{
  let checkpoint=self.arena.count;
  let id=self.arena.allocate(value).map_err(|_|ExecError::TargetTypeMismatch)?;
  match store_reference_with_mode(reference,id,mode,&mut self.bindings,self.namespace,self.arena){
   Ok(())=>Ok(()),
   Err(error)=>{
    self.arena.count=checkpoint;
    Err(match error{RuntimeError::TypeMismatch|RuntimeError::ValueTooLarge|RuntimeError::EmptyBuffer|RuntimeError::InvalidIntegerString=>ExecError::TargetTypeMismatch,_=>ExecError::NonIntegerName})
   }
  }
 }
 fn argument_value(&mut self,index:usize)->Result<AmlRuntimeValue,ExecError>{
  if index>=7{return Err(ExecError::BadArgumentCount)}
  let value=self.args[index];
  if value.kind==AmlValueKind::ObjectReference{
   let reference=reference_from_value(&value).map_err(|_|ExecError::NonIntegerName)?;
   dereference(reference,&self.bindings,self.arena).map_err(|_|ExecError::NonIntegerName)
  }else{Ok(value)}
 }
 fn store_value(&mut self,t:Target,value:AmlRuntimeValue,mode:StoreMode)->Result<(),ExecError>{
  match t{
   Target::Null=>Ok(()),
   Target::Local(i)=>{self.locals[i]=value;Ok(())},
   Target::Arg(i)=>{
    let current=self.args[i];
    if current.kind==AmlValueKind::ObjectReference{
     let reference=reference_from_value(&current).map_err(|_|ExecError::NonIntegerName)?;
     self.store_reference_value(reference,value,mode)
    }else{self.args[i]=value;Ok(())}
   }
   Target::Reference(reference)=>self.store_reference_value(reference,value,mode),
  }
 }
 /// Evaluate a TermArg without prematurely coercing Local/Arg/Name objects.
 /// Operators fall back to the scalar evaluator and therefore produce Integer.
 fn term_value(&mut self,r:&mut Reader<'_>)->Result<AmlRuntimeValue,ExecError>{
  let start=r.pos;let op=r.byte()?;
  match op{
   0x00=>Ok(Self::integer_value(0)),0x01=>Ok(Self::integer_value(1)),
   0xff=>Ok(Self::integer_value(self.integer_mask())),
   0x0a=>Ok(Self::integer_value(r.byte()? as u64)),
   0x0b=>{let b=r.take(2)?;Ok(Self::integer_value(u16::from_le_bytes([b[0],b[1]]) as u64))},
   0x0c=>{let b=r.take(4)?;Ok(Self::integer_value(u32::from_le_bytes([b[0],b[1],b[2],b[3]]) as u64))},
   0x0e=>{
    if self.integer_width_bits==32{return Err(ExecError::UnsupportedOpcode)}
    let b=r.take(8)?;Ok(Self::integer_value(u64::from_le_bytes(b.try_into().map_err(|_|ExecError::Truncated)?)))
   }
   0x60..=0x67=>Ok(self.locals[(op-0x60) as usize]),
   0x68..=0x6e=>self.argument_value((op-0x68) as usize),
   0x5c|0x5e|0x2e|0x2f|b'_'|b'A'..=b'Z'=>{
    r.pos=start;
    let (name,next)=parse_name_string(r.bytes,r.pos,r.end).map_err(|_|ExecError::InvalidName)?;
    r.pos=next;
    let node=resolve_name_string(self.namespace,self.scope,&name).map_err(|_|ExecError::NameNotFound)?;
    let canonical=self.namespace.resolve_alias(node).map_err(|_|ExecError::NameNotFound)?;
    let entry=*self.namespace.nodes.get(canonical as usize)
        .filter(|_|(canonical as usize)<self.namespace.count).ok_or(ExecError::NameNotFound)?;
    match entry.object_type{
     AmlObjectType::Name=>{
      let reference=self.name_reference(canonical)?;
      dereference(reference,&self.bindings,&self.arena).map_err(|_|ExecError::NonIntegerName)
     }
     AmlObjectType::Method=>{
      let count=(entry.method_flags&7) as usize;
      let mut call_args=[Self::integer_value(0);7];
      for slot in call_args.iter_mut().take(count){*slot=self.term_value(r)?;}
      execute_method_inner(self.aml,self.namespace,canonical,
          &call_args[..count],self.call_depth+1,self.integer_width_bits,self.arena,self.bindings)
     }
     _=>Err(ExecError::NonIntegerName),
    }
   }
   0x71=>{
    let target=self.target(r)?;
    match target{Target::Reference(reference)=>Ok(make_object_reference(reference)),
        _=>Err(ExecError::UnsupportedOpcode)}
   }
   0x83=>{
    let source=self.term_value(r)?;
    let reference=reference_from_value(&source).map_err(|_|ExecError::TargetTypeMismatch)?;
    dereference(reference,&self.bindings,self.arena).map_err(|_|ExecError::NonIntegerName)
   }
   _=>{
    r.pos=start;
    self.term(r).map(Self::integer_value)
   }
  }
 }
 fn term_depth(&mut self,r:&mut Reader<'_>,depth:u8)->Result<u64,ExecError>{
  if depth>32{return Err(ExecError::NestingLimit)}
  self.step()?;let op=r.byte()?;
  match op{
   0x00=>Ok(0),0x01=>Ok(1),0xff=>Ok(self.integer_mask()),
   0x71|0x83=>{let value=self.term_value(r)?;self.operand_integer(&value)},0x0a=>Ok(r.byte()? as u64),
   0x0b=>{let b=r.take(2)?;Ok(u16::from_le_bytes([b[0],b[1]])as u64)},
   0x0c=>{let b=r.take(4)?;Ok(u32::from_le_bytes([b[0],b[1],b[2],b[3]])as u64)},
   0x0e=>{if self.integer_width_bits==32{return Err(ExecError::UnsupportedOpcode)}let b=r.take(8)?;Ok(u64::from_le_bytes(b.try_into().map_err(|_|ExecError::Truncated)?))},
   0x60..=0x67=>self.operand_integer(&self.locals[(op-0x60)as usize]),0x68..=0x6e=>{let value=self.argument_value((op-0x68)as usize)?;self.operand_integer(&value)},
   0x72|0x74|0x77|0x79|0x7a|0x7b|0x7c|0x7d|0x7e|0x7f|0x85=>{
    let a=self.term_depth(r,depth+1)?;let b=self.term_depth(r,depth+1)?;
    let v=match op{
     0x72=>a.wrapping_add(b),0x74=>a.wrapping_sub(b),0x77=>a.wrapping_mul(b),
     0x79=>if b>=self.integer_width_bits as u64{0}else{a<<b},0x7a=>if b>=self.integer_width_bits as u64{0}else{a>>b},
     0x7b=>a&b,0x7c=>!(a&b),0x7d=>a|b,0x7e=>!(a|b),0x85=>{if b==0{return Err(ExecError::DivideByZero)}a%b},_=>a^b
    };
    let v=self.normalize(v);let t=self.target(r)?;self.store(t,v)?;Ok(v)
   }
   0x81|0x82=>{
    let value=self.term_depth(r,depth+1)?;
    let value=self.normalize(value);
    let result=if value==0{0}else if op==0x81{self.integer_width_bits as u64-if self.integer_width_bits==32{(value as u32).leading_zeros() as u64}else{value.leading_zeros() as u64}}else{value.trailing_zeros() as u64+1};
    let t=self.target(r)?;self.store(t,result)?;Ok(result)
   }
   0x75|0x76=>{
    let t=self.target(r)?;
    let current=match t{Target::Null=>0,Target::Local(i)=>self.operand_integer(&self.locals[i])?,Target::Arg(i)=>self.operand_integer(&self.args[i])?,Target::Reference(reference)=>{let value=dereference(reference,&self.bindings,&self.arena).map_err(|_|ExecError::NonIntegerName)?;self.operand_integer(&value)?}};
    let value=if op==0x75{current.wrapping_add(1)}else{current.wrapping_sub(1)};
    let value=self.normalize(value);self.store(t,value)?;Ok(value)
   }
   0x78=>{
    let dividend=self.term_depth(r,depth+1)?;let divisor=self.term_depth(r,depth+1)?;
    if divisor==0{return Err(ExecError::DivideByZero)}
    let remainder=dividend%divisor;let quotient=dividend/divisor;
    // AML Divide operand order is Dividend, Divisor, Remainder, Quotient.
    let rem=self.target(r)?;let q=self.target(r)?;
    self.store(rem,remainder)?;self.store(q,quotient)?;Ok(self.normalize(quotient))
   }
   0x80=>{let operand=self.term_depth(r,depth+1)?;let v=self.normalize(!operand);let t=self.target(r)?;self.store(t,v)?;Ok(v)},
   0x90=>{let a=self.term_depth(r,depth+1)?;let b=self.term_depth(r,depth+1)?;Ok((a!=0&&b!=0)as u64)},
   0x91=>{let a=self.term_depth(r,depth+1)?;let b=self.term_depth(r,depth+1)?;Ok((a!=0||b!=0)as u64)},
   0x92=>Ok((self.term_depth(r,depth+1)?==0)as u64),
   0x93|0x94|0x95=>{let a=self.term_depth(r,depth+1)?;let b=self.term_depth(r,depth+1)?;Ok(match op{0x93=>(a==b)as u64,0x94=>(a>b)as u64,_=>(a<b)as u64})}
   0x5c|0x5e|0x2e|0x2f|b'_'|b'A'..=b'Z'=>{
    r.pos-=1;
    let (name,next)=parse_name_string(r.bytes,r.pos,r.end).map_err(|_|ExecError::InvalidName)?;
    r.pos=next;
    let node=resolve_name_string(self.namespace,self.scope,&name).map_err(|_|ExecError::NameNotFound)?;
    let canonical=self.namespace.resolve_alias(node).map_err(|_|ExecError::NameNotFound)?;
    let entry=*self.namespace.nodes.get(canonical as usize).filter(|_|(canonical as usize)<self.namespace.count).ok_or(ExecError::NameNotFound)?;
    match entry.object_type {
     AmlObjectType::Method=>{
      let count=(entry.method_flags&7) as usize;
      let mut call_args=[Self::integer_value(0);7];
      for slot in call_args.iter_mut().take(count){*slot=self.term_value(r)?;}
      {let value=execute_method_inner(self.aml,self.namespace,canonical,&call_args[..count],self.call_depth+1,self.integer_width_bits,self.arena,self.bindings)?;self.operand_integer(&value)}
     }
     AmlObjectType::Name=>{
      let start=entry.body_start as usize;
      let end=start.checked_add(entry.body_length as usize).ok_or(ExecError::InvalidMethod)?;
      if end>self.aml.len(){return Err(ExecError::InvalidMethod)}
      let reference=self.name_reference(canonical)?;
      let value=dereference(reference,&self.bindings,&self.arena).map_err(|_|ExecError::NonIntegerName)?;
      self.operand_integer(&value)
     }
     _=>Err(ExecError::NonIntegerName),
    }
   }
   _=>Err(ExecError::UnsupportedOpcode)
  }
 }
 fn execute(&mut self,r:&mut Reader<'_>,depth:u8)->Result<ExecFlow,ExecError>{
  if depth>16{return Err(ExecError::LoopControl)}
  while r.pos<r.end{
   self.step()?;let op=r.byte()?;
   match op{
    0xa3=>{},
    0xa4=>return Ok(ExecFlow::Return(self.term_value(r)?)),
    0x70=>{let v=self.term_value(r)?;let t=self.target(r)?;self.store_value(t,v,StoreMode::Store)?;},
    0x9d=>{let v=self.term_value(r)?;let t=self.target(r)?;self.store_value(t,v,StoreMode::CopyObject)?;},
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
    0xa5=>return Ok(ExecFlow::Break),
    0x9f=>return Ok(ExecFlow::ContinueLoop),
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
 let namespace=AmlNamespace::new();
 let empty=Machine::uninitialized_value();
 let mut arena=AmlObjectArena::new();let mut bindings=AmlBindings::with_aml_revision(2);
 let mut machine=Machine{locals:[empty;8],args:[empty;7],arena:&mut arena,bindings:&mut bindings,integer_width_bits:64,steps:0,aml:&[],namespace:&namespace,scope:0,call_depth:0};
 let mut reader=Reader{bytes,pos:0,end:bytes.len()};
 let value=machine.term(&mut reader)?;
 Ok((value,reader.pos))
}

/// Execute one loaded method against the exact AML definition-block byte slice.
/// Only integer arguments and scalar operations are currently supported.
const MAX_METHOD_CALL_DEPTH:u8=16;

fn execute_method_inner(aml:&[u8],namespace:&AmlNamespace,method_index:u16,args:&[AmlRuntimeValue],call_depth:u8,integer_width_bits:u8,arena:&mut AmlObjectArena,bindings:&mut AmlBindings)->Result<AmlRuntimeValue,ExecError>{
 if call_depth>MAX_METHOD_CALL_DEPTH{return Err(ExecError::CallDepthLimit)}
 let index=method_index as usize;
 if index>=namespace.count{return Err(ExecError::InvalidMethod)}
 let node=namespace.nodes.get(index).ok_or(ExecError::InvalidMethod)?;
 if node.object_type!=AmlObjectType::Method{return Err(ExecError::InvalidMethod)}
 let expected=(node.method_flags&7)as usize;if args.len()!=expected||args.len()>7{return Err(ExecError::BadArgumentCount)}
 let start=node.body_start as usize;let end=start.checked_add(node.body_length as usize).ok_or(ExecError::InvalidMethod)?;
 if end>aml.len(){return Err(ExecError::InvalidMethod)}
 if !matches!(integer_width_bits,32|64){return Err(ExecError::UnsupportedOpcode)}
 let empty=Machine::uninitialized_value();
 let mut m=Machine{locals:[empty;8],args:[empty;7],arena,bindings,integer_width_bits,steps:0,aml,namespace,scope:node.parent,call_depth};
 for (slot,value) in m.args.iter_mut().zip(args.iter().copied()){*slot=value;}
 let mut r=Reader{bytes:aml,pos:start,end};let value=match m.execute(&mut r,0)?{ExecFlow::Continue=>Machine::integer_value(0),ExecFlow::Return(v)=>v,ExecFlow::Break|ExecFlow::ContinueLoop=>return Err(ExecError::LoopControl)};
 if value.kind==AmlValueKind::Package{return Err(ExecError::UnsupportedReturnType)}
 Ok(value)
}


/// Execute a scalar-returning AML method using the Integer width encoded by
/// the containing DefinitionBlock (32 bits for revision 1, otherwise 64).
/// Execute a method and preserve Integer, String, or Buffer return objects.
/// Package returns are rejected because their arena-backed element handles
/// cannot outlive this invocation without an owning result arena.
pub fn execute_method_typed_with_values(aml:&[u8],namespace:&AmlNamespace,
    method_index:u16,args:&[AmlRuntimeValue],integer_width_bits:u8)->Result<AmlRuntimeValue,ExecError>{
    let mut arena=AmlObjectArena::new();
    let mut bindings=AmlBindings::with_aml_revision(if integer_width_bits==32{1}else{2});
    execute_method_inner(aml,namespace,method_index,args,0,integer_width_bits,&mut arena,&mut bindings)
}

/// Compatibility scalar API: returns an Integer and rejects typed non-Integer
/// results rather than silently coercing a String or Buffer return.
pub fn execute_method_with_values(aml:&[u8],namespace:&AmlNamespace,
    method_index:u16,args:&[AmlRuntimeValue],integer_width_bits:u8)->Result<AmlValue,ExecError>{
    let value=execute_method_typed_with_values(aml,namespace,method_index,args,integer_width_bits)?;
    if value.kind!=AmlValueKind::Integer{return Err(ExecError::NonIntegerName)}
    Ok(AmlValue::Integer(value.integer))
}

/// Integer-argument compatibility entry point using an explicit AML width.
pub fn execute_method_with_integer_width(aml:&[u8],namespace:&AmlNamespace,
    method_index:u16,args:&[u64],integer_width_bits:u8)->Result<AmlValue,ExecError>{
    if args.len()>7{return Err(ExecError::BadArgumentCount)}
    let mut values=[Machine::integer_value(0);7];
    for (slot,value) in values.iter_mut().zip(args.iter().copied()){*slot=Machine::integer_value(value);}
    execute_method_with_values(aml,namespace,method_index,&values[..args.len()],integer_width_bits)
}

/// Execute a loaded method with a fresh AML frame. Nested invocations receive
/// independent Local/Arg storage and share only the bounded per-frame budget.
pub fn execute_method(aml:&[u8],namespace:&AmlNamespace,method_index:u16,args:&[u64])->Result<AmlValue,ExecError>{
    execute_method_with_integer_width(aml,namespace,method_index,args,64)
}
#[cfg(test)]
mod tests{
 use super::*;use crate::aml::{AmlNamespace,load_definition_block};
 #[test]fn implicit_source_conversion_reads_string_and_buffer_name_objects(){
  let string_aml=[
   0x08,b'S',b'T',b'R',b'0',0x0d,b'2',b'A',0,
   0x14,0x0e,b'T',b'E',b'S',b'T',0,0xa4,0x72,b'S',b'T',b'R',b'0',0x01,0x00
  ];
  let mut ns=AmlNamespace::new();load_definition_block(&string_aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&string_aml,&ns,id,&[]).unwrap(),AmlValue::Integer(0x2b));

  let buffer_aml=[
   0x08,b'B',b'U',b'F',b'0',0x11,0x05,0x0a,2,0x34,0x12,
   0x14,0x0e,b'T',b'E',b'S',b'T',0,0xa4,0x72,b'B',b'U',b'F',b'0',0x01,0x00
  ];
  let mut ns=AmlNamespace::new();load_definition_block(&buffer_aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&buffer_aml,&ns,id,&[]).unwrap(),AmlValue::Integer(0x1235));
 }

 #[test]fn method_local_and_argument_targets_store_integer_values_without_type_aliasing(){
  let mut namespace=AmlNamespace::new();
  let aml=[0x14,0x0e,b'T',b'E',b'S',b'T',1,0x70,0x68,0x60,0xa4,0x72,0x60,0x01,0x00];
  load_definition_block(&aml,&mut namespace).unwrap();
  let id=namespace.find_child(0,*b"TEST").unwrap();
  // Existing public entry point passes Integer args; typed Name operands are
  // exercised above while Local/Arg scalar values remain width-aware.
  assert_eq!(execute_method(&aml,&namespace,id,&[41]).unwrap(),AmlValue::Integer(42));
 }

 #[test]fn arg_object_reference_auto_dereferences_for_reads_and_writes(){
  let aml=[
   0x08,b'N',b'A',b'M',b'0',0x01,
   0x14,0x12,b'T',b'E',b'S',b'T',1,
   0x70,0x0a,42,0x68,
   0xa4,0x72,b'N',b'A',b'M',b'0',0x01,0x00
  ];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let node=ns.find_child(0,*b"NAM0").unwrap();let id=ns.find_child(0,*b"TEST").unwrap();
  let reference=make_object_reference(AmlReference::NamespaceNode(node));
  assert_eq!(execute_method_with_values(&aml,&ns,id,&[reference],64).unwrap(),AmlValue::Integer(43));
 }

 #[test]fn refof_and_derefof_preserve_namespace_object_references(){
  let aml=[0x08,b'N',b'A',b'M',b'0',0x01,
      0x14,0x0d,b'T',b'E',b'S',b'T',0,0xa4,0x83,0x71,b'N',b'A',b'M',b'0'];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]).unwrap(),AmlValue::Integer(1));
 }

 #[test]fn uninitialized_locals_and_unused_arguments_are_not_integer_zero(){
  let local=[0x14,0x08,b'T',b'E',b'S',b'T',0,0xa4,0x60];
  let mut ns=AmlNamespace::new();load_definition_block(&local,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&local,&ns,id,&[]),Err(ExecError::NonIntegerName));

  let argument=[0x14,0x08,b'T',b'E',b'S',b'T',0,0xa4,0x68];
  let mut ns=AmlNamespace::new();load_definition_block(&argument,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&argument,&ns,id,&[]),Err(ExecError::NonIntegerName));
 }

 #[test]fn method_return_preserves_string_and_buffer_object_types(){
  let string_aml=[0x08,b'S',b'T',b'R',b'0',0x0d,b'O',b'K',0,
      0x14,0x0b,b'T',b'E',b'S',b'T',0,0xa4,b'S',b'T',b'R',b'0'];
  let mut ns=AmlNamespace::new();load_definition_block(&string_aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  let value=execute_method_typed_with_values(&string_aml,&ns,id,&[],64).unwrap();
  assert_eq!(value.kind,AmlValueKind::String);assert_eq!(value.byte_length,2);
  assert_eq!(&value.bytes[..2],b"OK");
  assert_eq!(execute_method(&string_aml,&ns,id,&[]),Err(ExecError::NonIntegerName));

  let buffer_aml=[0x08,b'B',b'U',b'F',b'0',0x11,0x05,0x0a,2,0x34,0x12,
      0x14,0x0b,b'T',b'E',b'S',b'T',0,0xa4,b'B',b'U',b'F',b'0'];
  let mut ns=AmlNamespace::new();load_definition_block(&buffer_aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  let value=execute_method_typed_with_values(&buffer_aml,&ns,id,&[],64).unwrap();
  assert_eq!(value.kind,AmlValueKind::Buffer);assert_eq!(value.byte_length,2);
  assert_eq!(&value.bytes[..2],&[0x34,0x12]);
 }

 #[test]fn store_and_copy_object_apply_distinct_named_buffer_target_rules(){
  let common=[
   0x08,b'S',b'T',b'R',b'0',0x0d,b'A',b'B',0,
   0x08,b'B',b'U',b'F',b'0',0x11,0x05,0x0a,2,0,0
  ];
  let mut store_aml=common.to_vec();
  store_aml.extend_from_slice(&[0x14,0x17,b'T',b'E',b'S',b'T',0,
      0x70,b'S',b'T',b'R',b'0',b'B',b'U',b'F',b'0',
      0xa4,0x72,b'B',b'U',b'F',b'0',0x01,0x00]);
  let mut ns=AmlNamespace::new();load_definition_block(&store_aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&store_aml,&ns,id,&[]).unwrap(),AmlValue::Integer(0x4242));

  let mut copy_aml=common.to_vec();
  copy_aml.extend_from_slice(&[0x14,0x17,b'T',b'E',b'S',b'T',0,
      0x9d,b'S',b'T',b'R',b'0',b'B',b'U',b'F',b'0',
      0xa4,0x72,b'B',b'U',b'F',b'0',0x01,0x00]);
  let mut ns=AmlNamespace::new();load_definition_block(&copy_aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&copy_aml,&ns,id,&[]).unwrap(),AmlValue::Integer(0xac));
 }

 #[test]fn store_preserves_typed_name_and_argument_values_in_locals(){
  let name_aml=[
   0x08,b'S',b'T',b'R',b'0',0x0d,b'2',b'A',0,
   0x14,0x11,b'T',b'E',b'S',b'T',0,0x70,b'S',b'T',b'R',b'0',0x60,
   0xa4,0x72,0x60,0x01,0x00
  ];
  let mut ns=AmlNamespace::new();load_definition_block(&name_aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&name_aml,&ns,id,&[]).unwrap(),AmlValue::Integer(0x2b));

  let arg_aml=[0x14,0x0e,b'T',b'E',b'S',b'T',1,0x70,0x68,0x60,
      0xa4,0x72,0x60,0x01,0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&arg_aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  let mut string=AmlRuntimeValue::EMPTY;string.kind=AmlValueKind::String;
  string.byte_length=2;string.bytes[..2].copy_from_slice(b"2A");
  assert_eq!(execute_method_with_values(&arg_aml,&ns,id,&[string],64).unwrap(),AmlValue::Integer(0x2b));
 }

 #[test]fn typed_string_and_buffer_arguments_use_implicit_integer_conversion(){
  let aml=[0x14,0x0b,b'T',b'E',b'S',b'T',1,0xa4,0x72,0x68,0x01,0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  let mut string=AmlRuntimeValue::EMPTY;string.kind=AmlValueKind::String;
  string.byte_length=2;string.bytes[..2].copy_from_slice(b"2A");
  assert_eq!(execute_method_with_values(&aml,&ns,id,&[string],64).unwrap(),AmlValue::Integer(0x2b));
  let mut buffer=AmlRuntimeValue::EMPTY;buffer.kind=AmlValueKind::Buffer;
  buffer.byte_length=2;buffer.bytes[..2].copy_from_slice(&[0x34,0x12]);
  assert_eq!(execute_method_with_values(&aml,&ns,id,&[buffer],32).unwrap(),AmlValue::Integer(0x1235));
 }

 #[test]fn integer_width_controls_ones_arithmetic_and_qword_acceptance(){
  let aml=[0x14,0x08,b'T',b'E',b'S',b'T',0,0xa4,0xff];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method_with_integer_width(&aml,&ns,id,&[],32).unwrap(),AmlValue::Integer(0xffff_ffff));
  assert_eq!(execute_method_with_integer_width(&aml,&ns,id,&[],64).unwrap(),AmlValue::Integer(u64::MAX));

  let add=[0x08,b'S',b'T',b'R',b'0',0x0d,b'F',b'F',b'F',b'F',b'F',b'F',b'F',b'F',0,
   0x14,0x0e,b'T',b'E',b'S',b'T',0,0xa4,0x72,b'S',b'T',b'R',b'0',0x01,0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&add,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method_with_integer_width(&add,&ns,id,&[],32).unwrap(),AmlValue::Integer(0));
  assert_eq!(execute_method_with_integer_width(&add,&ns,id,&[],64).unwrap(),AmlValue::Integer(0x1_0000_0000));

  let qword=[0x14,0x10,b'T',b'E',b'S',b'T',0,0xa4,0x0e,1,0,0,0,0,0,0,0];
  let mut ns=AmlNamespace::new();load_definition_block(&qword,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method_with_integer_width(&qword,&ns,id,&[],32),Err(ExecError::UnsupportedOpcode));
 }

 #[test]fn invokes_nested_method_with_an_independent_local_frame(){
  // MTH0 copies Arg0 to its Local0 and returns it. MTH1 keeps 7 in its own
  // Local0 while invoking MTH0; the nested frame must not overwrite it.
  let aml=[
   0x14,0x0b,b'M',b'T',b'H',b'0',1,0x70,0x68,0x60,0xa4,0x60,
   0x14,0x13,b'M',b'T',b'H',b'1',1,0x70,0x0a,7,0x60,0x70,b'M',b'T',b'H',b'0',0x68,0x61,0xa4,0x60
  ];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"MTH1").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[42]).unwrap(),AmlValue::Integer(7));
 }
 #[test]fn rejects_recursive_method_invocation_at_call_depth_limit(){
  // A method that returns its own zero-argument invocation.
  let aml=[0x14,0x0b,b'R',b'E',b'C',b'U',0,0xa4,b'R',b'E',b'C',b'U'];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"RECU").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]),Err(ExecError::CallDepthLimit));
 }
 #[test]fn executes_integer_method_with_arg_and_add(){
  let aml=[0x14,0x0c,b'T',b'E',b'S',b'T',1,0xa4,0x72,0x68,0x0a,2,0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[40]).unwrap(),AmlValue::Integer(42));
 }
 #[test]fn resolves_integer_name_objects_from_method_bodies(){
  let aml=[0x08,b'V',b'A',b'L',b'0',0x0a,42,
           0x14,0x0b,b'T',b'E',b'S',b'T',0,0xa4,b'V',b'A',b'L',b'0'];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]).unwrap(),AmlValue::Integer(42));
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
  let aml=[0x14,0x12,b'T',b'E',b'S',b'T',0,0x70,0x0a,7,0x60,0x75,0x60,0x76,0x60,0xa4,0x80,0x60,0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]).unwrap(),AmlValue::Integer(!7u64));
 }
 #[test]fn rejects_namespace_slots_outside_the_live_node_count(){
  let aml=[0x14,0x09,b'T',b'E',b'S',b'T',0,0xa4,0x0a,7];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  assert_eq!(execute_method(&aml,&ns,0,&[]),Err(ExecError::InvalidMethod));
  assert_eq!(execute_method(&aml,&ns,u16::MAX,&[]),Err(ExecError::InvalidMethod));
 }
 #[test]fn bounds_non_terminating_aml_while_loops(){
  // While (One) { Noop } must stop at the interpreter step budget.
  let aml=[0x14,0x0a,b'T',b'E',b'S',b'T',0,0xa2,0x03,0x01,0xa3];
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
  assert_eq!(evaluate_term_argument(&[b'V',b'A',b'L',b'0']),Err(ExecError::NameNotFound));
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
   0xa2,0x03,0x01,0xa5,
   0xa4,0x60];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]).unwrap(),AmlValue::Integer(1));
 }
 #[test]fn continue_rechecks_while_predicate_after_advancing_state(){
  let aml=[0x14,0x14,b'T',b'E',b'S',b'T',0,
   0x70,0x0a,2,0x60,
   0xa2,0x07,0x94,0x60,0x00,0x76,0x60,0x9f,
   0xa4,0x60];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]).unwrap(),AmlValue::Integer(0));
 }
 #[test]fn break_propagates_through_nested_if(){
  let aml=[0x14,0x10,b'T',b'E',b'S',b'T',0,
   0xa2,0x06,0x01,0xa0,0x03,0x01,0xa5,
   0xa4,0x0a,9];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]).unwrap(),AmlValue::Integer(9));
 }
 #[test]fn continue_propagates_through_nested_if(){
  let aml=[0x14,0x17,b'T',b'E',b'S',b'T',0,
   0x70,0x0a,2,0x60,
   0xa2,0x0a,0x94,0x60,0x00,0xa0,0x05,0x01,0x76,0x60,0x9f,
   0xa4,0x60];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]).unwrap(),AmlValue::Integer(0));
 }
 #[test]fn break_only_exits_the_innermost_nested_while(){
  let aml=[0x14,0x1a,b'T',b'E',b'S',b'T',0,
   0x70,0x0a,2,0x60,
   0xa2,0x0d,0x94,0x60,0x00,0x70,0x01,0x61,
    0xa2,0x03,0x01,0xa5,0x76,0x60,
   0xa4,0x60];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]).unwrap(),AmlValue::Integer(0));
 }
 #[test]fn return_inside_while_escapes_all_loop_levels(){
  let aml=[0x14,0x0c,b'T',b'E',b'S',b'T',0,
   0xa2,0x05,0x01,0xa4,0x0a,42];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]).unwrap(),AmlValue::Integer(42));
 }
 #[test]fn rejects_break_outside_a_while_loop(){
  let aml=[0x14,0x07,b'T',b'E',b'S',b'T',0,0xa5];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]),Err(ExecError::LoopControl));
 }
 #[test]fn rejects_continue_outside_a_while_loop(){
  let aml=[0x14,0x07,b'T',b'E',b'S',b'T',0,0x9f];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]),Err(ExecError::LoopControl));
 }
 #[test]fn rejects_wrong_argument_count(){
  let aml=[0x14,0x09,b'T',b'E',b'S',b'T',0,0xa4,0x0a,7];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[1]),Err(ExecError::BadArgumentCount));
 }
}
