---
layout: default
title: Windows 11 x64 compatibility
---

# Windows 11 x64 compatibility

RdotOS compares public Windows 11 x64 contracts with the code currently implemented. The reference target is Windows 11 x64 build 10.0.26100.8036 (AMD64) where stated in the source comparisons.

- [Ke routine comparison](ke.md): exported routine inventory, scoped behavior status, resolver coverage, and scoring methodology.
- [Se security comparison](se.md): security routines, implementation scope, and unresolved semantics.

## How to read the comparisons

- **Exported/declarable** does not mean implemented.
- **Resolver coverage** means an import name has a resolution path; it does not prove correct behavior.
- **Behavior matches** are project-scoped assessments, not Microsoft ratings.
- Public documented layouts are checked where possible. Private Windows structures are not inferred from opaque names.
- Every score is meaningful only with its stated denominator, rubric, and source revision. It is not a guarantee that a Windows driver will load or run.
