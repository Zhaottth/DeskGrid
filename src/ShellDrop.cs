using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Windows.Forms;
using OleDataObject = System.Runtime.InteropServices.ComTypes.IDataObject;

namespace DeskGrid
{
    internal sealed class ShellDrop : IDisposable
    {
        private const string RecycleBinId = "::{645FF040-5081-101B-9F08-00AA002F954E}";
        private string destination;
        private OleDropTarget target;

        internal static bool CanDrop(string destination, string[] paths, int keyState)
        {
            if (paths.Length == 0) return false;
            if (destination == FileEntry.RecycleBin.Path) return true;
            string folder = FileEntry.FullPath(destination);
            if (folder == null) return false;
            folder = folder.TrimEnd('\\');
            foreach (string source in paths)
            {
                // The separator keeps similarly named siblings (e.g. Games and Games2) distinct.
                string from = source.TrimEnd('\\');
                if (String.Equals(folder, from, StringComparison.OrdinalIgnoreCase) ||
                    folder.StartsWith(from + "\\", StringComparison.OrdinalIgnoreCase)) return false;
                const int copyOrLinkModifier = 8 | 32; // MK_CONTROL or MK_ALT
                string parent = Path.GetDirectoryName(source);
                if ((keyState & copyOrLinkModifier) == 0 &&
                    parent != null && String.Equals(folder, parent.TrimEnd('\\'), StringComparison.OrdinalIgnoreCase)) return false;
            }
            return true;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct DropPoint
        {
            internal int X, Y;
            internal DropPoint(int x, int y) { X = x; Y = y; }
        }

        [ComImport, Guid("00000122-0000-0000-C000-000000000046"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
        private interface OleDropTarget
        {
            [PreserveSig] int DragEnter(OleDataObject data, uint keys, DropPoint point, ref uint effect);
            [PreserveSig] int DragOver(uint keys, DropPoint point, ref uint effect);
            [PreserveSig] int DragLeave();
            [PreserveSig] int Drop(OleDataObject data, uint keys, DropPoint point, ref uint effect);
        }

        [ComImport, Guid("000214E6-0000-0000-C000-000000000046"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
        private interface ShellFolder
        {
            // Only GetUIObjectOf is called. The preceding methods reserve its COM slots.
            void ParseDisplayName();
            void EnumObjects();
            void BindToObject();
            void BindToStorage();
            void CompareIds();
            void CreateViewObject();
            void GetAttributesOf();
            [PreserveSig] int GetUIObjectOf(IntPtr owner, uint count,
                [MarshalAs(UnmanagedType.LPArray, SizeParamIndex = 1)] IntPtr[] items, ref Guid iid,
                IntPtr reserved, [MarshalAs(UnmanagedType.Interface)] out OleDropTarget value);
        }

        [DllImport("shell32.dll", CharSet = CharSet.Unicode, ExactSpelling = true)]
        private static extern int SHParseDisplayName(string name, IntPtr bind, out IntPtr item, uint attributes, IntPtr returnedAttributes);
        [DllImport("shell32.dll", ExactSpelling = true)]
        private static extern int SHBindToParent(IntPtr item, ref Guid iid, [MarshalAs(UnmanagedType.Interface)] out ShellFolder parent, out IntPtr child);

        private static OleDropTarget Bind(IntPtr owner, string path)
        {
            IntPtr full;
            if (SHParseDisplayName(path, IntPtr.Zero, out full, 0, IntPtr.Zero) < 0 || full == IntPtr.Zero) return null;
            ShellFolder parent = null;
            try
            {
                var folderId = typeof(ShellFolder).GUID;
                IntPtr child;
                if (SHBindToParent(full, ref folderId, out parent, out child) < 0 || parent == null) return null;
                var targetId = typeof(OleDropTarget).GUID;
                OleDropTarget result;
                return parent.GetUIObjectOf(owner, 1, new[] { child }, ref targetId, IntPtr.Zero, out result) >= 0 ? result : null;
            }
            finally
            {
                if (parent != null) Marshal.ReleaseComObject(parent);
                Marshal.FreeCoTaskMem(full);
            }
        }

        internal DragDropEffects Hover(IntPtr owner, string path, DragEventArgs e)
        {
            var data = e.Data as OleDataObject;
            if (data == null) return DragDropEffects.None;
            var point = new DropPoint(e.X, e.Y);
            uint effect = (uint)e.AllowedEffect;
            try
            {
                int status;
                if (!String.Equals(path, destination, StringComparison.OrdinalIgnoreCase))
                {
                    Dispose();
                    destination = path;
                    target = Bind(owner, path == FileEntry.RecycleBin.Path ? RecycleBinId : path);
                    if (target == null) return DragDropEffects.None;
                    status = target.DragEnter(data, (uint)e.KeyState, point, ref effect);
                }
                else
                {
                    if (target == null) return DragDropEffects.None;
                    status = target.DragOver((uint)e.KeyState, point, ref effect);
                }
                return status >= 0 ? (DragDropEffects)effect & e.AllowedEffect : DragDropEffects.None;
            }
            catch (COMException) { Dispose(); return DragDropEffects.None; }
        }

        internal DragDropEffects Drop(DragEventArgs e)
        {
            if (target == null) return DragDropEffects.None;
            var data = e.Data as OleDataObject;
            if (data == null) return DragDropEffects.None;
            uint effect = (uint)e.AllowedEffect;
            try
            {
                return target.Drop(data, (uint)e.KeyState, new DropPoint(e.X, e.Y), ref effect) >= 0
                    ? (DragDropEffects)effect & e.AllowedEffect : DragDropEffects.None;
            }
            catch (COMException) { return DragDropEffects.None; }
            finally { Release(); }
        }

        public void Dispose()
        {
            try { if (target != null) target.DragLeave(); }
            catch (COMException) { }
            finally { Release(); }
        }

        private void Release()
        {
            if (target != null) Marshal.ReleaseComObject(target);
            target = null;
            destination = null;
        }
    }
}
