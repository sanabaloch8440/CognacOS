# Imported third-party source references

## Ext3Fsd

- Upstream: https://github.com/matt-wu/Ext3Fsd
- Pinned upstream commit: `1ae63687d39a1ff78ef8edb0117a945072ee1326`
- Source location in this tree: `third_party/Ext3Fsd` (Git submodule)
- Upstream project describes its sources as GNU GPL version 2. See the upstream README and driver readmes for the license notice and author attribution.
- This submodule preserves the upstream sources and history separately; it does not relicense them under RdotOS's project license.
- Ext3Fsd is a Windows kernel-mode ext2/ext3/ext4 filesystem driver, not a driver that can be loaded directly by RdotOS. Its Windows-specific IRP, Cache Manager, memory-manager, and I/O interfaces must be mapped to RdotOS abstractions during a Rust port.
- The upstream README documents known ext4 limitations and warns about possible data corruption. Treat this as a reference source only; do not load it or enable write support in RdotOS.

### Licensing note

The upstream project states GNU GPLv2. RdotOS is separately licensed under Apache-2.0 OR MIT. Keep this source as a clearly separated submodule with its original license notices. A Rust translation derived from these sources may also be subject to GPLv2; do not distribute that translated component under Apache/MIT alone without an applicable permission or legal review.

### Checkout

```sh
git submodule update --init --recursive third_party/Ext3Fsd
```
