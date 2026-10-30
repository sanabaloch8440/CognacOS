//! FAT32 filesystem driver for CognacOS, backed by the upstream rust-fatfs engine.
#![no_std]
#![deny(unsafe_code)]
#![allow(dead_code)]
pub use fatfs;
use fatfs::{FileSystem, FsOptions, FatType, Read, Seek, SeekFrom, Write};
use fatfs::{IoBase, IoError};
#[cfg(test)] extern crate std;

pub const MAX_SECTOR_SIZE: usize = 4096;
pub const MIN_SECTOR_SIZE: u32 = 512;
pub const FAT32_MIN_CLUSTERS: u32 = 65_525;
pub const FAT32_MAX_CLUSTERS: u32 = 0x0fff_fff5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiskError { InvalidGeometry, OutOfRange, WriteProtected, DeviceFailure, Overflow }
impl IoError for DiskError {
    fn is_interrupted(&self) -> bool { false }
    fn new_unexpected_eof_error() -> Self { Self::OutOfRange }
    fn new_write_zero_error() -> Self { Self::DeviceFailure }
}

/// Synchronous logical-block interface. Buffers must contain whole sectors.
pub trait BlockDevice {
    fn sector_size(&self) -> u32;
    fn sector_count(&self) -> u64;
    fn read_sectors(&mut self, lba: u64, output: &mut [u8]) -> Result<(), DiskError>;
    fn write_sectors(&mut self, lba: u64, input: &[u8]) -> Result<(), DiskError>;
    fn flush(&mut self) -> Result<(), DiskError>;
}

/// Byte-stream adapter restricted to one partition.
pub struct DiskStream<D: BlockDevice> {
    device: D, start_lba: u64, sectors: u64, position: u64,
    writable: bool, sector_size: usize,
}
impl<D: BlockDevice> DiskStream<D> {
    pub fn new(device: D, start_lba: u64, sectors: u64, writable: bool) -> Result<Self, DiskError> {
        let ss = device.sector_size();
        if !(512..=4096).contains(&ss) || !ss.is_power_of_two() || sectors == 0 {
            return Err(DiskError::InvalidGeometry);
        }
        if start_lba.checked_add(sectors).ok_or(DiskError::Overflow)? > device.sector_count() {
            return Err(DiskError::OutOfRange);
        }
        sectors.checked_mul(ss as u64).ok_or(DiskError::Overflow)?;
        Ok(Self { device, start_lba, sectors, position: 0, writable, sector_size: ss as usize })
    }
    pub fn capacity_bytes(&self) -> u64 { self.sectors * self.sector_size as u64 }
    pub fn is_writable(&self) -> bool { self.writable }
    pub fn into_device(self) -> D { self.device }
    fn read_sector(&mut self, lba: u64, out: &mut [u8]) -> Result<(), DiskError> {
        if lba >= self.sectors || out.len() != self.sector_size { return Err(DiskError::OutOfRange); }
        self.device.read_sectors(self.start_lba.checked_add(lba).ok_or(DiskError::Overflow)?, out)
    }
    fn write_sector(&mut self, lba: u64, input: &[u8]) -> Result<(), DiskError> {
        if !self.writable { return Err(DiskError::WriteProtected); }
        if lba >= self.sectors || input.len() != self.sector_size { return Err(DiskError::OutOfRange); }
        self.device.write_sectors(self.start_lba.checked_add(lba).ok_or(DiskError::Overflow)?, input)
    }
    fn read_current(&mut self, out: &mut [u8]) -> Result<usize, DiskError> {
        let cap = self.capacity_bytes();
        if self.position >= cap || out.is_empty() { return Ok(0); }
        let count = core::cmp::min(out.len() as u64, cap - self.position) as usize;
        let mut done = 0;
        let mut sector = [0u8; MAX_SECTOR_SIZE];
        while done < count {
            let at = self.position.checked_add(done as u64).ok_or(DiskError::Overflow)?;
            let lba = at / self.sector_size as u64;
            let off = (at % self.sector_size as u64) as usize;
            let n = core::cmp::min(self.sector_size - off, count - done);
            if let Err(error) = self.read_sector(lba, &mut sector[..self.sector_size]) {
                if done != 0 {
                    self.position = self.position.checked_add(done as u64).ok_or(DiskError::Overflow)?;
                    return Ok(done);
                }
                return Err(error);
            }
            out[done..done+n].copy_from_slice(&sector[off..off+n]);
            done += n;
        }
        self.position = self.position.checked_add(done as u64).ok_or(DiskError::Overflow)?;
        Ok(done)
    }
    fn write_current(&mut self, input: &[u8]) -> Result<usize, DiskError> {
        if !self.writable { return Err(DiskError::WriteProtected); }
        let cap = self.capacity_bytes();
        if self.position >= cap || input.is_empty() { return Ok(0); }
        let count = core::cmp::min(input.len() as u64, cap - self.position) as usize;
        let mut done = 0;
        let mut sector = [0u8; MAX_SECTOR_SIZE];
        while done < count {
            let at = self.position.checked_add(done as u64).ok_or(DiskError::Overflow)?;
            let lba = at / self.sector_size as u64;
            let off = (at % self.sector_size as u64) as usize;
            let n = core::cmp::min(self.sector_size - off, count - done);
            let result = if off == 0 && n == self.sector_size {
                self.write_sector(lba, &input[done..done+n])
            } else {
                self.read_sector(lba, &mut sector[..self.sector_size]).and_then(|()| {
                    sector[off..off+n].copy_from_slice(&input[done..done+n]);
                    self.write_sector(lba, &sector[..self.sector_size])
                })
            };
            if let Err(e) = result {
                if done != 0 {
                    self.position = self.position.checked_add(done as u64).ok_or(DiskError::Overflow)?;
                    return Ok(done);
                }
                return Err(e);
            }
            done += n;
        }
        self.position = self.position.checked_add(done as u64).ok_or(DiskError::Overflow)?;
        Ok(done)
    }
}
impl<D: BlockDevice> IoBase for DiskStream<D> { type Error = DiskError; }
impl<D: BlockDevice> Read for DiskStream<D> {
    fn read(&mut self, out: &mut [u8]) -> Result<usize, Self::Error> { self.read_current(out) }
}
impl<D: BlockDevice> Write for DiskStream<D> {
    fn write(&mut self, input: &[u8]) -> Result<usize, Self::Error> { self.write_current(input) }
    fn flush(&mut self) -> Result<(), Self::Error> { self.device.flush() }
}
impl<D: BlockDevice> Seek for DiskStream<D> {
    fn seek(&mut self, from: SeekFrom) -> Result<u64, Self::Error> {
        let cap = self.capacity_bytes();
        let next = match from {
            SeekFrom::Start(n) => n,
            SeekFrom::End(n) if n < 0 => cap.checked_sub(n.unsigned_abs()).ok_or(DiskError::OutOfRange)?,
            SeekFrom::End(n) => cap.checked_add(n as u64).ok_or(DiskError::Overflow)?,
            SeekFrom::Current(n) if n < 0 => self.position.checked_sub(n.unsigned_abs()).ok_or(DiskError::OutOfRange)?,
            SeekFrom::Current(n) => self.position.checked_add(n as u64).ok_or(DiskError::Overflow)?,
        };
        if next > cap { return Err(DiskError::OutOfRange); }
        self.position = next;
        Ok(next)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MountError { Disk(DiskError), InvalidBootSector, InvalidGeometry, NotFat32, FatFsCorrupt }

fn validate_fat32<D: BlockDevice>(stream: &mut DiskStream<D>) -> Result<(), MountError> {
    let mut boot = [0u8; MAX_SECTOR_SIZE];
    let ss = stream.sector_size;
    if stream.read(&mut boot[..ss]).map_err(MountError::Disk)? != ss { return Err(MountError::InvalidBootSector); }
    let u16at = |o: usize| u16::from_le_bytes([boot[o], boot[o+1]]);
    let u32at = |o: usize| u32::from_le_bytes([boot[o], boot[o+1], boot[o+2], boot[o+3]]);
    let bps = u16at(11) as u32; let spc = boot[13] as u32;
    let reserved = u16at(14) as u32; let fats = boot[16] as u32;
    let root_entries = u16at(17); let total16 = u16at(19) as u32; let fat16 = u16at(22);
    let total32 = u32at(32); let fat32 = u32at(36); let version = u16at(42);
    let root = u32at(44) & 0x0fff_ffff; let fsinfo = u16at(48) as u32; let backup = u16at(50) as u32;
    let total = if total32 != 0 { total32 } else { total16 };
    if bps != ss as u32 || spc == 0 || !spc.is_power_of_two() || reserved == 0
        || !(1..=2).contains(&fats) || root_entries != 0 || fat16 != 0 || fat32 == 0
        || version != 0 || total == 0 || total as u64 > stream.sectors
        || fsinfo == 0 || fsinfo >= reserved
        || (backup != 0 && backup != 0xffff && backup >= reserved)
        || u16at(510) != 0xaa55 {
        return Err(MountError::InvalidBootSector);
    }
    let first_data = reserved.checked_add(fats.checked_mul(fat32).ok_or(MountError::InvalidGeometry)?)
        .ok_or(MountError::InvalidGeometry)?;
    if first_data >= total { return Err(MountError::InvalidGeometry); }
    let clusters = (total - first_data) / spc;
    if !(FAT32_MIN_CLUSTERS..=FAT32_MAX_CLUSTERS).contains(&clusters)
        || root < 2 || root >= clusters.saturating_add(2)
        || (fat32 as u64).checked_mul(bps as u64).ok_or(MountError::InvalidGeometry)? / 4
            < clusters as u64 + 2 {
        return Err(MountError::InvalidGeometry);
    }
    stream.seek(SeekFrom::Start(0)).map_err(MountError::Disk)?;
    Ok(())
}

pub struct FatFilesystem<D: BlockDevice> { inner: FileSystem<DiskStream<D>>, writable: bool }
impl<D: BlockDevice> FatFilesystem<D> {
    pub fn mount_read_only(device: D, start_lba: u64, sectors: u64) -> Result<Self, MountError> {
        Self::mount_with_access(device, start_lba, sectors, false)
    }
    #[cfg(feature = "write-support")]
    pub fn mount_read_write(device: D, start_lba: u64, sectors: u64) -> Result<Self, MountError> {
        Self::mount_with_access(device, start_lba, sectors, true)
    }
    fn mount_with_access(device: D, start_lba: u64, sectors: u64, writable: bool) -> Result<Self, MountError> {
        let mut stream = DiskStream::new(device, start_lba, sectors, writable).map_err(MountError::Disk)?;
        validate_fat32(&mut stream)?;
        let fs = FileSystem::new(stream, FsOptions::new().strict(true)).map_err(|_| MountError::FatFsCorrupt)?;
        if fs.fat_type() != FatType::Fat32 { return Err(MountError::NotFat32); }
        Ok(Self { inner: fs, writable })
    }
    pub fn filesystem(&self) -> &FileSystem<DiskStream<D>> { &self.inner }
    pub fn is_writable(&self) -> bool { self.writable }
    pub fn flush(&self) -> Result<(), MountError> { self.inner.flush().map_err(|_| MountError::FatFsCorrupt) }
    pub fn unmount(self) -> Result<(), MountError> { self.inner.unmount().map_err(|_| MountError::FatFsCorrupt) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;
    use std::vec::Vec;
    struct MemoryDisk { bytes: Vec<u8>, sector_size: u32, flushes: usize }
    impl MemoryDisk { fn new(n: usize, ss: u32) -> Self { Self { bytes: vec![0; n*ss as usize], sector_size:ss, flushes:0 } } }
    impl BlockDevice for MemoryDisk {
        fn sector_size(&self)->u32 { self.sector_size }
        fn sector_count(&self)->u64 { (self.bytes.len()/self.sector_size as usize) as u64 }
        fn read_sectors(&mut self,lba:u64,out:&mut[u8])->Result<(),DiskError>{
            if out.len()%self.sector_size as usize!=0{return Err(DiskError::InvalidGeometry)}
            let s=(lba as usize).checked_mul(self.sector_size as usize).ok_or(DiskError::Overflow)?;
            let e=s.checked_add(out.len()).ok_or(DiskError::Overflow)?;
            out.copy_from_slice(self.bytes.get(s..e).ok_or(DiskError::OutOfRange)?);Ok(())
        }
        fn write_sectors(&mut self,lba:u64,input:&[u8])->Result<(),DiskError>{
            if input.len()%self.sector_size as usize!=0{return Err(DiskError::InvalidGeometry)}
            let s=(lba as usize).checked_mul(self.sector_size as usize).ok_or(DiskError::Overflow)?;
            let e=s.checked_add(input.len()).ok_or(DiskError::Overflow)?;
            self.bytes.get_mut(s..e).ok_or(DiskError::OutOfRange)?.copy_from_slice(input);Ok(())
        }
        fn flush(&mut self)->Result<(),DiskError>{self.flushes+=1;Ok(())}
    }
    #[test] fn partition_bounds_and_seek(){
        assert_eq!(DiskStream::new(MemoryDisk::new(8,512),7,2,false).err(),Some(DiskError::OutOfRange));
        let mut s=DiskStream::new(MemoryDisk::new(8,512),2,4,false).unwrap();
        assert_eq!(s.seek(SeekFrom::End(0)).unwrap(),2048);
        assert_eq!(s.seek(SeekFrom::Current(1)),Err(DiskError::OutOfRange));
    }
    #[test] fn unaligned_write_preserves_adjacent_bytes(){
        let mut s=DiskStream::new(MemoryDisk::new(4,512),1,2,true).unwrap();
        s.seek(SeekFrom::Start(7)).unwrap();assert_eq!(s.write(b"FAT32").unwrap(),5);
        s.seek(SeekFrom::Start(0)).unwrap();let mut b=[0;12];assert_eq!(s.read(&mut b).unwrap(),12);
        assert_eq!(&b[7..12],b"FAT32");assert!(b[..7].iter().all(|x|*x==0));
    }
    #[test] fn read_only_rejects_write(){
        let mut s=DiskStream::new(MemoryDisk::new(2,512),0,2,false).unwrap();
        assert_eq!(s.write(b"x"),Err(DiskError::WriteProtected));
    }
    #[test] fn rejects_non_fat32_boot_sector(){
        let mut s=DiskStream::new(MemoryDisk::new(128,512),0,128,false).unwrap();
        assert_eq!(validate_fat32(&mut s),Err(MountError::InvalidBootSector));
    }
}
