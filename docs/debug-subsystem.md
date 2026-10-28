# NTOSKRNL Debug Subsystem

CognacOS has a dedicated kernel debug subsystem at `kernel/src/ntoskrnl/debug.rs`. It complements the early UART console and the existing low-level exception trace; it is not yet a remote kernel debugger or a complete WinDbg/KD transport.

## Design

- **Structured trace ring:** 128 fixed-size records, sequence-stamped and allocation-free. Each record carries component, level, event ID, two arguments, and an instruction pointer.
- **Best-effort concurrent publication:** atomic slot ownership and sequence validation prevent readers from treating a partially written or overwritten slot as valid.
- **Dynamic component filters:** `DbgSetDebugFilterState` and `DbgQueryDebugFilterState` use the same filter table as the structured tracing subsystem.
- **Failure-path capture:** IRP completion-guard rejection, page faults, CPU exceptions, and kernel panics are recorded before serial diagnostics are emitted.
- **Post-failure dump:** fatal exception and panic paths print recent structured records to the UART, followed by the existing low-level kernel trace.
- **CI-retained evidence:** the build workflow retains the serial log and the beep-driver assessment as downloadable workflow artifacts. The assessment now extracts any structured trace dump and relevant crash/driver lines into `output.txt`, so evidence survives the CI job even though kernel RAM does not.

The implementation is inspired by Linux's bounded tracing/event model and dynamic-debug filtering, and by ReactOS's kernel debug-print/filter APIs. It intentionally avoids heap allocation and blocking locks in exception paths.

## Record format

The UART dump uses one line per record:

```text
DBG <sequence> <component> <level> <event> <arg0> <arg1> <RIP>
```

All fields are printed in hexadecimal. Event arguments are event-specific; consult the event site before interpreting them.

## Initial components and events

| Component | ID | Purpose |
|---|---:|---|
| Default | 0 | General kernel debug output |
| I/O manager | 1 | IRP dispatch and completion diagnostics |
| Driver loader | 2 | Driver lifecycle diagnostics |
| Memory manager | 3 | Memory-management diagnostics |
| Scheduler | 4 | Scheduling and cross-CPU diagnostics |
| Exception | 5 | CPU exceptions, page faults, and panic events |

| Event | ID | Arguments |
|---|---:|---|
| Debug print | `0x1` | Format-string address; no string is dereferenced by the trace ring |
| I/O call rejected | `0x1001` | `arg0`: next stack-location address; `arg1`: stack count in high 32 bits and current location in low 32 bits; RIP field stores current stack-location address |
| Driver load | `0x2001` | Reserved for driver lifecycle instrumentation |
| Driver dispatch | `0x2002` | Reserved for driver dispatch instrumentation |
| CPU exception | `0x3001` | `arg0`: vector; `arg1`: error code; RIP: saved instruction pointer |
| Page fault | `0x3002` | `arg0`: CR2 fault address; `arg1`: x86 page-fault error code; RIP: saved instruction pointer |
| Kernel panic | `0x3003` | `arg0`: source line where available |

## Filtering

Levels follow the Windows debug-filter convention: error = 0, warning = 1, info = 2, trace = 3. Error and warning levels are enabled by default for each component. Use the kernel's `DbgSetDebugFilterState(component, level, state)` API to enable or disable a level; `DbgQueryDebugFilterState` queries it. Trace events sent through the filter-aware API do not consume ring entries when disabled. Critical exception and I/O rejection records bypass filters so failures remain observable.

Example from kernel code:

```rust
crate::ntoskrnl::debug::set_filter(
    crate::ntoskrnl::debug::COMPONENT_IO,
    crate::ntoskrnl::debug::LEVEL_TRACE,
    true,
);
```

## Current limitations

- The ring is volatile and bounded; it is not persistent storage and may overwrite older records. CI preserves the serial log and assessment file as artifacts, but this is post-run evidence collection—not kernel-persistent tracing across resets.
- COM1 now has a non-blocking, bounded command console available from the kernel halt/debugger loop. Commands: `help`, `status`, `trace [count-hex]`, `filter <component-hex> <level-hex> <on|off>`, `persist`, and `restore`. Filter updates are live and affect the shared trace ring immediately. This is a polled local UART console, not a full interactive source-level debugger or symbol resolver.
- Trace snapshots can be saved/restored from GPT entry 3, but only when the disk image explicitly reserves that partition. The CI image now allocates a dedicated `COGNACDEBUG` partition and the kernel bounds all reads/writes to that partition. Snapshots have a magic/version, bounded record count, and CRC-32 validation. The volatile ring remains the active trace buffer; persistence is an explicit snapshot at halt or via `persist`, not a synchronous disk write for every event.
- KDNET, USB KD, and IEEE 1394 transports are still **not implemented**. They require working NIC/UDP and/or USB/1394 host-controller/device drivers and a transport-specific KD protocol/session implementation; the UART console must not be represented as those transports.
- `DbgPrint`/ `DbgPrintEx` retain the current limited ABI implementation; variadic formatting is not fully implemented, so the trace ring records the format-string address rather than a rendered message.
- Driver-load and dispatch event IDs are defined for a stable event vocabulary; broader instrumentation is incremental.
- A workflow's successful compilation does not prove a driver test passed. The `beep.sys` IOCTL runtime test must still be judged from its QEMU serial trace and assessment artifact.

## Validation

The workflow now runs host tests for the structured debug subsystem and the interactive console command parser (in addition to the older kernel diagnostics tests). The boot disk image reserves a dedicated third GPT partition for trace snapshots, separate from the ESP and ext4 data partition. They cover filter independence, record round-tripping, rejection of stale records after ring wraparound, and filtered events not consuming ring slots. QEMU panic/exception output should include the `NTOSKRNL DEBUG TRACE` section when recent records exist. The beep-driver assessment extracts that section if present and explicitly reports when no dump was emitted; it does not treat a missing trace dump as a successful driver test.
