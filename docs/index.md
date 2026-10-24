---
layout: default
title: RdotOS Documentation
---

# RdotOS

RdotOS is an experimental Rust-first x86-64 operating system with a UEFI boot path and NT-inspired kernel interfaces. Windows 11 x64 driver compatibility is a long-term goal, not a current guarantee.

[![Build and QEMU boot test](https://github.com/umarfarooq7869/RdotOS/actions/workflows/build-uefi.yml/badge.svg)](https://github.com/umarfarooq7869/RdotOS/actions/workflows/build-uefi.yml)

## Documentation

- [Project status and architecture](project.md)
- [Roadmap and milestones](roadmap.md)
- [Windows 11 x64 compatibility overview](compatibility/index.md)
- [Ke routine-by-routine comparison](compatibility/ke.md)
- [Se security comparison](compatibility/se.md)
- [Source repository](https://github.com/umarfarooq7869/RdotOS)
- [Build and QEMU workflow](https://github.com/umarfarooq7869/RdotOS/actions/workflows/build-uefi.yml)

## Compatibility disclaimer

An exported name, matching signature, PE32+ image, or successful emulator boot does not prove semantic or binary compatibility with Windows 11. Each comparison records implemented scope, missing behavior, and validation status. RdotOS is not currently a Windows replacement and does not claim general third-party WDK driver compatibility.
