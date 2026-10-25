/*
 * Per-process default heap for the CognacOS native NTDLL bootstrap.
 *
 * This is a bounded in-image heap, not a replacement for the full Windows
 * heap manager. It supports the default process heap and basic allocate/free/
 * realloc/size/validate semantics. Custom heap creation, LFH, user callbacks,
 * and Windows heap instrumentation flags are not implemented.
 */
#include "cognac_ntdll.h"

#define HEAP_MAGIC 0x434F4748u
#define BLOCK_MAGIC 0x434F4742u
#define HEAP_ZERO_MEMORY 0x00000008u
#define PROCESS_HEAP_BYTES (256u * 1024u)

typedef struct _COGNAC_HEAP_BLOCK {
    SIZE_T size;
    struct _COGNAC_HEAP_BLOCK *previous;
    struct _COGNAC_HEAP_BLOCK *next;
    ULONG magic;
    ULONG free;
} COGNAC_HEAP_BLOCK;

typedef struct _COGNAC_HEAP {
    uint64_t magic;
    COGNAC_HEAP_BLOCK *first;
    volatile UCHAR lock;
} COGNAC_HEAP;

__declspec(dllexport) COGNAC_HEAP CognacNtdllProcessHeapObject;
static UCHAR process_heap_arena[PROCESS_HEAP_BYTES] __attribute__((aligned(16)));

static void heap_lock(COGNAC_HEAP *heap)
{
    while (__atomic_test_and_set(&heap->lock, __ATOMIC_ACQUIRE))
        __asm__ volatile ("pause");
}

static void heap_unlock(COGNAC_HEAP *heap)
{
    __atomic_clear(&heap->lock, __ATOMIC_RELEASE);
}

static ULONG_PTR arena_start(void)
{
    return (ULONG_PTR)&process_heap_arena[0];
}

static ULONG_PTR arena_end(void)
{
    return (ULONG_PTR)&process_heap_arena[sizeof(process_heap_arena)];
}

static int block_address_valid(const COGNAC_HEAP_BLOCK *block)
{
    ULONG_PTR address = (ULONG_PTR)block;
    return address >= arena_start() &&
           address <= arena_end() - sizeof(COGNAC_HEAP_BLOCK);
}

static void heap_initialize_locked(COGNAC_HEAP *heap)
{
    COGNAC_HEAP_BLOCK *first;
    if (heap->magic == HEAP_MAGIC) return;
    first = (COGNAC_HEAP_BLOCK *)process_heap_arena;
    first->size = sizeof(process_heap_arena) - sizeof(*first);
    first->previous = NULL;
    first->next = NULL;
    first->magic = BLOCK_MAGIC;
    first->free = 1;
    heap->first = first;
    heap->magic = HEAP_MAGIC;
}

static COGNAC_HEAP_BLOCK *find_block_locked(PVOID address)
{
    ULONG_PTR pointer = (ULONG_PTR)address;
    COGNAC_HEAP_BLOCK *block;
    if (pointer < arena_start() + sizeof(COGNAC_HEAP_BLOCK) ||
        pointer >= arena_end()) return NULL;
    block = (COGNAC_HEAP_BLOCK *)(pointer - sizeof(COGNAC_HEAP_BLOCK));
    if (!block_address_valid(block) || block->magic != BLOCK_MAGIC ||
        block->size > arena_end() - ((ULONG_PTR)block + sizeof(*block)))
        return NULL;
    return block;
}

static void merge_next(COGNAC_HEAP_BLOCK *block)
{
    COGNAC_HEAP_BLOCK *next = block->next;
    if (!next || !block_address_valid(next) || next->magic != BLOCK_MAGIC || !next->free)
        return;
    block->size += sizeof(*next) + next->size;
    block->next = next->next;
    if (block->next) block->next->previous = block;
    next->magic = 0;
    next->previous = NULL;
    next->next = NULL;
}

__declspec(dllexport)
HANDLE WINAPI RtlGetProcessHeap(void)
{
    COGNAC_HEAP *heap = &CognacNtdllProcessHeapObject;
    heap_lock(heap);
    heap_initialize_locked(heap);
    heap_unlock(heap);
    return (HANDLE)heap;
}

__declspec(dllexport)
PVOID WINAPI RtlAllocateHeap(HANDLE handle, ULONG flags, SIZE_T requested_size)
{
    COGNAC_HEAP *heap = &CognacNtdllProcessHeapObject;
    COGNAC_HEAP_BLOCK *block;
    SIZE_T size = requested_size ? requested_size : 1;
    SIZE_T index;
    if (handle != (HANDLE)heap || size > sizeof(process_heap_arena) - sizeof(COGNAC_HEAP_BLOCK))
        return NULL;
    if (size > (SIZE_T)-1 - 15) return NULL;
    size = (size + 15) & ~(SIZE_T)15;
    heap_lock(heap);
    heap_initialize_locked(heap);
    for (block = heap->first; block; block = block->next) {
        if (!block_address_valid(block) || block->magic != BLOCK_MAGIC) {
            heap_unlock(heap);
            return NULL;
        }
        if (!block->free || block->size < size) continue;
        if (block->size >= size + sizeof(*block) + 16) {
            COGNAC_HEAP_BLOCK *split = (COGNAC_HEAP_BLOCK *)
                ((UCHAR *)(block + 1) + size);
            split->size = block->size - size - sizeof(*split);
            split->previous = block;
            split->next = block->next;
            split->magic = BLOCK_MAGIC;
            split->free = 1;
            if (split->next) split->next->previous = split;
            block->next = split;
            block->size = size;
        }
        block->free = 0;
        if (flags & HEAP_ZERO_MEMORY) {
            UCHAR *bytes = (UCHAR *)(block + 1);
            for (index = 0; index < block->size; ++index) bytes[index] = 0;
        }
        heap_unlock(heap);
        return (PVOID)(block + 1);
    }
    heap_unlock(heap);
    return NULL;
}

__declspec(dllexport)
SIZE_T WINAPI RtlSizeHeap(HANDLE handle, ULONG flags, PVOID address)
{
    COGNAC_HEAP *heap = &CognacNtdllProcessHeapObject;
    COGNAC_HEAP_BLOCK *block;
    SIZE_T result = (SIZE_T)-1;
    (void)flags;
    if (handle != (HANDLE)heap || !address) return result;
    heap_lock(heap);
    heap_initialize_locked(heap);
    block = find_block_locked(address);
    if (block && !block->free) result = block->size;
    heap_unlock(heap);
    return result;
}

__declspec(dllexport)
BOOLEAN WINAPI RtlFreeHeap(HANDLE handle, ULONG flags, PVOID address)
{
    COGNAC_HEAP *heap = &CognacNtdllProcessHeapObject;
    COGNAC_HEAP_BLOCK *block;
    (void)flags;
    if (handle != (HANDLE)heap || !address) return FALSE;
    heap_lock(heap);
    heap_initialize_locked(heap);
    block = find_block_locked(address);
    if (!block || block->free) {
        heap_unlock(heap);
        return FALSE;
    }
    block->free = 1;
    merge_next(block);
    if (block->previous && block_address_valid(block->previous) &&
        block->previous->magic == BLOCK_MAGIC && block->previous->free) {
        block = block->previous;
        merge_next(block);
    }
    heap_unlock(heap);
    return TRUE;
}

__declspec(dllexport)
PVOID WINAPI RtlReAllocateHeap(HANDLE handle, ULONG flags, PVOID address, SIZE_T size)
{
    SIZE_T old_size, copy_size, index;
    UCHAR *old_bytes, *new_bytes;
    PVOID replacement;
    if (!address) return RtlAllocateHeap(handle, flags, size);
    if (!size) {
        (void)RtlFreeHeap(handle, flags, address);
        return NULL;
    }
    old_size = RtlSizeHeap(handle, 0, address);
    if (old_size == (SIZE_T)-1) return NULL;
    replacement = RtlAllocateHeap(handle, flags & ~HEAP_ZERO_MEMORY, size);
    if (!replacement) return NULL;
    copy_size = old_size < size ? old_size : size;
    old_bytes = (UCHAR *)address;
    new_bytes = (UCHAR *)replacement;
    for (index = 0; index < copy_size; ++index) new_bytes[index] = old_bytes[index];
    if (flags & HEAP_ZERO_MEMORY)
        for (index = copy_size; index < size; ++index) new_bytes[index] = 0;
    if (!RtlFreeHeap(handle, flags, address)) {
        (void)RtlFreeHeap(handle, 0, replacement);
        return NULL;
    }
    return replacement;
}

__declspec(dllexport)
BOOLEAN WINAPI RtlValidateHeap(HANDLE handle, ULONG flags, PVOID address)
{
    COGNAC_HEAP *heap = &CognacNtdllProcessHeapObject;
    COGNAC_HEAP_BLOCK *block;
    ULONG_PTR expected;
    SIZE_T traversed = 0;
    BOOLEAN valid = TRUE;
    (void)flags;
    if (handle != (HANDLE)heap) return FALSE;
    heap_lock(heap);
    heap_initialize_locked(heap);
    if (address) {
        block = find_block_locked(address);
        valid = (BOOLEAN)(block && !block->free);
    } else {
        expected = arena_start();
        for (block = heap->first; block; block = block->next) {
            if (++traversed > PROCESS_HEAP_BYTES / sizeof(COGNAC_HEAP_BLOCK) ||
                !block_address_valid(block) || (ULONG_PTR)block != expected ||
                block->magic != BLOCK_MAGIC ||
                block->size > arena_end() - ((ULONG_PTR)block + sizeof(*block))) {
                valid = FALSE;
                break;
            }
            expected = (ULONG_PTR)(block + 1) + block->size;
            if (block->next && (!block_address_valid(block->next) ||
                block->next->previous != block)) {
                valid = FALSE;
                break;
            }
        }
        if (expected != arena_end()) valid = FALSE;
    }
    heap_unlock(heap);
    return valid;
}
