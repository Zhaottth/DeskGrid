# DeskGrid

[简体中文](README.md) · [繁體中文](README.zh-TW.md) · [English](README.en.md)

DeskGrid is an **open-source Stardock Fences alternative** for Windows 10 and Windows 11. It organizes desktop icons, folders, and shortcuts into separate group panels. Grouping or pinning an item saves its path and leaves the original file in place.

**[Download v2.0.0 for Windows x64](https://github.com/Zhaottth/DeskGrid/releases/download/v2.0.0/DeskGrid-2.0.0-windows-x64.zip)** · [Build from source](#build-from-source) · [Report an issue](https://github.com/Zhaottth/DeskGrid/issues)

## What's new in v2.0.0

- Rewritten in Rust with a native Win32 interface. Running the app no longer requires .NET Framework.
- The interface supports Simplified Chinese, Traditional Chinese, and English. It can read an existing `layout.xml` when you upgrade.
- Draw a group directly on the desktop with the right mouse button and organize system icons that are enabled on the Windows desktop. This version also fixes behavior related to group membership, panel stacking, drag and drop, and sorting.

## Getting started

Extract the release ZIP and double-click `DeskGrid.exe` to run it. DeskGrid supports Windows 10 version 1607 or later and Windows 11. If upgrading, first exit the previous version from the system tray.

Press **Ctrl + Alt + Page Up** or **Ctrl + Alt + Page Down** to cycle through existing desktop pages. To change the interface language, double-click the tray icon, open **Settings → Language**, and save.

1. The default groups are **Apps and shortcuts**, **Desktop folders**, **Documents and media**, and **Other desktop items**.
2. Drag the title bar to move a panel or the lower-right corner to resize it. Double-click the title bar to collapse or expand the panel.
3. Drop an item on empty space in another group or on a regular tab, or use the item's context menu to assign it to a group. Drops onto folder icons, folder portals, File Explorer, or the Recycle Bin are handled by Windows. Hold Ctrl while dropping to request a copy. Drop a file from File Explorer onto empty space in a regular group to pin it.
4. Double-click the DeskGrid tray icon to open the management window. Choose **Add folder portal** to browse a folder and its subfolders from the desktop.
5. Open the panel's **…** menu and choose **Dock as tab…**. Select a tab to switch its contents into view, or drop files directly onto a tab.
6. Open **Edit panel** to change opacity, tint icons, or fade the panel when the pointer leaves.
7. System icons enabled on the Windows desktop, such as **This PC** and **Control Panel**, can be opened or assigned to groups in DeskGrid. In **Settings**, you can show desktop items only in DeskGrid panels or enable double-clicking empty desktop space to toggle the panels.
8. Right-drag on empty desktop space to draw a group on the current page. Press Esc to cancel while drawing. **New page** in the management window creates and opens an empty page. Use **Manage pages** to switch or rename pages, and **Show on pages…** to display a panel on multiple pages.
9. Panels offer icon, list, and details views, with separate icon sizes and hover delays for expanding and collapsing. Hold Alt while moving a panel to temporarily disable snapping.
10. A preview follows the pointer while dragging. If desktop items are shown only in DeskGrid, you can drop file items onto empty desktop space and place them independently. Their positions are saved. Double-click to open them or drag them back into a group.
11. To reorder an item in a regular group, drag it to the left or right half of an icon and release when the insertion line appears. Use the edges of a folder icon to reorder around it, or the center to drop a file into the folder. The Recycle Bin stays in the first slot. In list and details views, use the top or bottom edge. Drag near the top or bottom of the content area to scroll, or press Esc to cancel. Manual order is saved automatically. To change it, use **Edit panel → Sort by**.

## Shortcuts and settings

| Action | Shortcut |
| --- | --- |
| Show or hide group panels | `Ctrl+Alt+H` |
| Bring panels to the front temporarily, then press again to exit | `Ctrl+Alt+Space` |
| Search by name across all pages and panels | `Ctrl+Alt+F` |
| Previous desktop page | `Ctrl+Alt+Page Up` |
| Next desktop page | `Ctrl+Alt+Page Down` |

These actions are also available from the tray menu. The interface language is saved with the layout. The management window also supports changing rule priority, aligning multiple panels, importing and exporting layouts, and starting DeskGrid at sign-in.

Files come from the personal and public desktop folders. System icons come from the current Windows desktop view. When enabled, the Recycle Bin appears first in the default group. **Recycle Bin** and **This PC** are also available from the tray menu. DeskGrid saves its layout in `%LOCALAPPDATA%\DeskGrid\layout.xml`.

## Build from source

On Windows, install Rust 1.89 or later with the MSVC toolchain, the Visual Studio C++ Build Tools, and the Windows SDK. Run the following in the project directory to build `dist\DeskGrid.exe`.

```powershell
.\build.ps1
```

Run `cargo test --locked` to check legacy layout compatibility, backups, and group sorting.

## License and credit

DeskGrid © 2026 Zhaottth. It is available under the [MIT License](LICENSE). Keep the copyright and permission notices in all copies or substantial portions of the project.

Thanks to the maintainers of Microsoft windows-rs, quick-xml, Serde, tempfile, uuid, embed-resource, and their dependencies. See [Third-party software notices](THIRD_PARTY_NOTICES.txt) for the Windows build's dependencies and license texts. The release ZIP also includes the notices for the Rust standard library used to build it.

DeskGrid is an independent project. Fences is a Stardock product.
