# CognacOS shell32 starter icon catalog

Binary assets are stored in `resources/icons/`. These are original ReactOS resource filenames and are **not yet assigned CognacOS numeric resource IDs**; the mapping below is a human-readable starter index for future RC/header integration.

| Local asset | Intended use | ReactOS resource symbol |
|---|---|---|
| `1.ico` | Generic file | `IDI_SHELL_FILE` |
| `3.ico` | Executable/application | `IDI_SHELL_EXE` |
| `4.ico` | Closed folder | `IDI_SHELL_FOLDER` |
| `5.ico` | Open folder | `IDI_SHELL_FOLDER_OPEN` |
| `9.ico` | Drive | `IDI_SHELL_DRIVE` |
| `12.ico` | CD-ROM drive | `IDI_SHELL_CDROM` |
| `14.ico` | Network / entire network | `IDI_SHELL_ENTIRE_NETWORK` |
| `16.ico` | This computer | `IDI_SHELL_MY_COMPUTER` |
| `17.ico` | Printer | `IDI_SHELL_PRINTER` |
| `22.ico` | Control Panel | `IDI_SHELL_CONTROL_PANEL` |
| `23.ico` | Search | `IDI_SHELL_SEARCH` |
| `24.ico` | Help | `IDI_SHELL_HELP` |
| `29.ico` | Shared item | `IDI_SHELL_SHARE` |
| `30.ico` | Shortcut overlay | `IDI_SHELL_SHORTCUT` |
| `32.ico` | Empty Recycle Bin | `IDI_SHELL_EMPTY_RECYCLE_BIN` |
| `33.ico` | Full Recycle Bin | `IDI_SHELL_FULL_RECYCLE_BIN` |
| `35.ico` | Desktop | `IDI_SHELL_DESKTOP` |
| `44.ico` | Favorites | `IDI_SHELL_FAVORITES` |
| `48.ico` | Locked item | `IDI_SHELL_LOCKED` |
| `54.ico` | Disconnected drive | `IDI_SHELL_NOT_CONNECTED_HDD` |

## Upstream

Source: [ReactOS shell32 resource declarations](https://github.com/reactos/reactos/blob/e46f4bb1a1ea7c722f941231565144fe0c286d3e/dll/win32/shell32/icon_res.rc) and `res/icons/`, pinned to commit `e46f4bb1a1ea7c722f941231565144fe0c286d3e`.

All 20 assets in this batch are listed in the upstream Tango attribution notice and are licensed CC BY-SA 2.5. See `resources/licenses/Tango.txt` and `resources/licenses/CC-BY-SA-2.5.txt`.
