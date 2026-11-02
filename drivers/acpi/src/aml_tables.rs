//! AML-6 ACPI definition-table lifecycle.
//! DSDT/SSDT SDT validation, bounded namespace installation, and strict
//! reverse-order unload. Firmware addresses and table mapping remain owned by
//! the ACPI table-discovery layer; callers provide validated readable slices.
#![no_std]

use crate::{parse_sdt_header,AcpiError};
use crate::aml::{AmlError,AmlNamespace,load_definition_block,MAX_NAMESPACE_NODES};
use crate::aml_exec::{self,AmlValue,ExecError};

pub const MAX_AML_TABLES:usize=32;
const SDT_HEADER_SIZE:usize=36;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum TableError {
    InvalidSdt(AcpiError),
    UnsupportedSignature,
    DsdTAlreadyLoaded,
    DsdTRequired,
    DuplicateTable,
    TableLimit,
    NamespaceCapacity,
    Parse(AmlError),
    InvalidHandle,
    UnloadOrder,
    NamespaceChanged,
    ExecutionBusy,
    MethodExecution(ExecError),
}
impl From<AcpiError> for TableError {fn from(e:AcpiError)->Self{Self::InvalidSdt(e)}}
impl From<AmlError> for TableError {fn from(e:AmlError)->Self{Self::Parse(e)}}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct TableHandle {slot:u8,generation:u32}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct AmlTableInfo {
    pub signature:[u8;4],
    pub revision:u8,
    pub oem_id:[u8;6],
    pub oem_table_id:[u8;8],
    pub oem_revision:u32,
    pub creator_id:u32,
    pub creator_revision:u32,
    pub length:u32,
    pub namespace_start:u16,
    pub namespace_end:u16,
}
#[derive(Clone,Copy)]
struct TableRecord {active:bool,generation:u32,info:AmlTableInfo}
impl TableRecord {
    const EMPTY:Self=Self{active:false,generation:0,info:AmlTableInfo{
        signature:[0;4],revision:0,oem_id:[0;6],oem_table_id:[0;8],
        oem_revision:0,creator_id:0,creator_revision:0,length:0,
        namespace_start:0,namespace_end:0,
    }};
}
/// Platform/runtime contract: stop new AML method entry and wait for all
/// in-flight AML methods to leave before namespace mutation. Implementations
/// must be safe against concurrent method dispatch on every CPU.
pub trait AmlExecutionGate {
    fn enter_quiescent(&mut self)->Result<(),()>;
    fn leave_quiescent(&mut self);
}

pub struct AmlTableManager {
    namespace:AmlNamespace,
    records:[TableRecord;MAX_AML_TABLES],
    count:usize,
    next_generation:u32,
    dsdt_loaded:bool,
}
impl AmlTableManager {
    pub const fn new()->Self {
        Self{namespace:AmlNamespace::new(),records:[TableRecord::EMPTY;MAX_AML_TABLES],
            count:0,next_generation:1,dsdt_loaded:false}
    }
    pub fn namespace(&self)->&AmlNamespace{&self.namespace}
    pub fn table_count(&self)->usize{self.count}
    pub fn dsdt_loaded(&self)->bool{self.dsdt_loaded}
    pub fn table_info(&self,handle:TableHandle)->Result<AmlTableInfo,TableError>{
        let record=self.records.get(handle.slot as usize).filter(|r|r.active&&r.generation==handle.generation).ok_or(TableError::InvalidHandle)?;
        Ok(record.info)
    }
    pub fn load_dsdt<G:AmlExecutionGate>(&mut self,bytes:&[u8],gate:&mut G)->Result<TableHandle,TableError>{
        self.with_quiescence(gate,|manager|{
            if manager.dsdt_loaded||manager.count!=0{return Err(TableError::DsdTAlreadyLoaded)}
            manager.load(bytes,*b"DSDT")
        })
    }
    pub fn load_ssdt<G:AmlExecutionGate>(&mut self,bytes:&[u8],gate:&mut G)->Result<TableHandle,TableError>{
        self.with_quiescence(gate,|manager|{
            if !manager.dsdt_loaded{return Err(TableError::DsdTRequired)}
            manager.load(bytes,*b"SSDT")
        })
    }
    fn with_quiescence<T,G:AmlExecutionGate>(&mut self,gate:&mut G,operation:impl FnOnce(&mut Self)->Result<T,TableError>)->Result<T,TableError>{
        gate.enter_quiescent().map_err(|_|TableError::ExecutionBusy)?;
        let result=operation(self);
        gate.leave_quiescent();
        result
    }
    fn load(&mut self,bytes:&[u8],expected:[u8;4])->Result<TableHandle,TableError>{
        if self.count>=MAX_AML_TABLES{return Err(TableError::TableLimit)}
        let header=parse_sdt_header(bytes)?;
        if header.signature!=expected{return Err(TableError::UnsupportedSignature)}
        let length=header.length as usize;
        if length<SDT_HEADER_SIZE||length>bytes.len(){return Err(TableError::InvalidSdt(AcpiError::BadLength))}
        let info=AmlTableInfo{
            signature:header.signature,revision:header.revision,oem_id:header.oem_id,
            oem_table_id:header.oem_table_id,oem_revision:header.oem_revision,
            creator_id:header.creator_id,creator_revision:header.creator_revision,
            length:header.length,namespace_start:self.namespace.count as u16,
            namespace_end:0,
        };
        if self.records.iter().take(self.count).any(|r|r.active&&same_identity(r.info,info)){
            return Err(TableError::DuplicateTable)
        }
        if self.namespace.count>=MAX_NAMESPACE_NODES{return Err(TableError::NamespaceCapacity)}
        let before=self.namespace.count;
        load_definition_block(&bytes[SDT_HEADER_SIZE..length],&mut self.namespace)?;
        if self.namespace.count>u16::MAX as usize{
            self.namespace.count=before;
            return Err(TableError::NamespaceCapacity)
        }
        let mut stored=info;
        stored.namespace_end=self.namespace.count as u16;
        let slot=self.count;
        let generation=self.next_generation;
        self.next_generation=self.next_generation.wrapping_add(1).max(1);
        self.records[slot]=TableRecord{active:true,generation,info:stored};
        self.count+=1;
        if expected==*b"DSDT"{self.dsdt_loaded=true;}
        Ok(TableHandle{slot:slot as u8,generation})
    }
    /// Execute a method only when its namespace node belongs to this table.
    /// The caller supplies the same validated table bytes retained by the ACPI
    /// table owner; the manager verifies the table identity and exact extent.
    pub fn execute_method(&self,handle:TableHandle,bytes:&[u8],method_index:u16,args:&[u64])->Result<AmlValue,TableError>{
        let info=self.table_info(handle)?;
        let header=parse_sdt_header(bytes)?;
        if !same_identity(info,AmlTableInfo{signature:header.signature,revision:header.revision,
            oem_id:header.oem_id,oem_table_id:header.oem_table_id,oem_revision:header.oem_revision,
            creator_id:header.creator_id,creator_revision:header.creator_revision,length:header.length,
            namespace_start:0,namespace_end:0})||header.length!=info.length{
            return Err(TableError::InvalidHandle)
        }
        if method_index<info.namespace_start||method_index>=info.namespace_end{
            return Err(TableError::InvalidHandle)
        }
        aml_exec::execute_method(&bytes[SDT_HEADER_SIZE..info.length as usize],&self.namespace,method_index,args)
            .map_err(TableError::MethodExecution)
    }
    /// Only the most recently installed table may be removed. This preserves
    /// namespace indices and prevents unloading definitions underneath later
    /// SSDTs. A caller must also quiesce AML method execution before unloading.
    pub fn unload<G:AmlExecutionGate>(&mut self,handle:TableHandle,gate:&mut G)->Result<AmlTableInfo,TableError>{
        self.with_quiescence(gate,|manager|manager.unload_quiesced(handle))
    }
    fn unload_quiesced(&mut self,handle:TableHandle)->Result<AmlTableInfo,TableError>{
        if self.count==0||handle.slot as usize!=self.count-1{return Err(TableError::UnloadOrder)}
        let record=self.records[handle.slot as usize];
        if !record.active||record.generation!=handle.generation{return Err(TableError::InvalidHandle)}
        if self.namespace.count!=record.info.namespace_end as usize{return Err(TableError::NamespaceChanged)}
        if record.info.signature==*b"DSDT"&&self.count!=1{return Err(TableError::UnloadOrder)}
        self.namespace.count=record.info.namespace_start as usize;
        self.records[handle.slot as usize]=TableRecord::EMPTY;
        self.count-=1;
        if record.info.signature==*b"DSDT"{self.dsdt_loaded=false;}
        Ok(record.info)
    }
}
impl Default for AmlTableManager {fn default()->Self{Self::new()}}

fn same_identity(a:AmlTableInfo,b:AmlTableInfo)->bool{
    a.signature==b.signature&&a.oem_id==b.oem_id&&a.oem_table_id==b.oem_table_id
        &&a.oem_revision==b.oem_revision&&a.creator_id==b.creator_id
        &&a.creator_revision==b.creator_revision
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestGate{busy:bool,entered:bool}
    impl AmlExecutionGate for TestGate{
        fn enter_quiescent(&mut self)->Result<(),()>{if self.busy{return Err(())}self.entered=true;Ok(())}
        fn leave_quiescent(&mut self){self.entered=false;}
    }
    fn gate()->TestGate{TestGate{busy:false,entered:false}}
    fn table(signature:[u8;4],name:[u8;4],value:u8)->[u8;43]{
        let mut b=[0u8;43];
        b[0..4].copy_from_slice(&signature);
        b[4..8].copy_from_slice(&(43u32).to_le_bytes());
        b[8]=2;
        b[10..16].copy_from_slice(b"COGNAC");
        b[16..24].copy_from_slice(b"AMLTEST ");
        b[24..28].copy_from_slice(&1u32.to_le_bytes());
        b[28..32].copy_from_slice(b"COGN");
        b[32..36].copy_from_slice(&1u32.to_le_bytes());
        b[36]=0x08;b[37..41].copy_from_slice(&name);b[41]=0x0a;b[42]=value;
        b[9]=0u8.wrapping_sub(b.iter().fold(0u8,|sum,x|sum.wrapping_add(*x)));
        b
    }
    #[test]
    fn loads_dsdt_then_ssdt_and_unloads_in_reverse(){
        let dsdt=table(*b"DSDT",*b"BASE",1);
        let ssdt=table(*b"SSDT",*b"EXTR",2);
        let mut manager=AmlTableManager::new();let mut gate=gate();
        assert_eq!(manager.load_ssdt(&ssdt,&mut gate),Err(TableError::DsdTRequired));
        let base=manager.load_dsdt(&dsdt,&mut gate).unwrap();
        let extra=manager.load_ssdt(&ssdt,&mut gate).unwrap();
        assert_eq!(manager.table_count(),2);
        assert!(manager.dsdt_loaded());
        assert!(manager.namespace().find_child(0,*b"BASE").is_some());
        assert!(manager.namespace.find_child(0,*b"EXTR").is_some());
        assert_eq!(manager.unload(base,&mut gate),Err(TableError::UnloadOrder));
        manager.unload(extra,&mut gate).unwrap();
        manager.unload(base,&mut gate).unwrap();
        assert_eq!(manager.table_count(),0);
        assert!(!manager.dsdt_loaded());
    }
    #[test]
    fn rejects_bad_checksum_duplicate_and_wrong_signature(){
        let dsdt=table(*b"DSDT",*b"BASE",1);
        let mut bad=dsdt;bad[42]^=1;
        let mut manager=AmlTableManager::new();let mut gate=gate();
        assert!(matches!(manager.load_dsdt(&bad,&mut gate),Err(TableError::InvalidSdt(AcpiError::BadChecksum))));
        let base=manager.load_dsdt(&dsdt,&mut gate).unwrap();
        assert_eq!(manager.load_ssdt(&dsdt,&mut gate),Err(TableError::UnsupportedSignature));
        let duplicate=table(*b"SSDT",*b"BASE",1);
        let ssdt=table(*b"SSDT",*b"EXTR",2);
        manager.load_ssdt(&ssdt,&mut gate).unwrap();
        assert_eq!(manager.load_ssdt(&duplicate,&mut gate),Err(TableError::DuplicateTable));
        assert!(manager.unload(base,&mut gate).is_err());
    }
    #[test]
    fn failed_aml_parse_does_not_publish_a_table(){
        let mut bad=table(*b"DSDT",*b"BASE",1);
        bad[36]=0x08;bad[37]=b'1';
        bad[9]=0;bad[9]=0u8.wrapping_sub(bad.iter().fold(0u8,|s,x|s.wrapping_add(*x)));
        let mut manager=AmlTableManager::new();let mut gate=gate();
        assert!(manager.load_dsdt(&bad,&mut gate).is_err());
        assert_eq!(manager.table_count(),0);
        assert_eq!(manager.namespace().count,1);
    }
}
