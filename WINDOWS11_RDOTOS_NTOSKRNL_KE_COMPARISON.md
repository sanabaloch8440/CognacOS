# Windows 11 x64 vs RdotOS — ntoskrnl Ke routine comparison

**Scope:** Public Windows 11 x64 WDK/DDI contracts compared with RdotOS implementation. The RdotOS export catalogue targets Windows 11 build 10.0.26100.8036 (AMD64). Microsoft's current ntoskrnl implementation is proprietary; this document compares public signatures and documented behavior, not private source code or binary equivalence.

- Implementation: kernel/src/ntoskrnl/ke/runtime.rs
- Import resolution: kernel/src/ntoskrnl/mod.rs
- Thread-record token: kernel/src/thread.rs
- Microsoft kernel DDI index: https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/_kernel/
- Microsoft WDM DDI index: https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/
- Microsoft-maintained public DDI sources: https://github.com/MicrosoftDocs/windows-driver-docs-ddi

## Summary and calculated compatibility

This pass adds **20 Ke routine implementations** and **20 name-based ntoskrnl import-resolution paths**. All use Rust's x64 Windows calling convention (extern "win64") and matching documented argument/return widths.

| Metric | Calculation | Result |
|---|---|---:|
| Source-level signature / x64 ABI shape | 20 matching parameter and return-width declarations ÷ 20 | **20/20 = 100%** |
| Name-based import resolution | New routine names resolving to callable implementations ÷ 20 | **20/20 = 100%** |
| Behavior match within the currently advertised topology | 9 processor-group / NUMA routines matching RdotOS's group-0/node-0 model ÷ 20 | **9/20 = 45%** |
| Partial or limited behavior | Remaining routines with documented gaps ÷ 20 | **11/20 = 55%** |
| Combined project score | (100% ABI + 45% behavior) ÷ 2 | **72.5%** |

**Important:** 72.5% is a project-defined progress metric, not a Microsoft metric, binary similarity score, or proof that Windows 11 drivers will run. Matching an ABI does not imply matching semantics. The behavior score counts topology queries as matches only within the explicitly supported single-group, single-node model. It does not imply compatibility with arbitrary NUMA or multi-group Windows systems.

### Scoring rules

- **ABI score** checks x64 calling convention, argument count/order, pointer/structure shape, integer widths, and return type. It does not claim Windows internal object layouts are reproduced.
- **Behavior score** counts nine processor-group / NUMA queries as matching the topology currently exposed by RdotOS. Timekeeping, APC state, event pulse atomicity, and thread object semantics remain partial.
- **Combined score** is the unweighted mean of the two percentages, shown for progress tracking only.

## Routine-by-routine comparison

Signatures below normalize the public WDK declarations to x64 C notation and omit SAL annotations. Rust names refer to kernel/src/ntoskrnl/ke/runtime.rs unless otherwise noted.

| # | Windows 11 public signature | RdotOS implementation and call path | Return value / implementation comparison |
|---:|---|---|---|
| 1 | VOID KeRaiseIrql(KIRQL NewIrql, PKIRQL OldIrql) | ke_raise_irql(new_irql: u8, old_irql: *mut u8); direct name resolver | **ABI match.** Writes previous IRQL and updates logical IRQL/APIC task priority. Current conservative implementation disables local interrupts until KeLowerIrql; Windows masks by hardware priority. Invalid lowering-by-raising is ignored rather than bugchecking. **Partial behavior.** [Microsoft docs](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-keraiseirql) |
| 2 | ULONGLONG KeQueryInterruptTime(VOID) | ke_query_interrupt_time(); calibrated TSC converted to 100-ns units | **ABI match.** Monotonic count origin is first query, not exact OS-start epoch; suspend/resume bias is not modeled. **Partial.** [Docs](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-kequeryinterrupttime) |
| 3 | ULONGLONG KeQueryInterruptTimePrecise(PULONG64 QpcTimeStamp) | ke_query_interrupt_time_precise(qpc_timestamp) | **ABI match.** Writes a TSC-backed performance-counter sample and returns 100-ns time. Shares the first-query epoch and lacks Windows suspend bias/timekeeper interpolation. **Partial.** [Docs](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-kequeryinterrupttimeprecise) |
| 4 | ULONGLONG KeQueryUnbiasedInterruptTime(VOID) | ke_query_unbiased_interrupt_time() | **ABI match.** Currently equals interrupt time because sleep states and sleep-time bias are not implemented. **Partial.** [Docs](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-kequeryunbiasedinterrupttime) |
| 5 | VOID KeQueryTickCount(PLARGE_INTEGER TickCount) | ke_query_tick_count(tick_count) | **ABI match.** Writes a 64-bit count derived from elapsed time at a nominal 1-ms increment. It is not driven from a canonical clock-interrupt count, so exact Windows tick semantics are not guaranteed. **Partial.** [Docs](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-kequerytickcount) |
| 6 | VOID KeQuerySystemTime(PLARGE_INTEGER CurrentTime) | ke_query_system_time(current_time), via HAL CMOS RTC | **ABI match.** Converts RTC calendar fields to 100-ns intervals since 1601-01-01. RTC-second precision, RTC-as-UTC assumption, and zero-on-RTC-failure differ from a kernel-maintained Windows system-time base. **Partial.** [Docs](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-kequerysystemtime) |
| 7 | VOID KeQuerySystemTimePrecise(PLARGE_INTEGER CurrentTime) | ke_query_system_time_precise(current_time) | **ABI match.** Currently delegates to the RTC-derived system-time routine and does **not** meet Microsoft's microsecond-precision contract. It is exposed for compatibility testing but is not ready for drivers that require precise wall time. **Known gap.** [Docs](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-kequerysystemtimeprecise) |
| 8 | PKTHREAD KeGetCurrentThread(VOID) | ke_get_current_thread() → thread::current_thread_object() | **Pointer ABI matches.** Returns an opaque pointer to an RdotOS thread-table record, or null for bootstrap/idle. This is not a Windows ETHREAD/KTHREAD layout and must only be treated as opaque by supported operations. **Partial.** [Microsoft WDM index](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/) |
| 9 | ULONG KeQueryHighestNodeNumber(VOID) | ke_query_highest_node_number() | Returns 0, matching the current one-node topology. **Match within stated scope.** [DDI index](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/_kernel/) |
| 10 | USHORT KeQueryMaximumGroupCount(VOID) | ke_query_maximum_group_count() | Returns 1, matching the current single-group model. **Match within stated scope.** [DDI index](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/_kernel/) |
| 11 | KAFFINITY KeQueryGroupAffinity(USHORT GroupNumber) | ke_query_group_affinity(group) | Returns active CPU mask for group 0; unsupported groups return zero. **Match within stated scope.** [DDI index](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/_kernel/) |
| 12 | ULONG KeQueryNodeActiveProcessorCount(USHORT NodeNumber) | ke_query_node_active_processor_count(node) | Returns online processor count for node 0 and zero for other nodes. **Match within stated scope.** [DDI index](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/_kernel/) |
| 13 | USHORT KeQueryNodeMaximumProcessorCount(USHORT NodeNumber) | ke_query_node_maximum_processor_count(node) | Returns configured maximum for node 0, zero for unsupported nodes. **Match within stated scope.** [Docs](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-kequerynodemaximumprocessorcount) |
| 14 | VOID KeQueryNodeActiveAffinity(USHORT NodeNumber, PGROUP_AFFINITY Affinity, PUSHORT Count) | ke_query_node_active_affinity(node, affinity, count) | Fills group 0's active mask and population count; supports only node 0 and one group. **Match within stated scope.** [Docs](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-kequerynodeactiveaffinity) |
| 15 | ULONG KeGetProcessorIndexFromNumber(PCPROCESSOR_NUMBER ProcNumber) | ke_get_processor_index_from_number(number) | Returns systemwide index for online group-0 processors; returns 0xffffffff for null, offline, or unsupported input. **Match within stated scope.** [Docs](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-kegetprocessorindexfromnumber) |
| 16 | NTSTATUS KeGetProcessorNumberFromIndex(ULONG ProcIndex, PPROCESSOR_NUMBER ProcNumber) | ke_get_processor_number_from_index(index, number) | Writes group 0, processor number and reserved zero; returns STATUS_SUCCESS or STATUS_INVALID_PARAMETER. **Match within stated scope.** [Docs](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-kegetprocessornumberfromindex) |
| 17 | NTSTATUS KeQueryNodeActiveAffinity2(USHORT NodeNumber, PGROUP_AFFINITY GroupAffinities, USHORT GroupAffinitiesCount, PUSHORT GroupAffinitiesRequired) | ke_query_node_active_affinity2(node, affinities, capacity, required) | Reports one affinity group for node 0; sets required count to one; returns STATUS_BUFFER_TOO_SMALL for insufficient capacity, STATUS_INVALID_PARAMETER for invalid node, success otherwise. **Match within stated scope.** [Docs](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-kequerynodeactiveaffinity2) |
| 18 | LONG KePulseEvent(PRKEVENT Event, KPRIORITY Increment, BOOLEAN Wait) | ke_pulse_event(event, increment, wait), calling KeSetEvent/KeResetEvent implementations | **ABI match.** Returns prior signal state. Set/reset are not one atomic critical section; Wait handoff and priority-boost semantics are incomplete. **Partial.** [Docs](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddk/nf-ntddk-kepulseevent) |
| 19 | BOOLEAN KeAreApcsDisabled(VOID) | ke_are_apcs_disabled() | Returns true when logical IRQL is above APC level. APC-disable nesting/counts are not maintained, so it can miss disabled APCs at lower IRQL. **Partial.** [WDM DDI index](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/) |
| 20 | BOOLEAN KeAreAllApcsDisabled(VOID) | ke_are_all_apcs_disabled() | Same IRQL-based approximation; special APC / guarded-region nesting is not tracked. **Partial.** [WDM DDI index](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/) |

## Return values, ABI, and call-path details

- New resolver entries are name-based and scoped to ntoskrnl.exe and accepted kernel aliases. No export ordinals are fabricated.
- KeGetProcessorNumberFromIndex returns NTSTATUS; KeGetProcessorIndexFromNumber returns an index or INVALID_PROCESSOR_INDEX. These different return contracts are intentional.
- KeQuerySystemTime and KeQuerySystemTimePrecise take an output pointer to an 8-byte LARGE_INTEGER; Rust uses *mut i64, preserving x64 memory shape.
- KeQueryTickCount also writes an 8-byte LARGE_INTEGER; it does not return the count in RAX.
- KeQueryInterruptTime returns a 64-bit unsigned value in RAX; the precise variant additionally writes the sampled QPC value through its optional output pointer.
- KeRaiseIrql writes prior 8-bit IRQL through OldIrql and returns void.
- KePulseEvent returns the previous event signal state as a 32-bit LONG; priority boosting and atomic pulse/wait handoff are not fully Windows-equivalent.
- KeGetCurrentThread has the correct pointer ABI but is not safe for code that assumes Windows internal KTHREAD/ETHREAD offsets.

## Public sources used

1. Microsoft Learn, [Windows kernel DDI catalogue](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/_kernel/).
2. Microsoft Learn, [WDM DDI catalogue](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/).
3. Microsoft Learn, [KeRaiseIrql](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-keraiseirql).
4. Microsoft Learn, [KeQueryInterruptTime](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-kequeryinterrupttime), [KeQueryInterruptTimePrecise](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-kequeryinterrupttimeprecise), [KeQueryUnbiasedInterruptTime](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-kequeryunbiasedinterrupttime), and [KeQueryTickCount](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-kequerytickcount).
5. Microsoft Learn, [KeQuerySystemTime](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-kequerysystemtime) and [KeQuerySystemTimePrecise](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-kequerysystemtimeprecise).
6. Microsoft Learn, [KeQueryNodeActiveAffinity](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-kequerynodeactiveaffinity), [KeQueryNodeMaximumProcessorCount](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-kequerynodemaximumprocessorcount), [KeGetProcessorIndexFromNumber](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-kegetprocessorindexfromnumber), [KeGetProcessorNumberFromIndex](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-kegetprocessornumberfromindex), and [KeQueryNodeActiveAffinity2](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-kequerynodeactiveaffinity2).
7. Microsoft Learn, [KePulseEvent](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddk/nf-ntddk-kepulseevent).
8. Microsoft-maintained public DDI source: [MicrosoftDocs/windows-driver-docs-ddi](https://github.com/MicrosoftDocs/windows-driver-docs-ddi).

## Follow-up needed for a stronger Windows 11 match

1. Maintain an interrupt-time epoch from boot and advance it from the clock interrupt; account for sleep/resume bias.
2. Maintain a kernel system-time base synchronized from RTC/firmware and interpolate it with the calibrated counter for KeQuerySystemTimePrecise.
3. Track APC disable and guarded-region nesting per thread.
4. Make event pulse atomic with waiter satisfaction; implement Wait handoff and priority boosts.
5. Replace the opaque RdotOS thread-record token with a stable NT-compatible thread object abstraction for supported driver-visible operations.
6. Expand ACPI SRAT/SLIT NUMA discovery and multi-group topology before claiming compatibility beyond one group/node.
7. Run the full freestanding build and OVMF/QEMU smoke/SMP matrix. Signature review is not runtime verification.
