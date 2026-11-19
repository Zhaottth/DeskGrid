using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Windows.Forms;
using Microsoft.Win32;

[assembly: AssemblyTitle("桌面格 · DeskGrid")]
[assembly: AssemblyDescription("Windows 桌面分组与文件夹面板")]
[assembly: AssemblyProduct("DeskGrid")]
[assembly: AssemblyFileVersion("1.3.0.0")]

namespace DeskGrid
{
    internal static class Program
    {
        [STAThread]
        private static void Main(string[] args)
        {
            var alternateData = Environment.GetEnvironmentVariable("DESKGRID_DATA_DIR");
            string data = Path.GetFullPath(alternateData ?? Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "DeskGrid"));
            string key;
            using (var hash = SHA256.Create())
                key = @"Local\DeskGrid-" + Convert.ToBase64String(hash.ComputeHash(Encoding.UTF8.GetBytes(data.ToUpperInvariant())))
                    .Substring(0, 16).Replace('/', '_');
            if (args.Length == 4 && args[0] == "--restore-icons")
            {
                int pid;
                long start;
                if (Int32.TryParse(args[1], out pid) && Int64.TryParse(args[2], out start) && key == args[3])
                {
                    try { IconVisibility.WatchParent(pid, start, data, key); }
                    catch (Exception error)
                    {
                        if (!FileEntry.ExpectedIoError(error) && !(error is Win32Exception)) throw;
                    }
                }
                return;
            }
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            using (var notification = new EventWaitHandle(false, EventResetMode.AutoReset, key + "-open"))
            using (var instance = new Mutex(false, key))
            {
                try
                {
                    if (!instance.WaitOne(0)) { notification.Set(); return; }
                }
                catch (AbandonedMutexException) { }
                try
                {
                    using (var app = new DesktopApp(data, key, notification))
                    {
                        app.Start(args.Contains("--autostart"));
                        Application.Run(app);
                    }
                }
                finally { instance.ReleaseMutex(); }
            }
        }
    }

    internal sealed class AppMessages : NativeWindow, IDisposable
    {
        private const uint Modifiers = 0x4003; // Ctrl + Alt, with no key repeat.
        private readonly Action<int> hotkey;
        internal readonly bool HideWorks;
        internal readonly bool PeekWorks;
        internal readonly bool SearchWorks;
        internal readonly bool PageUpWorks;
        internal readonly bool PageDownWorks;

        internal AppMessages(Action<int> hotkey)
        {
            this.hotkey = hotkey;
            CreateHandle(new CreateParams { Caption = "DeskGrid message window", Parent = new IntPtr(-3) });
            HideWorks = WindowsShell.RegisterHotKey(Handle, 1, Modifiers, (uint)Keys.H);
            PeekWorks = WindowsShell.RegisterHotKey(Handle, 2, Modifiers, (uint)Keys.Space);
            SearchWorks = WindowsShell.RegisterHotKey(Handle, 3, Modifiers, (uint)Keys.F);
            PageUpWorks = WindowsShell.RegisterHotKey(Handle, 4, Modifiers, (uint)Keys.PageUp);
            PageDownWorks = WindowsShell.RegisterHotKey(Handle, 5, Modifiers, (uint)Keys.PageDown);
        }

        protected override void WndProc(ref Message message)
        {
            if (message.Msg == WindowsShell.HotkeyMessage) hotkey(message.WParam.ToInt32());
            else if (message.Msg == DesktopDoubleClick.ToggleMessage) hotkey(1);
            else base.WndProc(ref message);
        }

        public void Dispose()
        {
            if (HideWorks) WindowsShell.UnregisterHotKey(Handle, 1);
            if (PeekWorks) WindowsShell.UnregisterHotKey(Handle, 2);
            if (SearchWorks) WindowsShell.UnregisterHotKey(Handle, 3);
            if (PageUpWorks) WindowsShell.UnregisterHotKey(Handle, 4);
            if (PageDownWorks) WindowsShell.UnregisterHotKey(Handle, 5);
            DestroyHandle();
        }
    }

    internal sealed class DesktopApp : ApplicationContext
    {
        private const string RunKey = @"Software\Microsoft\Windows\CurrentVersion\Run";
        private readonly string settingsPath;
        private readonly string[] desktopFolders;
        private readonly bool fixture;
        private readonly EventWaitHandle notification;
        private readonly IconVisibility iconVisibility;
        private readonly DirectoryFeed directories = new DirectoryFeed();
        private readonly Dictionary<string, GroupWindow> windows = new Dictionary<string, GroupWindow>();
        private readonly Dictionary<string, GroupWindow> looseWindows = new Dictionary<string, GroupWindow>(StringComparer.OrdinalIgnoreCase);
        private readonly System.Windows.Forms.Timer timer = new System.Windows.Forms.Timer { Interval = 500 };
        private readonly NotifyIcon tray;
        private readonly AppMessages messages;
        private readonly DesktopDoubleClick desktopDoubleClick;
        private readonly Icon trayIcon;
        private readonly ToolStripMenuItem hideItem;
        private readonly ToolStripMenuItem peekItem;
        private readonly ToolStripMenuItem nativeItem;
        private readonly ToolStripMenuItem pagesItem;
        private SettingsWindow settings;
        private SearchWindow search;
        private IntPtr host;
        private IntPtr desktopView;
        private IntPtr iconList;
        private Rectangle desktopBounds;
        private bool hidden;
        private bool peek;
        private bool closing;
        private bool disposed;
        private bool saveProblem;
        private bool iconProblem;
        private bool loadedBackup;

        internal DesktopLayout Layout { get; private set; }
        internal readonly ShellIcons Icons = new ShellIcons();

        internal DesktopApp(string data, string key, EventWaitHandle notification)
        {
            this.notification = notification;
            settingsPath = Path.Combine(data, "layout.xml");
            var testDesktop = Environment.GetEnvironmentVariable("DESKGRID_DESKTOP_DIR");
            fixture = !String.IsNullOrEmpty(testDesktop);
            desktopFolders = (fixture ? new[] { FileEntry.FullPath(testDesktop) } : new[]
            {
                Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory),
                Environment.GetFolderPath(Environment.SpecialFolder.CommonDesktopDirectory)
            }).Where(path => !String.IsNullOrEmpty(path)).Distinct(StringComparer.OrdinalIgnoreCase).ToArray();
            iconVisibility = new IconVisibility(data, key);
            Layout = Load();

            trayIcon = Icon.ExtractAssociatedIcon(Application.ExecutablePath);
            messages = new AppMessages(HandleHotkey);
            desktopDoubleClick = new DesktopDoubleClick(messages.Handle);
            var menu = Theme.Menu();
            menu.Items.Add("管理桌面…", null, delegate { ShowSettings(); });
            menu.Items.Add("新建分组…", null, delegate { NewGroup(false); });
            menu.Items.Add("添加文件夹入口…", null, delegate { NewGroup(true); });
            menu.Items.Add("搜索面板…" + (messages.SearchWorks ? "\tCtrl+Alt+F" : ""), null, delegate { ShowSearch(null); });
            pagesItem = (ToolStripMenuItem)menu.Items.Add("桌面页面");
            pagesItem.DropDownItems.Add("管理页面…", null, delegate { EditPages(); });
            pagesItem.DropDownOpening += delegate
            {
                var previous = pagesItem.DropDownItems.Cast<ToolStripItem>().ToArray();
                pagesItem.DropDownItems.Clear();
                foreach (var item in previous) item.Dispose();
                foreach (var page in Layout.Pages)
                {
                    var item = new ToolStripMenuItem(page.Title, null, delegate { SwitchPage(page.Id); })
                    {
                        Checked = page.Id == Layout.ActivePageId, ForeColor = Theme.Text
                    };
                    pagesItem.DropDownItems.Add(item);
                }
                pagesItem.DropDownItems.Add(new ToolStripSeparator());
                pagesItem.DropDownItems.Add("管理页面…", null, delegate { EditPages(); }).ForeColor = Theme.Text;
            };
            menu.Items.Add(new ToolStripSeparator());
            hideItem = (ToolStripMenuItem)menu.Items.Add("隐藏所有面板", null, delegate { ToggleHidden(); });
            peekItem = (ToolStripMenuItem)menu.Items.Add("将面板显示在最前", null, delegate { TogglePeek(); });
            nativeItem = (ToolStripMenuItem)menu.Items.Add("隐藏 Windows 原生桌面图标", null, delegate { SetNativeIconsHidden(!Layout.HideWindowsIcons); });
            nativeItem.Enabled = !fixture;
            menu.Items.Add("刷新文件", null, delegate { RefreshContents(); });
            var systemFolders = new ToolStripMenuItem("Windows 系统文件夹") { ForeColor = Theme.Text };
            systemFolders.DropDownItems.Add("回收站", null, delegate { OpenSystemFolder(FileEntry.RecycleBinFolder); }).ForeColor = Theme.Text;
            systemFolders.DropDownItems.Add("此电脑", null, delegate { OpenSystemFolder("MyComputerFolder"); }).ForeColor = Theme.Text;
            menu.Items.Add(systemFolders);
            menu.Items.Add(new ToolStripSeparator());
            menu.Items.Add("退出桌面格", null, delegate { ExitThread(); });
            tray = new NotifyIcon { Icon = trayIcon, Text = "桌面格 DeskGrid", ContextMenuStrip = menu };
            tray.MouseDoubleClick += delegate(object sender, MouseEventArgs e) { if (e.Button == MouseButtons.Left) ShowSettings(); };
            timer.Tick += delegate { Tick(); };
        }

        private void HandleHotkey(int id)
        {
            switch (id)
            {
                case 1: ToggleHidden(); break;
                case 2: TogglePeek(); break;
                case 3: ShowSearch(null); break;
                case 4: StepPage(-1); break;
                case 5: StepPage(1); break;
            }
        }

        internal void Start(bool quiet)
        {
            UpdateIconVisibility(true);
            tray.Visible = true;
            Tick();
            timer.Start();
            if (!quiet && loadedBackup) Notify("已从备份或默认配置恢复布局。");
            else if (!File.Exists(settingsPath))
            {
                Save();
                if (!quiet) Notify("已按类型整理桌面。双击桌面空白处可隐藏，双击托盘图标可管理分组。");
            }
        }

        private DesktopLayout Load()
        {
            foreach (string path in new[] { settingsPath, settingsPath + ".bak" })
            {
                if (!File.Exists(path)) continue;
                try { return LayoutStore.Read(path); }
                catch (Exception error)
                {
                    if (!IsLayoutError(error)) throw;
                    loadedBackup = true;
                }
            }
            return DesktopLayout.CreateDefault(Screen.PrimaryScreen.WorkingArea, WindowsShell.Dpi(IntPtr.Zero));
        }

        private static bool IsLayoutError(Exception error)
        {
            return FileEntry.ExpectedIoError(error) || error is InvalidOperationException || error is System.Xml.XmlException;
        }

        internal void Save()
        {
            try
            {
                LayoutStore.Write(settingsPath, Layout, !loadedBackup);
                loadedBackup = false;
                saveProblem = false;
            }
            catch (Exception error)
            {
                if (!IsLayoutError(error)) throw;
                if (!saveProblem) Notify("布局保存失败。\n" + error.Message);
                saveProblem = true;
            }
        }

        internal Rectangle FitBounds(Rectangle proposed, Point? cursor)
        {
            var screen = cursor.HasValue ? Screen.FromPoint(cursor.Value) : Screen.FromRectangle(proposed);
            var area = screen.WorkingArea;
            int dpi = WindowsShell.Dpi(host);
            int width = Math.Min(area.Width, Math.Max(220 * dpi / 96, proposed.Width));
            int height = Math.Min(area.Height, Math.Max(138 * dpi / 96, proposed.Height));
            return new Rectangle(
                Math.Max(area.Left, Math.Min(proposed.X, area.Right - width)),
                Math.Max(area.Top, Math.Min(proposed.Y, area.Bottom - height)),
                width, height);
        }

        internal Rectangle SnapMove(DesktopGroup frame, Rectangle proposed, Point cursor)
        {
            var result = FitBounds(proposed, cursor);
            if ((Control.ModifierKeys & Keys.Alt) != 0) return result;
            var area = Screen.FromPoint(cursor).WorkingArea;
            int tolerance = 10 * WindowsShell.Dpi(host) / 96;
            int gap = 12 * WindowsShell.Dpi(host) / 96;
            var horizontal = new List<int> { area.Left - result.Left, area.Right - result.Right };
            var vertical = new List<int> { area.Top - result.Top, area.Bottom - result.Bottom };
            foreach (var neighbor in Layout.Groups.Where(group => group != frame && group.TabHostId == null &&
                Layout.OnPage(group, Layout.ActivePageId) && area.IntersectsWith(group.Bounds)))
            {
                var bounds = neighbor.Bounds;
                horizontal.Add(bounds.Left - result.Left);
                horizontal.Add(bounds.Right - result.Right);
                vertical.Add(bounds.Top - result.Top);
                vertical.Add(bounds.Bottom - result.Bottom);
                if (bounds.Bottom > result.Top && bounds.Top < result.Bottom)
                {
                    horizontal.Add(bounds.Right + gap - result.Left);
                    horizontal.Add(bounds.Left - gap - result.Right);
                }
                if (bounds.Right > result.Left && bounds.Left < result.Right)
                {
                    vertical.Add(bounds.Bottom + gap - result.Top);
                    vertical.Add(bounds.Top - gap - result.Bottom);
                }
            }
            result.Offset(NearestSnap(horizontal, tolerance), NearestSnap(vertical, tolerance));
            return FitBounds(result, cursor);
        }

        private static int NearestSnap(IEnumerable<int> distances, int tolerance)
        {
            return distances.Where(distance => Math.Abs(distance) <= tolerance).OrderBy(distance => Math.Abs(distance)).FirstOrDefault();
        }

        private void Tick()
        {
            if (closing) return;
            if (notification.WaitOne(0)) ShowSettings();
            if (closing) return;
            desktopView = WindowsShell.DesktopView();
            var nextHost = WindowsShell.DesktopHost(desktopView);
            var nextBounds = WindowsShell.Bounds(nextHost);
            iconList = WindowsShell.IconList(desktopView);
            bool desktopChanged = nextHost != host || nextBounds != desktopBounds;
            host = nextHost;
            desktopBounds = nextBounds;
            SynchronizeWindows(desktopChanged);
            UpdateIconVisibility(false);
            UpdateContents(desktopChanged);
            UpdateTray();
        }

        private void SynchronizeWindows(bool reapply)
        {
            foreach (var group in Layout.Groups)
            {
                GroupWindow panel;
                if (!windows.TryGetValue(group.Id, out panel) || !panel.Alive)
                {
                    if (panel != null) panel.Dispose();
                    if (group.TabHostId == null) group.Bounds = FitBounds(group.Bounds, null);
                    panel = new GroupWindow(this, group);
                    // Materialize hidden handles so a hidden layout is retained between ticks.
                    GC.KeepAlive(panel.Handle);
                    windows[group.Id] = panel;
                    panel.ApplyDesktopState(host, peek, IsPanelShown(group));
                }
                else if (reapply)
                {
                    if (group.TabHostId == null) group.Bounds = FitBounds(group.Bounds, null);
                    panel.ApplyDesktopState(host, peek, IsPanelShown(group));
                }
            }
        }

        private bool IsPanelShown(DesktopGroup group)
        {
            return (!hidden || peek) && Layout.IsActiveTab(group) && Layout.OnPage(group, Layout.ActivePageId);
        }

        private void UpdateContents(bool reapplyLoose = false)
        {
            var active = new HashSet<string>(desktopFolders, StringComparer.OrdinalIgnoreCase);
            var desktop = new Dictionary<string, FileEntry>(StringComparer.OrdinalIgnoreCase);
            bool loadingDesktop = false;
            bool haveReadableDesktop = false;
            foreach (var path in desktopFolders)
            {
                var result = directories.Get(path);
                if (result == null) { loadingDesktop = true; continue; }
                if (!result.Unavailable) haveReadableDesktop = true;
                foreach (var item in result.Entries) desktop[item.Path] = item;
            }

            var pinned = new List<FileEntry>();
            foreach (var placement in Layout.Placements.Where(placement => placement.Pinned && !desktop.ContainsKey(placement.Path)))
            {
                active.Add(DirectoryFeed.PinnedKey(placement.Path));
                var result = directories.GetPinned(placement.Path);
                if (result != null) pinned.AddRange(result.Entries);
            }
            var groups = Layout.Route(desktop.Values, pinned, DateTime.UtcNow);
            var matches = search != null && !search.IsDisposed && search.Visible ? new List<PanelSearchResult>() : null;
            foreach (var panel in windows.Values)
            {
                if (panel.Group.IsPortal)
                {
                    active.Add(panel.Folder);
                    var result = directories.Get(panel.Folder);
                    panel.SetItems(result == null ? new FileEntry[0] : FileEntry.Sort(result.Entries, panel.Group.Sort).ToArray(),
                        result == null, result != null && result.Unavailable);
                    if (matches != null)
                    {
                        AddSearchEntries(matches, panel.Group, panel.Folder, result);
                        if (!SamePath(panel.Folder, panel.Group.Folder))
                        {
                            active.Add(panel.Group.Folder);
                            AddSearchEntries(matches, panel.Group, panel.Group.Folder, directories.Get(panel.Group.Folder));
                        }
                    }
                }
                else
                {
                    var items = panel.Group.OrderedItems(groups[panel.Group.Id]);
                    if (panel.Group.Id == Layout.DefaultGroupId) items = new[] { FileEntry.RecycleBin }.Concat(items);
                    var sorted = items.ToArray();
                    panel.SetItems(sorted, loadingDesktop, !loadingDesktop && !haveReadableDesktop);
                    if (matches != null)
                        matches.AddRange(sorted.Select(item => new PanelSearchResult(panel.Group, item, null)));
                }
            }
            foreach (var item in pinned) desktop[item.Path] = item;
            UpdateLooseWindows(desktop, matches, reapplyLoose);
            UpdateQuickHideTargets();
            directories.Retain(active);
            if (matches != null) search.UpdateSource(matches);
        }

        private void UpdateLooseWindows(Dictionary<string, FileEntry> entries, List<PanelSearchResult> matches, bool reapply)
        {
            var active = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            foreach (var placement in Layout.Placements.Where(item => item.Loose))
            {
                FileEntry entry;
                if (!entries.TryGetValue(placement.Path, out entry)) continue;
                active.Add(placement.Path);
                GroupWindow panel;
                var bounds = LooseBounds(placement);
                bool show = reapply;
                if (!looseWindows.TryGetValue(placement.Path, out panel) || !panel.Alive)
                {
                    if (panel != null) panel.Dispose();
                    var tile = DesktopGroup.Create(entry.Label, 0, bounds);
                    panel = new GroupWindow(this, tile, true);
                    looseWindows[placement.Path] = panel;
                    show = true;
                }
                if (panel.Group.Bounds != bounds) { panel.Group.Bounds = bounds; panel.UpdateGroup(); }
                panel.SetItems(new[] { entry }, false, false);
                if (show) panel.ApplyDesktopState(host, peek, !hidden || peek);
                if (matches != null) matches.Add(new PanelSearchResult(null, entry, null));
            }
            foreach (var path in looseWindows.Keys.Where(path => !active.Contains(path)).ToArray())
            {
                if (looseWindows[path].DraggingFile) continue;
                looseWindows[path].Dispose();
                looseWindows.Remove(path);
            }
        }

        private Rectangle LooseBounds(Placement placement)
        {
            var point = new Point(placement.Left, placement.Top);
            var area = Screen.FromPoint(point).WorkingArea;
            int dpi = WindowsShell.Dpi(host);
            int width = Math.Min(area.Width, 88 * dpi / 96), height = Math.Min(area.Height, 76 * dpi / 96);
            return new Rectangle(Math.Max(area.Left, Math.Min(area.Right - width, point.X)),
                Math.Max(area.Top, Math.Min(area.Bottom - height, point.Y)), width, height);
        }

        internal bool IsWallpaperDrop(Point point)
        {
            return !closing && desktopView != IntPtr.Zero && Layout.HideWindowsIcons &&
                (iconList == IntPtr.Zero || !WindowsShell.IconListHasVisibleStyle(iconList)) &&
                !windows.Values.Concat(looseWindows.Values).Any(panel => panel.VisibleScreenBounds.Contains(point)) &&
                WindowsShell.IsDesktopBackground(point, host);
        }

        internal void UpdateQuickHideTargets()
        {
            if (closing) return;
            bool enabled = Layout.QuickHideOnDoubleClick &&
                (iconList == IntPtr.Zero || !WindowsShell.IconListHasVisibleStyle(iconList));
            var occupied = enabled ? windows.Values.Concat(looseWindows.Values).Select(panel => panel.VisibleScreenBounds)
                .Where(bounds => !bounds.IsEmpty).ToArray() : new Rectangle[0];
            desktopDoubleClick.Update(desktopView, host, enabled, occupied);
        }

        internal void PlaceLoose(FileEntry item, Point point)
        {
            int dpi = WindowsShell.Dpi(host);
            Layout.Placements.RemoveAll(placement => SamePath(placement.Path, item.Path));
            Layout.Placements.Add(new Placement
            {
                Path = item.Path, Loose = true, Pinned = !IsDesktopItem(item.Path),
                Left = point.X - 44 * dpi / 96, Top = point.Y - 22 * dpi / 96
            });
            Save();
            RefreshContents();
        }

        private static void AddSearchEntries(List<PanelSearchResult> results, DesktopGroup group, string folder, DirectoryResult directory)
        {
            if (directory != null) results.AddRange(directory.Entries.Select(item => new PanelSearchResult(group, item, folder)));
        }

        internal void RefreshContents()
        {
            directories.Refresh();
            UpdateContents();
        }

        private void ReapplyWindows()
        {
            SynchronizeWindows(true);
            foreach (var tile in looseWindows.Values) tile.ApplyDesktopState(host, peek, !hidden || peek);
            UpdateQuickHideTargets();
            UpdateTray();
        }

        private void ToggleHidden()
        {
            if (peek) { peek = false; hidden = true; }
            else hidden = !hidden;
            ReapplyWindows();
        }

        private void TogglePeek()
        {
            peek = !peek;
            ReapplyWindows();
        }

        private void UpdateTray()
        {
            hideItem.Text = (!hidden || peek ? "隐藏所有面板" : "显示所有面板") + (messages.HideWorks ? "\tCtrl+Alt+H" : "");
            peekItem.Text = (peek ? "退出置顶预览" : "将面板显示在最前") + (messages.PeekWorks ? "\tCtrl+Alt+Space" : "");
            nativeItem.Checked = Layout.HideWindowsIcons;
            var page = Layout.Pages.First(item => item.Id == Layout.ActivePageId);
            pagesItem.Text = "桌面页面 · " + page.Title;
        }

        internal bool CanHideWindowsIcons { get { return !fixture; } }
        internal bool CanStartWithWindows { get { return !fixture; } }
        internal string SearchHelp
        {
            get
            {
                string searchKey = messages.SearchWorks ? "Ctrl+Alt+F  搜索面板" : "从托盘打开搜索";
                return searchKey + (messages.PageUpWorks && messages.PageDownWorks ? "       Ctrl+Alt+PgUp / PgDn  切换页面" : "");
            }
        }
        internal string HotkeyHelp
        {
            get
            {
                return (messages.HideWorks ? "Ctrl+Alt+H  隐藏 / 显示面板" : "隐藏快捷键已被占用") + "       " +
                    (messages.PeekWorks ? "Ctrl+Alt+空格  置顶预览" : "预览快捷键已被占用");
            }
        }

        internal void SetNativeIconsHidden(bool hide)
        {
            if (fixture) return;
            Layout.HideWindowsIcons = hide;
            UpdateIconVisibility(false);
            Save();
            UpdateTray();
            if (settings != null) settings.RefreshOptions();
        }

        internal void SetDesktopQuickHide(bool enabled)
        {
            Layout.QuickHideOnDoubleClick = enabled;
            UpdateQuickHideTargets();
            Save();
        }

        private void UpdateIconVisibility(bool recover)
        {
            if (fixture) return;
            try
            {
                if (recover) iconVisibility.Restore();
                else iconVisibility.Update(iconList, Layout.HideWindowsIcons);
                iconProblem = false;
            }
            catch (Exception error)
            {
                if (!IsLayoutError(error) && !(error is Win32Exception) && !(error is System.Security.SecurityException)) throw;
                if (!iconProblem && tray.Visible) Notify("无法切换 Windows 桌面图标。\n" + error.Message);
                iconProblem = true;
                Layout.HideWindowsIcons = false;
            }
        }

        internal bool StartsWithWindows()
        {
            if (fixture) return false;
            using (var key = Registry.CurrentUser.OpenSubKey(RunKey))
                return key != null && String.Equals(key.GetValue("DeskGrid") as string, StartupCommand(), StringComparison.OrdinalIgnoreCase);
        }

        internal void SetStartWithWindows(bool enabled)
        {
            if (fixture) return;
            try
            {
                using (var key = Registry.CurrentUser.CreateSubKey(RunKey))
                {
                    if (enabled) key.SetValue("DeskGrid", StartupCommand(), RegistryValueKind.String);
                    else key.DeleteValue("DeskGrid", false);
                }
            }
            catch (Exception error)
            {
                if (!FileEntry.ExpectedIoError(error)) throw;
                Notify("无法更新开机启动设置。\n" + error.Message);
            }
            if (settings != null) settings.RefreshOptions();
        }

        private static string StartupCommand() { return "\""+ Application.ExecutablePath + "\" --autostart"; }

        internal void ShowSettings()
        {
            if (closing) return;
            if (peek) { peek = false; ReapplyWindows(); }
            if (settings == null || settings.IsDisposed)
            {
                settings = new SettingsWindow(this, trayIcon);
                settings.Show();
            }
            else if (settings.WindowState == FormWindowState.Minimized) settings.WindowState = FormWindowState.Normal;
            settings.Activate();
        }

        internal void NewGroup(bool portal)
        {
            if (peek) { peek = false; ReapplyWindows(); }
            var group = DesktopGroup.Create(portal ? "" : "新分组", Layout.Groups.Count % Theme.Accents.Length, BoundsForNewGroup());
            group.PageIds.Add(Layout.ActivePageId);
            if (portal) group.Folder = "";
            using (var editor = new GroupEditor(group, portal))
            {
                if (ShowDialog(editor) != DialogResult.OK) return;
                editor.Apply(group);
            }
            Layout.Groups.Add(group);
            SynchronizeWindows(false);
            RefreshContents();
            Save();
            UpdateSettings(group.Id);
        }

        private Rectangle BoundsForNewGroup()
        {
            int dpi = WindowsShell.Dpi(host);
            var area = Screen.FromPoint(Cursor.Position).WorkingArea;
            int margin = 20 * dpi / 96;
            var first = FitBounds(new Rectangle(area.Left + 140 * dpi / 96, area.Top + 68 * dpi / 96,
                390 * dpi / 96, 270 * dpi / 96), Cursor.Position);
            var occupied = Layout.Groups.Where(group => group.TabHostId == null && Layout.OnPage(group, Layout.ActivePageId))
                .Select(group => Rectangle.Inflate(group.Bounds, margin / 2, margin / 2)).ToArray();
            for (int y = first.Top; y + first.Height <= area.Bottom - margin; y += margin)
            {
                for (int x = first.Left; x + first.Width <= area.Right - margin; x += margin)
                {
                    var candidate = new Rectangle(x, y, first.Width, first.Height);
                    if (occupied.All(bounds => !bounds.IntersectsWith(candidate))) return candidate;
                }
            }
            first.Offset(margin * (Layout.Groups.Count % 7), margin * (Layout.Groups.Count % 7));
            return FitBounds(first, Cursor.Position);
        }

        internal void EditGroup(DesktopGroup group)
        {
            if (peek) { peek = false; ReapplyWindows(); }
            string oldFolder = group.Folder;
            using (var editor = new GroupEditor(group, group.IsPortal, Layout.TabRoot(group)))
            {
                if (ShowDialog(editor) != DialogResult.OK) return;
                editor.Apply(group);
            }
            GroupWindow panel;
            if (windows.TryGetValue(group.Id, out panel))
            {
                if (!String.Equals(oldFolder, group.Folder, StringComparison.OrdinalIgnoreCase))
                {
                    panel.Dispose();
                    windows.Remove(group.Id);
                    SynchronizeWindows(false);
                }
                else panel.UpdateGroup();
            }
            Save();
            RefreshContents();
            UpdateSettings(group.Id);
        }

        internal void RemoveGroup(DesktopGroup group)
        {
            var remaining = Layout.Groups.Where(item => !item.IsPortal && item.Id != group.Id).ToList();
            if (remaining.Count == 0) return;
            string nextDefault = Layout.DefaultGroupId == group.Id ? remaining[0].Id : Layout.DefaultGroupId;
            if (MessageBox.Show(settings, "移除「" + group.Title + "」面板？已固定的外部文件会移到默认分组。", "移除面板",
                MessageBoxButtons.OKCancel, MessageBoxIcon.Question, MessageBoxDefaultButton.Button2) != DialogResult.OK) return;
            Layout.DefaultGroupId = nextDefault;
            Layout.DetachTab(group);
            Layout.Groups.Remove(group);
            Layout.Rules.RemoveAll(rule => rule.TargetId == group.Id);
            foreach (var placement in Layout.Placements.Where(placement => placement.GroupId == group.Id && placement.Pinned))
                placement.GroupId = nextDefault;
            Layout.Placements.RemoveAll(placement => placement.GroupId == group.Id);
            GroupWindow panel;
            if (windows.TryGetValue(group.Id, out panel)) panel.Dispose();
            windows.Remove(group.Id);
            Save();
            ReapplyWindows();
            RefreshContents();
            UpdateSettings(null);
        }

        internal void SetDefault(DesktopGroup group)
        {
            if (group.IsPortal) return;
            Layout.DefaultGroupId = group.Id;
            Save();
            RefreshContents();
            UpdateSettings(group.Id);
        }

        internal void ToggleCollapsed(GroupWindow panel)
        {
            var frame = Layout.TabRoot(panel.Group);
            frame.Collapsed = !frame.Collapsed;
            panel.ResetHover();
            UpdateQuickHideTargets();
            Save();
        }

        internal void SelectTab(DesktopGroup tab)
        {
            if (Layout.IsActiveTab(tab)) return;
            var root = Layout.TabRoot(tab);
            GroupWindow oldPanel, newPanel;
            if (windows.TryGetValue(root.ActiveTabId ?? root.Id, out oldPanel) && oldPanel.HoverExpanded &&
                windows.TryGetValue(tab.Id, out newPanel))
                newPanel.KeepHoverOpen();
            root.ActiveTabId = tab.Id == root.Id ? null : tab.Id;
            ReapplyWindows();
            RefreshContents();
            Save();
        }

        internal void EditPages()
        {
            if (peek) { peek = false; ReapplyWindows(); }
            using (var editor = new WorkspaceWindow(Layout.Pages, Layout.ActivePageId))
            {
                if (ShowDialog(editor) != DialogResult.OK) return;
                Layout.Pages = editor.EditedPages;
                Layout.ActivePageId = editor.SelectedId;
                Layout.NormalizePages();
            }
            Save();
            ReapplyWindows();
            UpdateSettings(null);
        }

        internal void EditPageMembership(DesktopGroup group)
        {
            if (peek) { peek = false; ReapplyWindows(); }
            var root = Layout.TabRoot(group);
            using (var editor = new PageMembershipWindow(root, Layout.Pages))
            {
                if (ShowDialog(editor) != DialogResult.OK) return;
                root.PageIds = editor.SelectedIds;
            }
            Save();
            ReapplyWindows();
            UpdateSettings(group.Id);
        }

        internal void AlignPanels()
        {
            var roots = Layout.Groups.Where(group => group.TabHostId == null && Layout.OnPage(group, Layout.ActivePageId)).ToArray();
            if (roots.Length < 2 || roots.All(group => group.Locked))
            {
                Notify("当前页面需要两个面板，且至少一个面板可以移动。");
                return;
            }
            using (var editor = new AlignmentWindow(roots))
            {
                if (ShowDialog(editor) != DialogResult.OK) return;
                var reference = editor.Reference.Bounds;
                foreach (var target in editor.Targets)
                    target.Bounds = FitBounds(AlignmentWindow.Align(reference, target.Bounds, editor.Operation), null);
            }
            Save();
            ReapplyWindows();
        }

        internal void SwitchPage(string id)
        {
            if (id == Layout.ActivePageId || !Layout.Pages.Any(page => page.Id == id)) return;
            Layout.ActivePageId = id;
            Save();
            ReapplyWindows();
            UpdateSettings(null);
        }

        private void StepPage(int offset)
        {
            int current = Layout.Pages.FindIndex(page => page.Id == Layout.ActivePageId);
            int next = (current + offset + Layout.Pages.Count) % Layout.Pages.Count;
            SwitchPage(Layout.Pages[next].Id);
        }

        internal void ShowSearch(DesktopGroup scope)
        {
            if (closing) return;
            if (search == null || search.IsDisposed)
            {
                search = new SearchWindow(this, scope);
                search.Show();
            }
            else search.SetScope(scope);
            search.Activate();
            UpdateContents();
        }

        internal void OpenSearchResult(PanelSearchResult result)
        {
            if (result.Entry.IsRecycleBin) OpenSystemFolder(FileEntry.RecycleBinFolder);
            else Open(result.Entry.Path);
        }

        internal void LocateSearchResult(PanelSearchResult result)
        {
            var group = result.Group;
            if (group == null)
            {
                GroupWindow tile;
                if (looseWindows.TryGetValue(result.Entry.Path, out tile))
                {
                    peek = true;
                    ReapplyWindows();
                    tile.RevealSearchResult(result);
                }
                return;
            }
            if (!Layout.Groups.Contains(group)) return;
            var root = Layout.TabRoot(group);
            if (!Layout.OnPage(group, Layout.ActivePageId)) Layout.ActivePageId = root.PageIds[0];
            root.ActiveTabId = root == group ? null : group.Id;
            root.Collapsed = false;
            peek = true;
            ReapplyWindows();
            GroupWindow panel;
            if (windows.TryGetValue(group.Id, out panel)) panel.RevealSearchResult(result);
            Save();
            UpdateSettings(group.Id);
        }

        internal void MergeTabs(DesktopGroup source, DesktopGroup target)
        {
            Layout.DockTabs(source, target);
            ReapplyWindows();
            RefreshContents();
            Save();
            UpdateSettings(source.Id);
        }

        internal void SeparateTab(DesktopGroup tab)
        {
            if (Layout.Tabs(tab).Length < 2) return;
            Layout.DetachTab(tab);
            tab.Bounds = BoundsForNewGroup();
            ReapplyWindows();
            RefreshContents();
            Save();
            UpdateSettings(tab.Id);
        }

        private DialogResult ShowDialog(Form dialog)
        {
            if (settings == null || !settings.Visible) return dialog.ShowDialog();
            return dialog.ShowDialog(settings);
        }

        internal void EditRules()
        {
            using (var editor = new RulesWindow(Layout))
            {
                if (ShowDialog(editor) != DialogResult.OK) return;
                Layout.Rules = editor.EditedRules;
            }
            Save();
            RefreshContents();
        }

        internal void ExportLayout()
        {
            using (var dialog = new SaveFileDialog { Filter = "桌面格布局 (*.xml)|*.xml", FileName = "DeskGrid-layout.xml", Title = "导出布局" })
            {
                if (dialog.ShowDialog(settings) != DialogResult.OK) return;
                if (SamePath(dialog.FileName, settingsPath) || SamePath(dialog.FileName, settingsPath + ".bak"))
                {
                    Notify("请选择另一个位置存放布局快照。");
                    return;
                }
                try { LayoutStore.Write(dialog.FileName, Layout, false); }
                catch (Exception error)
                {
                    if (!IsLayoutError(error)) throw;
                    MessageBox.Show(settings, error.Message, "布局导出失败", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                }
            }
        }

        internal void ImportLayout()
        {
            using (var dialog = new OpenFileDialog { Filter = "桌面格布局 (*.xml)|*.xml", Title = "导入布局" })
            {
                if (dialog.ShowDialog(settings) != DialogResult.OK) return;
                DesktopLayout imported;
                try { imported = LayoutStore.Read(dialog.FileName); }
                catch (Exception error)
                {
                    if (!IsLayoutError(error)) throw;
                    MessageBox.Show(settings, "无法读取此布局。", "布局导入失败", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                    return;
                }
                if (MessageBox.Show(settings, "用此快照替换当前面板、分类规则和手动归类？", "导入布局",
                    MessageBoxButtons.OKCancel, MessageBoxIcon.Question, MessageBoxDefaultButton.Button2) != DialogResult.OK) return;
                imported.HideWindowsIcons = Layout.HideWindowsIcons;
                Layout = imported;
                foreach (var panel in windows.Values) panel.Dispose();
                windows.Clear();
                SynchronizeWindows(false);
                RefreshContents();
                Save();
                UpdateSettings(null);
            }
        }

        private void UpdateSettings(string selected) { if (settings != null && !settings.IsDisposed) settings.RefreshGroups(selected); }

        private static bool SamePath(string a, string b) { return String.Equals(a, b, StringComparison.OrdinalIgnoreCase); }

        internal bool IsDesktopItem(string path)
        {
            string parent = Path.GetDirectoryName(path);
            return desktopFolders.Any(folder => SamePath(folder.TrimEnd('\\'), parent == null ? null : parent.TrimEnd('\\')));
        }

        internal bool IsManuallyPlaced(string path) { return Layout.Placements.Any(placement => SamePath(placement.Path, path)); }

        internal void Assign(DesktopGroup group, IEnumerable<string> paths, FileEntry[] displayed = null, int slot = -1)
        {
            if (group.IsPortal || !Layout.Groups.Contains(group)) return;
            var incoming = paths.Select(FileEntry.FullPath).Where(path => path != null && !SamePath(path, Path.GetPathRoot(path)))
                .Distinct(StringComparer.OrdinalIgnoreCase).ToArray();
            if (incoming.Length == 0) return;
            bool changed = slot >= 0 && group.InsertAt(displayed, slot, incoming);
            var existing = displayed == null ? null : new HashSet<string>(displayed.Select(item => item.Path), StringComparer.OrdinalIgnoreCase);
            foreach (var path in incoming)
            {
                if (existing != null && existing.Contains(path)) continue;
                Layout.Placements.RemoveAll(placement => SamePath(placement.Path, path));
                Layout.Placements.Add(new Placement { Path = path, GroupId = group.Id, Pinned = !IsDesktopItem(path) });
                changed = true;
            }
            if (!changed) return;
            Save();
            RefreshContents();
        }

        internal void ClearPlacement(string path)
        {
            Layout.Placements.RemoveAll(placement => SamePath(placement.Path, path));
            Save();
            RefreshContents();
        }

        internal void Open(string path)
        {
            int status;
            try
            {
                status = WindowsShell.OpenPath(path);
            }
            catch (Exception error)
            {
                if (!FileEntry.ExpectedIoError(error)) throw;
                Notify("无法打开此项目。请确认路径仍然有效。");
                RefreshContents();
                return;
            }
            if (status == 0) return;
            var label = Path.GetFileName(path.TrimEnd('\\', '/'));
            var reason = status == 2 || status == 3 ? "文件或快捷方式的目标已移动或不存在。" :
                status == 1155 ? "Windows 未为此文件类型设置默认应用。" :
                new Win32Exception(status).Message;
            Notify("无法打开「" + label + "」。" + reason);
            if (status == 2 || status == 3) RefreshContents();
        }

        internal void Reveal(string path)
        {
            try
            {
                using (var process = Process.Start(new ProcessStartInfo("explorer.exe", "/select,\"" + path + "\""))) { }
            }
            catch (Win32Exception) { Notify("无法启动文件资源管理器。"); }
        }

        internal void OpenSystemFolder(string name)
        {
            try { using (var process = Process.Start("explorer.exe", "shell:" + name)) { } }
            catch (Win32Exception) { Notify("无法启动文件资源管理器。"); }
        }

        internal void CopyPath(string path)
        {
            try { Clipboard.SetText(path); }
            catch (System.Runtime.InteropServices.ExternalException) { Notify("剪贴板正被其他程序占用。"); }
        }

        private void Notify(string text)
        {
            tray.ShowBalloonTip(3800, "桌面格", text, ToolTipIcon.Info);
        }

        protected override void ExitThreadCore()
        {
            closing = true;
            base.ExitThreadCore();
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing && disposed) return;
            if (disposing)
            {
                disposed = true;
                closing = true;
                timer.Dispose();
                desktopDoubleClick.Dispose();
                if (search != null) search.Dispose();
                if (settings != null) settings.Dispose();
                foreach (var panel in windows.Values) panel.Dispose();
                windows.Clear();
                foreach (var tile in looseWindows.Values) tile.Dispose();
                looseWindows.Clear();
                try { if (!fixture) iconVisibility.Close(); }
                catch (Exception error) { if (!FileEntry.ExpectedIoError(error)) throw; }
                messages.Dispose();
                tray.Visible = false;
                if (tray.ContextMenuStrip != null) tray.ContextMenuStrip.Dispose();
                tray.Dispose();
                Icons.Dispose();
                if (trayIcon != null) trayIcon.Dispose();
            }
            base.Dispose(disposing);
        }
    }
}
