//! Validated ACPI Generic Address Structure access owned by HAL.
//!
//! Only SystemMemory and SystemIO are implemented here. AML/ACPI callers pass
//! descriptors, never raw pointers. Memory apertures are mapped through the
//! kernel's uncached I/O-space mapper; port accesses use architecture HAL I/O.
use core::sync::atomic::{AtomicBool,AtomicU64,Ordering};

const PAGE_SIZE:u64=4096;
const MAX_GAS_MAPPINGS:usize=128;
const SPACE_SYSTEM_MEMORY:u8=0;
const SPACE_SYSTEM_IO:u8=1;
static MAP_LOCK:AtomicBool=AtomicBool::new(false);
static RMW_LOCK:AtomicBool=AtomicBool::new(false);
static MAPPED_PHYSICAL:[AtomicU64;MAX_GAS_MAPPINGS]=[const{AtomicU64::new(0)};MAX_GAS_MAPPINGS];
static MAPPED_VIRTUAL:[AtomicU64;MAX_GAS_MAPPINGS]=[const{AtomicU64::new(0)};MAX_GAS_MAPPINGS];

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum GasError{UnsupportedAddressSpace,InvalidAddress,InvalidWidth,InvalidAccessSize,Unaligned,CrossesPage,MappingFailure,MappingTableFull}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
struct Access{bytes:u8,field_width:u8,bit_offset:u8,mask:u64}
fn access(gas:crate::acpi::AcpiGas)->Result<Access,GasError>{
    if !matches!(gas.address_space_id,SPACE_SYSTEM_MEMORY|SPACE_SYSTEM_IO){return Err(GasError::UnsupportedAddressSpace)}
    if gas.address==0{return Err(GasError::InvalidAddress)}
    if gas.bit_width==0||gas.bit_width>64||gas.bit_offset>63
        ||gas.bit_offset as u16+gas.bit_width as u16>64{return Err(GasError::InvalidWidth)}
    let minimum=(gas.bit_offset as u16+gas.bit_width as u16+7)/8;
    let bytes=match gas.access_size{
        0=>if minimum<=1{1}else if minimum<=2{2}else if minimum<=4{4}else if minimum<=8{8}else{return Err(GasError::InvalidWidth)},
        1=>1,2=>2,3=>4,4=>8,_=>return Err(GasError::InvalidAccessSize),
    };
    if minimum>bytes as u16*1{return Err(GasError::InvalidWidth)}
    if gas.address%(bytes as u64)!=0{return Err(GasError::Unaligned)}
    if gas.address_space_id==SPACE_SYSTEM_IO&&(bytes==8||gas.address>u16::MAX as u64||gas.address.checked_add(bytes as u64).is_none_or(|end|end>0x1_0000)){return Err(GasError::InvalidAddress)}
    if gas.address_space_id==SPACE_SYSTEM_MEMORY&&gas.address%PAGE_SIZE+bytes as u64>PAGE_SIZE{return Err(GasError::CrossesPage)}
    let field_mask=if gas.bit_width==64{u64::MAX}else{(1u64<<gas.bit_width)-1};
    Ok(Access{bytes,field_width:gas.bit_width,bit_offset:gas.bit_offset,mask:field_mask<<gas.bit_offset})
}
fn lock_mapping_table(){while MAP_LOCK.compare_exchange_weak(false,true,Ordering::Acquire,Ordering::Relaxed).is_err(){core::hint::spin_loop();}}
struct MappingGuard;
impl Drop for MappingGuard{fn drop(&mut self){MAP_LOCK.store(false,Ordering::Release);}}
fn lock_rmw(){while RMW_LOCK.compare_exchange_weak(false,true,Ordering::Acquire,Ordering::Relaxed).is_err(){core::hint::spin_loop();}}
struct RmwGuard;
impl Drop for RmwGuard{fn drop(&mut self){RMW_LOCK.store(false,Ordering::Release);}}
fn memory_pointer(address:u64,bytes:u8)->Result<*mut u8,GasError>{
    let page=address&!(PAGE_SIZE-1);
    let offset=address-page;
    if offset+bytes as u64>PAGE_SIZE{return Err(GasError::CrossesPage)}
    if let Some(index)=MAPPED_PHYSICAL.iter().position(|p|p.load(Ordering::Acquire)==page+1){
        let base=MAPPED_VIRTUAL[index].load(Ordering::Acquire);
        if base==0{return Err(GasError::MappingFailure)}
        return Ok((base+offset) as *mut u8)
    }
    lock_mapping_table();let _guard=MappingGuard;
    if let Some(index)=MAPPED_PHYSICAL.iter().position(|p|p.load(Ordering::Acquire)==page+1){
        let base=MAPPED_VIRTUAL[index].load(Ordering::Acquire);
        return if base==0{Err(GasError::MappingFailure)}else{Ok((base+offset) as *mut u8)}
    }
    let index=MAPPED_PHYSICAL.iter().position(|p|p.load(Ordering::Relaxed)==0).ok_or(GasError::MappingTableFull)?;
    // map_io_space rejects firmware-described RAM and creates an uncached,
    // supervisor-only mapping. The physical page is retained for HAL lifetime.
    let base=crate::kernel_vm::map_io_space(page,PAGE_SIZE).map_err(|_|GasError::MappingFailure)?;
    MAPPED_VIRTUAL[index].store(base,Ordering::Relaxed);
    MAPPED_PHYSICAL[index].store(page+1,Ordering::Release);
    Ok((base+offset) as *mut u8)
}
unsafe fn read_raw(gas:crate::acpi::AcpiGas,a:Access)->Result<u64,GasError>{
    match gas.address_space_id{
        SPACE_SYSTEM_MEMORY=>{
            let p=memory_pointer(gas.address,a.bytes)?;
            Ok(unsafe{match a.bytes{
                1=>core::ptr::read_volatile(p) as u64,
                2=>core::ptr::read_volatile(p.cast::<u16>()) as u64,
                4=>core::ptr::read_volatile(p.cast::<u32>()) as u64,
                8=>core::ptr::read_volatile(p.cast::<u64>()),
                _=>return Err(GasError::InvalidWidth),
            }})
        }
        SPACE_SYSTEM_IO=>{
            if a.bytes==8{return Err(GasError::InvalidWidth)}
            if gas.address>u16::MAX as u64||gas.address.checked_add(a.bytes as u64).is_none_or(|end|end>0x1_0000){return Err(GasError::InvalidAddress)}
            let port=gas.address as u16;
            Ok(unsafe{match a.bytes{1=>read_port_u8(port) as u64,2=>read_port_u16(port) as u64,4=>read_port_u32(port) as u64,_=>return Err(GasError::InvalidWidth)}})
        }
        _=>Err(GasError::UnsupportedAddressSpace),
    }
}
unsafe fn write_raw(gas:crate::acpi::AcpiGas,a:Access,value:u64)->Result<(),GasError>{
    match gas.address_space_id{
        SPACE_SYSTEM_MEMORY=>{
            let p=memory_pointer(gas.address,a.bytes)?;
            unsafe{match a.bytes{
                1=>core::ptr::write_volatile(p,value as u8),
                2=>core::ptr::write_volatile(p.cast::<u16>(),value as u16),
                4=>core::ptr::write_volatile(p.cast::<u32>(),value as u32),
                8=>core::ptr::write_volatile(p.cast::<u64>(),value),
                _=>return Err(GasError::InvalidWidth),
            }}
            core::sync::atomic::fence(Ordering::SeqCst);
            Ok(())
        }
        SPACE_SYSTEM_IO=>{
            if gas.address>u16::MAX as u64||gas.address+a.bytes as u64>0x1_0000{return Err(GasError::InvalidAddress)}
            let port=gas.address as u16;
            unsafe{match a.bytes{1=>write_port_u8(port,value as u8),2=>write_port_u16(port,value as u16),4=>write_port_u32(port,value as u32),_=>return Err(GasError::InvalidWidth)}}
            Ok(())
        }
        _=>Err(GasError::UnsupportedAddressSpace),
    }
}
/// Read the normalized GAS field value (bit_offset is shifted to bit zero).
pub unsafe fn read(gas:crate::acpi::AcpiGas)->Result<u64,GasError>{
    let a=access(gas)?;
    let raw=unsafe{read_raw(gas,a)?};
    Ok((raw&a.mask)>>a.bit_offset)
}
/// Write a normalized GAS field value, preserving all bits outside the field.
pub unsafe fn write(gas:crate::acpi::AcpiGas,value:u64)->Result<(),GasError>{
    let a=access(gas)?;
    let field_limit=if a.field_width==64{u64::MAX}else{(1u64<<a.field_width)-1};
    if value&!field_limit!=0{return Err(GasError::InvalidWidth)}
    lock_rmw();let _guard=RmwGuard;
    let current=unsafe{read_raw(gas,a)?};
    let next=(current&!a.mask)|((value<<a.bit_offset)&a.mask);
    unsafe{write_raw(gas,a,next)}
}
/// Read and update a GAS field as one serialized operation for callers that
/// share a register. This lock does not replace device-specific synchronization.
pub unsafe fn update(gas:crate::acpi::AcpiGas,clear_mask:u64,set_mask:u64)->Result<u64,GasError>{
    let a=access(gas)?;
    let limit=if a.field_width==64{u64::MAX}else{(1u64<<a.field_width)-1};
    if (clear_mask|set_mask)&!limit!=0{return Err(GasError::InvalidWidth)}
    lock_rmw();let _guard=RmwGuard;
    let raw=unsafe{read_raw(gas,a)?};
    let field=(raw&a.mask)>>a.bit_offset;
    let next=((field&!clear_mask)|set_mask)&limit;
    let register=(raw&!a.mask)|((next<<a.bit_offset)&a.mask);
    unsafe{write_raw(gas,a,register)?}
    Ok(next)
}
/// Number of retained, uncached GAS pages. Intended for diagnostics/tests only.
pub fn mapped_page_count()->usize{MAPPED_PHYSICAL.iter().filter(|p|p.load(Ordering::Acquire)!=0).count()}

#[inline] unsafe fn read_port_u8(port:u16)->u8{
    let value:u8;unsafe{core::arch::asm!("in al, dx",in("dx")port,out("al")value,options(nomem,nostack,preserves_flags));}value
}
#[inline] unsafe fn write_port_u8(port:u16,value:u8){
    unsafe{core::arch::asm!("out dx, al",in("dx")port,in("al")value,options(nomem,nostack,preserves_flags));}
}
#[inline] unsafe fn read_port_u16(port:u16)->u16{crate::hal::read_port_u16(port)}
#[inline] unsafe fn write_port_u16(port:u16,value:u16){crate::hal::write_port_u16(port,value)}
#[inline] unsafe fn read_port_u32(port:u16)->u32{crate::hal::read_port_u32(port)}
#[inline] unsafe fn write_port_u32(port:u16,value:u32){crate::hal::write_port_u32(port,value)}

#[cfg(test)]
mod tests{
    use super::*;
    fn gas(space:u8,address:u64,width:u8,offset:u8,size:u8)->crate::acpi::AcpiGas{
        crate::acpi::AcpiGas{address_space_id:space,bit_width:width,bit_offset:offset,access_size:size,address}
    }
    #[test]fn validates_field_shapes(){
        assert_eq!(access(gas(0,0x1000,8,0,1)).unwrap().bytes,1);
        assert_eq!(access(gas(0,0x1000,16,0,2)).unwrap().bytes,2);
        assert_eq!(access(gas(0,0x1000,32,0,3)).unwrap().bytes,4);
        assert_eq!(access(gas(0,0x1000,64,0,4)).unwrap().bytes,8);
        assert_eq!(access(gas(0,0x1000,0,0,1)),Err(GasError::InvalidWidth));
        assert_eq!(access(gas(0,0x1000,9,0,1)),Err(GasError::InvalidWidth));
        assert_eq!(access(gas(0,0x1001,16,0,2)),Err(GasError::Unaligned));
        assert_eq!(access(gas(0,0x1000,8,0,5)),Err(GasError::InvalidAccessSize));
    }
    #[test]fn rejects_unknown_spaces_and_invalid_port_ranges(){
        assert_eq!(access(gas(0x7f,0x1000,8,0,1)),Err(GasError::UnsupportedAddressSpace));
        assert_eq!(access(gas(0,0,8,0,1)),Err(GasError::InvalidAddress));
        assert_eq!(access(gas(1,0x10000,8,0,1)),Err(GasError::InvalidAddress));
        assert_eq!(access(gas(1,u64::MAX,8,0,1)),Err(GasError::Unaligned));
        assert_eq!(access(gas(1,0xfffc,32,0,3)).map(|_|()),Ok(()));
        assert_eq!(access(gas(1,0xfffe,32,0,3)),Err(GasError::Unaligned));
    }
}
