//! Transactional AML namespace mutations used by executing AML terms.
//!
//! The parser's definition-block namespace remains immutable while a method is
//! executing. This bounded mutation layer is the single path for method-created
//! namespace objects (Name, Alias, and BufferField). It never performs firmware
//! I/O and never allocates from the global heap.
#![no_std]

use crate::aml::{AmlNamespace, AmlNode, AmlObjectType, AmlError, AmlName, MAX_NAMESPACE_NODES};
use crate::aml_objects::{AmlObjectArena, AmlValueKind};
use crate::aml_runtime::{AmlBindings, AmlReference, RuntimeError, StoreMode,
    make_object_reference, store_reference_with_mode, validate_runtime_value};

/// Runtime-created field descriptors are separate from OperationRegion Field
/// metadata: their backing store is an AML Buffer object, not a namespace node.
pub const MAX_DYNAMIC_BUFFER_FIELDS: usize = 128;
pub const DYNAMIC_OBJECT_OWNER: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AmlDynamicBufferField {
    pub namespace_node: u16,
    pub buffer_value: u16,
    pub bit_offset: u32,
    pub bit_width: u32,
    pub owner_table: u32,
    pub parent: u16,
    pub name: [u8; 4],
}

impl AmlDynamicBufferField {
    const EMPTY: Self = Self {
        namespace_node: u16::MAX, buffer_value: u16::MAX,
        bit_offset: 0, bit_width: 0, owner_table: 0, parent: 0, name: [0; 4],
    };
}

/// Fixed-capacity dynamic-object side table. A single instance belongs to one
/// AML namespace/runtime and must outlive every method that can reference it.
pub struct AmlDynamicNamespace {
    fields: [AmlDynamicBufferField; MAX_DYNAMIC_BUFFER_FIELDS],
    field_count: usize,
    next_slot: usize,
}

impl AmlDynamicNamespace {
    pub const fn new() -> Self {
        Self { fields: [AmlDynamicBufferField::EMPTY; MAX_DYNAMIC_BUFFER_FIELDS], field_count: 0, next_slot: 0 }
    }

    /// High-water mark of allocated field slots. IDs are never recycled after
    /// unload, preventing stale ObjectReference values from resolving to a new field.
    pub const fn field_count(&self) -> usize { self.field_count }

    /// Retire every dynamic field owned by an unloading DefinitionBlock.
    /// Slots become tombstones and are deliberately not reused during this
    /// namespace lifetime, so outstanding references fail closed.
    pub fn remove_owner_table(&mut self, owner_table: u32) -> usize {
        let mut removed = 0;
        for field in self.fields.iter_mut().take(self.next_slot) {
            if field.namespace_node != u16::MAX && field.owner_table == owner_table {
                *field = AmlDynamicBufferField::EMPTY;
                self.field_count -= 1;
                removed += 1;
            }
        }
        removed
    }

    fn validate_parent(namespace: &AmlNamespace, parent: u16) -> Result<(), AmlError> {
        let node = namespace.nodes.get(parent as usize)
            .filter(|_| (parent as usize) < namespace.count)
            .ok_or(AmlError::InvalidParent)?;
        if !matches!(node.object_type, AmlObjectType::Root | AmlObjectType::Scope
            | AmlObjectType::Device | AmlObjectType::Processor
            | AmlObjectType::PowerResource | AmlObjectType::ThermalZone) {
            return Err(AmlError::InvalidParent);
        }
        Ok(())
    }

    fn insert(namespace: &mut AmlNamespace, parent: u16, name: [u8; 4],
        kind: AmlObjectType, owner_table: u32) -> Result<u16, AmlError> {
        Self::validate_parent(namespace, parent)?;
        if namespace.find_child(parent, name).is_some() { return Err(AmlError::DuplicateName); }
        if namespace.count >= MAX_NAMESPACE_NODES {
            return Err(AmlError::NamespaceCapacity {
                capacity: MAX_NAMESPACE_NODES, requested: namespace.count.saturating_add(1),
            });
        }
        let index = namespace.count;
        let mut node = AmlNode::ROOT;
        node.parent = parent;
        node.name = name;
        node.object_type = kind;
        node.source_table = owner_table;
        namespace.nodes[index] = node;
        namespace.count += 1;
        Ok(index as u16)
    }

    /// Create a method-time Name object and bind its initial value. The caller
    /// supplies the current DefinitionBlock owner so table teardown can account
    /// for dynamic descendants consistently with statically loaded objects.
    pub fn create_name(&mut self, namespace: &mut AmlNamespace, bindings: &mut AmlBindings,
        arena: &AmlObjectArena, parent: u16, name: [u8; 4], value: u16,
        owner_table: u32) -> Result<u16, RuntimeError> {
        let object = arena.get(value).ok_or(RuntimeError::InvalidReference)?;
        validate_runtime_value(object, arena)?;
        let node = Self::insert(namespace, parent, name, AmlObjectType::Name, owner_table)
            .map_err(|_| RuntimeError::InvalidNode)?;
        if let Err(error) = bindings.bind_checked(namespace, node, value, arena) {
            namespace.count -= 1;
            namespace.nodes[namespace.count] = AmlNode::ROOT;
            return Err(error);
        }
        Ok(node)
    }

    /// Create an Alias to the canonical target. Alias chains are flattened at
    /// creation, so a later target alias change cannot introduce a cycle.
    pub fn create_alias(&mut self, namespace: &mut AmlNamespace, parent: u16,
        name: [u8; 4], target: u16, owner_table: u32) -> Result<u16, AmlError> {
        let canonical = namespace.resolve_alias(target)?;
        if canonical as usize >= namespace.count { return Err(AmlError::InvalidParent); }
        let node = Self::insert(namespace, parent, name, AmlObjectType::Alias, owner_table)?;
        namespace.nodes[node as usize].alias_target = canonical;
        Ok(node)
    }

    /// Implement CreateBit/Byte/Word/DWord/QWordField and CreateField's shared
    /// namespace mutation. Width and offset are in bits, matching AML semantics.
    /// The backing Buffer extent is checked before publishing the namespace node.
    pub fn create_buffer_field(&mut self, namespace: &mut AmlNamespace,
        arena: &AmlObjectArena, parent: u16, name: [u8; 4], buffer_value: u16,
        bit_offset: u32, bit_width: u32, owner_table: u32) -> Result<u16, RuntimeError> {
        let buffer = arena.get(buffer_value).ok_or(RuntimeError::InvalidReference)?;
        if buffer.kind != AmlValueKind::Buffer || buffer.byte_length as usize > buffer.bytes.len()
            || bit_width == 0 {
            return Err(RuntimeError::TypeMismatch);
        }
        if bit_offset > u16::MAX as u32 || bit_width > u16::MAX as u32 {
            return Err(RuntimeError::ValueTooLarge);
        }
        let end = bit_offset.checked_add(bit_width).ok_or(RuntimeError::ValueTooLarge)?;
        let extent = (buffer.byte_length as u32).checked_mul(8).ok_or(RuntimeError::ValueTooLarge)?;
        if end > extent { return Err(RuntimeError::InvalidReference); }
        if self.next_slot >= self.fields.len() { return Err(RuntimeError::ValueTooLarge); }
        let node = Self::insert(namespace, parent, name, AmlObjectType::Field, owner_table)
            .map_err(|_| RuntimeError::InvalidNode)?;
        self.fields[self.next_slot] = AmlDynamicBufferField {
            namespace_node: node, buffer_value, bit_offset, bit_width, owner_table, parent, name,
        };
        self.next_slot += 1;
        self.field_count += 1;
        Ok(node)
    }

    /// Resolve the containing scope for a Create*Field NameString. The last
    /// NameSeg is the field name; all preceding segments identify its parent.
    fn resolve_field_parent(namespace: &AmlNamespace, current: u16, name: &AmlName)
        -> Result<(u16, [u8; 4]), RuntimeError> {
        if name.count == 0 { return Err(RuntimeError::InvalidNode); }
        let mut candidate = if name.rooted { 0 } else { current };
        for _ in 0..name.parents {
            if candidate == 0 || candidate as usize >= namespace.count {
                return Err(RuntimeError::InvalidNode);
            }
            candidate = namespace.nodes[candidate as usize].parent;
        }
        let explicit = name.rooted || name.parents != 0;
        loop {
            let first = namespace.find_child(candidate, name.segments[0]);
            if name.count == 1 {
                if first.is_some() { return Err(RuntimeError::InvalidNode); }
                return Ok((candidate, name.segments[0]));
            }
            if let Some(mut parent) = first {
                for segment in name.segments.iter().take(name.count as usize - 1).skip(1) {
                    parent = namespace.find_child(parent, *segment).ok_or(RuntimeError::InvalidNode)?;
                }
                return Ok((parent, name.segments[name.count as usize - 1]));
            }
            if explicit || candidate == 0 { return Err(RuntimeError::InvalidNode); }
            candidate = namespace.nodes.get(candidate as usize).ok_or(RuntimeError::InvalidNode)?.parent;
        }
    }

    /// Runtime Create*Field path: keep dynamically-created names in the
    /// bounded side table so static firmware namespace storage remains stable.
    /// Synthetic node IDs occupy a disjoint range above MAX_NAMESPACE_NODES.
    pub fn create_buffer_field_at(&mut self, namespace: &AmlNamespace, current: u16,
        name: &AmlName, arena: &AmlObjectArena, buffer_value: u16,
        bit_offset: u32, bit_width: u32, owner_table: u32) -> Result<u16, RuntimeError> {
        let (parent, leaf) = Self::resolve_field_parent(namespace, current, name)?;
        if namespace.find_child(parent, leaf).is_some()
            || self.fields[..self.next_slot].iter().any(|field| field.parent == parent && field.name == leaf) {
            return Err(RuntimeError::InvalidNode);
        }
        let buffer = arena.get(buffer_value).ok_or(RuntimeError::InvalidReference)?;
        if buffer.kind != AmlValueKind::Buffer || buffer.byte_length as usize > buffer.bytes.len()
            || bit_width == 0 { return Err(RuntimeError::TypeMismatch); }
        if bit_offset > u16::MAX as u32 || bit_width > u16::MAX as u32 {
            return Err(RuntimeError::ValueTooLarge);
        }
        let end = bit_offset.checked_add(bit_width).ok_or(RuntimeError::ValueTooLarge)?;
        let extent = (buffer.byte_length as u32).checked_mul(8).ok_or(RuntimeError::ValueTooLarge)?;
        if end > extent { return Err(RuntimeError::InvalidReference); }
        if self.next_slot >= self.fields.len() { return Err(RuntimeError::ValueTooLarge); }
        let synthetic = MAX_NAMESPACE_NODES.checked_add(self.next_slot)
            .filter(|id| *id <= u16::MAX as usize).ok_or(RuntimeError::ValueTooLarge)? as u16;
        self.fields[self.next_slot] = AmlDynamicBufferField {
            namespace_node: synthetic, buffer_value, bit_offset, bit_width, owner_table, parent, name: leaf,
        };
        self.next_slot += 1;
        self.field_count += 1;
        Ok(synthetic)
    }

    /// Find a dynamic BufferField by its AML NameString and lexical scope.
    pub fn resolve_buffer_field(&self, namespace: &AmlNamespace, current: u16,
        name: &AmlName) -> Result<u16, RuntimeError> {
        let (parent, leaf) = Self::resolve_field_parent(namespace, current, name)?;
        self.fields[..self.next_slot].iter()
            .find(|field| field.namespace_node != u16::MAX && field.parent == parent && field.name == leaf)
            .map(|field| field.namespace_node).ok_or(RuntimeError::InvalidNode)
    }

    pub fn buffer_field(&self, namespace: &AmlNamespace, node: u16)
        -> Result<AmlDynamicBufferField, RuntimeError> {
        if (node as usize) < MAX_NAMESPACE_NODES && node as usize >= namespace.count { return Err(RuntimeError::InvalidNode); }
        self.fields[..self.next_slot].iter().find(|field| field.namespace_node != u16::MAX && field.namespace_node == node)
            .copied().ok_or(RuntimeError::InvalidReference)
    }

    /// AML Store/CopyObject against an existing namespace Name uses the same
    /// typed conversion and ownership rules as ordinary interpreter targets.
    pub fn store_name(&mut self, namespace: &AmlNamespace, bindings: &mut AmlBindings,
        arena: &mut AmlObjectArena, node: u16, value: u16, mode: StoreMode)
        -> Result<(), RuntimeError> {
        let reference = AmlReference::NamespaceNode(node);
        store_reference_with_mode(reference, value, mode, bindings, namespace, arena)
    }

    pub fn field_reference(&self, namespace: &AmlNamespace, node: u16)
        -> Result<crate::aml_runtime::AmlReference, RuntimeError> {
        let field = self.buffer_field(namespace, node)?;
        Ok(crate::aml_runtime::AmlReference::BufferField {
            buffer: field.buffer_value,
            bit_offset: u16::try_from(field.bit_offset).map_err(|_| RuntimeError::ValueTooLarge)?,
            bit_width: u16::try_from(field.bit_width).map_err(|_| RuntimeError::ValueTooLarge)?,
        })
    }

    /// Device/Name references used by AML ReferenceOf and dynamic Alias users.
    pub fn object_reference(node: u16) -> AmlRuntimeReference {
        AmlRuntimeReference(make_object_reference(AmlReference::NamespaceNode(node)))
    }
}

impl Default for AmlDynamicNamespace { fn default() -> Self { Self::new() } }

/// Small wrapper keeps the exported helper's return type explicit without
/// exposing mutable namespace internals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AmlRuntimeReference(pub crate::aml_objects::AmlRuntimeValue);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aml_objects::{AmlObjectArena, AmlRuntimeValue};

    fn integer(arena: &mut AmlObjectArena, value: u64) -> u16 {
        let mut object = AmlRuntimeValue::EMPTY;
        object.kind = AmlValueKind::Integer;
        object.integer = value;
        arena.allocate(object).unwrap()
    }

    #[test]
    fn creates_name_and_alias_with_canonical_target() {
        let mut namespace = AmlNamespace::new();
        let mut dynamic = AmlDynamicNamespace::new();
        let mut bindings = AmlBindings::new();
        let mut arena = AmlObjectArena::new();
        let value = integer(&mut arena, 0x42);
        let name = dynamic.create_name(&mut namespace, &mut bindings, &arena,
            0, *b"TEST", value, 7).unwrap();
        let alias = dynamic.create_alias(&mut namespace, 0, *b"ALIA", name, 7).unwrap();
        assert_eq!(namespace.resolve_alias(alias), Ok(name));
        assert_eq!(namespace.nodes[name as usize].source_table, 7);
    }

    #[test]
    fn rejects_duplicate_or_non_container_parent_without_mutation() {
        let mut namespace = AmlNamespace::new();
        let mut dynamic = AmlDynamicNamespace::new();
        let mut bindings = AmlBindings::new();
        let mut arena = AmlObjectArena::new();
        let value = integer(&mut arena, 1);
        dynamic.create_name(&mut namespace, &mut bindings, &arena, 0,
            *b"TEST", value, 1).unwrap();
        let before = namespace.count;
        assert_eq!(dynamic.create_name(&mut namespace, &mut bindings, &arena, 0,
            *b"TEST", value, 1), Err(RuntimeError::InvalidNode));
        assert_eq!(namespace.count, before);
        assert_eq!(dynamic.create_name(&mut namespace, &mut bindings, &arena, 1,
            *b"FAIL", value, 1), Err(RuntimeError::InvalidNode));
        assert_eq!(namespace.count, before);
    }

    #[test]
    fn create_field_validates_buffer_extent_and_publishes_descriptor_atomically() {
        let mut namespace = AmlNamespace::new();
        let mut dynamic = AmlDynamicNamespace::new();
        let mut arena = AmlObjectArena::new();
        let mut buffer = AmlRuntimeValue::EMPTY;
        buffer.kind = AmlValueKind::Buffer;
        buffer.byte_length = 4;
        let buffer_id = arena.allocate(buffer).unwrap();
        let field = dynamic.create_buffer_field(&mut namespace, &arena, 0,
            *b"FLD0", buffer_id, 4, 12, 9).unwrap();
        let descriptor = dynamic.buffer_field(&namespace, field).unwrap();
        assert_eq!((descriptor.bit_offset, descriptor.bit_width), (4, 12));
        let before = namespace.count;
        assert_eq!(dynamic.create_buffer_field(&mut namespace, &arena, 0,
            *b"BAD0", buffer_id, 31, 2, 9), Err(RuntimeError::InvalidReference));
        assert_eq!(namespace.count, before);
    }

    #[test]
    fn dynamic_field_width_must_fit_runtime_reference_abi() {
        let mut namespace = AmlNamespace::new();
        let mut dynamic = AmlDynamicNamespace::new();
        let mut arena = AmlObjectArena::new();
        let mut buffer = AmlRuntimeValue::EMPTY;
        buffer.kind = AmlValueKind::Buffer;
        buffer.byte_length = 256;
        let id = arena.allocate(buffer).unwrap();
        let field = dynamic.create_buffer_field(&mut namespace, &arena, 0,
            *b"WIDE", id, 0, 65_536, 1);
        assert_eq!(field, Err(RuntimeError::ValueTooLarge));
        assert_eq!(namespace.count, 1);
    }
    #[test]
    fn unload_retires_owned_fields_without_recycling_reference_ids() {
        let mut namespace = AmlNamespace::new();
        let mut dynamic = AmlDynamicNamespace::new();
        let mut arena = AmlObjectArena::new();
        let mut buffer = AmlRuntimeValue::EMPTY;
        buffer.kind = AmlValueKind::Buffer;
        buffer.byte_length = 4;
        let id = arena.allocate(buffer).unwrap();
        let name = AmlName { segments: [*b"FLD0", [0;4], [0;4], [0;4], [0;4], [0;4], [0;4], [0;4]],
            count: 1, rooted: false, parents: 0 };
        let first = dynamic.create_buffer_field_at(&namespace, 0, &name, &arena,
            id, 0, 32, 17).unwrap();
        assert_eq!(dynamic.field_count(), 1);
        assert_eq!(dynamic.remove_owner_table(17), 1);
        assert_eq!(dynamic.field_count(), 0);
        assert_eq!(dynamic.field_reference(&namespace, first), Err(RuntimeError::InvalidReference));
        let second = dynamic.create_buffer_field_at(&namespace, 0, &name, &arena,
            id, 0, 32, 18).unwrap();
        assert_ne!(first, second);
        assert_eq!(dynamic.field_count(), 1);
    }

}
