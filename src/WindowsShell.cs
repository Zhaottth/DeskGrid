using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Imaging;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Windows.Forms;
using Microsoft.Win32;

namespace DeskGrid
{
    internal static class WindowsShell
    {
        internal const int ToolWindow = 0x80;
        internal const int TransparentWindow = 0x20;
        internal const int NoActivateWindow = 0x08000000;
        internal const int HotkeyMessage = 0x0312;
        internal const int DpiChangedMessage = 0x02E0;
        internal const int MouseActivateMessage = 0x0021;
        private const int StyleIndex = -16;
        private const int ExStyleIndex = -20;
        private const int Layered = 0x00080000;
        private const int Child = 0x40000000;
        private const int Popup = unchecked((int)0x80000000);
        private const int Visible = 0x10000000;
        private const uint NoActivate = 0x10;
        private const uint NoOwnerOrder = 0x200;
        private const uint NoMove = 0x2;
        private const uint NoSize = 0x1;
        private const uint NoZOrder = 0x4;
        private const uint FrameChanged = 0x20;
        private static readonly IntPtr Topmost = new IntPtr(-1);
        private static readonly IntPtr NotTopmost = new IntPtr(-2);

        [StructLayout(LayoutKind.Sequential)]
        internal struct NativeRect
        {
            internal int Left, Top, Right, Bottom;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct NativePoint
        {
            internal int X, Y;
            internal NativePoint(int x, int y) { X = x; Y = y; }
        }

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        private struct FileInfo
        {
            internal IntPtr Icon;
            internal int Index;
            internal uint Attributes;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 260)] internal string DisplayName;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 80)] internal string TypeName;
        }

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        private struct StockIconInfo
        {
            internal uint Size;
            internal IntPtr Icon;
            internal int ImageIndex, IconIndex;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 260)] internal string Path;
        }

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        private struct ShellExecuteInfo
        {
            internal int Size;
            internal uint Mask;
            internal IntPtr Owner;
            internal string Verb, FileName, Parameters, Directory;
            internal int Show;
            internal IntPtr Instance, IdList;
            internal string ClassName;
            internal IntPtr ClassKey;
            internal uint HotKey;
            internal IntPtr IconOrMonitor, Process;
        }

        private delegate bool EnumWindowsProc(IntPtr window, IntPtr param);
        [DllImport("user32.dll")] private static extern IntPtr GetShellWindow();
        [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindowsProc callback, IntPtr param);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern IntPtr FindWindowEx(IntPtr parent, IntPtr after, string className, string name);
        [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr window);
        [DllImport("user32.dll")] internal static extern bool IsWindow(IntPtr window);
        [DllImport("user32.dll")] private static extern IntPtr GetParent(IntPtr window);
        [DllImport("user32.dll", SetLastError = true)] private static extern IntPtr SetParent(IntPtr window, IntPtr parent);
        [DllImport("user32.dll", EntryPoint = "GetWindowLongW")] private static extern int GetWindowLong(IntPtr window, int index);
        [DllImport("user32.dll", EntryPoint = "SetWindowLongW")] private static extern int SetWindowLong(IntPtr window, int index, int value);
        [DllImport("user32.dll")] private static extern bool ScreenToClient(IntPtr window, ref NativePoint point);
        [DllImport("user32.dll")] private static extern bool GetWindowRect(IntPtr window, out NativeRect rect);
        [DllImport("user32.dll")] private static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int w, int h, uint flags);
        [DllImport("user32.dll")] private static extern bool ShowWindowAsync(IntPtr window, int command);
        [DllImport("user32.dll")] private static extern bool SetLayeredWindowAttributes(IntPtr window, uint key, byte alpha, uint flags);
        [DllImport("user32.dll")] private static extern IntPtr WindowFromPoint(NativePoint point);
        [DllImport("user32.dll")] private static extern bool IsChild(IntPtr parent, IntPtr child);
        [DllImport("user32.dll")] private static extern IntPtr GetAncestor(IntPtr window, uint relationship);
        [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        private static extern int GetClassName(IntPtr window, StringBuilder text, int length);
        [DllImport("user32.dll")] private static extern uint GetDpiForWindow(IntPtr window);
        [DllImport("user32.dll")] private static extern uint GetDpiForSystem();
        [DllImport("user32.dll")] private static extern bool DestroyIcon(IntPtr icon);
        [DllImport("user32.dll")] internal static extern bool RegisterHotKey(IntPtr window, int id, uint modifiers, uint key);
        [DllImport("user32.dll")] internal static extern bool UnregisterHotKey(IntPtr window, int id);
        [DllImport("shell32.dll", CharSet = CharSet.Unicode)] private static extern IntPtr SHGetFileInfo(string path, uint attributes, out FileInfo info, uint size, uint flags);
        [DllImport("shell32.dll", ExactSpelling = true)] private static extern int SHGetStockIconInfo(int id, uint flags, ref StockIconInfo info);
        [DllImport("shell32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern bool ShellExecuteEx(ref ShellExecuteInfo info);

        internal static IntPtr DesktopView()
        {
            var view = FindWindowEx(GetShellWindow(), IntPtr.Zero, "SHELLDLL_DefView", null);
            if (view != IntPtr.Zero && IsWindowVisible(view)) return view;
            EnumWindows((window, ignored) =>
            {
                var candidate = FindWindowEx(window, IntPtr.Zero, "SHELLDLL_DefView", null);
                if (candidate == IntPtr.Zero || !IsWindowVisible(candidate)) return true;
                view = candidate;
                return false;
            }, IntPtr.Zero);
            return view;
        }

        internal static IntPtr DesktopHost(IntPtr view) { return view == IntPtr.Zero ? IntPtr.Zero : GetParent(view); }
        internal static IntPtr IconList(IntPtr view)
        {
            return view == IntPtr.Zero ? IntPtr.Zero : FindWindowEx(view, IntPtr.Zero, "SysListView32", null);
        }

        internal static bool IconListHasVisibleStyle(IntPtr list) { return (GetWindowLong(list, StyleIndex) & Visible) != 0; }
        internal static void SetIconListVisible(IntPtr list, bool visible) { ShowWindowAsync(list, visible ? 8 : 0); }

        internal static bool IsDesktopBackground(Point point, IntPtr host)
        {
            if (host == IntPtr.Zero) return false;
            var hit = WindowFromPoint(new NativePoint(point.X, point.Y));
            if (hit == IntPtr.Zero) return false;
            if (hit == host || IsChild(host, hit)) return true;

            // Explorer may put the wallpaper in a separate desktop worker.
            var root = GetAncestor(hit, 2); // GA_ROOT
            if (root == IntPtr.Zero) return false;
            uint hostProcess, hitProcess;
            GetWindowThreadProcessId(host, out hostProcess);
            GetWindowThreadProcessId(root, out hitProcess);
            if (hostProcess == 0 || hostProcess != hitProcess) return false;
            var className = new StringBuilder(64);
            GetClassName(root, className, className.Capacity);
            string name = className.ToString();
            return name == "WorkerW" || name == "Progman";
        }

        internal static void SetPanelOpacity(IntPtr window, int percent)
        {
            int style = GetWindowLong(window, ExStyleIndex);
            if (percent == 100)
            {
                if ((style & Layered) != 0)
                {
                    SetWindowLong(window, ExStyleIndex, style & ~Layered);
                    SetWindowPos(window, IntPtr.Zero, 0, 0, 0, 0, NoMove | NoSize | NoZOrder | NoActivate | FrameChanged);
                }
                return;
            }
            if ((style & Layered) == 0) SetWindowLong(window, ExStyleIndex, style | Layered);
            SetLayeredWindowAttributes(window, 0, (byte)(percent * 255 / 100), 2);
        }

        internal static void SetIconTransparency(IntPtr window, Color colorKey)
        {
            int style = GetWindowLong(window, ExStyleIndex);
            if ((style & Layered) == 0) SetWindowLong(window, ExStyleIndex, style | Layered);
            uint color = (uint)(colorKey.R | colorKey.G << 8 | colorKey.B << 16);
            SetLayeredWindowAttributes(window, color, 255, 1);
        }
        internal static int Dpi(IntPtr window)
        {
            uint dpi = window == IntPtr.Zero ? GetDpiForSystem() : GetDpiForWindow(window);
            return dpi != 0 ? (int)dpi : 96;
        }

        internal static Rectangle Bounds(IntPtr window)
        {
            NativeRect rect;
            return GetWindowRect(window, out rect)
                ? Rectangle.FromLTRB(rect.Left, rect.Top, rect.Right, rect.Bottom) : Rectangle.Empty;
        }

        internal static bool Mount(IntPtr window, IntPtr host, bool peek, Rectangle bounds)
        {
            int style = GetWindowLong(window, StyleIndex);
            if (host != IntPtr.Zero)
            {
                SetWindowPos(window, NotTopmost, 0, 0, 0, 0, NoMove | NoSize | NoActivate);
                SetWindowLong(window, StyleIndex, (style & ~Popup) | Child);
                if (GetParent(window) != host) SetParent(window, host);
                if (GetParent(window) != host)
                {
                    SetWindowLong(window, StyleIndex, (style & ~Child) | Popup);
                    return false;
                }
            }
            else
            {
                if ((style & Child) != 0) SetParent(window, IntPtr.Zero);
                SetWindowLong(window, StyleIndex, (style & ~Child) | Popup);
            }
            Position(window, host, bounds, true, peek);
            return true;
        }

        internal static void Position(IntPtr window, IntPtr host, Rectangle bounds, bool raise, bool peek)
        {
            var point = new NativePoint(bounds.X, bounds.Y);
            if (host != IntPtr.Zero) ScreenToClient(host, ref point);
            uint flags = NoActivate | NoOwnerOrder | (raise ? FrameChanged : NoZOrder);
            SetWindowPos(window, peek ? Topmost : IntPtr.Zero, point.X, point.Y, bounds.Width, bounds.Height, flags);
        }

        internal static void Raise(IntPtr window, bool peek)
        {
            SetWindowPos(window, peek ? Topmost : IntPtr.Zero, 0, 0, 0, 0, NoMove | NoSize | NoActivate | NoOwnerOrder);
        }

        internal static void MoveOverlay(IntPtr window, Point location)
        {
            SetWindowPos(window, IntPtr.Zero, location.X, location.Y, 0, 0, NoSize | NoZOrder | NoActivate | NoOwnerOrder);
        }

        internal static int OpenPath(string path)
        {
            var info = new ShellExecuteInfo
            {
                Size = Marshal.SizeOf(typeof(ShellExecuteInfo)), Mask = 0x400 | 0x100, FileName = path, Show = 1,
                Directory = String.Equals(Path.GetExtension(path), ".exe", StringComparison.OrdinalIgnoreCase)
                    ? Path.GetDirectoryName(path) : null
            };
            if (ShellExecuteEx(ref info)) return 0;
            int error = Marshal.GetLastWin32Error();
            return error != 0 ? error : info.Instance.ToInt64() == 31 ? 1155 : 31;
        }

        internal static Icon GetIcon(FileEntry entry, bool resolvePath)
        {
            FileInfo info;
            const uint icon = 0x100;
            const uint useAttributes = 0x10;
            const uint large = 0;
            uint attributes = entry.IsDirectory ? 0x10u : 0x80u;
            string path = resolvePath ? entry.Path : (entry.IsDirectory ? "folder" : "item" + entry.Extension);
            if (SHGetFileInfo(path, attributes, out info, (uint)Marshal.SizeOf(typeof(FileInfo)), icon | large | (resolvePath ? 0u : useAttributes)) == IntPtr.Zero ||
                info.Icon == IntPtr.Zero)
                return null;
            return CopyShellIcon(info.Icon);
        }

        internal static Icon GetRecycleBinIcon()
        {
            const int recycleBin = 31;
            const uint iconHandle = 0x100;
            var info = new StockIconInfo { Size = (uint)Marshal.SizeOf(typeof(StockIconInfo)) };
            return SHGetStockIconInfo(recycleBin, iconHandle, ref info) >= 0 && info.Icon != IntPtr.Zero
                ? CopyShellIcon(info.Icon) : null;
        }

        private static Icon CopyShellIcon(IntPtr icon)
        {
            try
            {
                using (var nativeIcon = Icon.FromHandle(icon)) return (Icon)nativeIcon.Clone();
            }
            finally { DestroyIcon(icon); }
        }

        internal static bool ExplorerPrefersHiddenIcons()
        {
            using (var key = Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced"))
                return key != null && Object.Equals(key.GetValue("HideIcons"), 1);
        }
    }

    internal sealed class ShellIcons : IDisposable
    {
        private readonly Dictionary<string, Icon> cache = new Dictionary<string, Icon>(StringComparer.OrdinalIgnoreCase);
        private readonly Queue<string> order = new Queue<string>();
        private readonly Dictionary<Icon, Bitmap> bitmaps = new Dictionary<Icon, Bitmap>();

        internal Icon For(FileEntry entry)
        {
            bool pathIcon = !entry.Path.StartsWith(@"\\", StringComparison.Ordinal) &&
                (String.Equals(entry.Extension, ".lnk", StringComparison.OrdinalIgnoreCase) ||
                 String.Equals(entry.Extension, ".exe", StringComparison.OrdinalIgnoreCase) ||
                 String.Equals(entry.Extension, ".url", StringComparison.OrdinalIgnoreCase));
            string key = entry.IsRecycleBin ? entry.Path : pathIcon ? entry.Path + "|" + entry.ModifiedUtc.Ticks :
                entry.IsDirectory ? "<folder>" : entry.Extension;
            Icon icon;
            if (cache.TryGetValue(key, out icon)) return icon ?? SystemIcons.Application;
            if (order.Count == 320)
            {
                string oldest = order.Dequeue();
                var expired = cache[oldest];
                if (expired != null)
                {
                    Bitmap bitmap;
                    if (bitmaps.TryGetValue(expired, out bitmap))
                    {
                        bitmap.Dispose();
                        bitmaps.Remove(expired);
                    }
                    expired.Dispose();
                }
                cache.Remove(oldest);
            }
            icon = entry.IsRecycleBin ? WindowsShell.GetRecycleBinIcon() : WindowsShell.GetIcon(entry, pathIcon);
            if (icon == null && pathIcon) icon = WindowsShell.GetIcon(entry, false);
            cache.Add(key, icon);
            order.Enqueue(key);
            return icon ?? SystemIcons.Application;
        }

        internal void Draw(Graphics graphics, FileEntry entry, Rectangle destination, ImageAttributes tint)
        {
            var icon = For(entry);
            if (tint == null) { graphics.DrawIcon(icon, destination); return; }
            Bitmap bitmap;
            if (!bitmaps.TryGetValue(icon, out bitmap))
            {
                bitmap = icon.ToBitmap();
                bitmaps.Add(icon, bitmap);
            }
            graphics.DrawImage(bitmap, destination, 0, 0, bitmap.Width, bitmap.Height, GraphicsUnit.Pixel, tint);
        }

        public void Dispose()
        {
            foreach (var bitmap in bitmaps.Values) bitmap.Dispose();
            bitmaps.Clear();
            foreach (var icon in cache.Values) if (icon != null) icon.Dispose();
            cache.Clear();
            order.Clear();
        }
    }

    internal sealed class DesktopDoubleClick : IDisposable
    {
        private const int LeftDown = 0x0201;
        internal const int ToggleMessage = 0x8000 + 71;
        private readonly IntPtr receiver;
        private readonly HookProc callback;
        private readonly IntPtr hook;
        private readonly uint delay = GetDoubleClickTime();
        private readonly Size tolerance = SystemInformation.DoubleClickSize;
        private IntPtr view;
        private IntPtr host;
        private bool listening;
        private Rectangle[] panels = new Rectangle[0];
        private Point lastPoint;
        private uint lastTime;
        private bool haveClick;

        [StructLayout(LayoutKind.Sequential)]
        private struct MouseEvent
        {
            internal int X, Y;
            internal uint MouseData, Flags, Time;
            internal IntPtr Extra;
        }

        private delegate IntPtr HookProc(int code, IntPtr message, IntPtr data);
        [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern IntPtr SetWindowsHookEx(int kind, HookProc callback, IntPtr module, uint thread);
        [DllImport("user32.dll")] private static extern IntPtr CallNextHookEx(IntPtr hook, int code, IntPtr message, IntPtr data);
        [DllImport("user32.dll")] private static extern bool UnhookWindowsHookEx(IntPtr hook);
        [DllImport("user32.dll")] private static extern uint GetDoubleClickTime();
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern bool PostMessage(IntPtr window, int message, IntPtr parameter, IntPtr extra);
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] private static extern IntPtr GetModuleHandle(string name);

        internal DesktopDoubleClick(IntPtr receiver)
        {
            this.receiver = receiver;
            callback = Observe;
            hook = SetWindowsHookEx(14, callback, GetModuleHandle(null), 0);
        }

        internal void Update(IntPtr desktopView, IntPtr desktopHost, bool enabled, Rectangle[] occupied)
        {
            if (desktopView != view || enabled != listening) haveClick = false;
            view = desktopView;
            host = desktopHost;
            panels = occupied;
            listening = enabled && view != IntPtr.Zero;
        }

        private IntPtr Observe(int code, IntPtr message, IntPtr data)
        {
            if (code >= 0 && listening && message.ToInt64() == LeftDown)
            {
                var mouse = (MouseEvent)Marshal.PtrToStructure(data, typeof(MouseEvent));
                var point = new Point(mouse.X, mouse.Y);
                if (Array.Exists(panels, panel => panel.Contains(point)) ||
                    !WindowsShell.IsDesktopBackground(point, host)) haveClick = false;
                else if (haveClick && unchecked(mouse.Time - lastTime) <= delay &&
                    Math.Abs(point.X - lastPoint.X) <= tolerance.Width / 2 && Math.Abs(point.Y - lastPoint.Y) <= tolerance.Height / 2)
                {
                    haveClick = false;
                    PostMessage(receiver, ToggleMessage, IntPtr.Zero, IntPtr.Zero);
                }
                else
                {
                    haveClick = true;
                    lastTime = mouse.Time;
                    lastPoint = point;
                }
            }
            return CallNextHookEx(hook, code, message, data);
        }

        public void Dispose()
        {
            if (hook != IntPtr.Zero) UnhookWindowsHookEx(hook);
        }
    }

    internal sealed class IconVisibility
    {
        private readonly string recoveryPath;
        private readonly string instanceKey;
        private Process watchdog;
        private IntPtr hidden;

        internal IconVisibility(string dataFolder, string instanceKey)
        {
            this.instanceKey = instanceKey;
            using (var process = Process.GetCurrentProcess())
                recoveryPath = Path.Combine(dataFolder, "desktop-icons-" + process.SessionId + ".txt");
        }

        internal void Update(IntPtr list, bool hide)
        {
            if (hidden != IntPtr.Zero && (!hide || list != hidden)) Restore();
            if (!hide || list == IntPtr.Zero || WindowsShell.ExplorerPrefersHiddenIcons()) return;
            if (!WindowsShell.IconListHasVisibleStyle(list)) return;
            if (watchdog == null || watchdog.HasExited)
            {
                if (watchdog != null) watchdog.Dispose();
                using (var process = Process.GetCurrentProcess())
                    watchdog = Process.Start(new ProcessStartInfo(Application.ExecutablePath,
                        "--restore-icons " + process.Id + " " + process.StartTime.ToUniversalTime().Ticks + " " + instanceKey)
                    { UseShellExecute = false, CreateNoWindow = true });
            }
            if (watchdog == null) throw new InvalidOperationException("无法启动桌面图标恢复进程。");
            Directory.CreateDirectory(Path.GetDirectoryName(recoveryPath));
            File.WriteAllText(recoveryPath, list.ToInt64().ToString());
            hidden = list;
            WindowsShell.SetIconListVisible(list, false);
        }

        internal void Restore()
        {
            RestoreFromDisk(recoveryPath);
            hidden = IntPtr.Zero;
        }

        internal void Close()
        {
            Restore();
            if (watchdog != null) watchdog.Dispose();
        }

        internal static void WatchParent(int pid, long startedAt, string dataFolder, string instanceKey)
        {
            int session;
            using (var self = Process.GetCurrentProcess()) session = self.SessionId;
            try
            {
                using (var parent = Process.GetProcessById(pid))
                {
                    if (parent.SessionId == session && parent.StartTime.ToUniversalTime().Ticks == startedAt)
                        parent.WaitForExit();
                }
            }
            catch (ArgumentException) { }
            using (var exclusion = new Mutex(false, instanceKey))
            {
                try { if (!exclusion.WaitOne(0)) return; }
                catch (AbandonedMutexException) { }
                try { RestoreFromDisk(Path.Combine(dataFolder, "desktop-icons-" + session + ".txt")); }
                finally { exclusion.ReleaseMutex(); }
            }
        }

        private static void RestoreFromDisk(string path)
        {
            if (!File.Exists(path)) return;
            long value;
            var list = WindowsShell.IconList(WindowsShell.DesktopView());
            if (list != IntPtr.Zero && Int64.TryParse(File.ReadAllText(path), out value) &&
                list.ToInt64() == value && !WindowsShell.ExplorerPrefersHiddenIcons())
                WindowsShell.SetIconListVisible(list, true);
            File.Delete(path);
        }
    }
}
