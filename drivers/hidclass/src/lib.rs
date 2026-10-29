//! Bounded HID descriptor and keyboard top-level-collection discovery primitives.
#![no_std]
pub use cognacos_nt_abi as nt_abi;
#[cfg(test)] extern crate std;
#[derive(Clone,Copy,Debug,PartialEq,Eq)] pub enum HidError{Truncated,MalformedDescriptor,TooManyCollections,UnsupportedReportSize}
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)] pub struct Collection{pub usage_page:u16,pub usage:u16,pub collection_type:u8}
pub const MAX_COLLECTIONS:usize=16;
/// Parse short-item HID report descriptors and record top-level collections. Long items are
/// skipped only after validating their complete length. This parser does not interpret report fields.
pub fn top_level_collections(d:&[u8],out:&mut[Collection])->Result<usize,HidError>{
 let mut i=0usize;let mut page=0u16;let mut usage=0u16;let mut depth=0u32;let mut count=0usize;
 while i<d.len(){let prefix=d[i];i+=1;if prefix==0xfe{let n=*d.get(i).ok_or(HidError::Truncated)? as usize;i+=1;i=i.checked_add(1+n).filter(|e|*e<=d.len()).ok_or(HidError::Truncated)?;continue}
 let sz=match prefix&3{0=>0,1=>1,2=>2,_=>4};let end=i.checked_add(sz).filter(|e|*e<=d.len()).ok_or(HidError::Truncated)?;let v=match sz{0=>0,1=>d[i] as u32,2=>u16::from_le_bytes([d[i],d[i+1]]) as u32,4=>u32::from_le_bytes([d[i],d[i+1],d[i+2],d[i+3]]),_=>0};i=end;
 match (prefix>>2)&3{1=>match prefix&0xfc{0x04=>page=v as u16,_=>{}},2=>match prefix&0xfc{0x08=>usage=v as u16,_=>{}},0=>match prefix&0xfc{0xa0=>{depth+=1;if depth==1{if count>=out.len()||count>=MAX_COLLECTIONS{return Err(HidError::TooManyCollections)}out[count]=Collection{usage_page:page,usage,collection_type:v as u8};count+=1}},0xc0=>{if depth==0{return Err(HidError::MalformedDescriptor)}depth-=1},_=>{}},_=>{}}
 }
 if depth!=0{return Err(HidError::MalformedDescriptor)}Ok(count)
}
pub fn is_keyboard_collection(c:Collection)->bool{c.usage_page==0x01&&(c.usage==0x06||c.usage==0x07)&&c.collection_type==1}
#[cfg(test)]mod tests{use super::*;#[test]fn finds_keyboard_collection(){let d=[0x05,0x01,0x09,0x06,0xa1,0x01,0xc0];let mut o=[Collection::default();4];let n=top_level_collections(&d,&mut o).unwrap();assert_eq!(n,1);assert!(is_keyboard_collection(o[0]));}#[test]fn rejects_truncated_item(){let mut o=[Collection::default();1];assert_eq!(top_level_collections(&[0x05],&mut o),Err(HidError::Truncated));}}
