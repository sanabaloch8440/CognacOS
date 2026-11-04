# CognacOS licensing and provenance policy

## First-party project code

Unless a file or directory contains a more specific notice, first-party
CognacOS code is offered under either the MIT License or Apache License 2.0,
at the recipient's option (SPDX: `MIT OR Apache-2.0`). The complete texts are
in [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).

The dual-license declaration applies only to material for which CognacOS has
the necessary copyright and relicensing rights. It does not relicense third-
party code by implication.

## Third-party code and dependencies

- Keep each third-party component under its actual upstream license.
- Preserve copyright, license, patent, attribution, and NOTICE obligations.
- Do not remove upstream headers or replace them with the CognacOS SPDX
  expression.
- Record vendored or adapted components and their source revision in the
  relevant directory's documentation and in a third-party notices inventory.
- A dependency's license does not change merely because a CognacOS crate
  depends on it. Cargo package metadata describes the package itself, not
  its dependencies.

## Source headers

New first-party source files should carry
`SPDX-License-Identifier: MIT OR Apache-2.0` in the appropriate comment
syntax. Existing files without headers are not assigned invented individual
copyright statements. Existing upstream notices must remain intact. See
[LICENSE-HEADER.txt](LICENSE-HEADER.txt).

## ACPI implementation independence

CognacOS's ACPI/AML implementation must be independently authored. Do not integrate, copy, adapt, or create close code-level derivatives of ACPICA, ReactOS, or Linux ACPI implementation code or tests. ACPI specifications and operating-system interface documentation may be used as behavioral references; implementation structure, code, and tests must be independently developed. Public implementation repositories may be consulted only to clarify specification questions, never as sources for reusable code, implementation structure, or fixtures. Any future external code must undergo provenance and license review before inclusion.
