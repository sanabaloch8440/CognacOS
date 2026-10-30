//! Read-side external extent-tree traversal.
use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeExtent { pub logical_start: u32, pub physical_start: u64, pub length: u32, pub unwritten: bool }

pub trait ExtentNodeReader {
    fn read_extent_node(&mut self, physical_block: u64, out: &mut [u8]) -> Result<(), Error>;
}
