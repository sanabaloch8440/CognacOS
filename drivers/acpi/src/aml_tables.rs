//! AML-6 ACPI definition-table lifecycle.
//! DSDT/SSDT SDT validation, bounded namespace installation, and strict
//! reverse-order unload. Firmware addresses and table mapping remain owned by
//! the ACPI table-discovery layer; callers provide validated readable slices.
#![no_std]

use crate::{parse_sdt_header,AcpiError};
use crate::aml::{AmlError,AmlNamespace,load_definition_block,MAX_NAMESPACE_NODES};
use crate::aml_exec::{self,AmlValue,ExecError};
use crate::acpi_device::{self,AcpiDeviceIdentity,AcpiMethodInvoker,AcpiMethodValue,IdentityError};
use core::sync::atomic::{AtomicUsize,Ordering};

pub const MAX_AML_TABLES:usize=32;
const SDT_HEADER_SIZE:usize=36;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum TableError {
    InvalidSdt(AcpiError),
    UnsupportedSignature,
    DsdtAlreadyLoaded,
    DsdtRequired,
    DuplicateTable,
    TableLimit,
    NamespaceCapacity,
    Parse(AmlError),
    InvalidHandle,
    UnloadOrder,
    NamespaceChanged,
    ExecutionBusy,
    MethodExecution(ExecError),
    DeviceIdentity(IdentityError),
}
impl From<AcpiError> for TableError {fn from(e:AcpiError)->Self{Self::InvalidSdt(e)}}
impl From<AmlError> for TableError {fn from(e:AmlError)->Self{Self::Parse(e)}}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct TableHandle {slot:u8,generation:u32}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct LoadedTableSet {pub dsdt:TableHandle,pub ssdts:[Option<TableHandle>;MAX_AML_TABLES-1],pub ssdt_count:usize}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct AmlTableInfo {
    pub signature:[u8;4],
    pub revision:u8,
    pub checksum:u8,
    pub content_fingerprint:u64,
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
        signature:[0;4],revision:0,checksum:0,content_fingerprint:0,oem_id:[0;6],oem_table_id:[0;8],
        oem_revision:0,creator_id:0,creator_revision:0,length:0,
        namespace_start:0,namespace_end:0,
    }};
}
/// Platform/runtime contract: stop new AML method entry and wait for all
/// in-flight AML methods to leave before namespace mutation. Implementations
/// must be safe against concurrent method dispatch on every CPU.
pub trait AmlExecutionGate {
    fn enter_method(&self)->Result<(),()>;
    fn leave_method(&self);
    fn enter_quiescent(&self)->Result<(),()>;
    fn leave_quiescent(&self);
}

/// Bounded, fail-fast barrier shared by AML method dispatch and table updates.
/// It prevents new method entry while a DSDT/SSDT namespace change is active.
pub struct AmlExecutionBarrier {state:AtomicUsize,spin_limit:usize}
impl AmlExecutionBarrier {
    const QUIESCING:usize=1usize<<(usize::BITS-1);
    const ACTIVE_MASK:usize=Self::QUIESCING-1;
    pub const fn new(spin_limit:usize)->Self{Self{state:AtomicUsize::new(0),spin_limit}}
    pub fn active_methods(&self)->usize{self.state.load(Ordering::Acquire)&Self::ACTIVE_MASK}
    pub fn is_quiescing(&self)->bool{self.state.load(Ordering::Acquire)&Self::QUIESCING!=0}
}
impl AmlExecutionGate for AmlExecutionBarrier {
    fn enter_method(&self)->Result<(),()>{
        for _ in 0..self.spin_limit{
            let state=self.state.load(Ordering::Acquire);
            if state&Self::QUIESCING!=0||state&Self::ACTIVE_MASK==Self::ACTIVE_MASK{return Err(())}
            if self.state.compare_exchange_weak(state,state+1,Ordering::AcqRel,Ordering::Acquire).is_ok(){return Ok(())}
            core::hint::spin_loop();
        }
        Err(())
    }
    fn leave_method(&self){self.state.fetch_sub(1,Ordering::Release);}
    fn enter_quiescent(&self)->Result<(),()>{
        let mut acquired=false;
        for _ in 0..self.spin_limit{
            let state=self.state.load(Ordering::Acquire);
            if state&Self::QUIESCING!=0{return Err(())}
            if self.state.compare_exchange_weak(state,state|Self::QUIESCING,Ordering::AcqRel,Ordering::Acquire).is_ok(){acquired=true;break}
            core::hint::spin_loop();
        }
        if !acquired{return Err(())}
        for _ in 0..self.spin_limit{
            if self.active_methods()==0{return Ok(())}
            core::hint::spin_loop();
        }
        self.state.fetch_and(Self::ACTIVE_MASK,Ordering::Release);
        Err(())
    }
    fn leave_quiescent(&self){self.state.store(0,Ordering::Release);}
}

struct GatedDeviceInvoker<'a,'g,G: AmlExecutionGate,I:AcpiMethodInvoker>{
    inner:&'a mut I,gate:&'g G,
}
impl<G:AmlExecutionGate,I:AcpiMethodInvoker> AcpiMethodInvoker for GatedDeviceInvoker<'_,'_,G,I>{
    fn evaluate_value(&mut self,namespace:&AmlNamespace,aml:&[u8],method:u16)->Result<AcpiMethodValue,ExecError>{
        self.gate.enter_method().map_err(|_|ExecError::ExecutionBusy)?;
        let result=self.inner.evaluate_value(namespace,aml,method);
        self.gate.leave_method();
        result
    }
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
    /// # Safety
    /// The RSDP, root-table, FADT, DSDT, and SSDT physical ranges must be mapped
    /// readable for the duration of this call; the bootstrap implementation
    /// currently supports only addresses below 4 GiB.
    ///
    /// Safe, mapper-backed firmware discovery for ACPI 2.0+ systems. This
    /// path supports XSDT entries and firmware tables above 4 GiB when the
    /// platform mapper can map them; it never casts a physical address.
    pub fn load_from_mapper<M:crate::AcpiPhysicalMapper,G:AmlExecutionGate>(&mut self,mapper:&M,rsdp_address:u64,gate:&mut G)->Result<LoadedTableSet,TableError>{
        let fadt=crate::find_table_with_mapper(mapper,rsdp_address,*b"FACP",0)?;
        let fadt_info=crate::parse_fadt(fadt)?;
        let dsdt_address=fadt_info.effective_dsdt();
        if dsdt_address==0{return Err(TableError::InvalidSdt(AcpiError::AddressOutOfRange))}
        let dsdt_header=mapper.map_physical(dsdt_address,SDT_HEADER_SIZE)?;
        if dsdt_header.len()<SDT_HEADER_SIZE{return Err(TableError::InvalidSdt(AcpiError::Truncated))}
        let dsdt_length=u32::from_le_bytes(dsdt_header.get(4..8).ok_or(AcpiError::Truncated)?.try_into().map_err(|_|AcpiError::Truncated)?) as usize;
        if !(SDT_HEADER_SIZE..=1024*1024).contains(&dsdt_length){return Err(TableError::InvalidSdt(AcpiError::BadLength))}
        let dsdt=mapper.map_physical(dsdt_address,dsdt_length)?;
        crate::validate_sdt(dsdt,Some(*b"DSDT"))?;
        let mut ssdts:[&[u8];MAX_AML_TABLES-1]=[&[];MAX_AML_TABLES-1];
        let mut count=0usize;
        for instance in 0..MAX_AML_TABLES{
            match crate::find_table_with_mapper(mapper,rsdp_address,*b"SSDT",instance){
                Ok(table)=>{if count==MAX_AML_TABLES-1{return Err(TableError::TableLimit)}ssdts[count]=table;count+=1;}
                Err(AcpiError::MissingRoot)=>break,
                Err(error)=>return Err(TableError::InvalidSdt(error)),
            }
        }
        self.load_firmware_tables(dsdt,&ssdts[..count],gate)
    }

    /// Discover the DSDT through FADT.X_DSDT/DSDT and SSDTs through the
    /// RSDT/XSDT, then install the complete set transactionally. This uses the
    /// current bootstrap identity-map contract and therefore rejects firmware
    /// table addresses outside the first 4 GiB. The caller must invoke this
    /// only while those physical ranges are mapped and readable.
    pub unsafe fn load_from_rsdp<G:AmlExecutionGate>(&mut self,rsdp_address:u64,gate:&G)->Result<LoadedTableSet,TableError>{
        const IDENTITY_LIMIT:u64=0x1_0000_0000;
        const MAX_TABLE_LENGTH:usize=1024*1024;
        let fadt=unsafe{crate::find_table(rsdp_address,*b"FACP",0)}?;
        let fadt_info=crate::parse_fadt(fadt)?;
        let dsdt_address=fadt_info.effective_dsdt();
        if dsdt_address==0||dsdt_address.checked_add(36).is_none_or(|end|end>IDENTITY_LIMIT){
            return Err(TableError::InvalidSdt(AcpiError::AddressOutOfRange))
        }
        let dsdt_header=unsafe{core::slice::from_raw_parts(dsdt_address as *const u8,SDT_HEADER_SIZE)};
        let dsdt_length=u32::from_le_bytes(dsdt_header[4..8].try_into().map_err(|_|AcpiError::Truncated)?) as usize;
        if !(SDT_HEADER_SIZE..=MAX_TABLE_LENGTH).contains(&dsdt_length)
            ||dsdt_address.checked_add(dsdt_length as u64).is_none_or(|end|end>IDENTITY_LIMIT){
            return Err(TableError::InvalidSdt(AcpiError::BadLength))
        }
        let dsdt=unsafe{core::slice::from_raw_parts(dsdt_address as *const u8,dsdt_length)};
        crate::validate_sdt(dsdt,Some(*b"DSDT"))?;
        let mut ssdts:[&[u8];MAX_AML_TABLES-1]=[&[];MAX_AML_TABLES-1];
        let mut ssdt_count=0usize;
        for instance in 0..=MAX_AML_TABLES-1{
            match unsafe{crate::find_table(rsdp_address,*b"SSDT",instance)}{
                Ok(table)=>{
                    if instance==MAX_AML_TABLES-1{return Err(TableError::TableLimit)}
                    ssdts[ssdt_count]=table;ssdt_count+=1;
                }
                Err(AcpiError::MissingRoot)=>break,
                Err(error)=>return Err(TableError::InvalidSdt(error)),
            }
        }
        self.load_firmware_tables(dsdt,&ssdts[..ssdt_count],gate)
    }
    pub fn namespace(&self)->&AmlNamespace{&self.namespace}
    pub fn table_count(&self)->usize{self.count}
    pub fn dsdt_loaded(&self)->bool{self.dsdt_loaded}
    pub fn table_info(&self,handle:TableHandle)->Result<AmlTableInfo,TableError>{
        let record=self.records.get(handle.slot as usize).filter(|r|r.active&&r.generation==handle.generation).ok_or(TableError::InvalidHandle)?;
        Ok(record.info)
    }
    /// Atomically install the boot DSDT and all supplied SSDTs. If any
    /// definition block fails, every table added by this call is rolled back.
    pub fn load_firmware_tables<G:AmlExecutionGate>(&mut self,dsdt:&[u8],ssdts:&[&[u8]],gate:&G)->Result<LoadedTableSet,TableError>{
        self.with_quiescence(gate,|manager|{
            if manager.count!=0||manager.dsdt_loaded{return Err(TableError::DsdtAlreadyLoaded)}
            if ssdts.len()>MAX_AML_TABLES-1{return Err(TableError::TableLimit)}
            let before=manager.count;
            let result=(||{
                let dsdt_handle=manager.load(dsdt,*b"DSDT")?;
                let mut handles=[None;MAX_AML_TABLES-1];
                for (index,table) in ssdts.iter().enumerate(){
                    handles[index]=Some(manager.load(table,*b"SSDT")?);
                }
                Ok(LoadedTableSet{dsdt:dsdt_handle,ssdts:handles,ssdt_count:ssdts.len()})
            })();
            if result.is_err(){
                while manager.count>before{
                    let slot=manager.count-1;
                    let record=manager.records[slot];
                    let handle=TableHandle{slot:slot as u8,generation:record.generation};
                    if manager.unload_quiesced(handle).is_err(){return Err(TableError::NamespaceChanged)}
                }
            }
            result
        })
    }
    pub fn load_dsdt<G:AmlExecutionGate>(&mut self,bytes:&[u8],gate:&G)->Result<TableHandle,TableError>{
        self.with_quiescence(gate,|manager|{
            if manager.dsdt_loaded||manager.count!=0{return Err(TableError::DsdtAlreadyLoaded)}
            manager.load(bytes,*b"DSDT")
        })
    }
    pub fn load_ssdt<G:AmlExecutionGate>(&mut self,bytes:&[u8],gate:&G)->Result<TableHandle,TableError>{
        self.with_quiescence(gate,|manager|{
            if !manager.dsdt_loaded{return Err(TableError::DsdtRequired)}
            manager.load(bytes,*b"SSDT")
        })
    }
    fn with_quiescence<T,G:AmlExecutionGate>(&mut self,gate:&G,operation:impl FnOnce(&mut Self)->Result<T,TableError>)->Result<T,TableError>{
        gate.enter_quiescent().map_err(|_|TableError::ExecutionBusy)?;
        let result=operation(self);
        gate.leave_quiescent();
        result
    }
    fn load(&mut self,bytes:&[u8],expected:[u8;4])->Result<TableHandle,TableError>{
        if self.count>=MAX_AML_TABLES{return Err(TableError::TableLimit)}
        let generation=self.next_generation;
        let next_generation=generation.checked_add(1).ok_or(TableError::TableLimit)?;
        let header=parse_sdt_header(bytes)?;
        if header.signature!=expected{return Err(TableError::UnsupportedSignature)}
        let length=header.length as usize;
        if length<SDT_HEADER_SIZE||length>bytes.len(){return Err(TableError::InvalidSdt(AcpiError::BadLength))}
        let info=AmlTableInfo{
            signature:header.signature,revision:header.revision,checksum:header.checksum,content_fingerprint:fingerprint(&bytes[..length]),oem_id:header.oem_id,
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
        for node in before..self.namespace.count{self.namespace.nodes[node].source_table=self.count as u8;}
        if self.namespace.count>u16::MAX as usize{
            self.namespace.count=before;
            return Err(TableError::NamespaceCapacity)
        }
        let mut stored=info;
        stored.namespace_end=self.namespace.count as u16;
        let slot=self.count;
        self.next_generation=next_generation;
        self.records[slot]=TableRecord{active:true,generation,info:stored};
        self.count+=1;
        if expected==*b"DSDT"{self.dsdt_loaded=true;}
        Ok(TableHandle{slot:slot as u8,generation})
    }
    /// Evaluate ACPI-1 identity objects for a device owned by this loaded table.
    /// AML method execution is routed through the manager's in-flight execution
    /// gate so unload and table replacement cannot race device identification.
    pub fn evaluate_device_identity<G:AmlExecutionGate,I:AcpiMethodInvoker>(&self,handle:TableHandle,table_bytes:&[&[u8]],device:u16,gate:&G,method_runtime:&mut I)->Result<AcpiDeviceIdentity,TableError>{
        let info=self.table_info(handle)?;
        if device<info.namespace_start||device>=info.namespace_end{return Err(TableError::InvalidHandle)}
        if table_bytes.len()<self.count{return Err(TableError::InvalidHandle)}
        let mut aml_tables:[&[u8];MAX_AML_TABLES]=[&[];MAX_AML_TABLES];
        for slot in 0..self.count{
            let bytes=table_bytes[slot];
            let record=self.records[slot];
            if !record.active{return Err(TableError::InvalidHandle)}
            let header=parse_sdt_header(bytes)?;
            if header.signature!=record.info.signature||header.length!=record.info.length
                ||header.checksum!=record.info.checksum||fingerprint(&bytes[..header.length as usize])!=record.info.content_fingerprint{
                return Err(TableError::InvalidHandle)
            }
            aml_tables[slot]=&bytes[SDT_HEADER_SIZE..header.length as usize];
        }
        let mut invoker=GatedDeviceInvoker{inner:method_runtime,gate};
        acpi_device::evaluate_device_identity(&self.namespace,&aml_tables[..self.count],device,&mut invoker)
            .map_err(|error|match error{
                IdentityError::Method(method)=>TableError::MethodExecution(method),
                other=>TableError::DeviceIdentity(other),
            })
    }
    /// Execute a method only when its namespace node belongs to this table.
    /// The caller supplies the same validated table bytes retained by the ACPI
    /// table owner; the manager verifies the table identity and exact extent.
    pub fn execute_method<G:AmlExecutionGate>(&self,handle:TableHandle,bytes:&[u8],method_index:u16,args:&[u64],gate:&G)->Result<AmlValue,TableError>{
        gate.enter_method().map_err(|_|TableError::ExecutionBusy)?;
        let result=self.execute_method_quiesced(handle,bytes,method_index,args);
        gate.leave_method();
        result
    }
    fn execute_method_quiesced(&self,handle:TableHandle,bytes:&[u8],method_index:u16,args:&[u64])->Result<AmlValue,TableError>{
        let info=self.table_info(handle)?;
        let header=parse_sdt_header(bytes)?;
        if !same_identity(info,AmlTableInfo{signature:header.signature,revision:header.revision,checksum:header.checksum,
            content_fingerprint:fingerprint(&bytes[..header.length as usize]),oem_id:header.oem_id,oem_table_id:header.oem_table_id,oem_revision:header.oem_revision,
            creator_id:header.creator_id,creator_revision:header.creator_revision,length:header.length,
            namespace_start:0,namespace_end:0})||header.length!=info.length
            ||info.checksum!=header.checksum||info.content_fingerprint!=fingerprint(&bytes[..header.length as usize]){
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
    /// SSDTs. The execution gate must stop new method entry and drain active calls.
    pub fn unload<G:AmlExecutionGate>(&mut self,handle:TableHandle,gate:&G)->Result<AmlTableInfo,TableError>{
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

fn fingerprint(bytes:&[u8])->u64{
    // Detect stale or substituted table buffers across method dispatch. This
    // is an integrity fingerprint, not a cryptographic authentication scheme.
    bytes.iter().fold(0xcbf29ce484222325u64,|h,b|(h^(*b as u64)).wrapping_mul(0x100000001b3))
}
fn same_identity(a:AmlTableInfo,b:AmlTableInfo)->bool{
    a.signature==b.signature&&a.oem_id==b.oem_id&&a.oem_table_id==b.oem_table_id
        &&a.revision==b.revision&&a.oem_revision==b.oem_revision&&a.creator_id==b.creator_id
        &&a.creator_revision==b.creator_revision
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestGate{busy:core::sync::atomic::AtomicBool,entered:core::sync::atomic::AtomicBool}
    impl AmlExecutionGate for TestGate{
        fn enter_method(&self)->Result<(),()>{if self.busy.load(Ordering::Acquire){Err(())}else{Ok(())}}
        fn leave_method(&self){}
        fn enter_quiescent(&self)->Result<(),()>{if self.busy.load(Ordering::Acquire){return Err(())}self.entered.store(true,Ordering::Release);Ok(())}
        fn leave_quiescent(&self){self.entered.store(false,Ordering::Release);}
    }
    fn gate()->TestGate{TestGate{busy:core::sync::atomic::AtomicBool::new(false),entered:core::sync::atomic::AtomicBool::new(false)}}
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
    fn method_table()->[u8;46]{
        let mut b=[0u8;46];
        b[0..4].copy_from_slice(b"DSDT");
        b[4..8].copy_from_slice(&(46u32).to_le_bytes());
        b[8]=2;b[10..16].copy_from_slice(b"COGNAC");
        b[16..24].copy_from_slice(b"AMLTEST ");
        b[24..28].copy_from_slice(&1u32.to_le_bytes());
        b[28..32].copy_from_slice(b"COGN");
        b[32..36].copy_from_slice(&1u32.to_le_bytes());
        b[36..46].copy_from_slice(&[0x14,0x09,b'T',b'E',b'S',b'T',0,0xa4,0x0a,7]);
        b[9]=0u8.wrapping_sub(b.iter().fold(0u8,|sum,x|sum.wrapping_add(*x)));
        b
    }
    fn identity_dsdt()->std::vec::Vec<u8>{
        let mut aml=std::vec::Vec::from([0x5b,0x82,0,b'D',b'E',b'V',b'0',
            0x08,b'_',b'H',b'I',b'D',0x0d,b'A',b'C',b'M',b'E',b'0',b'0',b'0',b'1',0,
            0x08,b'_',b'S',b'T',b'A',0x0a,0x0f]);
        aml[2]=(aml.len()-2) as u8;
        let length=36+aml.len();let mut table=std::vec![0u8;length];
        table[0..4].copy_from_slice(b"DSDT");table[4..8].copy_from_slice(&(length as u32).to_le_bytes());
        table[8]=2;table[10..16].copy_from_slice(b"COGNAC");table[16..24].copy_from_slice(b"AMLTEST ");
        table[24..28].copy_from_slice(&1u32.to_le_bytes());table[28..32].copy_from_slice(b"COGN");
        table[32..36].copy_from_slice(&1u32.to_le_bytes());table[36..].copy_from_slice(&aml);
        table[9]=0u8.wrapping_sub(table.iter().fold(0u8,|sum,b|sum.wrapping_add(*b)));table
    }
    #[test]
    fn table_manager_evaluates_identity_with_table_ownership_and_method_gate(){
        let dsdt=identity_dsdt();let mut manager=AmlTableManager::new();let gate=gate();
        let handle=manager.load_dsdt(&dsdt,&gate).unwrap();
        let device=manager.namespace().find_child(0,*b"DEV0").unwrap();
        let mut methods=crate::acpi_device::IntegerMethodInvoker;
        let identity=manager.evaluate_device_identity(handle,&[dsdt.as_slice()],device,&gate,&mut methods).unwrap();
        assert!(identity.has_hid);assert_eq!(identity.hid.as_bytes(),b"ACME0001");
        assert_eq!(identity.sta,crate::acpi_device::DEVICE_STATUS_DEFAULT);
    }

    #[test]
    fn firmware_table_set_load_is_transactional(){
        let dsdt=table(*b"DSDT",*b"BASE",1);
        let mut bad_ssdt=table(*b"SSDT",*b"FAIL",2);
        bad_ssdt[37]=b'1';bad_ssdt[9]=0;
        bad_ssdt[9]=0u8.wrapping_sub(bad_ssdt.iter().fold(0u8,|sum,x|sum.wrapping_add(*x)));
        let mut manager=AmlTableManager::new();let mut gate=gate();
        assert!(manager.load_firmware_tables(&dsdt,&[&bad_ssdt],&gate).is_err());
        assert_eq!(manager.table_count(),0);assert!(!manager.dsdt_loaded());
        assert_eq!(manager.namespace().count,1);
        let ssdt=table(*b"SSDT",*b"EXTR",3);
        let loaded=manager.load_firmware_tables(&dsdt,&[&ssdt],&gate).unwrap();
        assert_eq!(loaded.ssdt_count,1);assert_eq!(manager.table_count(),2);
    }

    #[test]
    fn method_dispatch_uses_owning_table_and_rejects_foreign_nodes(){
        let dsdt=method_table();let mut manager=AmlTableManager::new();let mut gate=gate();
        let handle=manager.load_dsdt(&dsdt,&gate).unwrap();
        let method=manager.namespace().find_child(0,*b"TEST").unwrap();
        assert_eq!(manager.execute_method(handle,&dsdt,method,&[],&gate),Ok(AmlValue::Integer(7)));
        assert_eq!(manager.execute_method(handle,&dsdt,0,&[],&gate),Err(TableError::InvalidHandle));
        let mut substituted=dsdt;substituted[45]=8;substituted[9]=0;
        substituted[9]=0u8.wrapping_sub(substituted.iter().fold(0u8,|sum,x|sum.wrapping_add(*x)));
        assert_eq!(manager.execute_method(handle,&substituted,method,&[],&gate),Err(TableError::InvalidHandle));
    }
    #[test]
    fn execution_barrier_drains_active_methods_and_blocks_new_entries(){
        let barrier=AmlExecutionBarrier::new(8);
        assert_eq!(barrier.enter_method(),Ok(()));
        assert_eq!(barrier.active_methods(),1);
        assert_eq!(barrier.enter_quiescent(),Err(()));
        assert!(!barrier.is_quiescing());
        barrier.leave_method();
        assert_eq!(barrier.enter_quiescent(),Ok(()));
        assert!(barrier.is_quiescing());
        assert_eq!(barrier.enter_method(),Err(()));
        barrier.leave_quiescent();
        assert!(!barrier.is_quiescing());
        assert_eq!(barrier.enter_method(),Ok(()));
        barrier.leave_method();
    }

    #[test]
    fn table_mutation_requires_execution_quiescence(){
        let dsdt=table(*b"DSDT",*b"BASE",1);let mut manager=AmlTableManager::new();
        let mut gate=TestGate{busy:core::sync::atomic::AtomicBool::new(true),entered:core::sync::atomic::AtomicBool::new(false)};
        assert_eq!(manager.load_dsdt(&dsdt,&gate),Err(TableError::ExecutionBusy));
        assert_eq!(manager.table_count(),0);assert!(!gate.entered.load(Ordering::Acquire));
        gate.busy.store(false,Ordering::Release);
        let handle=manager.load_dsdt(&dsdt,&gate).unwrap();
        assert!(!gate.entered.load(Ordering::Acquire));
        manager.unload(handle,&gate).unwrap();
        assert!(!gate.entered.load(Ordering::Acquire));
        let replacement=manager.load_dsdt(&dsdt,&gate).unwrap();
        assert_ne!(handle.generation,replacement.generation);
        assert_eq!(manager.table_info(handle),Err(TableError::InvalidHandle));
    }

    #[test]
    fn loads_dsdt_then_ssdt_and_unloads_in_reverse(){
        let dsdt=table(*b"DSDT",*b"BASE",1);
        let ssdt=table(*b"SSDT",*b"EXTR",2);
        let mut manager=AmlTableManager::new();let mut gate=gate();
        assert_eq!(manager.load_ssdt(&ssdt,&gate),Err(TableError::DsdtRequired));
        let base=manager.load_dsdt(&dsdt,&gate).unwrap();
        let extra=manager.load_ssdt(&ssdt,&gate).unwrap();
        assert_eq!(manager.table_count(),2);
        assert!(manager.dsdt_loaded());
        assert!(manager.namespace().find_child(0,*b"BASE").is_some());
        assert!(manager.namespace().find_child(0,*b"EXTR").is_some());
        assert_eq!(manager.unload(base,&gate),Err(TableError::UnloadOrder));
        manager.unload(extra,&gate).unwrap();
        manager.unload(base,&gate).unwrap();
        assert_eq!(manager.table_count(),0);
        assert!(!manager.dsdt_loaded());
    }
    #[test]
    fn rejects_bad_checksum_duplicate_and_wrong_signature(){
        let dsdt=table(*b"DSDT",*b"BASE",1);
        let mut bad=dsdt;bad[42]^=1;
        let mut manager=AmlTableManager::new();let mut gate=gate();
        assert!(matches!(manager.load_dsdt(&bad,&gate),Err(TableError::InvalidSdt(AcpiError::BadChecksum))));
        let base=manager.load_dsdt(&dsdt,&gate).unwrap();
        assert_eq!(manager.load_ssdt(&dsdt,&gate),Err(TableError::UnsupportedSignature));
        let duplicate=table(*b"SSDT",*b"BASE",1);
        let ssdt=table(*b"SSDT",*b"EXTR",2);
        manager.load_ssdt(&ssdt,&gate).unwrap();
        assert_eq!(manager.load_ssdt(&duplicate,&gate),Err(TableError::DuplicateTable));
        assert!(manager.unload(base,&gate).is_err());
    }
    #[test]
    fn failed_aml_parse_does_not_publish_a_table(){
        let mut bad=table(*b"DSDT",*b"BASE",1);
        bad[36]=0x08;bad[37]=b'1';
        bad[9]=0;bad[9]=0u8.wrapping_sub(bad.iter().fold(0u8,|s,x|s.wrapping_add(*x)));
        let mut manager=AmlTableManager::new();let mut gate=gate();
        assert!(manager.load_dsdt(&bad,&gate).is_err());
        assert_eq!(manager.table_count(),0);
        assert_eq!(manager.namespace().count,1);
    }
}
