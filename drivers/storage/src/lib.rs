//! RdotOS storage-stack primitives following the documented Windows class/port/miniport split.
//! This is an independent implementation, not Microsoft's proprietary driver source.
#![no_std]
#[cfg(test)] extern crate std;

pub const SECTOR_SIZE_512: u32 = 512;
pub const MAX_TRANSFER_BYTES: u32 = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageError { InvalidArgument, OutOfRange, Unsupported, Busy, Timeout, DeviceError, QueueFull, InvalidState, Overflow }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geometry { pub sector_count: u64, pub logical_sector_size: u32, pub physical_sector_size: u32 }
impl Geometry {
    pub fn validate(self) -> Result<Self, StorageError> {
        if self.sector_count == 0 || self.logical_sector_size < 512 || !self.logical_sector_size.is_power_of_two()
            || self.physical_sector_size < self.logical_sector_size || !self.physical_sector_size.is_power_of_two()
            || self.physical_sector_size % self.logical_sector_size != 0 { return Err(StorageError::InvalidArgument); }
        self.sector_count.checked_mul(self.logical_sector_size as u64).ok_or(StorageError::Overflow)?;
        Ok(self)
    }
    pub fn capacity_bytes(self) -> Result<u64, StorageError> {
        self.validate()?.sector_count.checked_mul(self.logical_sector_size as u64).ok_or(StorageError::Overflow)
    }
    pub fn validate_io(self, lba: u64, sectors: u32) -> Result<u32, StorageError> {
        self.validate()?;
        if sectors == 0 { return Err(StorageError::InvalidArgument); }
        let end = lba.checked_add(sectors as u64).ok_or(StorageError::Overflow)?;
        if end > self.sector_count { return Err(StorageError::OutOfRange); }
        let bytes = sectors.checked_mul(self.logical_sector_size).ok_or(StorageError::Overflow)?;
        if bytes > MAX_TRANSFER_BYTES { return Err(StorageError::InvalidArgument); }
        Ok(bytes)
    }
}

/// Portable request representation shared by the disk class and controller miniports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockOperation { Read, Write, Flush, Identify }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockRequest { pub operation: BlockOperation, pub lba: u64, pub sectors: u32, pub buffer_physical: u64, pub byte_length: u32, pub request_id: u64 }
impl BlockRequest {
    pub fn validate(self, g: Geometry) -> Result<Self, StorageError> {
        match self.operation {
            BlockOperation::Read | BlockOperation::Write => {
                let n = g.validate_io(self.lba, self.sectors)?;
                if self.buffer_physical == 0 || self.buffer_physical & 1 != 0 || self.byte_length != n { return Err(StorageError::InvalidArgument); }
            }
            BlockOperation::Flush | BlockOperation::Identify => {
                if self.sectors != 0 || self.byte_length != 0 { return Err(StorageError::InvalidArgument); }
            }
        }
        Ok(self)
    }
}

/// AHCI register offsets and bit definitions (AHCI 1.3.1 register model).
pub mod ahci {
    use super::StorageError;
    pub const CAP: usize = 0x00; pub const GHC: usize = 0x04; pub const IS: usize = 0x08;
    pub const PI: usize = 0x0c; pub const VS: usize = 0x10; pub const BOHC: usize = 0x28;
    pub const PORT_BASE: usize = 0x100; pub const PORT_STRIDE: usize = 0x80;
    pub const P_IS: usize = 0x10; pub const P_IE: usize = 0x14; pub const P_CMD: usize = 0x18;
    pub const P_TFD: usize = 0x20; pub const P_SIG: usize = 0x24; pub const P_SSTS: usize = 0x28;
    pub const P_SCTL: usize = 0x2c; pub const P_SERR: usize = 0x30; pub const P_SACT: usize = 0x34;
    pub const P_CI: usize = 0x38; pub const P_SNTF: usize = 0x3c; pub const P_FBS: usize = 0x40;
    pub const GHC_HR: u32 = 1; pub const GHC_IE: u32 = 1 << 1; pub const GHC_AE: u32 = 1 << 31;
    pub const CMD_ST: u32 = 1; pub const CMD_FRE: u32 = 1 << 4; pub const CMD_FR: u32 = 1 << 14; pub const CMD_CR: u32 = 1 << 15;
    pub const TFD_BSY: u32 = 1 << 7; pub const TFD_DRQ: u32 = 1 << 3;
    pub const SATA_SIG_ATA: u32 = 0x00000101; pub const SATA_SIG_ATAPI: u32 = 0xeb140101;
    pub const SATA_SIG_PM: u32 = 0x96690101; pub const SATA_SIG_SEMB: u32 = 0xc33c0101;
    #[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum PortDevice { None, Ata, Atapi, PortMultiplier, Semb, Unknown }
    pub fn classify_port(ssts: u32, signature: u32) -> PortDevice {
        let det = ssts & 0xf; let ipm = (ssts >> 8) & 0xf;
        if det != 3 || ipm != 1 { return PortDevice::None; }
        match signature { SATA_SIG_ATA => PortDevice::Ata, SATA_SIG_ATAPI => PortDevice::Atapi,
            SATA_SIG_PM => PortDevice::PortMultiplier, SATA_SIG_SEMB => PortDevice::Semb, _ => PortDevice::Unknown }
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum PortState { Stopped, Starting, Running, Stopping, Failed }
    /// Guarded state machine: stop command engine before freeing DMA structures.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct PortLifecycle { pub state: PortState }
    impl PortLifecycle {
        pub const fn new() -> Self { Self { state: PortState::Stopped } }
        pub fn begin_start(&mut self) -> Result<(), StorageError> { if self.state != PortState::Stopped { return Err(StorageError::InvalidState); } self.state=PortState::Starting; Ok(()) }
        pub fn started(&mut self) -> Result<(), StorageError> { if self.state != PortState::Starting { return Err(StorageError::InvalidState); } self.state=PortState::Running; Ok(()) }
        pub fn begin_stop(&mut self) -> Result<(), StorageError> { if self.state != PortState::Running { return Err(StorageError::InvalidState); } self.state=PortState::Stopping; Ok(()) }
        pub fn stopped(&mut self) -> Result<(), StorageError> { if self.state != PortState::Stopping { return Err(StorageError::InvalidState); } self.state=PortState::Stopped; Ok(()) }
        pub fn fail(&mut self) { self.state=PortState::Failed; }
    }
    pub fn port_register_offset(port: u8, register: usize) -> Result<usize, StorageError> {
        if port >= 32 || register >= PORT_STRIDE { return Err(StorageError::InvalidArgument); }
        PORT_BASE.checked_add((port as usize).checked_mul(PORT_STRIDE).ok_or(StorageError::Overflow)?)
            .and_then(|x| x.checked_add(register)).ok_or(StorageError::Overflow)
    }
}

/// AHCI Register Host-to-Device FIS for LBA48 DMA reads/writes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AhciRegisterFis {
    pub fis_type: u8, pub flags: u8, pub command: u8, pub feature_low: u8,
    pub lba0: u8, pub lba1: u8, pub lba2: u8, pub device: u8,
    pub lba3: u8, pub lba4: u8, pub lba5: u8, pub feature_high: u8,
    pub count_low: u8, pub count_high: u8, pub icc: u8, pub control: u8,
    pub reserved: [u8; 4],
}
pub fn build_ahci_rw_fis(req: BlockRequest, geometry: Geometry) -> Result<AhciRegisterFis, StorageError> {
    req.validate(geometry)?;
    if req.operation != BlockOperation::Read && req.operation != BlockOperation::Write { return Err(StorageError::Unsupported); }
    if req.lba >= (1u64 << 48) || req.sectors > u16::MAX as u32 { return Err(StorageError::OutOfRange); }
    Ok(AhciRegisterFis {
        fis_type: 0x27, flags: 0x80,
        command: if req.operation == BlockOperation::Read { 0x25 } else { 0x35 },
        device: 1 << 6,
        lba0: req.lba as u8, lba1: (req.lba >> 8) as u8, lba2: (req.lba >> 16) as u8,
        lba3: (req.lba >> 24) as u8, lba4: (req.lba >> 32) as u8, lba5: (req.lba >> 40) as u8,
        count_low: req.sectors as u8, count_high: (req.sectors >> 8) as u8,
        ..AhciRegisterFis::default()
    })
}

/// NVMe 1.x controller register map, queue entry layout and bounded queue bookkeeping.
pub mod nvme {
    use super::StorageError;
    pub const CAP: usize=0x00; pub const VS: usize=0x08; pub const INTMS: usize=0x0c;
    pub const INTMC: usize=0x10; pub const CC: usize=0x14; pub const CSTS: usize=0x1c;
    pub const AQA: usize=0x24; pub const ASQ: usize=0x28; pub const ACQ: usize=0x30;
    pub const CC_EN: u32=1; pub const CSTS_RDY: u32=1; pub const CSTS_CFS: u32=1<<1;
    pub const ADMIN_QUEUE_ENTRIES: u16=32; pub const MAX_IO_QUEUE_ENTRIES: u16=256;
    #[repr(C, align(16))]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct SubmissionEntry { pub opcode:u8, pub flags:u8, pub command_id:u16, pub nsid:u32, pub reserved:[u32;2], pub metadata:u64, pub data_ptr:[u64;2], pub cdw10:u32, pub cdw11:u32, pub cdw12:u32, pub cdw13:u32, pub cdw14:u32, pub cdw15:u32 }
    #[repr(C, align(16))]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct CompletionEntry { pub result:u32, pub reserved:u32, pub sq_head:u16, pub sq_id:u16, pub command_id:u16, pub status:u16 }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Queue { pub depth:u16, pub tail:u16, pub head:u16, pub phase:u8, pub outstanding:u16 }
    impl Queue {
        pub fn new(depth:u16)->Result<Self,StorageError>{ if depth<2 || !depth.is_power_of_two() || depth>MAX_IO_QUEUE_ENTRIES { return Err(StorageError::InvalidArgument); } Ok(Self{depth,tail:0,head:0,phase:1,outstanding:0}) }
        pub fn submit(&mut self)->Result<u16,StorageError>{ if self.outstanding>=self.depth-1{return Err(StorageError::QueueFull)} let id=self.tail; self.tail=(self.tail+1)&(self.depth-1); self.outstanding+=1; Ok(id) }
        pub fn complete(&mut self, status:u16)->Result<bool,StorageError>{ if self.outstanding==0{return Err(StorageError::InvalidState)} self.outstanding-=1; self.head=(self.head+1)&(self.depth-1); if status & 1 != 0 { return Err(StorageError::DeviceError); } Ok(true) }
    }
    /// Build an NVMe NVM Read/Write command. PRP list setup and DMA mapping remain caller-owned.
    pub fn build_rw_command(req: super::BlockRequest, geometry: super::Geometry, namespace_id: u32, command_id: u16) -> Result<SubmissionEntry, StorageError> {
        req.validate(geometry)?;
        if namespace_id == 0 || (req.operation != super::BlockOperation::Read && req.operation != super::BlockOperation::Write) {
            return Err(StorageError::InvalidArgument);
        }
        if req.buffer_physical & 0x3 != 0 { return Err(StorageError::InvalidArgument); }
        Ok(SubmissionEntry {
            opcode: if req.operation == super::BlockOperation::Read { 0x02 } else { 0x01 },
            command_id, nsid: namespace_id,
            data_ptr: [req.buffer_physical, 0],
            cdw10: req.lba as u32, cdw11: (req.lba >> 32) as u32,
            cdw12: req.sectors - 1,
            ..SubmissionEntry::default()
        })
    }
    pub fn doorbell_offset(queue_id:u16, completion:bool, dstrd:u8)->Result<usize,StorageError>{
        if queue_id>2047 || dstrd>7{return Err(StorageError::InvalidArgument)}
        let stride=4usize.checked_shl(dstrd as u32).ok_or(StorageError::Overflow)?;
        let index=(queue_id as usize).checked_mul(2).and_then(|n|n.checked_add(if completion{1}else{0})).ok_or(StorageError::Overflow)?;
        0x1000usize.checked_add(index.checked_mul(stride).ok_or(StorageError::Overflow)?).ok_or(StorageError::Overflow)
    }
}

/// Disk class helpers: validate SCSI READ/WRITE(10/16) CDBs before lower dispatch.
pub mod disk {
    use super::{BlockOperation, BlockRequest, Geometry, StorageError, MAX_TRANSFER_BYTES};
    pub const SCSI_READ10:u8=0x28; pub const SCSI_WRITE10:u8=0x2a;
    pub const SCSI_READ16:u8=0x88; pub const SCSI_WRITE16:u8=0x8a;
    #[derive(Clone,Copy,Debug,PartialEq,Eq)] pub struct ParsedCdb { pub operation:BlockOperation, pub lba:u64, pub sectors:u32 }
    pub fn parse_rw_cdb(cdb:&[u8], geometry:Geometry)->Result<ParsedCdb,StorageError>{
        let (op,lba,count)=match cdb.first().copied().ok_or(StorageError::InvalidArgument)? {
            SCSI_READ10|SCSI_WRITE10=>{if cdb.len()<10{return Err(StorageError::InvalidArgument)} (cdb[0],u32::from_be_bytes([cdb[2],cdb[3],cdb[4],cdb[5]]) as u64,u16::from_be_bytes([cdb[7],cdb[8]]) as u32)}
            SCSI_READ16|SCSI_WRITE16=>{if cdb.len()<16{return Err(StorageError::InvalidArgument)} (cdb[0],u64::from_be_bytes(cdb[2..10].try_into().map_err(|_|StorageError::InvalidArgument)?),u32::from_be_bytes(cdb[10..14].try_into().map_err(|_|StorageError::InvalidArgument)?))}
            _=>return Err(StorageError::Unsupported)
        };
        let sectors=if count==0 {return Err(StorageError::InvalidArgument)} else {count};
        let bytes=geometry.validate_io(lba,sectors)?;
        if bytes>MAX_TRANSFER_BYTES{return Err(StorageError::InvalidArgument)}
        Ok(ParsedCdb{operation:if op==SCSI_READ10||op==SCSI_READ16{BlockOperation::Read}else{BlockOperation::Write},lba,sectors})
    }
    pub fn request_from_cdb(cdb:&[u8], geometry:Geometry, physical_buffer:u64, request_id:u64)->Result<BlockRequest,StorageError>{
        let p=parse_rw_cdb(cdb,geometry)?; let bytes=geometry.validate_io(p.lba,p.sectors)?;
        BlockRequest{operation:p.operation,lba:p.lba,sectors:p.sectors,buffer_physical:physical_buffer,byte_length:bytes,request_id}.validate(geometry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn geometry_and_io_are_overflow_and_bounds_checked(){
        let g=Geometry{sector_count:1000,logical_sector_size:512,physical_sector_size:4096};
        assert_eq!(g.capacity_bytes(),Ok(512000)); assert_eq!(g.validate_io(999,1),Ok(512));
        assert_eq!(g.validate_io(999,2),Err(StorageError::OutOfRange));
        assert!(g.validate_io(u64::MAX,2).is_err()); assert!(g.validate_io(0,2049).is_err());
    }
    #[test] fn ahci_detects_only_active_sata_devices(){
        assert_eq!(ahci::classify_port(0x103,ahci::SATA_SIG_ATA),ahci::PortDevice::Ata);
        assert_eq!(ahci::classify_port(0x103,ahci::SATA_SIG_ATAPI),ahci::PortDevice::Atapi);
        assert_eq!(ahci::classify_port(0x101,ahci::SATA_SIG_ATA),ahci::PortDevice::None);
        assert!(ahci::port_register_offset(31,ahci::P_CI).is_ok()); assert!(ahci::port_register_offset(32,0).is_err());
    }
    #[test] fn ahci_lifecycle_requires_ordered_quiescence(){
        let mut p=ahci::PortLifecycle::new(); assert!(p.started().is_err());
        p.begin_start().unwrap(); p.started().unwrap(); p.begin_stop().unwrap(); p.stopped().unwrap();
        assert_eq!(p.state,ahci::PortState::Stopped);
    }
    #[test] fn ahci_and_nvme_encode_lba48_requests() {
        let g=Geometry{sector_count:0x20_0000_0000_0000,logical_sector_size:512,physical_sector_size:512};
        let req=BlockRequest{operation:BlockOperation::Read,lba:0x1234_5678_9abc,sectors:2,buffer_physical:0x2000,byte_length:1024,request_id:1};
        let fis=build_ahci_rw_fis(req,g).unwrap();
        assert_eq!(fis.command,0x25); assert_eq!([fis.lba0,fis.lba1,fis.lba2,fis.lba3,fis.lba4,fis.lba5],[0xbc,0x9a,0x78,0x56,0x34,0x12]);
        let cmd=nvme::build_rw_command(req,g,1,7).unwrap();
        assert_eq!(cmd.opcode,0x02); assert_eq!(cmd.command_id,7); assert_eq!(cmd.cdw10,0x5678_9abc); assert_eq!(cmd.cdw11,0x1234); assert_eq!(cmd.cdw12,1);
    }
    #[test] fn nvme_queue_tracks_outstanding_and_phase(){
        let mut q=nvme::Queue::new(4).unwrap(); assert_eq!(q.submit(),Ok(0)); assert_eq!(q.submit(),Ok(1));
        q.complete(0).unwrap(); q.complete(0).unwrap(); assert!(q.complete(0).is_err());
        assert_eq!(nvme::doorbell_offset(1,false,0),Ok(0x1008));
    }
    #[test] fn disk_cdb_maps_big_endian_lba_and_checks_end(){
        let g=Geometry{sector_count:0x10000,logical_sector_size:512,physical_sector_size:512};
        let c=[0x28,0,0,0,0,10,0,0,2,0];
        let p=disk::parse_rw_cdb(&c,g).unwrap(); assert_eq!(p.lba,10); assert_eq!(p.sectors,2); assert_eq!(p.operation,BlockOperation::Read);
        let mut bad=c; bad[4]=0xff; bad[5]=0xff; assert!(disk::parse_rw_cdb(&bad,g).is_err());
    }
}
