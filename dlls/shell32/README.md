# CognacOS shell32 resource library

This directory is the home for reusable shell resources for CognacOS `shell32`: icons, bitmap artwork, cursor/animation assets, resource IDs, and their provenance records.

## Upstream source

- ReactOS shell32: https://github.com/reactos/reactos/tree/master/dll/win32/shell32
- Resource declarations: https://github.com/reactos/reactos/blob/master/dll/win32/shell32/shell32.rc
- ReactOS project license and notices: https://github.com/reactos/reactos/blob/master/COPYING
- ReactOS intellectual-property guidance: https://reactos.org/intellectual-property-guideline/

## Important licensing rule

ReactOS is a mixed-license repository. Do **not** assume every file under `dll/win32/shell32` has the same license. The source tree notes that the shell icons, bitmaps, and AVI resources have license information in their corresponding resource directories. In particular, ReactOS documents Tango-derived icon artwork under CC-BY-SA 2.5; the relevant upstream `Icon_Licence` notices must travel with those assets. Other files may be LGPL-2.1-or-later, GPL-2.0, or carry separate third-party terms.

When importing a file, preserve its original copyright/license notice and add an entry to `RESOURCE_MANIFEST.md` with:
- original ReactOS path and immutable upstream commit;
- local destination path;
- original author/copyright line, when supplied;
- exact license and the path to its license text;
- any local modification.

Do not remove attribution, relicense third-party artwork as CognacOS-only, or copy a resource until its individual license has been checked. If a resource cannot be traced to a clear license, leave it out and create an original replacement instead.

## Layout

- `icons/` — approved shell/file/folder/device icons and their per-asset notices.
- `bitmaps/` — approved bitmap resources and notices.
- `animations/` — approved animation resources and notices.
- `RESOURCE_MANIFEST.md` — file-by-file provenance and license ledger.
- `LICENSES/` — full license texts and upstream attribution notices copied alongside the corresponding assets.

## Imported starter set

The first batch of 20 Tango Project icons is now stored under `resources/icons/`: file, executable, folder/open folder, drive, CD-ROM, network, computer, printer, Control Panel, search, help, share, shortcut, empty/full Recycle Bin, desktop, favorites, locked item, and disconnected drive. See `RESOURCE_CATALOG.md` for names and exact upstream paths.

These resources are a starting library for future shell32 implementation; importing artwork does not import ReactOS shell32 code or imply that the CognacOS shell32 APIs are already implemented.
