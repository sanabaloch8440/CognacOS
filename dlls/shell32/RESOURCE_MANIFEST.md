# Shell32 resource provenance manifest

No third-party binary artwork is registered in this manifest yet. Add one row per imported file only after checking the license notice shipped with that exact upstream resource.

| Local path | ReactOS source path | Upstream commit | Copyright / author | License | Modified? |
|---|---|---|---|---|---|
| — | — | — | — | — | — |

## Known upstream license leads (verify per file before copying)

- ReactOS shell32 resource declarations say that icon, bitmap, and AVI licenses are in their corresponding resource directories: `dll/win32/shell32/shell32.rc`.
- Tango-derived icon artwork has a `CC-BY-SA-2.5` notice under `dll/win32/shell32/res/icons/Icon_Licence/`; preserve the upstream `COPYING`, `Tango.txt`, and any icon-specific notice (such as `RenameIcon.txt`) with the applicable assets.
- The ReactOS repository's general `COPYING` file is GPL-2.0, but that does not override a resource's separate license.
