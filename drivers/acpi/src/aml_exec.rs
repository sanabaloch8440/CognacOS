//! Bounded AML method execution with typed scalar/data operands and target stores.
//! This constrained interpreter fails closed on unsupported AML terms and hardware access.
/*
 * FIXME(ACPI-PRODUCTION) — AML method execution
 * FIXME(ACPI-PRODUCTION): This is a constrained AML executor, not yet a full production interpreter. Operand/target semantics, core control flow, method frames, and the Serialized-method host interface are implemented; a concrete kernel-backed AmlMethodSynchronizer is still required. Remaining work includes Mutex/Acquire/Release, Events/Global Lock, namespace mutation, OperationRegion/Field handlers, module-level execution, _OSI policy, shared execution budgets/timeouts, and cleanup on every exit path. Keep hardware access behind validated region handlers; never execute AML in interrupt context. Add differential tests against the public ACPI specification for every supported opcode.
 */

#![no_std]
use crate::aml::{AmlNamespace,AmlObjectType,decode_package_length,parse_name_string,resolve_name_string};
use crate::aml_osi::platform_osi;
use crate::aml_objects::{decode_namespace_name,AmlObjectArena,AmlRuntimeValue,AmlValueKind};
use crate::aml_mutation::AmlDynamicNamespace;
use crate::aml_runtime::{to_integer_implicit,to_integer_with_width,to_buffer,integer_to_string,AmlBindings,AmlReference,RuntimeError,StoreMode,dereference,store_reference_with_mode,make_object_reference,reference_from_value,validate_runtime_value};
pub const MAX_AML_STEPS:usize=100_000;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum ExecError{InvalidMethod,BadArgumentCount,Truncated,UnsupportedOpcode,StepLimit,BadTarget,BadPackage,LoopControl,ExecutionBusy,DivideByZero,NestingLimit,InvalidName,NameNotFound,NonIntegerName,CallDepthLimit,TargetTypeMismatch,UnsupportedReturnType,SynchronizationUnavailable,SyncLevelViolation,UnsupportedArgumentValue}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum AmlValue{Integer(u64)}
impl AmlValue{pub const fn integer(self)->u64{match self{Self::Integer(v)=>v}}}
#[derive(Clone,Copy)]
struct Reader<'a>{bytes:&'a [u8],pos:usize,end:usize}
impl<'a> Reader<'a>{
 fn byte(&mut self)->Result<u8,ExecError>{if self.pos>=self.end{return Err(ExecError::Truncated)}let v=self.bytes[self.pos];self.pos+=1;Ok(v)}
 fn take(&mut self,n:usize)->Result<&'a [u8],ExecError>{let e=self.pos.checked_add(n).ok_or(ExecError::Truncated)?;if e>self.end{return Err(ExecError::Truncated)}let s=&self.bytes[self.pos..e];self.pos=e;Ok(s)}
 // Decode a PkgLength against this Reader's end, never against the backing AML slice alone.
 // This prevents a nested control-flow package from consuming its parent's next term.
 fn pkg_end(&mut self)->Result<usize,ExecError>{let (_,e)=decode_package_length(self.bytes,self.pos,self.end).map_err(|_|ExecError::BadPackage)?;Ok(e)}
}
#[derive(Clone,Copy)]
enum Target{Null,Local(usize),Arg(usize),Reference(AmlReference)}
/// Host-provided implicit mutex support for AML Serialized methods.
///
/// The provider must coordinate all callers of a method object, block until
/// acquisition is permitted (or return ExecutionBusy), and implement
/// per-thread ownership so nested calls obey ACPI mutex reentrancy rules.
pub trait AmlMethodSynchronizer:Sync{
 fn acquire(&self,method_index:u16,sync_level:u8)->Result<(),ExecError>;
 fn release(&self,method_index:u16);
}
/// Run a non-interpreter method-result decoder under the method's implicit mutex.
/// This is used only by bounded fast paths that decode a direct Return(String,
/// Buffer, or Package) without entering the AML execution machine. The closure
/// is always followed by release, including ordinary error returns.
pub fn with_method_synchronization<T>(
 namespace:&AmlNamespace,
 method_index:u16,
 sync:&dyn AmlMethodSynchronizer,
 operation:impl FnOnce()->Result<T,ExecError>,
)->Result<T,ExecError>{
 let node=namespace.nodes.get(method_index as usize)
  .filter(|_|(method_index as usize)<namespace.count)
  .ok_or(ExecError::InvalidMethod)?;
 if node.object_type!=AmlObjectType::Method{return Err(ExecError::InvalidMethod)}
 if node.method_flags&0x08==0{return operation()}
 let sync_level=(node.method_flags>>4)&0x0f;
 // This helper is used only for top-level direct-result decoding. No outer
 // AML method owns a SyncLevel here, so every encoded level is admissible.
 sync.acquire(method_index,sync_level)?;
 let result=operation();
 sync.release(method_index);
 result
}
struct NoMethodSynchronizer;
static NO_METHOD_SYNCHRONIZER:NoMethodSynchronizer=NoMethodSynchronizer;
impl AmlMethodSynchronizer for NoMethodSynchronizer{
 fn acquire(&self,_method_index:u16,_sync_level:u8)->Result<(),ExecError>{Err(ExecError::SynchronizationUnavailable)}
 fn release(&self,_method_index:u16){}
}
// Execution outcomes propagate through nested If/Else blocks. Only the nearest
// While consumes Break/ContinueLoop; Return escapes every enclosing construct.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum ExecFlow{Continue,Return(AmlRuntimeValue),Break,ContinueLoop}
struct Machine<'a,'b>{locals:[AmlRuntimeValue;8],args:[AmlRuntimeValue;7],frame_cells:[u16;128],frame_cell_count:usize,arena:&'b mut AmlObjectArena,bindings:&'b mut AmlBindings,integer_width_bits:u8,steps:usize,aml:&'a [u8],namespace:&'a AmlNamespace,scope:u16,call_depth:u8,sync:&'a dyn AmlMethodSynchronizer,dynamic:&'b mut AmlDynamicNamespace,current_sync_level:u8,owner_table:u32}
impl<'a,'b> Machine<'a,'b>{
 fn step(&mut self)->Result<(),ExecError>{self.steps+=1;if self.steps>MAX_AML_STEPS{Err(ExecError::StepLimit)}else{Ok(())}}
 fn frame_slot_reference(&mut self,argument:bool,index:usize)->Result<AmlReference,ExecError>{
  if self.frame_cell_count>=self.frame_cells.len(){return Err(ExecError::TargetTypeMismatch)}
  let current=if argument{*self.args.get(index).ok_or(ExecError::BadTarget)?}else{*self.locals.get(index).ok_or(ExecError::BadTarget)?};
  if current.kind==AmlValueKind::ObjectReference{
   if let Ok(AmlReference::ArenaValue(id))=reference_from_value(&current){
    if self.frame_cells[..self.frame_cell_count].contains(&id){return Ok(AmlReference::ArenaValue(id))}
   }
  }
  let id=self.arena.allocate(current).map_err(|_|ExecError::TargetTypeMismatch)?;
  if argument{self.args[index]=make_object_reference(AmlReference::ArenaValue(id));}else{self.locals[index]=make_object_reference(AmlReference::ArenaValue(id));}
  self.frame_cells[self.frame_cell_count]=id;self.frame_cell_count+=1;Ok(AmlReference::ArenaValue(id))
 }
 fn own_frame_value(&mut self,value:AmlRuntimeValue)->Result<u16,ExecError>{
  if self.frame_cell_count>=self.frame_cells.len(){return Err(ExecError::TargetTypeMismatch)}
  let id=self.arena.allocate(value).map_err(|_|ExecError::TargetTypeMismatch)?;
  self.frame_cells[self.frame_cell_count]=id;self.frame_cell_count+=1;Ok(id)
 }
 fn retire_frame_cells(&mut self){for id in self.frame_cells[..self.frame_cell_count].iter().copied(){if let Some(value)=self.arena.get_mut(id){*value=AmlRuntimeValue::EMPTY;}}self.frame_cell_count=0;}
 fn reference_object_id(&self,reference:AmlReference)->Result<u16,ExecError>{
  match reference{
   AmlReference::NamespaceNode(node)=>self.bindings.value_id(node).map_err(|_|ExecError::BadTarget),
   AmlReference::ArenaValue(id)=>Ok(id),
   AmlReference::PackageElement{package,index}=>{let object=self.arena.get(package).ok_or(ExecError::BadTarget)?;
    if object.kind!=AmlValueKind::Package||index as usize>=object.element_count as usize{return Err(ExecError::BadTarget)}Ok(object.elements[index as usize])}
   _=>Err(ExecError::BadTarget),
  }
 }
 fn store_reference_target(&mut self,target:Target,reference:AmlReference)->Result<(),ExecError>{
  let value=make_object_reference(reference);
  match target{Target::Local(i)=>{self.locals[i]=value;Ok(())},Target::Arg(i)=>{self.args[i]=value;Ok(())},
   Target::Reference(r)=>self.store_reference_value(r,value,StoreMode::CopyObject),Target::Null=>Err(ExecError::BadTarget)}
 }
 fn optional_source_reference(&mut self,r:&mut Reader<'_>)->Result<Option<AmlReference>,ExecError>{
  let start=r.pos;let op=*r.bytes.get(start).ok_or(ExecError::Truncated)?;
  match op{
   0x60..=0x67=>{r.pos+=1;Ok(Some(self.frame_slot_reference(false,(op-0x60)as usize)?))}
   0x68..=0x6e=>{r.pos+=1;Ok(Some(self.frame_slot_reference(true,(op-0x68)as usize)?))}
   0x5c|0x5e|0x2e|0x2f|b'_'|b'A'..=b'Z'=>{
    let (name,next)=parse_name_string(r.bytes,start,r.end).map_err(|_|ExecError::InvalidName)?;r.pos=next;
    if let Ok(node)=resolve_name_string(self.namespace,self.scope,&name){
     let canonical=self.namespace.resolve_alias(node).map_err(|_|ExecError::BadTarget)?;
     let object=self.namespace.nodes.get(canonical as usize).filter(|_|(canonical as usize)<self.namespace.count).ok_or(ExecError::BadTarget)?;
     return match object.object_type{AmlObjectType::Name=>Ok(Some(self.name_reference(canonical)?)),
      AmlObjectType::Device=>Ok(Some(AmlReference::NamespaceNode(canonical))),_=>Ok(None)};
    }
    if let Ok(node)=self.dynamic.resolve_buffer_field(self.namespace,self.scope,&name){return Ok(Some(self.dynamic.field_reference(self.namespace,node).map_err(|_|ExecError::BadTarget)?));}
    Ok(None)
   }
   _=>Err(ExecError::BadTarget)
  }
 }
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
    let node=match resolve_name_string(self.namespace,self.scope,&name) {
     Ok(node)=>node,
     Err(_)=>{
      let dynamic_node=self.dynamic.resolve_buffer_field(self.namespace,self.scope,&name).map_err(|_|ExecError::BadTarget)?;
      let reference=self.dynamic.field_reference(self.namespace,dynamic_node).map_err(|_|ExecError::BadTarget)?;
      return Ok(Target::Reference(reference));
     }
    };
    let canonical=self.namespace.resolve_alias(node).map_err(|_|ExecError::BadTarget)?;
    let entry=self.namespace.nodes.get(canonical as usize)
        .filter(|_|(canonical as usize)<self.namespace.count).ok_or(ExecError::BadTarget)?;
    if entry.object_type==AmlObjectType::Name { return Ok(Target::Reference(self.name_reference(canonical)?)); }
    r.pos = r.pos; // preserve the parsed target boundary
    Err(ExecError::BadTarget)
   }
  }
 }
 fn integer_value(value:u64)->AmlRuntimeValue{let mut out=AmlRuntimeValue::EMPTY;out.kind=AmlValueKind::Integer;out.integer=value;out}
 fn uninitialized_value()->AmlRuntimeValue{let mut out=AmlRuntimeValue::EMPTY;out.kind=AmlValueKind::Uninitialized;out}
 fn to_bcd(value:u64,width_bits:u8)->Result<u64,ExecError>{
  let digits=(width_bits/4) as usize;let mut remaining=value;let mut result=0u64;
  for index in 0..digits{result|=(remaining%10)<<(index*4);remaining/=10;}
  if remaining!=0{return Err(ExecError::TargetTypeMismatch)}
  Ok(result)
 }
 fn from_bcd(value:u64,width_bits:u8)->Result<u64,ExecError>{
  let digits=(width_bits/4) as usize;let mut result=0u64;let mut multiplier=1u64;
  for index in 0..digits{
   let digit=(value>>(index*4))&0xf;
   if digit>9{return Err(ExecError::TargetTypeMismatch)}
   result=result.checked_add(digit.checked_mul(multiplier).ok_or(ExecError::TargetTypeMismatch)?)
       .ok_or(ExecError::TargetTypeMismatch)?;
   multiplier=multiplier.checked_mul(10).ok_or(ExecError::TargetTypeMismatch)?;
  }
  Ok(result)
 }
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
   let reference=match reference{
    AmlReference::NamespaceNode(node)=>self.name_reference(node)?,
    other=>other,
   };
   dereference(reference,&self.bindings,self.arena).map_err(|_|ExecError::NonIntegerName)
  }else{Ok(value)}
 }
 fn store_value(&mut self,t:Target,value:AmlRuntimeValue,mode:StoreMode)->Result<(),ExecError>{
  match t{
   Target::Null=>Ok(()),
   Target::Local(i)=>{let current=self.locals[i];if current.kind==AmlValueKind::ObjectReference{let reference=reference_from_value(&current).map_err(|_|ExecError::NonIntegerName)?;self.store_reference_value(reference,value,mode)}else{self.locals[i]=value;Ok(())}},
   Target::Arg(i)=>{
    let current=self.args[i];
    if current.kind==AmlValueKind::ObjectReference{
     let reference=reference_from_value(&current).map_err(|_|ExecError::NonIntegerName)?;
     let reference=match reference{
      AmlReference::NamespaceNode(node)=>self.name_reference(node)?,
      other=>other,
     };
     self.store_reference_value(reference,value,mode)
    }else{self.args[i]=value;Ok(())}
   }
   Target::Reference(reference)=>self.store_reference_value(reference,value,mode),
  }
 }
 /// Evaluate a TermArg without prematurely coercing Local/Arg/Name objects.
 /// Operators fall back to the scalar evaluator and therefore produce Integer.
 fn conversion_value(&mut self,r:&mut Reader<'_>,op:u8)->Result<AmlRuntimeValue,ExecError>{
  let source=self.term_value(r)?;
  let converted=match op{
   0x96=>{
    let length=match source.kind{
     AmlValueKind::Integer=>(self.integer_width_bits/8) as usize,
     AmlValueKind::String=>{
      let length=source.byte_length as usize;
      if length>source.bytes.len(){return Err(ExecError::TargetTypeMismatch)}
      if length==0{0}else{length.checked_add(1).ok_or(ExecError::TargetTypeMismatch)?}
     }
     AmlValueKind::Buffer=>source.byte_length as usize,
     _=>return Err(ExecError::TargetTypeMismatch),
    };
    let mut value=to_buffer(&source,length).map_err(|_|ExecError::TargetTypeMismatch)?;
    if source.kind==AmlValueKind::String&&length>0{value.bytes[length-1]=0;}
    value
   }
   0x97=>match source.kind{
    AmlValueKind::Integer=>integer_to_string(self.normalize(source.integer)),
    AmlValueKind::String=>source,
    AmlValueKind::Buffer=>{
     let length=source.byte_length as usize;
     if length>source.bytes.len(){return Err(ExecError::TargetTypeMismatch)}
     if length.saturating_mul(4).saturating_sub(1)>source.bytes.len(){return Err(ExecError::TargetTypeMismatch)}
     let mut value=AmlRuntimeValue::EMPTY;value.kind=AmlValueKind::String;let mut out=0usize;
     for byte in source.bytes[..length].iter(){
      if out!=0{value.bytes[out]=b',';out+=1;}
      let mut digits=[0u8;3];let mut count=0usize;let mut number=*byte as usize;
      if number==0{digits[0]=b'0';count=1;}else{while number!=0{digits[count]=b'0'+(number%10) as u8;count+=1;number/=10;}}
      for digit in digits[..count].iter().rev(){value.bytes[out]=*digit;out+=1;}
     }
     value.byte_length=out as u16;value
    }
    _=>return Err(ExecError::TargetTypeMismatch),
   },
   0x98=>match source.kind{
    AmlValueKind::Integer=>{
     let digits=(self.integer_width_bits/4) as usize;
     let mut value=AmlRuntimeValue::EMPTY;value.kind=AmlValueKind::String;value.byte_length=digits as u16;
     let alphabet=b"0123456789ABCDEF";let integer=self.normalize(source.integer);
     for index in 0..digits{let shift=(digits-1-index)*4;value.bytes[index]=alphabet[((integer>>shift)&0xf) as usize];}
     value
    }
    AmlValueKind::String=>source,
    AmlValueKind::Buffer=>{
     let length=source.byte_length as usize;
     if length>source.bytes.len(){return Err(ExecError::TargetTypeMismatch)}
     let output_length=length.saturating_mul(3).saturating_sub(1);
     if output_length>source.bytes.len(){return Err(ExecError::TargetTypeMismatch)}
     let mut value=AmlRuntimeValue::EMPTY;value.kind=AmlValueKind::String;
     let alphabet=b"0123456789ABCDEF";let mut out=0usize;
     for (index,byte) in source.bytes[..length].iter().enumerate(){
      if index!=0{value.bytes[out]=b',';out+=1;}
      value.bytes[out]=alphabet[(byte>>4) as usize];value.bytes[out+1]=alphabet[(byte&0xf) as usize];out+=2;
     }
     value.byte_length=out as u16;value
    }
    _=>return Err(ExecError::TargetTypeMismatch),
   },
   0x99=>Self::integer_value(to_integer_with_width(&source,self.integer_width_bits).map_err(|_|ExecError::TargetTypeMismatch)?),
   0x9c=>{
    if source.kind!=AmlValueKind::Buffer{return Err(ExecError::TargetTypeMismatch)}
    let requested=self.term(r)? as usize;let source_length=source.byte_length as usize;
    if source_length>source.bytes.len(){return Err(ExecError::TargetTypeMismatch)}
    let mut length=core::cmp::min(requested,source_length);
    if let Some(nul)=source.bytes[..length].iter().position(|byte|*byte==0){length=nul;}
    let mut value=AmlRuntimeValue::EMPTY;value.kind=AmlValueKind::String;
    value.byte_length=length as u16;value.bytes[..length].copy_from_slice(&source.bytes[..length]);value
   }
   _=>return Err(ExecError::UnsupportedOpcode),
  };
  // Explicit conversion targets bypass the implicit Store result conversion.
  let target=self.target(r)?;
  self.store_value(target,converted,StoreMode::CopyObject)?;
  Ok(converted)
 }
 fn create_field_term(&mut self,r:&mut Reader<'_>,op:u8)->Result<AmlRuntimeValue,ExecError>{
  let source_start=r.pos;
  let source=self.term_value(r)?;
  if source.kind!=AmlValueKind::Buffer{return Err(ExecError::TargetTypeMismatch)}
  let source_opcode=*r.bytes.get(source_start).ok_or(ExecError::Truncated)?;
  let buffer_id=if (0x60..=0x67).contains(&source_opcode) {
   let slot=(source_opcode-0x60) as usize;
   let current=self.locals[slot];
   let id=if current.kind==AmlValueKind::ObjectReference {
    match reference_from_value(&current).map_err(|_|ExecError::NonIntegerName)? {
     AmlReference::ArenaValue(id)=>id,
     _=>{let id=self.arena.allocate(source).map_err(|_|ExecError::TargetTypeMismatch)?;self.locals[slot]=make_object_reference(AmlReference::ArenaValue(id));id}
    }
   } else {let id=self.arena.allocate(source).map_err(|_|ExecError::TargetTypeMismatch)?;self.locals[slot]=make_object_reference(AmlReference::ArenaValue(id));id};
   id
  } else if (0x68..=0x6e).contains(&source_opcode) {
   let slot=(source_opcode-0x68) as usize;
   let current=self.args[slot];
   let id=if current.kind==AmlValueKind::ObjectReference {
    match reference_from_value(&current).map_err(|_|ExecError::NonIntegerName)? {
     AmlReference::ArenaValue(id)=>id,
     _=>{let id=self.arena.allocate(source).map_err(|_|ExecError::TargetTypeMismatch)?;self.args[slot]=make_object_reference(AmlReference::ArenaValue(id));id}
    }
   } else {let id=self.arena.allocate(source).map_err(|_|ExecError::TargetTypeMismatch)?;self.args[slot]=make_object_reference(AmlReference::ArenaValue(id));id};
   id
  } else if matches!(source_opcode,0x5c|0x5e|0x2e|0x2f|b'_'|b'A'..=b'Z') {
   let (source_name,_) = parse_name_string(r.bytes,source_start,r.end).map_err(|_|ExecError::InvalidName)?;
   let source_node=resolve_name_string(self.namespace,self.scope,&source_name).map_err(|_|ExecError::NameNotFound)?;
   let canonical=self.namespace.resolve_alias(source_node).map_err(|_|ExecError::NameNotFound)?;
   if self.namespace.nodes[canonical as usize].object_type==AmlObjectType::Name {
    let _=self.name_reference(canonical)?;
    self.bindings.value_id(canonical).map_err(|_|ExecError::NonIntegerName)?
   } else { self.arena.allocate(source).map_err(|_|ExecError::TargetTypeMismatch)? }
  } else { self.arena.allocate(source).map_err(|_|ExecError::TargetTypeMismatch)? };
  let (bit_offset,bit_width)=if op==0x5b {
   let index_value=self.term_value(r)?;let index=self.operand_integer(&index_value)? as u32;
   let width_value=self.term_value(r)?;let width=self.operand_integer(&width_value)? as u32;
   (index,width)
  } else {
   let index_value=self.term_value(r)?;let index=self.operand_integer(&index_value)? as u32;
   let width=match op{0x8d=>1,0x8c=>8,0x8b=>16,0x8a=>32,0x8f=>64,_=>return Err(ExecError::UnsupportedOpcode)};
   if op==0x8d{(index,width)}else{(index.checked_mul(8).ok_or(ExecError::TargetTypeMismatch)?,width)}
  };
  let (name,next)=parse_name_string(r.bytes,r.pos,r.end).map_err(|_|ExecError::InvalidName)?;
  r.pos=next;
  let node=self.dynamic.create_buffer_field_at(self.namespace,self.scope,&name,
      self.arena,buffer_id,bit_offset,bit_width,self.owner_table)
      .map_err(|_|ExecError::TargetTypeMismatch)?;
  let reference=self.dynamic.field_reference(self.namespace,node).map_err(|_|ExecError::BadTarget)?;
  Ok(make_object_reference(reference))
 }
 fn dynamic_field_value(&self,name:&crate::aml::AmlName)->Result<AmlRuntimeValue,ExecError>{
  let node=self.dynamic.resolve_buffer_field(self.namespace,self.scope,name)
      .map_err(|_|ExecError::NameNotFound)?;
  let reference=self.dynamic.field_reference(self.namespace,node).map_err(|_|ExecError::BadTarget)?;
  dereference(reference,&self.bindings,self.arena).map_err(|_|ExecError::NonIntegerName)
 }
 fn term_value(&mut self,r:&mut Reader<'_>)->Result<AmlRuntimeValue,ExecError>{
  let start=r.pos;let op=r.byte()?;
  match op{
   0x00=>Ok(Self::integer_value(0)),0x01=>Ok(Self::integer_value(1)),
   0x8a|0x8b|0x8c|0x8d|0x8f=>self.create_field_term(r,op),
   0x5b=>{let extended=r.byte()?;match extended{
    0x13=>self.create_field_term(r,0x5b),
    0x12=>{let source=self.optional_source_reference(r)?;let target=self.target(r)?;
     if let Some(reference)=source{self.store_reference_target(target,reference)?;Ok(Self::integer_value(1))}
     else{Ok(Self::integer_value(0))}},
    _=>{r.pos=start;self.term(r).map(Self::integer_value)}
   }},
   0xff=>Ok(Self::integer_value(self.integer_mask())),
   0x0a=>Ok(Self::integer_value(r.byte()? as u64)),
   0x0b=>{let b=r.take(2)?;Ok(Self::integer_value(u16::from_le_bytes([b[0],b[1]]) as u64))},
   0x0c=>{let b=r.take(4)?;Ok(Self::integer_value(u32::from_le_bytes([b[0],b[1],b[2],b[3]]) as u64))},
   0x0e=>{
    if self.integer_width_bits==32{return Err(ExecError::UnsupportedOpcode)}
    let b=r.take(8)?;Ok(Self::integer_value(u64::from_le_bytes(b.try_into().map_err(|_|ExecError::Truncated)?)))
   }
   // PackageOp: evaluate each element as a full typed TermArg and keep
   // nested Package/Buffer/String objects in the caller-owned AML arena.
   0x12=>{
    let package_end=r.pkg_end()?;
    let package_length_bytes=1+((r.bytes[r.pos]>>6) as usize);r.pos+=package_length_bytes;
    let count=r.byte()? as usize;
    if count>crate::aml_objects::MAX_PACKAGE_ELEMENTS{return Err(ExecError::BadPackage)}
    let mut package=AmlRuntimeValue::EMPTY;package.kind=AmlValueKind::Package;
    for index in 0..count {
     if r.pos>=package_end{return Err(ExecError::BadPackage)}
     let child=self.term_value(r)?;
     validate_runtime_value(&child,self.arena).map_err(|_|ExecError::BadPackage)?;
     let child_id=self.arena.allocate(child).map_err(|_|ExecError::TargetTypeMismatch)?;
     package.elements[index]=child_id;
    }
    if r.pos!=package_end{return Err(ExecError::BadPackage)}
    r.pos=package_end;package.element_count=count as u8;
    Ok(package)
   }
   // VarPackageOp: the element count is itself an evaluated TermArg.
   0x13=>{
    let package_end=r.pkg_end()?;
    let package_length_bytes=1+((r.bytes[r.pos]>>6) as usize);r.pos+=package_length_bytes;
    let count=self.term_value(r)?;
    let count=self.operand_integer(&count)? as usize;
    if count>crate::aml_objects::MAX_PACKAGE_ELEMENTS{return Err(ExecError::BadPackage)}
    let mut package=AmlRuntimeValue::EMPTY;package.kind=AmlValueKind::Package;
    for index in 0..count {
     if r.pos>=package_end{return Err(ExecError::BadPackage)}
     let child=self.term_value(r)?;
     validate_runtime_value(&child,self.arena).map_err(|_|ExecError::BadPackage)?;
     let child_id=self.arena.allocate(child).map_err(|_|ExecError::TargetTypeMismatch)?;
     package.elements[index]=child_id;
    }
    if r.pos>package_end{return Err(ExecError::BadPackage)}
    r.pos=package_end;package.element_count=count as u8;
    Ok(package)
   }
   0x0d=>{
    let mut value=AmlRuntimeValue::EMPTY;value.kind=AmlValueKind::String;
    loop { let byte=r.byte()?; if byte==0 { break; }
     let length=value.byte_length as usize;
     if length>=value.bytes.len(){return Err(ExecError::TargetTypeMismatch)}
     value.bytes[length]=byte;value.byte_length+=1;
    }
    Ok(value)
   }
   0x60..=0x67=>{let value=self.locals[(op-0x60) as usize];if value.kind==AmlValueKind::ObjectReference{let reference=reference_from_value(&value).map_err(|_|ExecError::NonIntegerName)?;dereference(reference,&self.bindings,self.arena).map_err(|_|ExecError::NonIntegerName)}else{Ok(value)}},
   0x68..=0x6e=>self.argument_value((op-0x68) as usize),
   0x5c|0x5e|0x2e|0x2f|b'_'|b'A'..=b'Z'=>{
    r.pos=start;
    let (name,next)=parse_name_string(r.bytes,r.pos,r.end).map_err(|_|ExecError::InvalidName)?;
    r.pos=next;
    // _OSI is an interpreter-predefined interface, not a firmware namespace
    // method. Resolve it before namespace lookup so firmware need not define it.
    if name.count==1 && name.segments[0]==*b"_OSI" {
     let argument=self.term_value(r)?;
     let supported=if argument.kind==AmlValueKind::String {
      platform_osi(&argument.bytes[..argument.byte_length as usize])
     } else { 0 };
     return Ok(Self::integer_value(supported));
    }
    let node=match resolve_name_string(self.namespace,self.scope,&name){Ok(node)=>node,Err(_)=>return self.dynamic_field_value(&name)};
    let canonical=self.namespace.resolve_alias(node).map_err(|_|ExecError::NameNotFound)?;
    let entry=*self.namespace.nodes.get(canonical as usize)
        .filter(|_|(canonical as usize)<self.namespace.count).ok_or(ExecError::NameNotFound)?;
    match entry.object_type{
     AmlObjectType::Field=>return self.dynamic_field_value(&name),
     AmlObjectType::Name=>{
      let reference=self.name_reference(canonical)?;
      dereference(reference,&self.bindings,&self.arena).map_err(|_|ExecError::NonIntegerName)
     }
     AmlObjectType::Method=>{
      let count=(entry.method_flags&7) as usize;
      let mut call_args=[Self::integer_value(0);7];
      for slot in call_args.iter_mut().take(count){*slot=self.term_value(r)?;}
      execute_method_inner(self.aml,self.namespace,canonical,
          &call_args[..count],self.call_depth+1,self.integer_width_bits,self.arena,self.bindings,self.sync,self.dynamic,self.current_sync_level,entry.source_table)
     }
     // Device objects used as _PRT Source values are namespace references,
     // not scalar Names. Preserve the reference so the PRT decoder can resolve
     // aliases and canonical paths after method evaluation.
     AmlObjectType::Device=>Ok(make_object_reference(AmlReference::NamespaceNode(canonical))),
     _=>Err(ExecError::NonIntegerName),
    }
   }
   0x71=>{
    let target=self.target(r)?;
    let reference=match target{Target::Reference(reference)=>reference,Target::Local(i)=>self.frame_slot_reference(false,i)?,
     Target::Arg(i)=>self.frame_slot_reference(true,i)?,Target::Null=>return Err(ExecError::BadTarget)};
    Ok(make_object_reference(reference))
   }
   0x88=>{
    let source_start=r.pos;let source=self.term_value(r)?;
    let source_opcode=*r.bytes.get(source_start).ok_or(ExecError::Truncated)?;
    let source_id=match source_opcode{
     0x60..=0x67=>{let slot=(source_opcode-0x60)as usize;let current=self.locals[slot];
      if current.kind==AmlValueKind::ObjectReference{self.reference_object_id(reference_from_value(&current).map_err(|_|ExecError::BadTarget)?)?}
      else{let id=self.own_frame_value(source)?;self.locals[slot]=make_object_reference(AmlReference::ArenaValue(id));id}}
     0x68..=0x6e=>{let slot=(source_opcode-0x68)as usize;let current=self.args[slot];
      if current.kind==AmlValueKind::ObjectReference{self.reference_object_id(reference_from_value(&current).map_err(|_|ExecError::BadTarget)?)?}
      else{let id=self.own_frame_value(source)?;self.args[slot]=make_object_reference(AmlReference::ArenaValue(id));id}}
     0x5c|0x5e|0x2e|0x2f|b'_'|b'A'..=b'Z'=>{let (name,_)=parse_name_string(r.bytes,source_start,r.end).map_err(|_|ExecError::InvalidName)?;
      let node=resolve_name_string(self.namespace,self.scope,&name).map_err(|_|ExecError::NameNotFound)?;
      let canonical=self.namespace.resolve_alias(node).map_err(|_|ExecError::NameNotFound)?;
      if self.namespace.nodes[canonical as usize].object_type==AmlObjectType::Name{let reference=self.name_reference(canonical)?;self.reference_object_id(reference)?}
      else{return Err(ExecError::TargetTypeMismatch)}}
     _=>self.own_frame_value(source)?};
    let index_value=self.term_value(r)?;let index=u16::try_from(self.operand_integer(&index_value)?).map_err(|_|ExecError::TargetTypeMismatch)?;
    let reference=crate::aml_runtime::index_reference(self.arena,source_id,index).map_err(|_|ExecError::BadTarget)?;
    let target=self.target(r)?;self.store_reference_target(target,reference)?;Ok(make_object_reference(reference))
   }
   0x96..=0x99|0x9c=>self.conversion_value(r,op),
   0x83=>{
    let source_start=r.pos;
    let source_opcode=*r.bytes.get(source_start).ok_or(ExecError::Truncated)?;
    let source=match source_opcode{
     0x60..=0x67=>{r.pos+=1;self.locals[(source_opcode-0x60)as usize]},
     0x68..=0x6e=>{r.pos+=1;self.args[(source_opcode-0x68)as usize]},
     _=>self.term_value(r)?,
    };
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
   0x5b=>{
    let extended=r.byte()?;
    if extended==0x13 { r.pos-=2; let value=self.term_value(r)?; return self.operand_integer(&value); }
    let operand=self.term_depth(r,depth+1)?;
    let result=match extended{
     0x28=>Self::from_bcd(operand,self.integer_width_bits)?,
     0x29=>Self::to_bcd(operand,self.integer_width_bits)?,
     _=>return Err(ExecError::UnsupportedOpcode),
    };
    let target=self.target(r)?;
    self.store_value(target,Self::integer_value(result),StoreMode::CopyObject)?;
    Ok(result)
   }
   0x71|0x83|0x88=>{r.pos-=1;let value=self.term_value(r)?;self.operand_integer(&value)}
   0x5b=>{r.pos-=2;let value=self.term_value(r)?;self.operand_integer(&value)},
   0x96..=0x99|0x9c=>{let value=self.conversion_value(r,op)?;self.operand_integer(&value)},0x0a=>Ok(r.byte()? as u64),
   0x0b=>{let b=r.take(2)?;Ok(u16::from_le_bytes([b[0],b[1]])as u64)},
   0x0c=>{let b=r.take(4)?;Ok(u32::from_le_bytes([b[0],b[1],b[2],b[3]])as u64)},
   0x0e=>{if self.integer_width_bits==32{return Err(ExecError::UnsupportedOpcode)}let b=r.take(8)?;Ok(u64::from_le_bytes(b.try_into().map_err(|_|ExecError::Truncated)?))},
   0x8a|0x8b|0x8c|0x8d|0x8f|0x5b=>{r.pos-=1;let value=self.term_value(r)?;self.operand_integer(&value)},
   0x60..=0x67=>{let value=self.locals[(op-0x60)as usize];if value.kind==AmlValueKind::ObjectReference{let reference=reference_from_value(&value).map_err(|_|ExecError::NonIntegerName)?;let deref=dereference(reference,&self.bindings,self.arena).map_err(|_|ExecError::NonIntegerName)?;self.operand_integer(&deref)}else{self.operand_integer(&value)}},0x68..=0x6e=>{let value=self.argument_value((op-0x68)as usize)?;self.operand_integer(&value)},
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
    let current=match t{Target::Null=>0,Target::Local(i)=>self.operand_integer(&self.locals[i])?,Target::Arg(i)=>{let value=self.argument_value(i)?;self.operand_integer(&value)?},Target::Reference(reference)=>{let value=dereference(reference,&self.bindings,&self.arena).map_err(|_|ExecError::NonIntegerName)?;self.operand_integer(&value)?}};
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
    if name.count==1 && name.segments[0]==*b"_OSI" {
     let argument=self.term_value(r)?;
     return Ok(if argument.kind==AmlValueKind::String {
      platform_osi(&argument.bytes[..argument.byte_length as usize])
     } else { 0 });
    }
    let node=resolve_name_string(self.namespace,self.scope,&name).map_err(|_|ExecError::NameNotFound)?;
    let canonical=self.namespace.resolve_alias(node).map_err(|_|ExecError::NameNotFound)?;
    let entry=*self.namespace.nodes.get(canonical as usize).filter(|_|(canonical as usize)<self.namespace.count).ok_or(ExecError::NameNotFound)?;
    match entry.object_type {
     AmlObjectType::Method=>{
      let count=(entry.method_flags&7) as usize;
      let mut call_args=[Self::integer_value(0);7];
      for slot in call_args.iter_mut().take(count){*slot=self.term_value(r)?;}
      {let value=execute_method_inner(self.aml,self.namespace,canonical,&call_args[..count],self.call_depth+1,self.integer_width_bits,self.arena,self.bindings,self.sync,self.dynamic,self.current_sync_level,entry.source_table)?;self.operand_integer(&value)}
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
   0x8a|0x8b|0x8c|0x8d|0x8f|0x5b=>{r.pos-=1;let value=self.term_value(r)?;self.operand_integer(&value)}
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
     // Bound the nested term list to its own AML package before evaluating any child.
     // Predicate and body readers share that boundary, not the full method slice.
     let end=r.pkg_end()?;let plen=1+((r.bytes[r.pos]>>6)as usize);let begin=r.pos+plen;
     if begin>end{return Err(ExecError::BadPackage)}
     let mut body=Reader{bytes:r.bytes,pos:begin,end};
     let mut predicate=true;
     if op==0xa0{
      // If evaluates its predicate once; Else is parsed after the If package and
      // executed only when the predicate is false.
      predicate=self.term(&mut body)?!=0;if predicate{let flow=self.execute(&mut body,depth+1)?;if flow!=ExecFlow::Continue{return Ok(flow);}}}
     else{
      // While re-evaluates its predicate at the start of the bounded body on every iteration.
      let loop_start=body.pos;
      loop{self.step()?;let mut cond=Reader{bytes:body.bytes,pos:loop_start,end:body.end};if self.term(&mut cond)?==0{break}
       let mut iteration=Reader{bytes:body.bytes,pos:cond.pos,end:body.end};
       let flow=self.execute(&mut iteration,depth+1)?;
       if iteration.pos<=loop_start{return Err(ExecError::LoopControl)}
       // Break exits this loop; ContinueLoop restarts this loop's predicate;
       // Return is propagated unchanged to the method boundary.
       // The nearest While consumes Break/ContinueLoop; Return propagates outward.
       match flow{ExecFlow::Continue=>{},ExecFlow::Return(v)=>return Ok(ExecFlow::Return(v)),ExecFlow::Break=>break,ExecFlow::ContinueLoop=>continue}
      }
     }
     r.pos=end;
     // Else is a sibling package immediately following If, not part of its body.
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
    _=>{r.pos-=1;let _=self.term_value(r)?;}
   }
  }
  Ok(ExecFlow::Continue)
 }
}

/// Evaluate one bounded TermArg using caller-owned AML runtime storage.
/// Any temporary arena entries are rolled back before returning.
pub fn evaluate_term_argument_in_context(
    bytes: &[u8], arena: &mut AmlObjectArena, bindings: &mut AmlBindings,
) -> Result<(u64, usize), ExecError> {
    let namespace = AmlNamespace::new();
    let mut dynamic = AmlDynamicNamespace::new();
    let checkpoint = arena.count;
    let empty = Machine::uninitialized_value();
    let mut machine = Machine {
        locals: [empty; 8], args: [empty; 7], arena, bindings,
        integer_width_bits: 64, steps: 0, aml: &[], namespace: &namespace,
        scope: 0, call_depth: 0, sync: &NO_METHOD_SYNCHRONIZER, dynamic: &mut dynamic, current_sync_level: 0, owner_table: 0,
    };
    let mut reader = Reader { bytes, pos: 0, end: bytes.len() };
    let result = machine.term(&mut reader).map(|value| (value, reader.pos));
    machine.arena.count = checkpoint;
    result
}

/// Standalone convenience wrapper. Kernel decoders should use the context-taking
/// form so large arenas remain in their caller-owned pool allocation.
pub fn evaluate_term_argument(bytes: &[u8]) -> Result<(u64, usize), ExecError> {
    let mut arena = {
        #[cfg(test)]
        { test_heap_arena() }
        #[cfg(not(test))]
        { AmlObjectArena::new() }
    };
    let mut bindings = AmlBindings::with_aml_revision(2);
    evaluate_term_argument_in_context(bytes, &mut arena, &mut bindings)
}

/// Execute one loaded method against the exact AML definition-block byte slice.
/// Execute a method using bounded AML runtime objects and an explicit context.
const MAX_METHOD_CALL_DEPTH:u8=16;

fn execute_method_inner(aml:&[u8],namespace:&AmlNamespace,method_index:u16,args:&[AmlRuntimeValue],call_depth:u8,integer_width_bits:u8,arena:&mut AmlObjectArena,bindings:&mut AmlBindings,sync:&dyn AmlMethodSynchronizer,dynamic:&mut AmlDynamicNamespace,current_sync_level:u8,owner_table:u32)->Result<AmlRuntimeValue,ExecError>{
 if call_depth>MAX_METHOD_CALL_DEPTH{return Err(ExecError::CallDepthLimit)}
 let index=method_index as usize;
 if index>=namespace.count{return Err(ExecError::InvalidMethod)}
 let node=namespace.nodes.get(index).ok_or(ExecError::InvalidMethod)?;
 if node.object_type!=AmlObjectType::Method{return Err(ExecError::InvalidMethod)}
 let expected=(node.method_flags&7)as usize;if args.len()!=expected||args.len()>7{return Err(ExecError::BadArgumentCount)}
 for argument in args{validate_runtime_value(argument,arena).map_err(|_|ExecError::UnsupportedArgumentValue)?;}
 if !matches!(integer_width_bits,32|64){return Err(ExecError::UnsupportedOpcode)}
 let serialized=(node.method_flags&0x08)!=0;
 let sync_level=(node.method_flags>>4)&0x0f;
 if serialized&&sync_level<current_sync_level{return Err(ExecError::SyncLevelViolation)}
 if serialized{sync.acquire(method_index,sync_level)?;}
 let active_sync_level=if serialized{sync_level}else{current_sync_level};
 // Each invocation owns its own LocalX/ArgX frame and execution budget.
 // Namespace objects and the method synchronizer are shared with nested calls.
 let result=(||{
  let start=node.body_start as usize;let end=start.checked_add(node.body_length as usize).ok_or(ExecError::InvalidMethod)?;
  if end>aml.len(){return Err(ExecError::InvalidMethod)}
  let empty=Machine::uninitialized_value();
  let mut m=Machine{locals:[empty;8],args:[empty;7],frame_cells:[u16::MAX;128],frame_cell_count:0,arena,bindings,integer_width_bits,steps:0,aml,namespace,scope:node.parent,call_depth,sync,dynamic,current_sync_level:active_sync_level,owner_table:node.source_table};
  for (slot,value) in m.args.iter_mut().zip(args.iter().copied()){*slot=value;}
  let mut r=Reader{bytes:aml,pos:start,end};
  let execution=(||{
   let value=match m.execute(&mut r,0)?{ExecFlow::Continue=>Machine::integer_value(0),ExecFlow::Return(v)=>v,
    ExecFlow::Break|ExecFlow::ContinueLoop=>return Err(ExecError::LoopControl),};Ok(value)
  })();
  m.retire_frame_cells();
  execution
 })();
 if serialized{sync.release(method_index);}
 result
}


/// Execute a scalar-returning AML method using the Integer width encoded by
/// the containing DefinitionBlock (32 bits for revision 1, otherwise 64).
/// Execute a method and preserve Integer, String, or Buffer return objects.
/// Package returns are rejected because their arena-backed element handles
/// cannot outlive this invocation without an owning result arena.
pub fn execute_method_typed_with_sync(aml:&[u8],namespace:&AmlNamespace,
    method_index:u16,args:&[AmlRuntimeValue],integer_width_bits:u8,
    sync:&dyn AmlMethodSynchronizer)->Result<AmlRuntimeValue,ExecError>{
    // This convenience API owns a fresh arena, so caller-supplied arena handles
    // cannot be valid here. Namespace-node references are self-contained and remain valid.
    if args.iter().any(|value|value.kind==AmlValueKind::Package){
        return Err(ExecError::UnsupportedArgumentValue)
    }
    let mut arena = {
        #[cfg(test)]
        { test_heap_arena() }
        #[cfg(not(test))]
        { AmlObjectArena::new() }
    };
    let mut bindings=AmlBindings::with_aml_revision(if integer_width_bits==32{1}else{2});
    let mut dynamic=AmlDynamicNamespace::new();
    let value=execute_method_inner(aml,namespace,method_index,args,0,integer_width_bits,&mut arena,&mut bindings,sync,&mut dynamic,0,0)?;
    // The convenience wrapper destroys its local arena on return, so it must
    // not expose Package handles whose elements live in that arena.
    if value.kind==AmlValueKind::Package || dynamic.field_count()!=0{return Err(ExecError::UnsupportedReturnType)}
    Ok(value)
}

/// Execute a method in a caller-owned AML object context.
///
/// Unlike the convenience wrappers, this API keeps the value arena and
/// namespace bindings alive across the call. Package arguments, Package
/// returns, and references into nested runtime objects therefore remain valid
/// for the caller's context lifetime. The caller must not reset either context
/// while any returned Package or ObjectReference is still in use.
pub fn execute_method_in_dynamic_context(
    aml:&[u8], namespace:&AmlNamespace, method_index:u16, args:&[AmlRuntimeValue],
    integer_width_bits:u8, arena:&mut AmlObjectArena, bindings:&mut AmlBindings,
    dynamic:&mut AmlDynamicNamespace, sync:&dyn AmlMethodSynchronizer,
)->Result<AmlRuntimeValue,ExecError>{
    if args.len()>7{return Err(ExecError::BadArgumentCount)}
    for argument in args {
        validate_runtime_value(argument,arena)
            .map_err(|_|ExecError::UnsupportedArgumentValue)?;
    }
    if !matches!(integer_width_bits,32|64){return Err(ExecError::UnsupportedOpcode)}
    execute_method_inner(aml,namespace,method_index,args,0,integer_width_bits,
        arena,bindings,sync,dynamic,0,0)
}

/// Compatibility wrapper for methods that do not publish dynamic fields.
/// Callers using Create*Field must retain an AmlDynamicNamespace and call
/// execute_method_in_dynamic_context across the AML namespace lifetime.
pub fn execute_method_in_context(
    aml:&[u8], namespace:&AmlNamespace, method_index:u16, args:&[AmlRuntimeValue],
    integer_width_bits:u8, arena:&mut AmlObjectArena, bindings:&mut AmlBindings,
    sync:&dyn AmlMethodSynchronizer,
)->Result<AmlRuntimeValue,ExecError>{
    let mut dynamic=AmlDynamicNamespace::new();
    let value=execute_method_in_dynamic_context(aml,namespace,method_index,args,
        integer_width_bits,arena,bindings,&mut dynamic,sync)?;
    if dynamic.field_count()!=0 { return Err(ExecError::UnsupportedReturnType); }
    Ok(value)
}

/// Convenience entry point for non-Serialized methods. Serialized methods fail
/// closed unless the caller uses execute_method_typed_with_sync and supplies
/// the OS's per-thread implicit-mutex provider.
pub fn execute_method_typed_with_values(aml:&[u8],namespace:&AmlNamespace,
    method_index:u16,args:&[AmlRuntimeValue],integer_width_bits:u8)->Result<AmlRuntimeValue,ExecError>{
    execute_method_typed_with_sync(aml,namespace,method_index,args,integer_width_bits,&NO_METHOD_SYNCHRONIZER)
}

/// Compatibility scalar API: returns an Integer and rejects typed non-Integer
/// results rather than silently coercing a String or Buffer return.
pub fn execute_method_with_values(aml:&[u8],namespace:&AmlNamespace,
    method_index:u16,args:&[AmlRuntimeValue],integer_width_bits:u8)->Result<AmlValue,ExecError>{
    let value=execute_method_typed_with_values(aml,namespace,method_index,args,integer_width_bits)?;
    if value.kind!=AmlValueKind::Integer{return Err(ExecError::NonIntegerName)}
    Ok(AmlValue::Integer(value.integer))
}

/// Scalar-argument entry point for callers that provide an OS method synchronizer.
pub fn execute_method_with_sync(aml:&[u8],namespace:&AmlNamespace,method_index:u16,
    args:&[u64],integer_width_bits:u8,sync:&dyn AmlMethodSynchronizer)->Result<AmlValue,ExecError>{
    if args.len()>7{return Err(ExecError::BadArgumentCount)}
    let mut values=[Machine::integer_value(0);7];
    for (slot,value) in values.iter_mut().zip(args.iter().copied()){*slot=Machine::integer_value(value);}
    let result=execute_method_typed_with_sync(aml,namespace,method_index,&values[..args.len()],integer_width_bits,sync)?;
    if result.kind!=AmlValueKind::Integer{return Err(ExecError::NonIntegerName)}
    Ok(AmlValue::Integer(result.integer))
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
fn test_heap_arena() -> std::boxed::Box<AmlObjectArena> {
    let mut storage = std::boxed::Box::<AmlObjectArena>::new_uninit();
    unsafe {
        core::ptr::write_bytes(
            storage.as_mut_ptr().cast::<u8>(), 0,
            core::mem::size_of::<AmlObjectArena>(),
        );
        storage.assume_init()
    }
}

#[cfg(test)]
mod tests{

 use super::*;use crate::aml::{AmlNamespace,load_definition_block};
 use core::sync::atomic::{AtomicUsize,Ordering};
 struct RecordingSync{acquires:AtomicUsize,releases:AtomicUsize,last_method:AtomicUsize,last_level:AtomicUsize,busy:bool}
 impl RecordingSync{fn new()->Self{Self{acquires:AtomicUsize::new(0),releases:AtomicUsize::new(0),last_method:AtomicUsize::new(usize::MAX),last_level:AtomicUsize::new(usize::MAX),busy:false}}}
 impl AmlMethodSynchronizer for RecordingSync{
  fn acquire(&self,method_index:u16,sync_level:u8)->Result<(),ExecError>{
   if self.busy{return Err(ExecError::ExecutionBusy)}
   self.acquires.fetch_add(1,Ordering::SeqCst);self.last_method.store(method_index as usize,Ordering::SeqCst);
   self.last_level.store(sync_level as usize,Ordering::SeqCst);Ok(())
  }
  fn release(&self,_method_index:u16){self.releases.fetch_add(1,Ordering::SeqCst);}
 }
 #[test]fn executes_explicit_aml_conversion_opcodes_and_stores_typed_targets(){
  let decimal=[0x08,b'S',b'T',b'R',b'0',0x0d,b'4',b'2',0,
   0x14,0x0d,b'T',b'E',b'S',b'T',0,0xa4,0x99,b'S',b'T',b'R',b'0',0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&decimal,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&decimal,&ns,id,&[]).unwrap(),AmlValue::Integer(42));

  let dec=[0x14,0x0b,b'T',b'E',b'S',b'T',0,0xa4,0x97,0x0a,42,0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&dec,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  let value=execute_method_typed_with_values(&dec,&ns,id,&[],64).unwrap();
  assert_eq!(value.kind,AmlValueKind::String);assert_eq!(&value.bytes[..2],b"42");

  let hex=[0x14,0x0b,b'T',b'E',b'S',b'T',0,0xa4,0x98,0x0a,42,0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&hex,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  let value=execute_method_typed_with_values(&hex,&ns,id,&[],32).unwrap();
  assert_eq!(value.kind,AmlValueKind::String);assert_eq!(value.byte_length,8);
  assert_eq!(&value.bytes[..8],b"0000002A");

  // Buffer conversion uses comma-separated byte values, not host-endian integers.
  let decimal_buffer=[0x08,b'B',b'U',b'F',b'0',0x11,0x05,0x0a,2,10,255,
   0x14,0x0d,b'T',b'E',b'S',b'T',0,0xa4,0x97,b'B',b'U',b'F',b'0',0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&decimal_buffer,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  let value=execute_method_typed_with_values(&decimal_buffer,&ns,id,&[],64).unwrap();
  assert_eq!(&value.bytes[..value.byte_length as usize],b"10,255");

  let hex_buffer=[0x08,b'B',b'U',b'F',b'0',0x11,0x05,0x0a,2,10,255,
   0x14,0x0d,b'T',b'E',b'S',b'T',0,0xa4,0x98,b'B',b'U',b'F',b'0',0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&hex_buffer,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  let value=execute_method_typed_with_values(&hex_buffer,&ns,id,&[],64).unwrap();
  assert_eq!(&value.bytes[..value.byte_length as usize],b"0A,FF");

  let to_buffer=[0x08,b'S',b'T',b'R',b'0',0x0d,b'O',b'K',0,
   0x14,0x0d,b'T',b'E',b'S',b'T',0,0xa4,0x96,b'S',b'T',b'R',b'0',0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&to_buffer,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  let value=execute_method_typed_with_values(&to_buffer,&ns,id,&[],64).unwrap();
  assert_eq!(value.kind,AmlValueKind::Buffer);assert_eq!(value.byte_length,3);
  assert_eq!(&value.bytes[..3],b"OK\0");

  let to_string=[0x08,b'B',b'U',b'F',b'0',0x11,0x06,0x0a,3,b'O',b'K',0,
   0x14,0x0f,b'T',b'E',b'S',b'T',0,0xa4,0x9c,b'B',b'U',b'F',b'0',0x0a,3,0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&to_string,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  let value=execute_method_typed_with_values(&to_string,&ns,id,&[],64).unwrap();
  assert_eq!(value.kind,AmlValueKind::String);assert_eq!(value.byte_length,2);
  assert_eq!(&value.bytes[..2],b"OK");
 }
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

  let read_aml=[0x08,b'N',b'A',b'M',b'0',0x01,
      0x14,0x0b,b'T',b'E',b'S',b'T',1,0xa4,0x72,0x68,0x01,0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&read_aml,&mut ns).unwrap();
  let node=ns.find_child(0,*b"NAM0").unwrap();let id=ns.find_child(0,*b"TEST").unwrap();
  let reference=make_object_reference(AmlReference::NamespaceNode(node));
  assert_eq!(execute_method_with_values(&read_aml,&ns,id,&[reference],64).unwrap(),AmlValue::Integer(2));
 }

 #[test]fn refof_and_derefof_preserve_namespace_object_references(){
  let aml=[0x08,b'N',b'A',b'M',b'0',0x01,
      0x14,0x0d,b'T',b'E',b'S',b'T',0,0xa4,0x83,0x71,b'N',b'A',b'M',b'0'];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]).unwrap(),AmlValue::Integer(1));
 }

 #[test]fn refof_local_derefof_and_frame_lifetime_are_enforced(){
  let aml=[0x14,0x0e,b'T',b'E',b'S',b'T',0,
   0x70,0x0a,42,0x60,0xa4,0x83,0x71,0x60];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]).unwrap(),AmlValue::Integer(42));

  let escaping=[0x14,0x09,b'T',b'E',b'S',b'T',0,0xa4,0x71,0x60];
  let mut ns=AmlNamespace::new();load_definition_block(&escaping,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  let mut arena=test_heap_arena();let mut bindings=AmlBindings::with_aml_revision(2);
  let result=execute_method_in_context(&escaping,&ns,id,&[],64,&mut arena,&mut bindings,&NO_METHOD_SYNCHRONIZER).unwrap();
  assert_eq!(result.kind,AmlValueKind::ObjectReference);
  assert_eq!(validate_runtime_value(&result,&arena),Err(RuntimeError::InvalidReference));
 }

 #[test]fn index_returns_live_package_element_reference(){
  let aml=[0x08,b'P',b'K',b'G',b'0',0x12,0x06,0x02,0x0a,0x11,0x0a,0x22,
   0x14,0x0f,b'T',b'E',b'S',b'T',0,0xa4,0x83,0x88,b'P',b'K',b'G',b'0',0x01,0x60];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]).unwrap(),AmlValue::Integer(0x22));
 }

 #[test]fn condrefof_reports_present_and_missing_namespace_objects(){
  let present=[0x08,b'N',b'A',b'M',b'0',0x01,
   0x14,0x0e,b'T',b'E',b'S',b'T',0,0xa4,0x5b,0x12,b'N',b'A',b'M',b'0',0x60];
  let mut ns=AmlNamespace::new();load_definition_block(&present,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&present,&ns,id,&[]).unwrap(),AmlValue::Integer(1));

  let missing=[0x14,0x0e,b'T',b'E',b'S',b'T',0,0xa4,0x5b,0x12,b'M',b'I',b'S',b'S',0x60];
  let mut ns=AmlNamespace::new();load_definition_block(&missing,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&missing,&ns,id,&[]).unwrap(),AmlValue::Integer(0));
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
 #[test]fn rejects_method_arguments_with_unowned_arena_handles_or_invalid_extents(){
  let aml=[0x14,0x08,b'T',b'E',b'S',b'T',1,0xa4,0x68];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();

  let mut bad_buffer=AmlRuntimeValue::EMPTY;bad_buffer.kind=AmlValueKind::Buffer;
  bad_buffer.byte_length=(bad_buffer.bytes.len()+1) as u16;
  assert_eq!(execute_method_typed_with_values(&aml,&ns,id,&[bad_buffer],64),Err(ExecError::UnsupportedArgumentValue));

  let mut bad_package=AmlRuntimeValue::EMPTY;bad_package.kind=AmlValueKind::Package;
  bad_package.element_count=1;bad_package.elements[0]=u16::MAX;
  assert_eq!(execute_method_typed_with_values(&aml,&ns,id,&[bad_package],64),Err(ExecError::UnsupportedArgumentValue));

  let reference=make_object_reference(AmlReference::PackageElement{package:u16::MAX,index:0});
  assert_eq!(execute_method_typed_with_values(&aml,&ns,id,&[reference],64),Err(ExecError::UnsupportedArgumentValue));
 }
 #[test]fn shared_context_preserves_package_arguments_and_returns(){
  let aml=[0x14,0x08,b'T',b'E',b'S',b'T',1,0xa4,0x68];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  let mut arena = test_heap_arena();
  let mut child=AmlRuntimeValue::EMPTY;child.kind=AmlValueKind::Integer;child.integer=0x1234;
  let child_id=arena.allocate(child).unwrap();
  let mut package=AmlRuntimeValue::EMPTY;package.kind=AmlValueKind::Package;
  package.element_count=1;package.elements[0]=child_id;
  let package_id=arena.allocate(package).unwrap();
  let argument=*arena.get(package_id).unwrap();
  let mut bindings=AmlBindings::with_aml_revision(2);
  let result=execute_method_in_context(&aml,&ns,id,&[argument],64,
      &mut arena,&mut bindings,&NO_METHOD_SYNCHRONIZER).unwrap();
  assert_eq!(result.kind,AmlValueKind::Package);
  assert_eq!(result.element_count,1);
  let returned_child=arena.get(result.elements[0]).unwrap();
  assert_eq!(returned_child.kind,AmlValueKind::Integer);
  assert_eq!(returned_child.integer,0x1234);
 }

 #[test]fn serialized_method_requires_host_sync_and_releases_its_implicit_mutex(){
  let aml=[0x14,0x08,b'T',b'E',b'S',b'T',0x38,0xa4,0x01];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]),Err(ExecError::SynchronizationUnavailable));

  let sync=RecordingSync::new();
  let value=execute_method_typed_with_sync(&aml,&ns,id,&[],64,&sync).unwrap();
  assert_eq!(value.kind,AmlValueKind::Integer);assert_eq!(value.integer,1);
  assert_eq!(sync.acquires.load(Ordering::SeqCst),1);
  assert_eq!(sync.releases.load(Ordering::SeqCst),1);
  assert_eq!(sync.last_method.load(Ordering::SeqCst),id as usize);
  assert_eq!(sync.last_level.load(Ordering::SeqCst),3);
 }
 #[test]fn serialized_method_propagates_busy_lock_without_releasing_unowned_mutex(){
  let aml=[0x14,0x08,b'T',b'E',b'S',b'T',0x18,0xa4,0x01];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  let sync=RecordingSync{busy:true,..RecordingSync::new()};
  assert_eq!(execute_method_typed_with_sync(&aml,&ns,id,&[],64,&sync),Err(ExecError::ExecutionBusy));
  assert_eq!(sync.acquires.load(Ordering::SeqCst),0);
  assert_eq!(sync.releases.load(Ordering::SeqCst),0);
 }
 #[test]fn serialized_method_releases_implicit_mutex_after_execution_error(){
  let aml=[0x14,0x07,b'T',b'E',b'S',b'T',0x18,0x16];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();let sync=RecordingSync::new();
  assert_eq!(execute_method_typed_with_sync(&aml,&ns,id,&[],64,&sync),Err(ExecError::UnsupportedOpcode));
  assert_eq!(sync.acquires.load(Ordering::SeqCst),1);
  assert_eq!(sync.releases.load(Ordering::SeqCst),1);
 }
 #[test]fn serialized_recursive_method_reuses_same_thread_lock_until_call_depth_limit(){
  let aml=[0x14,0x0b,b'T',b'E',b'S',b'T',0x18,0xa4,b'T',b'E',b'S',b'T'];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();let sync=RecordingSync::new();
  assert_eq!(execute_method_with_sync(&aml,&ns,id,&[],64,&sync),Err(ExecError::CallDepthLimit));
  assert_eq!(sync.acquires.load(Ordering::SeqCst),17);
  assert_eq!(sync.releases.load(Ordering::SeqCst),17);
 }
 struct ConcurrentMethodSync{held:core::sync::atomic::AtomicBool,active:AtomicUsize,max_active:AtomicUsize}
 impl ConcurrentMethodSync{fn new()->Self{Self{held:core::sync::atomic::AtomicBool::new(false),active:AtomicUsize::new(0),max_active:AtomicUsize::new(0)}}}
 impl AmlMethodSynchronizer for ConcurrentMethodSync{
  fn acquire(&self,_method_index:u16,_sync_level:u8)->Result<(),ExecError>{
   while self.held.compare_exchange(false,true,Ordering::Acquire,Ordering::Relaxed).is_err(){std::thread::yield_now();}
   let active=self.active.fetch_add(1,Ordering::AcqRel)+1;
   self.max_active.fetch_max(active,Ordering::AcqRel);
   // Keep the lock held long enough for the competing host threads to contend.
   std::thread::sleep(std::time::Duration::from_millis(2));
   Ok(())
  }
  fn release(&self,_method_index:u16){
   self.active.fetch_sub(1,Ordering::AcqRel);
   self.held.store(false,Ordering::Release);
  }
 }
 #[test]fn serialized_method_invocations_are_exclusive_across_host_threads(){
  let aml=[0x14,0x08,b'T',b'E',b'S',b'T',0x08,0xa4,0x01];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  let sync=ConcurrentMethodSync::new();
  std::thread::scope(|scope|{
   for _ in 0..6{
    scope.spawn(||{
     let result=execute_method_typed_with_sync(&aml,&ns,id,&[],64,&sync).unwrap();
     assert_eq!(result.kind,AmlValueKind::Integer);
     assert_eq!(result.integer,1);
    });
   }
  });
  assert_eq!(sync.max_active.load(Ordering::Acquire),1);
  assert_eq!(sync.active.load(Ordering::Acquire),0);
 }
 #[test]fn serialized_nested_methods_enforce_monotonic_sync_levels(){
  let allowed=[0x14,0x08,b'I',b'N',b'N',b'R',0x38,0xa4,0x01,
   0x14,0x0b,b'O',b'U',b'T',b'R',0x28,0xa4,b'I',b'N',b'N',b'R'];
  let mut ns=AmlNamespace::new();load_definition_block(&allowed,&mut ns).unwrap();
  let id=ns.find_child(0,*b"OUTR").unwrap();let sync=RecordingSync::new();
  assert_eq!(execute_method_typed_with_sync(&allowed,&ns,id,&[],64,&sync).unwrap().integer,1);
  assert_eq!(sync.acquires.load(Ordering::SeqCst),2);
  assert_eq!(sync.releases.load(Ordering::SeqCst),2);

  let denied=[0x14,0x08,b'I',b'N',b'N',b'R',0x28,0xa4,0x01,
   0x14,0x0b,b'O',b'U',b'T',b'R',0x38,0xa4,b'I',b'N',b'N',b'R'];
  let mut ns=AmlNamespace::new();load_definition_block(&denied,&mut ns).unwrap();
  let id=ns.find_child(0,*b"OUTR").unwrap();let sync=RecordingSync::new();
  assert_eq!(execute_method_typed_with_sync(&denied,&ns,id,&[],64,&sync),Err(ExecError::SyncLevelViolation));
  assert_eq!(sync.acquires.load(Ordering::SeqCst),1);
  assert_eq!(sync.releases.load(Ordering::SeqCst),1);
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
 #[test]fn control_flow_executes_else_only_when_if_predicate_is_false(){
  let aml=[0x14,0x10,b'T',b'E',b'S',b'T',0,
   0xa0,0x04,0x00,0xa4,0x01,
   0xa1,0x04,0xa4,0x0a,2];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]).unwrap(),AmlValue::Integer(2));

  let true_aml=[0x14,0x10,b'T',b'E',b'S',b'T',0,
   0xa0,0x04,0x01,0xa4,0x01,
   0xa1,0x04,0xa4,0x0a,2];
  let mut ns=AmlNamespace::new();load_definition_block(&true_aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&true_aml,&ns,id,&[]).unwrap(),AmlValue::Integer(1));
 }
 #[test]fn control_flow_rejects_if_and_else_packages_that_escape_method_bounds(){
  let bad_if=[0x14,0x0b,b'T',b'E',b'S',b'T',0,0xa0,0x7f,0x01,0xa4,0x01];
  let mut ns=AmlNamespace::new();load_definition_block(&bad_if,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&bad_if,&ns,id,&[]),Err(ExecError::BadPackage));

  let bad_else=[0x14,0x0d,b'T',b'E',b'S',b'T',0,
   0xa0,0x04,0x00,0xa4,0x01,0xa1,0x7f];
  let mut ns=AmlNamespace::new();load_definition_block(&bad_else,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&bad_else,&ns,id,&[]),Err(ExecError::BadPackage));

  let bad_while=[0x14,0x0b,b'T',b'E',b'S',b'T',0,0xa2,0x7f,0x01,0xa4,0x01];
  let mut ns=AmlNamespace::new();load_definition_block(&bad_while,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&bad_while,&ns,id,&[]),Err(ExecError::BadPackage));
 }
 #[test]fn control_flow_return_escapes_a_live_while_body(){
  let aml=[0x14,0x0c,b'T',b'E',b'S',b'T',0,0xa2,0x05,0x01,0xa4,0x0a,42];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]).unwrap(),AmlValue::Integer(42));
 }
 #[test]fn rejects_else_without_a_preceding_if(){
  let aml=[0x14,0x0a,b'T',b'E',b'S',b'T',0,0xa1,0x03,0xa4,0x01];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]),Err(ExecError::UnsupportedOpcode));
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
 #[test]fn covers_conversion_integer_buffer_and_width_boundaries(){
  let to_buffer_integer=[0x14,0x0e,b'T',b'E',b'S',b'T',0,
   0xa4,0x96,0x0c,0x34,0x12,0x00,0x00,0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&to_buffer_integer,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  let value=execute_method_typed_with_values(&to_buffer_integer,&ns,id,&[],32).unwrap();
  assert_eq!(value.kind,AmlValueKind::Buffer);assert_eq!(value.byte_length,4);
  assert_eq!(&value.bytes[..4],&[0x34,0x12,0,0]);

  let to_integer_buffer=[0x08,b'B',b'U',b'F',b'0',0x11,0x07,0x0a,4,0x78,0x56,0x34,0x12,
   0x14,0x0d,b'T',b'E',b'S',b'T',0,0xa4,0x99,b'B',b'U',b'F',b'0',0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&to_integer_buffer,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&to_integer_buffer,&ns,id,&[]).unwrap(),AmlValue::Integer(0x12345678));

  let bcd32=[0x14,0x0f,b'T',b'E',b'S',b'T',0,
   0xa4,0x5b,0x29,0x0c,0x00,0xe1,0xf5,0x05,0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&bcd32,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method_with_integer_width(&bcd32,&ns,id,&[],32),Err(ExecError::TargetTypeMismatch));
  assert_eq!(execute_method_with_integer_width(&bcd32,&ns,id,&[],64).unwrap(),AmlValue::Integer(0x100000000));
 }
 #[test]fn evaluates_composite_logical_comparisons_from_their_aml_encoding(){
  // LNot(LEqual(1, 2)) == LNotEqual(1, 2).
  assert_eq!(evaluate_term_argument(&[0x92,0x93,0x01,0x0a,2]),Ok((1,5)));
  // LNot(LLess(2, 1)) == LGreaterEqual(2, 1).
  assert_eq!(evaluate_term_argument(&[0x92,0x95,0x0a,2,0x01]),Ok((1,5)));
  // LNot(LGreater(1, 2)) == LLessEqual(1, 2).
  assert_eq!(evaluate_term_argument(&[0x92,0x94,0x01,0x0a,2]),Ok((1,5)));
 }
 #[test]fn executes_to_bcd_and_from_bcd_extended_conversion_opcodes(){
  // Decimal 305419896 is packed as nine BCD digits: 0x305419896.
  assert_eq!(evaluate_term_argument(&[0x5b,0x29,0x0c,0x78,0x56,0x34,0x12,0x00]),Ok((0x305419896,8)));
  assert_eq!(evaluate_term_argument(&[0x5b,0x28,0x0e,0x96,0x98,0x41,0x05,0x03,0x00,0x00,0x00,0x00]),Ok((305419896,12)));
  assert_eq!(evaluate_term_argument(&[0x5b,0x28,0x0c,0xfa,0x12,0x00,0x00,0x00]),Err(ExecError::TargetTypeMismatch));
  // A 64-bit BCD result cannot encode more than sixteen decimal digits.
  assert_eq!(evaluate_term_argument(&[0x5b,0x29,0x0e,0x00,0x00,0xc1,0x6f,0xf2,0x86,0x23,0x00]),Err(ExecError::TargetTypeMismatch));
 }
 #[test]fn covers_subtract_and_explicit_prefixed_hex_to_integer(){
  assert_eq!(evaluate_term_argument(&[0x74,0x0a,5,0x0a,2,0x00]),Ok((3,6)));
  let aml=[0x08,b'S',b'T',b'R',b'0',0x0d,b'0',b'x',b'2',b'A',0,
   0x14,0x0d,b'T',b'E',b'S',b'T',0,0xa4,0x99,b'S',b'T',b'R',b'0',0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&aml,&ns,id,&[]).unwrap(),AmlValue::Integer(42));
 }
 #[test]fn to_string_stops_at_nul_when_length_is_ones(){
  let aml=[0x08,b'B',b'U',b'F',b'0',0x11,0x06,0x0a,3,b'A',0,b'B',
   0x14,0x0e,b'T',b'E',b'S',b'T',0,0xa4,0x9c,b'B',b'U',b'F',b'0',0xff,0x00];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  let value=execute_method_typed_with_values(&aml,&ns,id,&[],64).unwrap();
  assert_eq!(value.kind,AmlValueKind::String);assert_eq!(value.byte_length,1);
  assert_eq!(value.bytes[0],b'A');
 }
 #[test]fn evaluates_logical_and_all_bitwise_operator_families(){
  assert_eq!(evaluate_term_argument(&[0x7b,0x0a,0xf0,0x0a,0x0f,0x00]),Ok((0,6)));
  assert_eq!(evaluate_term_argument(&[0x7c,0x0a,0xf0,0x0a,0x0f,0x00]),Ok((u64::MAX,6)));
  assert_eq!(evaluate_term_argument(&[0x7d,0x0a,0xf0,0x0a,0x0f,0x00]),Ok((0xff,6)));
  assert_eq!(evaluate_term_argument(&[0x7e,0x0a,0xf0,0x0a,0x0f,0x00]),Ok((!0xffu64,6)));
  assert_eq!(evaluate_term_argument(&[0x7f,0x0a,0xf0,0x0a,0x0f,0x00]),Ok((0xff,6)));
  assert_eq!(evaluate_term_argument(&[0x90,0x01,0x00]),Ok((0,3)));
  assert_eq!(evaluate_term_argument(&[0x91,0x00,0x01]),Ok((1,3)));
  assert_eq!(evaluate_term_argument(&[0x92,0x00]),Ok((1,2)));
  assert_eq!(evaluate_term_argument(&[0x93,0x01,0x01]),Ok((1,3)));
  assert_eq!(evaluate_term_argument(&[0x94,0x0a,2,0x0a,1]),Ok((1,5)));
  assert_eq!(evaluate_term_argument(&[0x95,0x0a,1,0x0a,2]),Ok((1,5)));
 }
 #[test]fn arithmetic_operator_targets_apply_named_object_result_conversion(){
  let aml=[0x08,b'S',b'T',b'R',b'0',0x0d,b'0',b'0',0,
   0x14,0x13,b'T',b'E',b'S',b'T',0,
   0x72,0x01,0x0a,2,b'S',b'T',b'R',b'0',
   0xa4,b'S',b'T',b'R',b'0'];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  let value=execute_method_typed_with_values(&aml,&ns,id,&[],64).unwrap();
  assert_eq!(value.kind,AmlValueKind::String);
  assert_eq!(value.byte_length,16);
  assert_eq!(&value.bytes[..16],b"0000000000000003");
 }
 #[test]fn explicit_conversion_target_replaces_existing_name_type(){
  let aml=[0x08,b'S',b'T',b'R',b'0',0x0d,b'4',b'2',0,
   0x08,b'V',b'A',b'L',b'0',0x0d,b'x',0,
   0x14,0x14,b'T',b'E',b'S',b'T',0,
   0x99,b'S',b'T',b'R',b'0',b'V',b'A',b'L',b'0',
   0xa4,b'V',b'A',b'L',b'0'];
  let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  let value=execute_method_typed_with_values(&aml,&ns,id,&[],64).unwrap();
  assert_eq!(value.kind,AmlValueKind::Integer);assert_eq!(value.integer,42);
 }
 #[test]fn evaluates_predefined_osi_for_windows_11_firmware_profiles(){
  // Return(_OSI("Windows 2021")) and Return(_OSI("Windows 2022")).
  let windows_11=[0x14,0x19,b'T',b'E',b'S',b'T',0,0xa4,b'_',b'O',b'S',b'I',0x0d,b'W',b'i',b'n',b'd',b'o',b'w',b's',b' ',b'2',b'0',b'2',b'1',0];
  let mut ns=AmlNamespace::new();load_definition_block(&windows_11,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&windows_11,&ns,id,&[]).unwrap(),AmlValue::Integer(0xFFFF_FFFF));
  let windows_22=[0x14,0x19,b'T',b'E',b'S',b'T',0,0xa4,b'_',b'O',b'S',b'I',0x0d,b'W',b'i',b'n',b'd',b'o',b'w',b's',b' ',b'2',b'0',b'2',b'2',0];
  let mut ns=AmlNamespace::new();load_definition_block(&windows_22,&mut ns).unwrap();
  let id=ns.find_child(0,*b"TEST").unwrap();
  assert_eq!(execute_method(&windows_22,&ns,id,&[]).unwrap(),AmlValue::Integer(0xFFFF_FFFF));
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
 #[test]
 fn executes_create_dword_field_store_and_reload_through_persistent_context(){
  // Name(BUF0, Buffer(4) {0,0,0,0}); Method(TEST) {
  //   CreateDWordField(BUF0, Zero, FLD0); Store(One, FLD0); Return(FLD0)
  // }
  let aml=[
   0x08,b'B',b'U',b'F',b'0',0x11,0x07,0x0a,4,0,0,0,0,
   0x14,0x1c,b'T',b'E',b'S',b'T',0,
   0x8a,b'B',b'U',b'F',b'0',0x0a,0,b'F',b'L',b'D',b'0',
   0x70,0x01,b'F',b'L',b'D',b'0',
   0xa4,b'F',b'L',b'D',b'0'
  ];
  let mut namespace=AmlNamespace::new();
  load_definition_block(&aml,&mut namespace).unwrap();
  let method=namespace.find_child(0,*b"TEST").unwrap();
  let mut arena=test_heap_arena();
  let mut bindings=AmlBindings::with_aml_revision(2);
  let mut dynamic=AmlDynamicNamespace::new();
  let result=execute_method_in_dynamic_context(&aml,&namespace,method,&[],64,
      &mut arena,&mut bindings,&mut dynamic,&NO_METHOD_SYNCHRONIZER).unwrap();
  assert_eq!(result.kind,AmlValueKind::Integer);
  assert_eq!(result.integer,1);
  assert_eq!(dynamic.field_count(),1);
  let field=dynamic.resolve_buffer_field(&namespace,0,
      &crate::aml::AmlName{segments:[*b"FLD0",[0;4],[0;4],[0;4],[0;4],[0;4],[0;4],[0;4]],count:1,rooted:false,parents:0}).unwrap();
  let reference=dynamic.field_reference(&namespace,field).unwrap();
  assert_eq!(dereference(reference,&bindings,&arena).unwrap().integer,1);
 }

 #[test]
 fn create_field_rejects_out_of_range_bit_extent_without_publishing_name(){
  let aml=[
   0x08,b'B',b'U',b'F',b'0',0x11,0x07,0x0a,4,0,0,0,0,
   0x14,0x16,b'T',b'E',b'S',b'T',0,
   0x5b,0x13,b'B',b'U',b'F',b'0',0x0a,31,0x0a,2,b'F',b'L',b'D',b'0',
   0xa4,0x00
  ];
  let mut namespace=AmlNamespace::new();
  load_definition_block(&aml,&mut namespace).unwrap();
  let method=namespace.find_child(0,*b"TEST").unwrap();
  let mut arena=test_heap_arena();
  let mut bindings=AmlBindings::with_aml_revision(2);
  let mut dynamic=AmlDynamicNamespace::new();
  assert_eq!(execute_method_in_dynamic_context(&aml,&namespace,method,&[],64,
      &mut arena,&mut bindings,&mut dynamic,&NO_METHOD_SYNCHRONIZER),
      Err(ExecError::TargetTypeMismatch));
  assert_eq!(dynamic.field_count(),0);
 }

}
