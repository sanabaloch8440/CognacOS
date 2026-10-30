# CognacOS FAT32 driver

CognacOS uses the upstream [rust-fatfs](https://github.com/rafalh/rust-fatfs)
engine, pinned to commit `38b3a7950bf0e765e34286d1dfe76c519ee347e2`. The
dependency is built without `std`; the driver supplies a partition-bounded
sector adapter.

## Initial integration

- FAT32-only BPB preflight before mounting; FatFs performs its own BPB and
  FSInfo parsing and validation.
- Partition-relative stream with overflow-safe LBA and byte-range checks.
- 512, 1024, 2048, and 4096-byte logical sector support.
- Read-only mount by default.
- FatFs supplies FAT12/16/32 parsing, FAT allocation, VFAT long names,
  directory traversal, file streams, and volume formatting primitives.
- Partial-sector writes use read-modify-write; device flush is forwarded.
- Explicit write mounting is behind the `write-support` Cargo feature.

## Safety and current limitations

Writable mounting is deliberately disabled in the default kernel build.
FAT32 has no journal, so an interrupted metadata update can leave FAT copies,
directory entries, and FSInfo out of sync. Before enabling writes by default,
CognacOS needs Linux/Windows-created image tests, FAT mirror consistency tests,
FSInfo backup/repair tests, injected I/O failures, power-loss tests, and
mount/unmount dirty-bit validation.

The first integration does not yet register a filesystem device with the NT
I/O Manager or discover and mount partitions automatically. The block-device
trait is the boundary to connect to the storage stack; partition discovery and
mount orchestration are the next integration stage.

## License

The adapter is MIT-licensed. Upstream rust-fatfs is MIT-licensed; see the
upstream repository for its copyright and license terms.
