using System;
using System.Collections.Generic;
using System.Drawing;
using System.IO;
using System.Linq;
using System.Text.RegularExpressions;
using System.Threading.Tasks;
using System.Xml;
using System.Xml.Serialization;

namespace DeskGrid
{
    public enum SortOrder { Name, Type, Newest, Size, Manual }
    public enum PanelView { Icons, List, Details }
    public enum RuleKind { Extension, Name, Folder, ModifiedWithinDays }

    public sealed class DesktopLayout
    {
        internal const string MainPageId = "desktop";
        public List<DesktopPage> Pages = new List<DesktopPage>();
        public string ActivePageId = MainPageId;
        public List<DesktopGroup> Groups = new List<DesktopGroup>();
        public List<DesktopRule> Rules = new List<DesktopRule>();
        public List<Placement> Placements = new List<Placement>();
        public string DefaultGroupId;
        public bool HideWindowsIcons;
        public bool QuickHideOnDoubleClick = true;

        internal static DesktopLayout CreateDefault(Rectangle screen, int dpi)
        {
            int pad = 20 * dpi / 96;
            int left = Math.Min(140 * dpi / 96, screen.Width / 8);
            int top = Math.Min(68 * dpi / 96, screen.Height / 10);
            int width = Math.Min(390 * dpi / 96, (screen.Width - left - pad * 2) / 2);
            int height = Math.Min(270 * dpi / 96, (screen.Height - top - pad * 2) / 2);
            var layout = new DesktopLayout { HideWindowsIcons = true };
            layout.Groups.Add(DesktopGroup.Create("应用与快捷方式", 0, new Rectangle(screen.X + left, screen.Y + top, width, height)));
            layout.Groups.Add(DesktopGroup.Create("桌面文件夹", 1, new Rectangle(screen.X + left + width + pad, screen.Y + top, width, height)));
            layout.Groups.Add(DesktopGroup.Create("文档与媒体", 2, new Rectangle(screen.X + left, screen.Y + top + height + pad, width, height)));
            layout.Groups.Add(DesktopGroup.Create("其他桌面项目", 3, new Rectangle(screen.X + left + width + pad, screen.Y + top + height + pad, width, height)));
            layout.DefaultGroupId = layout.Groups[3].Id;
            layout.Rules.Add(new DesktopRule { Kind = RuleKind.Folder, TargetId = layout.Groups[1].Id });
            layout.Rules.Add(new DesktopRule { Kind = RuleKind.Extension, Value = "lnk; url; exe; appref-ms; msi; website; bat; cmd", TargetId = layout.Groups[0].Id });
            layout.Rules.Add(new DesktopRule { Kind = RuleKind.Extension, Value = "pdf; doc; docx; ppt; pptx; xls; xlsx; csv; txt; md; rtf; png; jpg; jpeg; webp; gif; bmp; svg; heic; mp4; mov; mkv; mp3; wav", TargetId = layout.Groups[2].Id });
            layout.NormalizePages();
            return layout;
        }

        internal void Normalize()
        {
            var ids = new HashSet<string>(StringComparer.Ordinal);
            Groups = (Groups ?? new List<DesktopGroup>())
                .Where(group => group != null && !String.IsNullOrWhiteSpace(group.Id) && ids.Add(group.Id))
                .ToList();
            foreach (var group in Groups)
            {
                if (String.IsNullOrWhiteSpace(group.Title)) group.Title = "未命名";
                group.Accent = Math.Max(0, Math.Min(group.Accent, Theme.Accents.Length - 1));
                group.Opacity = Math.Max(35, Math.Min(100, group.Opacity));
                if (!DesktopGroup.IconSizes.Contains(group.IconSize)) group.IconSize = 32;
                group.ExpandDelay = Math.Max(0, Math.Min(5000, group.ExpandDelay));
                group.RetractDelay = Math.Max(0, Math.Min(5000, group.RetractDelay));
                if (!String.IsNullOrWhiteSpace(group.Folder))
                {
                    group.Folder = FileEntry.FullPath(group.Folder);
                    if (group.Folder == null) throw new InvalidDataException("布局含有无效的文件夹地址。");
                }
                else group.Folder = null;
                group.ManualOrder = (group.ManualOrder ?? new List<string>()).Select(FileEntry.FullPath)
                    .Where(path => path != null).Distinct(StringComparer.OrdinalIgnoreCase).ToList();
                if (group.IsPortal && group.Sort == SortOrder.Manual) group.Sort = SortOrder.Name;
            }
            NormalizePages();
            foreach (var group in Groups)
            {
                var parent = Groups.Find(candidate => candidate.Id == group.TabHostId);
                if (parent == null || parent == group || parent.TabHostId != null) group.TabHostId = null;
            }
            foreach (var group in Groups)
            {
                if (group.TabHostId != null || !Tabs(group).Any(tab => tab.Id == group.ActiveTabId)) group.ActiveTabId = null;
            }
            var targets = new HashSet<string>(Groups.Where(group => !group.IsPortal).Select(group => group.Id));
            if (targets.Count == 0) throw new InvalidDataException("布局中至少需要一个桌面分组。");
            if (!targets.Contains(DefaultGroupId)) DefaultGroupId = Groups.First(group => !group.IsPortal).Id;
            Rules = (Rules ?? new List<DesktopRule>())
                .Where(rule => rule != null && targets.Contains(rule.TargetId) && rule.IsValid())
                .ToList();
            var paths = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            Placements = (Placements ?? new List<Placement>())
                .Where(placement => placement != null && (placement.Loose || targets.Contains(placement.GroupId)))
                .Where(placement =>
                {
                    placement.Path = FileEntry.FullPath(placement.Path);
                    return placement.Path != null && paths.Add(placement.Path);
                }).ToList();
        }

        internal void NormalizePages()
        {
            var pageIds = new HashSet<string>(StringComparer.Ordinal);
            Pages = (Pages ?? new List<DesktopPage>())
                .Where(page => page != null && !String.IsNullOrWhiteSpace(page.Id) && pageIds.Add(page.Id)).ToList();
            if (Pages.Count == 0)
            {
                Pages.Add(new DesktopPage { Id = MainPageId, Title = "桌面" });
                pageIds.Add(MainPageId);
            }
            foreach (var page in Pages)
                if (String.IsNullOrWhiteSpace(page.Title)) page.Title = "未命名页面";
            if (!pageIds.Contains(ActivePageId)) ActivePageId = Pages[0].Id;
            foreach (var group in Groups)
            {
                group.PageIds = (group.PageIds ?? new List<string>()).Where(pageIds.Contains).Distinct().ToList();
                if (group.PageIds.Count == 0) group.PageIds.Add(Pages[0].Id);
            }
        }

        internal bool OnPage(DesktopGroup group, string pageId)
        {
            return TabRoot(group).PageIds.Contains(pageId);
        }

        internal string PageNames(DesktopGroup group)
        {
            return String.Join("、", Pages.Where(page => OnPage(group, page.Id)).Select(page => page.Title));
        }

        internal Dictionary<string, List<FileEntry>> Route(IEnumerable<FileEntry> desktopItems,
            IEnumerable<FileEntry> pinnedItems, DateTime now)
        {
            var groups = Groups.Where(group => !group.IsPortal).ToDictionary(group => group.Id, group => new List<FileEntry>());
            var placements = Placements.ToDictionary(placement => placement.Path, StringComparer.OrdinalIgnoreCase);
            var matchers = Rules.Select(rule => new { rule.TargetId, Match = rule.Compile(now) }).ToArray();
            foreach (var item in desktopItems)
            {
                Placement placement;
                placements.TryGetValue(item.Path, out placement);
                if (placement != null && placement.Loose) continue;
                string destination = placement == null ? null : placement.GroupId;
                if (destination == null)
                {
                    var rule = matchers.FirstOrDefault(matcher => matcher.Match(item));
                    destination = rule == null ? DefaultGroupId : rule.TargetId;
                }
                groups[destination].Add(item);
            }
            foreach (var item in pinnedItems)
            {
                var placement = placements[item.Path];
                if (!placement.Loose) groups[placement.GroupId].Add(item);
            }
            return groups;
        }

        internal DesktopGroup TabRoot(DesktopGroup group)
        {
            return group.TabHostId == null ? group : Groups.Find(candidate => candidate.Id == group.TabHostId) ?? group;
        }

        internal DesktopGroup[] Tabs(DesktopGroup group)
        {
            var root = TabRoot(group);
            return new[] { root }.Concat(Groups.Where(candidate => candidate.TabHostId == root.Id)).ToArray();
        }

        internal bool IsActiveTab(DesktopGroup group)
        {
            var root = TabRoot(group);
            return group.Id == (root.ActiveTabId ?? root.Id);
        }

        internal void DockTabs(DesktopGroup source, DesktopGroup target)
        {
            var targetRoot = TabRoot(target);
            var sourceRoot = TabRoot(source);
            if (targetRoot == sourceRoot) return;
            var moving = source.TabHostId == null ? Tabs(source) : new[] { source };
            if (moving.Length == 1 && sourceRoot.ActiveTabId == source.Id) sourceRoot.ActiveTabId = null;
            foreach (var tab in moving)
            {
                tab.TabHostId = targetRoot.Id;
                tab.ActiveTabId = null;
            }
            targetRoot.ActiveTabId = source.Id;
        }

        internal void DetachTab(DesktopGroup tab)
        {
            var root = TabRoot(tab);
            var remaining = Tabs(root).Where(group => group != tab).ToArray();
            if (remaining.Length == 0) return;
            if (tab == root)
            {
                var next = remaining[0];
                next.TabHostId = null;
                next.Bounds = root.Bounds;
                next.Collapsed = root.Collapsed;
                next.Locked = root.Locked;
                next.HoverExpand = root.HoverExpand;
                next.ExpandDelay = root.ExpandDelay;
                next.RetractDelay = root.RetractDelay;
                next.PageIds = root.PageIds.ToList();
                next.ActiveTabId = remaining.Any(group => group.Id == root.ActiveTabId) ? root.ActiveTabId : null;
                foreach (var sibling in remaining.Skip(1)) sibling.TabHostId = next.Id;
            }
            else if (root.ActiveTabId == tab.Id) root.ActiveTabId = null;
            tab.PageIds = root.PageIds.ToList();
            tab.ActiveTabId = null;
            tab.TabHostId = null;
        }
    }

    public sealed class DesktopPage
    {
        public string Id;
        public string Title;
        public override string ToString() { return Title; }
    }

    public sealed class DesktopGroup
    {
        internal static readonly int[] IconSizes = { 24, 32, 48 };
        public string Id;
        public string Title;
        public string Folder;
        public int Accent;
        public int Left;
        public int Top;
        public int Width;
        public int Height;
        public SortOrder Sort;
        public List<string> ManualOrder = new List<string>();
        public bool Collapsed;
        public bool Locked;
        public int Opacity = 82;
        public bool TintIcons;
        public bool DimWhenIdle;
        public PanelView View;
        public int IconSize = 32;
        public bool HoverExpand;
        public int ExpandDelay = 350;
        public int RetractDelay = 800;
        public List<string> PageIds = new List<string>();
        public string TabHostId;
        public string ActiveTabId;

        public override string ToString() { return Title; }

        internal IEnumerable<FileEntry> OrderedItems(IEnumerable<FileEntry> items)
        {
            if (Sort != SortOrder.Manual) return FileEntry.Sort(items, Sort);
            var ranks = new Dictionary<string, int>(StringComparer.OrdinalIgnoreCase);
            foreach (string path in ManualOrder) ranks[path] = ranks.Count;
            return FileEntry.Sort(items, SortOrder.Name).OrderBy(item =>
            {
                int rank;
                return ranks.TryGetValue(item.Path, out rank) ? rank : Int32.MaxValue;
            });
        }

        internal bool InsertAt(FileEntry[] displayed, int slot, string[] paths)
        {
            var comparer = StringComparer.OrdinalIgnoreCase;
            var current = displayed.Where(item => !item.IsRecycleBin).Select(item => item.Path).ToArray();
            // A pinned path can still be loading when another item is dropped.
            var known = Sort == SortOrder.Manual ? ManualOrder.Concat(current).Distinct(comparer) : current;
            var moving = new HashSet<string>(paths, comparer);
            var order = known.Where(path => !moving.Contains(path)).ToList();
            var next = displayed.Skip(slot).FirstOrDefault(item => !item.IsRecycleBin && !moving.Contains(item.Path));
            int index = next == null ? order.Count : order.FindIndex(path => comparer.Equals(path, next.Path));
            order.InsertRange(index, paths);
            var visible = new HashSet<string>(current.Concat(paths), comparer);
            if (order.Where(visible.Contains).SequenceEqual(current, comparer)) return false;
            ManualOrder = order;
            Sort = SortOrder.Manual;
            return true;
        }

        [XmlIgnore]
        public bool IsPortal { get { return Folder != null; } }

        [XmlIgnore]
        public Rectangle Bounds
        {
            get { return new Rectangle(Left, Top, Width, Height); }
            set { Left = value.X; Top = value.Y; Width = value.Width; Height = value.Height; }
        }

        internal static DesktopGroup Create(string title, int accent, Rectangle bounds)
        {
            return new DesktopGroup { Id = Guid.NewGuid().ToString("N"), Title = title, Accent = accent, Bounds = bounds };
        }
    }

    public sealed class DesktopRule
    {
        public RuleKind Kind;
        public string Value;
        public string TargetId;


        internal bool IsValid()
        {
            int days;
            switch (Kind)
            {
                case RuleKind.Folder: return true;
                case RuleKind.Extension: return Extensions().Length != 0;
                case RuleKind.Name: return !String.IsNullOrWhiteSpace(Value) && Value.Length <= 160;
                case RuleKind.ModifiedWithinDays: return Int32.TryParse(Value, out days) && days >= 1 && days <= 36500;
                default: return false;
            }
        }

        internal Func<FileEntry, bool> Compile(DateTime now)
        {
            switch (Kind)
            {
                case RuleKind.Folder:
                    return item => item.IsDirectory;
                case RuleKind.Extension:
                    var extensions = new HashSet<string>(Extensions(), StringComparer.OrdinalIgnoreCase);
                    return item => !item.IsDirectory && extensions.Contains(item.Extension.TrimStart('.'));
                case RuleKind.Name:
                    var pattern = Value.Trim();
                    var regex = new Regex("\\A" + Regex.Escape(pattern).Replace("\\*", ".*").Replace("\\?", ".") + "\\z",
                        RegexOptions.IgnoreCase | RegexOptions.CultureInvariant | RegexOptions.Singleline, TimeSpan.FromMilliseconds(50));
                    return item =>
                    {
                        try { return regex.IsMatch(item.Name); }
                        catch (RegexMatchTimeoutException) { return false; }
                    };
                case RuleKind.ModifiedWithinDays:
                    var cutoff = now.AddDays(-Int32.Parse(Value));
                    return item => item.ModifiedUtc >= cutoff;
                default:
                    return item => false;
            }
        }

        private string[] Extensions()
        {
            return (Value ?? "").Split(new[] { ';', ',', '，', '；', ' ' }, StringSplitOptions.RemoveEmptyEntries)
                .Select(part => part.Trim().TrimStart('*', '.'))
                .Where(part => part.Length != 0)
                .ToArray();
        }
    }

    public sealed class Placement
    {
        public string Path;
        public string GroupId;
        public bool Pinned;
        public bool Loose;
        public int Left;
        public int Top;
    }

    internal sealed class FileEntry
    {
        internal const string RecycleBinFolder = "RecycleBinFolder";
        internal static readonly FileEntry RecycleBin = new FileEntry
        {
            Path = "shell:" + RecycleBinFolder, Name = "回收站", Label = "回收站", Extension = ""
        };

        internal string Path;
        internal string Name;
        internal string Extension;
        internal string Label;
        internal bool IsDirectory;
        internal DateTime ModifiedUtc;
        internal long SizeBytes = -1;
        internal bool IsRecycleBin { get { return this == RecycleBin; } }

        internal static string FullPath(string path)
        {
            if (String.IsNullOrWhiteSpace(path)) return null;
            try
            {
                if (!System.IO.Path.IsPathRooted(path)) return null;
                string full = System.IO.Path.GetFullPath(path);
                return full.Length > System.IO.Path.GetPathRoot(full).Length ? full.TrimEnd('\\', '/') : full;
            }
            catch (Exception error)
            {
                if (!ExpectedIoError(error)) throw;
                return null;
            }
        }

        internal static FileEntry FromInfo(FileSystemInfo info)
        {
            var extension = info is DirectoryInfo ? "" : info.Extension;
            return new FileEntry
            {
                Path = info.FullName, Name = info.Name,
                Label = String.Equals(extension, ".lnk", StringComparison.OrdinalIgnoreCase) ||
                        String.Equals(extension, ".url", StringComparison.OrdinalIgnoreCase)
                    ? System.IO.Path.GetFileNameWithoutExtension(info.Name) : info.Name,
                Extension = extension, IsDirectory = info is DirectoryInfo, ModifiedUtc = info.LastWriteTimeUtc,
                SizeBytes = info is FileInfo ? ((FileInfo)info).Length : -1
            };
        }

        internal static bool ExpectedIoError(Exception error)
        {
            return error is IOException || error is UnauthorizedAccessException ||
                   error is System.Security.SecurityException || error is ArgumentException ||
                   error is NotSupportedException;
        }

        internal static IEnumerable<FileEntry> Sort(IEnumerable<FileEntry> items, SortOrder sort)
        {
            switch (sort)
            {
                case SortOrder.Type:
                    return items.OrderByDescending(item => item.IsDirectory)
                        .ThenBy(item => item.Extension, StringComparer.OrdinalIgnoreCase)
                        .ThenBy(item => item.Label, StringComparer.CurrentCultureIgnoreCase);
                case SortOrder.Newest:
                    return items.OrderByDescending(item => item.ModifiedUtc)
                        .ThenBy(item => item.Label, StringComparer.CurrentCultureIgnoreCase);
                case SortOrder.Size:
                    return items.OrderByDescending(item => item.SizeBytes)
                        .ThenBy(item => item.Label, StringComparer.CurrentCultureIgnoreCase);
                default:
                    return items.OrderByDescending(item => item.IsDirectory)
                        .ThenBy(item => item.Label, StringComparer.CurrentCultureIgnoreCase);
            }
        }
    }

    internal sealed class DirectoryResult
    {
        internal FileEntry[] Entries = new FileEntry[0];
        internal bool Unavailable;
    }

    internal sealed class DirectoryFeed
    {
        private sealed class Feed
        {
            internal Task<DirectoryResult> Pending;
            internal DirectoryResult Result;
            internal DateTime NextRead;
        }

        private readonly Dictionary<string, Feed> feeds = new Dictionary<string, Feed>(StringComparer.OrdinalIgnoreCase);

        internal DirectoryResult Get(string path) { return Get(path, () => ReadDirectory(path)); }
        internal DirectoryResult GetPinned(string path) { return Get(PinnedKey(path), () => ReadPinned(path)); }
        internal static string PinnedKey(string path) { return "file|" + path; }

        private DirectoryResult Get(string path, Func<DirectoryResult> read)
        {
            if (String.IsNullOrEmpty(path)) return new DirectoryResult();
            Feed feed;
            if (!feeds.TryGetValue(path, out feed))
            {
                feed = new Feed();
                feeds.Add(path, feed);
            }
            if (feed.Pending != null && feed.Pending.IsCompleted)
            {
                if (feed.Pending.IsFaulted)
                {
                    GC.KeepAlive(feed.Pending.Exception);
                    feed.Result = new DirectoryResult { Unavailable = true };
                }
                else feed.Result = feed.Pending.Result;
                feed.Pending = null;
                feed.NextRead = DateTime.UtcNow.AddSeconds(2);
            }
            if (feed.Pending == null && DateTime.UtcNow >= feed.NextRead)
                feed.Pending = Task.Run(read);
            return feed.Result;
        }

        internal void Refresh()
        {
            foreach (var feed in feeds.Values) feed.NextRead = DateTime.MinValue;
        }

        internal void Retain(HashSet<string> active)
        {
            foreach (string path in feeds.Keys.Where(path => !active.Contains(path) && feeds[path].Pending == null).ToArray())
                feeds.Remove(path);
            foreach (string path in feeds.Keys.Where(path => !active.Contains(path) && feeds[path].Pending != null && feeds[path].Pending.IsCompleted).ToArray())
            {
                var task = feeds[path].Pending;
                if (task.IsFaulted) GC.KeepAlive(task.Exception);
                feeds.Remove(path);
            }
        }

        private static DirectoryResult ReadDirectory(string path)
        {
            try
            {
                var entries = new List<FileEntry>();
                foreach (var info in new DirectoryInfo(path).EnumerateFileSystemInfos())
                {
                    try
                    {
                        if ((info.Attributes & (FileAttributes.Hidden | FileAttributes.System)) == 0)
                            entries.Add(FileEntry.FromInfo(info));
                    }
                    catch (Exception error) { if (!FileEntry.ExpectedIoError(error)) throw; }
                }
                return new DirectoryResult { Entries = entries.ToArray() };
            }
            catch (Exception error)
            {
                if (!FileEntry.ExpectedIoError(error)) throw;
                return new DirectoryResult { Unavailable = true };
            }
        }

        private static DirectoryResult ReadPinned(string path)
        {
            try
            {
                bool directory = (File.GetAttributes(path) & FileAttributes.Directory) != 0;
                FileSystemInfo info = directory ? (FileSystemInfo)new DirectoryInfo(path) : new FileInfo(path);
                return new DirectoryResult { Entries = new[] { FileEntry.FromInfo(info) } };
            }
            catch (Exception error)
            {
                if (!FileEntry.ExpectedIoError(error)) throw;
                return new DirectoryResult { Unavailable = true };
            }
        }
    }

    internal static class LayoutStore
    {
        private static readonly XmlSerializer Serializer = new XmlSerializer(typeof(DesktopLayout));

        internal static DesktopLayout Read(string path)
        {
            using (var stream = File.OpenRead(path))
            using (var reader = XmlReader.Create(stream, new XmlReaderSettings { DtdProcessing = DtdProcessing.Prohibit }))
            {
                var layout = (DesktopLayout)Serializer.Deserialize(reader);
                layout.Normalize();
                return layout;
            }
        }

        internal static void Write(string path, DesktopLayout layout, bool keepBackup)
        {
            Directory.CreateDirectory(Path.GetDirectoryName(path));
            string temporary = path + "." + Guid.NewGuid().ToString("N") + ".tmp";
            try
            {
                using (var stream = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None))
                {
                    Serializer.Serialize(stream, layout);
                    stream.Flush(true);
                }
                if (File.Exists(path)) File.Replace(temporary, path, keepBackup ? path + ".bak" : null);
                else File.Move(temporary, path);
            }
            finally
            {
                if (File.Exists(temporary)) File.Delete(temporary);
            }
        }
    }
}
