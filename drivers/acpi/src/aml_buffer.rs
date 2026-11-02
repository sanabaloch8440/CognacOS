//! Bounded extraction of AML Buffer initializers from namespace Name objects
//! and direct control-method Return(Buffer(...)) results. BufferSize TermArgs
//! are evaluated by the independent bounded scalar expression evaluator.
#![no_std]

use crate::aml::{AmlNamespace,AmlObjectType};

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum BufferError { InvalidNode, NotBuffer, Truncated, InvalidPackage, BufferTooLarge, InitializerTooLong }


/// Decode one Name(Buffer(...)) initializer into caller-owned storage.
/// The output is zero-filled to BufferSize as required by AML Buffer semantics.
pub fn decode_named_buffer(aml:&[u8],namespace:&AmlNamespace,node_index:u16,output:&mut [u8])->Result<usize,BufferError>{
    let node=namespace.nodes.get(node_index as usize).filter(|_|(node_index as usize)<namespace.count).ok_or(BufferError::InvalidNode)?;
    if node.object_type!=AmlObjectType::Name{return Err(BufferError::InvalidNode)}
    let start=node.body_start as usize;
    let finish=start.checked_add(node.body_length as usize).filter(|end|*end<=aml.len()).ok_or(BufferError::Truncated)?;
    if start>=finish||aml[start]!=0x11{return Err(BufferError::NotBuffer)}
    let length_pos=start+1;
    let lead=*aml.get(length_pos).ok_or(BufferError::Truncated)?;
    let follow=(lead>>6) as usize;
    if follow!=0&&lead&0x30!=0{return Err(BufferError::InvalidPackage)}
    let mut package_length=(if follow==0{lead&0x3f}else{lead&0x0f}) as usize;
    for index in 0..follow {
        let byte=*aml.get(length_pos+1+index).filter(|_|length_pos+1+index<finish).ok_or(BufferError::Truncated)?;
        package_length|=(byte as usize)<<(4+8*index);
    }
    let package_end=length_pos.checked_add(package_length).filter(|end|*end==finish).ok_or(BufferError::InvalidPackage)?;
    let mut cursor=length_pos+1+follow;
    if cursor>package_end{return Err(BufferError::InvalidPackage)}
    let (size,consumed)=crate::aml_exec::evaluate_term_argument(&aml[cursor..package_end])
        .map_err(|_|BufferError::InvalidPackage)?;
    cursor=cursor.checked_add(consumed).filter(|end|*end<=package_end).ok_or(BufferError::InvalidPackage)?;
    let declared=usize::try_from(size).map_err(|_|BufferError::BufferTooLarge)?;
    if declared>output.len(){return Err(BufferError::BufferTooLarge)}
    let initializer_length=package_end-cursor;
    if initializer_length>declared{return Err(BufferError::InitializerTooLong)}
    output[..declared].fill(0);
    output[..initializer_length].copy_from_slice(&aml[cursor..package_end]);
    Ok(declared)
}


/// Decode a method body whose complete result is Return(Buffer(...)).
/// The package length must cover the exact Buffer object. BufferSize is
/// evaluated as a bounded AML TermArg; the initializer is copied as opaque
/// bytes and zero-padded to the declared size.
pub fn decode_returned_buffer(method_body:&[u8],output:&mut [u8])->Result<usize,BufferError>{
    if method_body.get(0)!=Some(&0xa4)||method_body.get(1)!=Some(&0x11){
        return Err(BufferError::NotBuffer);
    }
    let (_,package_end)=crate::aml::decode_package_length(method_body,2,method_body.len())
        .map_err(|_|BufferError::InvalidPackage)?;
    if package_end!=method_body.len(){return Err(BufferError::InvalidPackage)}
    let lead=*method_body.get(2).ok_or(BufferError::Truncated)?;
    let length_bytes=1+((lead>>6) as usize);
    let mut cursor=2usize.checked_add(length_bytes).ok_or(BufferError::InvalidPackage)?;
    if cursor>package_end{return Err(BufferError::InvalidPackage)}
    let (size,consumed)=crate::aml_exec::evaluate_term_argument(&method_body[cursor..package_end])
        .map_err(|_|BufferError::InvalidPackage)?;
    cursor=cursor.checked_add(consumed).filter(|end|*end<=package_end).ok_or(BufferError::InvalidPackage)?;
    let declared=usize::try_from(size).map_err(|_|BufferError::BufferTooLarge)?;
    if declared>output.len(){return Err(BufferError::BufferTooLarge)}
    let initializer_length=package_end-cursor;
    if initializer_length>declared{return Err(BufferError::InitializerTooLong)}
    output[..declared].fill(0);
    output[..initializer_length].copy_from_slice(&method_body[cursor..package_end]);
    Ok(declared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aml::load_definition_block;
    #[test]
    fn extracts_zero_padded_crs_buffer_from_namespace_name(){
        // Name(_CRS, Buffer(4) { 0x79, 0x00 })
        let aml=[0x08,b'_',b'C',b'R',b'S',0x11,0x05,0x0a,4,0x79,0x00];
        let mut namespace=AmlNamespace::new();load_definition_block(&aml,&mut namespace).unwrap();
        let node=namespace.find_child(0,*b"_CRS").unwrap();let mut out=[0xa5;8];
        assert_eq!(decode_named_buffer(&aml,&namespace,node,&mut out),Ok(4));
        assert_eq!(&out[..4],&[0x79,0x00,0,0]);
        assert_eq!(out[4],0xa5);
    }
    #[test]
    fn evaluates_computed_buffer_size_in_named_object(){
        // Name(_CRS, Buffer(Add(ByteConst(2), ByteConst(2), NullName)) { 0x79 })
        let aml=[0x08,b'_',b'C',b'R',b'S',0x11,0x08,0x72,0x0a,2,0x0a,2,0x00,0x79];
        let mut namespace=AmlNamespace::new();load_definition_block(&aml,&mut namespace).unwrap();
        let node=namespace.find_child(0,*b"_CRS").unwrap();let mut output=[0xa5;8];
        assert_eq!(decode_named_buffer(&aml,&namespace,node,&mut output),Ok(4));
        assert_eq!(&output[..4],&[0x79,0,0,0]);
    }
    #[test]
    fn decodes_single_byte_package_lengths_above_nibble_range(){
        let mut aml=std::vec::Vec::from([0x08,b'_',b'C',b'R',b'S',0x11,0x17,0x0a,20]);
        aml.extend_from_slice(&[0x5a;20]);
        let mut namespace=AmlNamespace::new();load_definition_block(&aml,&mut namespace).unwrap();
        let node=namespace.find_child(0,*b"_CRS").unwrap();let mut output=[0u8;20];
        assert_eq!(decode_named_buffer(&aml,&namespace,node,&mut output),Ok(20));
        assert_eq!(output,[0x5a;20]);
    }

    #[test]
    fn decodes_buffer_returned_by_control_method(){
        // Return(Buffer(4) { 0x79, 0x00 }) with an exact enclosing package.
        let body=[0xa4,0x11,0x05,0x0a,4,0x79,0x00];
        let mut output=[0xa5;8];
        assert_eq!(decode_returned_buffer(&body,&mut output),Ok(4));
        assert_eq!(&output[..4],&[0x79,0x00,0,0]);
        assert_eq!(output[4],0xa5);
    }
    #[test]
    fn evaluates_computed_buffer_size_termarg(){
        // Return(Buffer(Add(ByteConst(2), ByteConst(2), NullName)) { 0x79 })
        let body=[0xa4,0x11,0x08,0x72,0x0a,2,0x0a,2,0x00,0x79];
        let mut output=[0xa5;8];
        assert_eq!(decode_returned_buffer(&body,&mut output),Ok(4));
        assert_eq!(&output[..4],&[0x79,0,0,0]);
    }
    #[test]
    fn rejects_malformed_or_oversized_method_buffer_results(){
        assert_eq!(decode_returned_buffer(&[0xa4,0x11,0x05,0x0a,8,1,2],&mut [0;4]),
            Err(BufferError::BufferTooLarge));
        assert_eq!(decode_returned_buffer(&[0xa4,0x11,0x04,0x0a,1,1,2],&mut [0;4]),
            Err(BufferError::InvalidPackage));
        assert_eq!(decode_returned_buffer(&[0xa4,0x11,0x05,0x0a,1,1,2],&mut [0;4]),
            Err(BufferError::InitializerTooLong));
    }
    #[test]
    fn rejects_non_buffer_and_oversized_buffer(){
        let aml=[0x08,b'_',b'C',b'R',b'S',0x0a,1];
        let mut namespace=AmlNamespace::new();load_definition_block(&aml,&mut namespace).unwrap();
        let node=namespace.find_child(0,*b"_CRS").unwrap();
        assert_eq!(decode_named_buffer(&aml,&namespace,node,&mut [0;8]),Err(BufferError::NotBuffer));
    }
    #[test]
    fn rejects_initializer_larger_than_declared_size(){
        let aml=[0x08,b'_',b'C',b'R',b'S',0x11,0x05,0x0a,1,0xaa,0xbb];
        let mut namespace=AmlNamespace::new();load_definition_block(&aml,&mut namespace).unwrap();
        let node=namespace.find_child(0,*b"_CRS").unwrap();
        assert_eq!(decode_named_buffer(&aml,&namespace,node,&mut [0;8]),Err(BufferError::InitializerTooLong));
    }

    #[test]
    fn evaluates_modulo_and_find_set_bit_buffer_sizes(){
        let modulo=[0xa4,0x11,0x08,0x85,0x0a,23,0x0a,5,0x00,0xaa];
        let mut small=[0xcc;8];
        assert_eq!(decode_returned_buffer(&modulo,&mut small),Ok(3));
        assert_eq!(&small[..3],&[0xaa,0,0]);

        let bit_scan=[0xa4,0x11,0x09,0x81,0x0c,0x00,0x80,0x00,0x00,0x00,0xaa];
        let mut large=[0xcc;16];
        assert_eq!(decode_returned_buffer(&bit_scan,&mut large),Ok(16));
        assert_eq!(large[0],0xaa);
        assert!(large[1..].iter().all(|byte|*byte==0));
    }

}
