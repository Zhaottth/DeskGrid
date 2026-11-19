using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Linq;
using System.Windows.Forms;

namespace DeskGrid
{
    internal static class Theme
    {
        internal static readonly Color Background = Color.FromArgb(19, 25, 36);
        internal static readonly Color Surface = Color.FromArgb(26, 34, 47);
        internal static readonly Color Border = Color.FromArgb(61, 75, 94);
        internal static readonly Color Text = Color.FromArgb(238, 242, 250);
        internal static readonly Color Muted = Color.FromArgb(165, 181, 201);
        internal static readonly Color Hover = Color.FromArgb(41, 54, 72);
        internal static readonly Color Selected = Color.FromArgb(50, 69, 92);
        internal static readonly Color[] Accents =
        {
            Color.FromArgb(116, 190, 255), Color.FromArgb(105, 221, 190), Color.FromArgb(194, 165, 255),
            Color.FromArgb(244, 194, 113), Color.FromArgb(255, 152, 171), Color.FromArgb(184, 226, 126)
        };
        internal static readonly string[] AccentNames = { "雾蓝", "薄荷", "淡紫", "琥珀", "玫瑰", "青柠" };
        internal static readonly string[] SortNames = { "名称", "类型", "最近修改", "大小", "手动排列" };
        internal static readonly string[] ViewNames = { "图标", "列表", "详细信息" };

        internal static GraphicsPath Rounded(Rectangle rect, int radius)
        {
            var path = new GraphicsPath();
            int d = Math.Max(2, Math.Min(radius * 2, Math.Min(rect.Width, rect.Height)));
            path.AddArc(rect.X, rect.Y, d, d, 180, 90);
            path.AddArc(rect.Right - d, rect.Y, d, d, 270, 90);
            path.AddArc(rect.Right - d, rect.Bottom - d, d, d, 0, 90);
            path.AddArc(rect.X, rect.Bottom - d, d, d, 90, 90);
            path.CloseFigure();
            return path;
        }

        internal static Color Blend(Color baseColor, Color overlay, int percent)
        {
            return Color.FromArgb(
                (baseColor.R * (100 - percent) + overlay.R * percent) / 100,
                (baseColor.G * (100 - percent) + overlay.G * percent) / 100,
                (baseColor.B * (100 - percent) + overlay.B * percent) / 100);
        }

        internal static ContextMenuStrip Menu()
        {
            return new ContextMenuStrip
            {
                BackColor = Surface, ForeColor = Text, ShowImageMargin = false,
                Renderer = new ToolStripProfessionalRenderer(new MenuColors())
            };
        }

        private sealed class MenuColors : ProfessionalColorTable
        {
            public override Color ToolStripDropDownBackground { get { return Surface; } }
            public override Color MenuBorder { get { return Border; } }
            public override Color MenuItemBorder { get { return Selected; } }
            public override Color MenuItemSelected { get { return Hover; } }
            public override Color MenuItemPressedGradientBegin { get { return Hover; } }
            public override Color MenuItemPressedGradientEnd { get { return Hover; } }
            public override Color SeparatorDark { get { return Border; } }
            public override Color SeparatorLight { get { return Border; } }
        }
    }

    internal sealed class GroupWindow : Form
    {
        private const TextFormatFlags SingleLine = TextFormatFlags.EndEllipsis | TextFormatFlags.SingleLine |
            TextFormatFlags.VerticalCenter | TextFormatFlags.NoPrefix | TextFormatFlags.NoPadding;
        private static readonly Color LooseBackground = Color.FromArgb(1, 2, 3);
        private readonly DesktopApp app;
        private readonly bool loose;
        private readonly Stack<string> previousFolders = new Stack<string>();
        private FileEntry[] items = new FileEntry[0];
        private string selectedPath;
        private string folder;
        private bool loading;
        private bool unavailable;
        private int firstRow;
        private int hovered = -1;
        private FileEntry pressedItem;
        private Point pressedScreen;
        private Rectangle initialBounds;
        private bool geometryDragging;
        private MouseAction mouseAction;
        private bool dropHover;
        private string dropItemPath;
        private Rectangle insertionMark;
        private string[] dropPaths;
        private readonly Timer dragScroll = new Timer { Interval = 250 };
        private DragEventArgs lastDragOver;
        private readonly ShellDrop shellDrop = new ShellDrop();
        private readonly Timer hoverTimer = new Timer();
        private bool hoverExpanded;
        private bool contextOpen;
        private DragPreview dragPreview;
        private Point? wallpaperDrop;
        private bool handledMouseDown;
        private bool disposingWindow;
        private string pendingReveal;
        private IntPtr host;
        private bool peek;
        private bool mouseInside;
        private int appliedOpacity = -1;
        private ImageAttributes iconTint;
        private int dpi = 96;
        private Font titleFont;
        private Font detailFont;
        private Font itemFont;
        private readonly ToolTip tip = new ToolTip { InitialDelay = 650, ReshowDelay = 450, AutoPopDelay = 6000 };
        private enum MouseAction { None, Move, Resize, Item }

        internal DesktopGroup Group { get; private set; }
        private DesktopGroup Frame { get { return app.Layout.TabRoot(Group); } }
        internal string Folder { get { return folder; } }
        internal bool Alive { get { return IsHandleCreated && WindowsShell.IsWindow(Handle); } }
        internal bool HoverExpanded { get { return hoverExpanded; } }
        internal bool DraggingFile { get { return dragPreview != null; } }
        internal Rectangle VisibleScreenBounds { get { return Visible && Alive ? WindowsShell.Bounds(Handle) : Rectangle.Empty; } }
        private bool IsCollapsed { get { return Frame.Collapsed && !hoverExpanded; } }
        private int Header { get { return ScalePx(42); } }
        private int Sidebar { get { return ScalePx(26); } }
        private int TabHeight { get { return app.Layout.Tabs(Group).Length > 1 ? ScalePx(33) : 0; } }
        private int PortalTop { get { return Header + TabHeight; } }
        private int CellHeight { get { return ScalePx(Group.View == PanelView.Icons ? Group.IconSize + 38 : Group.IconSize + 9); } }
        private int ContentTop { get { return PortalTop + (Group.IsPortal ? Sidebar : 0); } }
        private int DetailsHeader { get { return Group.View == PanelView.Details ? ScalePx(25) : 0; } }
        private int GridTop { get { return ContentTop + DetailsHeader + ScalePx(7); } }
        private int GridBottom { get { return Math.Max(GridTop, ClientSize.Height - ScalePx(9)); } }
        private int Columns { get { return Group.View == PanelView.Icons ? Math.Max(1, (ClientSize.Width - ScalePx(26)) / ScalePx(Group.IconSize + 41)) : 1; } }
        private int CellWidth { get { return Math.Max(1, (ClientSize.Width - ScalePx(23)) / Columns); } }
        private int VisibleRows { get { return Math.Max(1, (GridBottom - GridTop) / CellHeight); } }
        private int LastRow { get { return Math.Max(0, (items.Length + Columns - 1) / Columns - VisibleRows); } }
        private Rectangle MenuButton { get { return new Rectangle(ClientSize.Width - ScalePx(36), ScalePx(8), ScalePx(27), ScalePx(26)); } }
        private Rectangle CollapseButton { get { return new Rectangle(MenuButton.Left - ScalePx(29), MenuButton.Top, MenuButton.Width, MenuButton.Height); } }
        private Rectangle BackButton { get { return new Rectangle(ScalePx(10), PortalTop, ScalePx(25), Sidebar); } }
        private Rectangle FolderButton { get { return new Rectangle(ClientSize.Width - ScalePx(36), PortalTop, ScalePx(27), Sidebar); } }
        private Rectangle Grip { get { return new Rectangle(ClientSize.Width - ScalePx(16), ClientSize.Height - ScalePx(16), ScalePx(16), ScalePx(16)); } }

        internal GroupWindow(DesktopApp app, DesktopGroup group, bool loose = false)
        {
            this.app = app;
            this.loose = loose;
            Group = group;
            folder = group.Folder;
            Text = "DeskGrid · " + group.Title;
            FormBorderStyle = FormBorderStyle.None;
            ShowInTaskbar = false;
            StartPosition = FormStartPosition.Manual;
            AutoScaleMode = AutoScaleMode.None;
            BackColor = Theme.Background;
            Bounds = group.Bounds;
            AllowDrop = true;
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint |
                     ControlStyles.OptimizedDoubleBuffer | ControlStyles.ResizeRedraw | ControlStyles.Selectable, true);
            HandleCreated += delegate { appliedOpacity = -1; UpdateDpi(); };
            UpdateDpi();
            UpdateColors();
            hoverTimer.Tick += delegate { ApplyHover(); };
            dragScroll.Tick += delegate
            {
                var e = lastDragOver;
                if (e == null) return;
                var point = PointToClient(new Point(e.X, e.Y));
                firstRow += DragScrollStep(point);
                UpdateDrop(e, false);
                Invalidate();
            };
        }

        protected override bool ShowWithoutActivation { get { return true; } }
        protected override CreateParams CreateParams
        {
            get { var parameters = base.CreateParams; parameters.ExStyle |= WindowsShell.ToolWindow; return parameters; }
        }

        internal void ApplyDesktopState(IntPtr desktop, bool isPeek, bool shown)
        {
            host = isPeek ? IntPtr.Zero : desktop;
            peek = isPeek;
            if (!shown || (!peek && host == IntPtr.Zero))
            {
                hoverTimer.Stop();
                hoverExpanded = false;
                Hide();
                return;
            }
            var bounds = VisibleBounds();
            if (!WindowsShell.Mount(Handle, host, peek, bounds))
            {
                Hide();
                return;
            }
            appliedOpacity = -1;
            ApplyOpacity();
            if (!Visible) Show();
            WindowsShell.Position(Handle, host, VisibleBounds(), true, peek);
            Invalidate();
        }

        internal void UpdateGroup()
        {
            Text = "DeskGrid · " + Group.Title;
            UpdateColors();
            if (!Frame.HoverExpand)
            {
                hoverExpanded = false;
                hoverTimer.Stop();
            }
            ApplyOpacity();
            WindowsShell.Position(Handle, host, VisibleBounds(), false, peek);
            firstRow = Math.Min(firstRow, LastRow);
            Invalidate();
        }

        internal void SetItems(FileEntry[] newItems, bool isLoading, bool isUnavailable)
        {
            if (loading == isLoading && unavailable == isUnavailable && items.Length == newItems.Length
                && items.Zip(newItems, (oldItem, newItem) => oldItem.Path == newItem.Path
                    && oldItem.ModifiedUtc == newItem.ModifiedUtc && oldItem.IsDirectory == newItem.IsDirectory &&
                    oldItem.SizeBytes == newItem.SizeBytes).All(equal => equal))
                return;
            loading = isLoading;
            unavailable = isUnavailable;
            items = newItems;
            if (IndexOfSelection() < 0) selectedPath = null;
            firstRow = Math.Min(firstRow, LastRow);
            if (!loading && pendingReveal != null)
            {
                RevealSelection(pendingReveal);
                pendingReveal = null;
            }
            hovered = -1;
            tip.SetToolTip(this, null);
            Invalidate();
        }

        private int ScalePx(int logical) { return Math.Max(1, (logical * dpi + 48) / 96); }

        private Rectangle VisibleBounds()
        {
            var bounds = Frame.Bounds;
            if (IsCollapsed) bounds.Height = Header;
            return bounds;
        }

        internal void ResetHover()
        {
            hoverTimer.Stop();
            hoverExpanded = false;
            UpdateGroup();
        }

        internal void KeepHoverOpen()
        {
            hoverExpanded = true;
            ScheduleHover(false);
        }

        private void ScheduleHover(bool expand)
        {
            hoverTimer.Stop();
            if (!Frame.Collapsed || !Frame.HoverExpand || hoverExpanded == expand) return;
            hoverTimer.Interval = Math.Max(1, expand ? Frame.ExpandDelay : Frame.RetractDelay);
            hoverTimer.Start();
        }

        private void ApplyHover()
        {
            hoverTimer.Stop();
            if (!Visible || !Frame.Collapsed || !Frame.HoverExpand) return;
            bool inside = ClientRectangle.Contains(PointToClient(Cursor.Position));
            if (!inside && (contextOpen || DraggingFile || dropHover || mouseAction != MouseAction.None))
            {
                ScheduleHover(false);
                return;
            }
            if (hoverExpanded == inside) return;
            hoverExpanded = inside;
            WindowsShell.Position(Handle, host, VisibleBounds(), inside, peek);
            app.UpdateQuickHideTargets();
            Invalidate();
        }

        private void ApplyOpacity()
        {
            if (!IsHandleCreated) return;
            if (loose)
            {
                if (appliedOpacity != 100) WindowsShell.SetIconTransparency(Handle, LooseBackground);
                appliedOpacity = 100;
                return;
            }
            int opacity = Group.DimWhenIdle && !mouseInside && !peek ? Math.Max(30, Group.Opacity - 30) : Group.Opacity;
            if (opacity == appliedOpacity) return;
            WindowsShell.SetPanelOpacity(Handle, opacity);
            appliedOpacity = opacity;
        }

        private void UpdateColors()
        {
            if (loose) { BackColor = LooseBackground; return; }
            var accent = Theme.Accents[Group.Accent];
            BackColor = Theme.Blend(Theme.Background, accent, 5);
            if (iconTint != null) { iconTint.Dispose(); iconTint = null; }
            if (!Group.TintIcons) return;
            float r = accent.R / 255f, g = accent.G / 255f, b = accent.B / 255f;
            iconTint = new ImageAttributes();
            iconTint.SetColorMatrix(new ColorMatrix(new[]
            {
                new[] { .26f * r, .26f * g, .26f * b, 0f, 0f },
                new[] { .62f * r, .62f * g, .62f * b, 0f, 0f },
                new[] { .12f * r, .12f * g, .12f * b, 0f, 0f },
                new[] { 0f, 0f, 0f, 1f, 0f },
                new[] { .20f, .20f, .20f, 0f, 1f }
            }));
        }

        private void UpdateDpi()
        {
            int current = WindowsShell.Dpi(IsHandleCreated ? Handle : IntPtr.Zero);
            if (titleFont != null && current == dpi) return;
            dpi = current;
            DisposeFonts();
            titleFont = new Font("Microsoft YaHei UI", ScalePx(14), FontStyle.Bold, GraphicsUnit.Pixel);
            detailFont = new Font("Microsoft YaHei UI", ScalePx(11), FontStyle.Regular, GraphicsUnit.Pixel);
            itemFont = new Font("Microsoft YaHei UI", ScalePx(12), FontStyle.Regular, GraphicsUnit.Pixel);
            UpdateRegion();
        }

        private void DisposeFonts()
        {
            if (titleFont != null) titleFont.Dispose();
            if (detailFont != null) detailFont.Dispose();
            if (itemFont != null) itemFont.Dispose();
        }

        private void UpdateRegion()
        {
            if (ClientSize.Width < 2 || ClientSize.Height < 2) return;
            using (var shape = Theme.Rounded(ClientRectangle, ScalePx(14)))
            {
                var old = Region;
                Region = new Region(shape);
                if (old != null) old.Dispose();
            }
        }

        protected override void OnResize(EventArgs e)
        {
            base.OnResize(e);
            if (Group == null) return;
            firstRow = Math.Min(firstRow, LastRow);
            UpdateRegion();
        }

        protected override void WndProc(ref Message message)
        {
            if (message.Msg == WindowsShell.MouseActivateMessage && !peek)
            {
                message.Result = new IntPtr(3); // MA_NOACTIVATE still delivers the click.
                return;
            }
            base.WndProc(ref message);
            if (message.Msg == WindowsShell.DpiChangedMessage)
            {
                UpdateDpi();
                Invalidate();
            }
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            if (disposingWindow) return;
            var graphics = e.Graphics;
            graphics.SmoothingMode = SmoothingMode.AntiAlias;
            if (loose)
            {
                DrawLooseIcon(graphics);
                return;
            }
            var accent = Theme.Accents[Group.Accent];
            var titleArea = new Rectangle(0, 0, Math.Max(1, ClientSize.Width), Header);
            using (var surface = new LinearGradientBrush(titleArea, Theme.Blend(Theme.Surface, accent, 20),
                Theme.Blend(Theme.Surface, accent, 7), LinearGradientMode.Horizontal))
                graphics.FillRectangle(surface, titleArea);
            using (var outline = new Pen(dropHover ? accent : Theme.Border, dropHover ? 2 : 1))
            using (var shape = Theme.Rounded(new Rectangle(1, 1, Math.Max(2, Width - 2), Math.Max(2, Height - 2)), ScalePx(13)))
                graphics.DrawPath(outline, shape);
            DrawHeader(graphics, accent);
            if (IsCollapsed) return;
            DrawTabBar(graphics);
            if (Group.IsPortal) DrawPortalBar(graphics);
            if (DetailsHeader != 0) DrawDetailsHeader(graphics);
            if (items.Length > 0) DrawGrid(graphics);
            else
            {
                string message = loading ? "正在读取…" : unavailable ? "无法读取此文件夹" :
                    Group.IsPortal ? "文件夹为空" : "拖入文件，或添加自动分类规则";
                TextRenderer.DrawText(graphics, message, itemFont,
                    new Rectangle(ScalePx(12), GridTop, Math.Max(1, Width - ScalePx(24)), Math.Max(1, GridBottom - GridTop)), Theme.Muted,
                    SingleLine | TextFormatFlags.HorizontalCenter);
            }
            if (!Frame.Locked) DrawGrip(graphics);
            if (!insertionMark.IsEmpty)
                using (var brush = new SolidBrush(accent)) graphics.FillRectangle(brush, insertionMark);
        }

        private void DrawLooseIcon(Graphics graphics)
        {
            if (items.Length == 0) return;
            var tile = ClientRectangle;
            if (mouseInside || selectedPath != null || dropHover)
            {
                using (var brush = new SolidBrush(dropHover ? Theme.Selected : Theme.Hover))
                using (var shape = Theme.Rounded(Rectangle.Inflate(tile, -ScalePx(2), -ScalePx(2)), ScalePx(8)))
                    graphics.FillPath(brush, shape);
            }
            int icon = ScalePx(34);
            app.Icons.Draw(graphics, items[0], new Rectangle((Width - icon) / 2, ScalePx(7), icon, icon), null);
            var label = new Rectangle(ScalePx(4), ScalePx(44), Width - ScalePx(8), ScalePx(29));
            var flags = TextFormatFlags.WordBreak | TextFormatFlags.EndEllipsis | TextFormatFlags.HorizontalCenter |
                TextFormatFlags.TextBoxControl | TextFormatFlags.NoPrefix;
            var shadow = label;
            shadow.Offset(0, ScalePx(1));
            TextRenderer.DrawText(graphics, items[0].Label, itemFont, shadow, Color.Black, flags);
            TextRenderer.DrawText(graphics, items[0].Label, itemFont, label, Color.White, flags);
        }

        private void DrawHeader(Graphics graphics, Color accent)
        {
            using (var color = new SolidBrush(accent))
            using (var mark = Theme.Rounded(new Rectangle(ScalePx(15), ScalePx(13), ScalePx(4), ScalePx(16)), ScalePx(2)))
                graphics.FillPath(color, mark);
            string count = items.Length.ToString();
            var countSize = TextRenderer.MeasureText(graphics, count, detailFont, Size.Empty, TextFormatFlags.NoPadding);
            var title = new Rectangle(ScalePx(28), 0, Math.Max(1, CollapseButton.Left - ScalePx(36) - countSize.Width), Header);
            TextRenderer.DrawText(graphics, Group.Title, titleFont, title, Theme.Text, SingleLine);
            var titleWidth = Math.Min(title.Width, TextRenderer.MeasureText(graphics, Group.Title, titleFont, Size.Empty, TextFormatFlags.NoPadding).Width);
            TextRenderer.DrawText(graphics, count, detailFont,
                new Rectangle(title.Left + titleWidth + ScalePx(7), 0, countSize.Width + 1, Header), accent, SingleLine);
            DrawButton(graphics, MenuButton, "…", Theme.Muted);
            DrawButton(graphics, CollapseButton, IsCollapsed ? "▾" : hoverExpanded ? "⊙" : "⌃", Theme.Muted);
            if (!IsCollapsed)
            {
                using (var border = new Pen(Theme.Border))
                    graphics.DrawLine(border, 1, Header - 1, ClientSize.Width - 2, Header - 1);
            }
        }

        private void DrawButton(Graphics graphics, Rectangle rect, string label, Color text)
        {
            if (rect.Contains(PointToClient(Cursor.Position)))
            {
                using (var background = new SolidBrush(Theme.Hover))
                using (var rounded = Theme.Rounded(rect, ScalePx(6)))
                    graphics.FillPath(background, rounded);
            }
            TextRenderer.DrawText(graphics, label, titleFont, rect, text, SingleLine | TextFormatFlags.HorizontalCenter);
        }

        private void DrawPortalBar(Graphics graphics)
        {
            if (previousFolders.Count != 0) DrawButton(graphics, BackButton, "‹", Theme.Text);
            DrawButton(graphics, FolderButton, "↗", Theme.Muted);
            int x = previousFolders.Count == 0 ? ScalePx(15) : BackButton.Right + ScalePx(4);
            TextRenderer.DrawText(graphics, folder, detailFont,
                new Rectangle(x, PortalTop, Math.Max(1, FolderButton.Left - x - ScalePx(4)), Sidebar), Theme.Muted,
                SingleLine | TextFormatFlags.PathEllipsis);
        }

        private IEnumerable<KeyValuePair<DesktopGroup, Rectangle>> TabSlots()
        {
            if (IsCollapsed) yield break;
            var tabs = app.Layout.Tabs(Group);
            if (tabs.Length < 2) yield break;
            int max = Math.Max(2, (ClientSize.Width - ScalePx(18)) / ScalePx(68));
            int count = Math.Min(max, tabs.Length);
            int first = Math.Max(0, Array.IndexOf(tabs, Group) - count + 1);
            int cell = (ClientSize.Width - ScalePx(18)) / count;
            for (int i = 0; i < count; i++)
                yield return new KeyValuePair<DesktopGroup, Rectangle>(tabs[first + i],
                    new Rectangle(ScalePx(9) + cell * i, Header + ScalePx(4), Math.Max(1, cell - ScalePx(3)), ScalePx(25)));
        }

        private DesktopGroup TabAt(Point point)
        {
            return TabSlots().FirstOrDefault(slot => slot.Value.Contains(point)).Key;
        }

        private void DrawTabBar(Graphics graphics)
        {
            var pointer = PointToClient(Cursor.Position);
            foreach (var slot in TabSlots())
            {
                var accent = Theme.Accents[slot.Key.Accent];
                var rect = slot.Value;
                bool selected = slot.Key == Group;
                using (var brush = new SolidBrush(Theme.Blend(Theme.Surface, accent, selected ? 28 : rect.Contains(pointer) ? 19 : 8)))
                using (var outline = Theme.Rounded(rect, ScalePx(7)))
                    graphics.FillPath(brush, outline);
                TextRenderer.DrawText(graphics, slot.Key.Title, detailFont, Rectangle.Inflate(rect, -ScalePx(7), 0),
                    selected ? Theme.Text : Theme.Muted, SingleLine | TextFormatFlags.HorizontalCenter);
            }
        }

        private Rectangle Cell(int index)
        {
            return new Rectangle(ScalePx(9) + (index % Columns) * CellWidth, GridTop + (index / Columns - firstRow) * CellHeight, CellWidth, CellHeight);
        }

        private void DrawGrid(Graphics graphics)
        {
            int columns = Columns;
            var state = graphics.Save();
            var grid = new Rectangle(ScalePx(7), GridTop, Math.Max(1, ClientSize.Width - ScalePx(14)), GridBottom - GridTop);
            graphics.SetClip(grid);
            int selected = IndexOfSelection();
            for (int index = firstRow * columns; index < Math.Min(items.Length, (firstRow + VisibleRows) * columns); index++)
            {
                var rect = Cell(index);
                var item = items[index];
                bool destination = String.Equals(dropItemPath, item.Path, StringComparison.OrdinalIgnoreCase);
                if (index == hovered || index == selected || destination)
                {
                    var highlight = destination ? Theme.Blend(Theme.Selected,
                        item.IsRecycleBin ? Theme.Accents[4] : Theme.Accents[Group.Accent], 35) :
                        index == selected ? Theme.Selected : Theme.Hover;
                    using (var brush = new SolidBrush(highlight))
                    using (var shape = Theme.Rounded(Rectangle.Inflate(rect, -ScalePx(2), -ScalePx(2)), ScalePx(8)))
                        graphics.FillPath(brush, shape);
                }
                if (Group.View == PanelView.Icons) DrawIconCell(graphics, item, rect);
                else DrawListCell(graphics, item, rect);
            }
            graphics.Restore(state);
            if (LastRow == 0) return;
            int track = Math.Max(1, grid.Height);
            int thumb = Math.Min(track, Math.Max(ScalePx(14), track * VisibleRows / (LastRow + VisibleRows)));
            int y = GridTop + (track - thumb) * firstRow / LastRow;
            using (var shape = Theme.Rounded(new Rectangle(Width - ScalePx(7), y, ScalePx(3), thumb), ScalePx(2)))
            using (var brush = new SolidBrush(Theme.Border))
                graphics.FillPath(brush, shape);
        }

        private void DrawIconCell(Graphics graphics, FileEntry item, Rectangle rect)
        {
            int size = ScalePx(Group.IconSize);
            app.Icons.Draw(graphics, item, new Rectangle(rect.X + (rect.Width - size) / 2, rect.Y + ScalePx(7), size, size), iconTint);
            TextRenderer.DrawText(graphics, item.Label, itemFont,
                new Rectangle(rect.X + ScalePx(4), rect.Y + size + ScalePx(9), Math.Max(1, rect.Width - ScalePx(8)), ScalePx(26)),
                Theme.Text, TextFormatFlags.WordBreak | TextFormatFlags.EndEllipsis | TextFormatFlags.HorizontalCenter |
                TextFormatFlags.TextBoxControl | TextFormatFlags.NoPrefix);
        }

        private int SizeWidth { get { return Group.View == PanelView.Details ? ScalePx(60) : 0; } }
        private int DateWidth { get { return Group.View == PanelView.Details && ClientSize.Width >= ScalePx(335) ? ScalePx(104) : 0; } }

        private void DrawDetailsHeader(Graphics graphics)
        {
            var rect = new Rectangle(ScalePx(15), ContentTop + ScalePx(4), ClientSize.Width - ScalePx(29), DetailsHeader);
            TextRenderer.DrawText(graphics, "名称", detailFont, rect, Theme.Muted, SingleLine);
            DrawDetailColumns(graphics, rect, "大小", "修改时间");
            using (var border = new Pen(Theme.Border))
                graphics.DrawLine(border, rect.X, rect.Bottom, rect.Right, rect.Bottom);
        }

        private void DrawListCell(Graphics graphics, FileEntry item, Rectangle rect)
        {
            int size = ScalePx(Group.IconSize);
            int x = rect.Left + ScalePx(7);
            app.Icons.Draw(graphics, item, new Rectangle(x, rect.Y + (rect.Height - size) / 2, size, size), iconTint);
            int start = x + size + ScalePx(8);
            TextRenderer.DrawText(graphics, item.Label, itemFont,
                new Rectangle(start, rect.Top, Math.Max(1, rect.Right - SizeWidth - DateWidth - start - ScalePx(6)), rect.Height),
                Theme.Text, SingleLine);
            if (DetailsHeader != 0)
                DrawDetailColumns(graphics, Rectangle.Inflate(rect, -ScalePx(5), 0), FileSize(item.SizeBytes),
                    item.ModifiedUtc == DateTime.MinValue ? "" : item.ModifiedUtc.ToLocalTime().ToString("yyyy-MM-dd"));
        }

        private void DrawDetailColumns(Graphics graphics, Rectangle rect, string size, string date)
        {
            TextRenderer.DrawText(graphics, size, detailFont,
                new Rectangle(rect.Right - DateWidth - SizeWidth, rect.Top, SizeWidth - ScalePx(7), rect.Height),
                Theme.Muted, SingleLine | TextFormatFlags.Right);
            if (DateWidth != 0)
                TextRenderer.DrawText(graphics, date, detailFont, new Rectangle(rect.Right - DateWidth, rect.Top, DateWidth, rect.Height),
                    Theme.Muted, SingleLine | TextFormatFlags.Right);
        }

        private static string FileSize(long bytes)
        {
            if (bytes < 0) return "";
            if (bytes < 1024) return bytes + " B";
            double value = bytes / 1024.0;
            string[] units = { "KB", "MB", "GB", "TB" };
            int unit = 0;
            while (value >= 1024 && unit < units.Length - 1) { value /= 1024; unit++; }
            return value.ToString(value >= 10 ? "0" : "0.#") + " " + units[unit];
        }

        private void DrawGrip(Graphics graphics)
        {
            using (var pen = new Pen(Theme.Border))
            {
                int x = Width - ScalePx(5), y = Height - ScalePx(5);
                graphics.DrawLine(pen, x - ScalePx(7), y, x, y - ScalePx(7));
                graphics.DrawLine(pen, x - ScalePx(3), y, x, y - ScalePx(3));
            }
        }

        private int HitItem(Point point)
        {
            if (loose) return items.Length != 0 && ClientRectangle.Contains(point) ? 0 : -1;
            if (IsCollapsed || point.Y < GridTop || point.Y >= GridBottom || point.X < ScalePx(9)) return -1;
            int column = (point.X - ScalePx(9)) / CellWidth;
            int row = (point.Y - GridTop) / CellHeight;
            int index = (row + firstRow) * Columns + column;
            return column < Columns && row < VisibleRows && index < items.Length ? index : -1;
        }

        private int IndexOfSelection()
        {
            if (selectedPath == null) return -1;
            return Array.FindIndex(items, item => String.Equals(item.Path, selectedPath, StringComparison.OrdinalIgnoreCase));
        }

        protected override void OnMouseDown(MouseEventArgs e)
        {
            base.OnMouseDown(e);
            if (disposingWindow) return;
            if (e.Button == MouseButtons.Left || e.Button == MouseButtons.Right)
                WindowsShell.Raise(Handle, peek);
            int hit = HitItem(e.Location);
            if (e.Button == MouseButtons.Right)
            {
                if (hit >= 0) selectedPath = items[hit].Path;
                Invalidate();
                ShowMenu(e.Location, hit);
                return;
            }
            if (e.Button != MouseButtons.Left) return;
            handledMouseDown = loose;
            if (!loose && (CollapseButton.Contains(e.Location) || MenuButton.Contains(e.Location) ||
                (Group.IsPortal && !IsCollapsed && (BackButton.Contains(e.Location) || FolderButton.Contains(e.Location))) ||
                TabAt(e.Location) != null))
                return;
            pressedScreen = PointToScreen(e.Location);
            initialBounds = Frame.Bounds;
            geometryDragging = false;
            if (!loose && !Frame.Locked && !IsCollapsed && Grip.Contains(e.Location)) mouseAction = MouseAction.Resize;
            else if (!loose && !Frame.Locked && e.Y < Header) mouseAction = MouseAction.Move;
            else if (hit >= 0)
            {
                if (peek) Focus();
                var item = items[hit];
                selectedPath = item.Path;
                if (!item.IsRecycleBin)
                {
                    mouseAction = MouseAction.Item;
                    pressedItem = item;
                }
            }
            else selectedPath = null;
            if (mouseAction != MouseAction.None)
            {
                handledMouseDown = true;
                Capture = true;
            }
            Invalidate();
        }

        protected override void OnMouseMove(MouseEventArgs e)
        {
            base.OnMouseMove(e);
            var screen = PointToScreen(e.Location);
            int dx = screen.X - pressedScreen.X, dy = screen.Y - pressedScreen.Y;
            if (mouseAction != MouseAction.None && (e.Button & MouseButtons.Left) == 0)
                EndGesture();
            bool crossedThreshold = Math.Abs(dx) >= SystemInformation.DragSize.Width / 2 ||
                Math.Abs(dy) >= SystemInformation.DragSize.Height / 2;
            if (mouseAction == MouseAction.Move || mouseAction == MouseAction.Resize)
            {
                if (!geometryDragging && !crossedThreshold) return;
                geometryDragging = true;
                var proposed = initialBounds;
                if (mouseAction == MouseAction.Move) proposed.Offset(dx, dy);
                else proposed.Size = new Size(proposed.Width + dx, proposed.Height + dy);
                Frame.Bounds = mouseAction == MouseAction.Move ? app.SnapMove(Frame, proposed, screen) : app.FitBounds(proposed, screen);
                WindowsShell.Position(Handle, host, VisibleBounds(), false, peek);
                return;
            }
            if (mouseAction == MouseAction.Item && pressedItem != null && crossedThreshold)
            {
                var picked = pressedItem;
                mouseAction = MouseAction.None;
                pressedItem = null;
                Capture = false;
                tip.SetToolTip(this, null);
                var data = new DataObject(DataFormats.FileDrop, new[] { picked.Path });
                wallpaperDrop = null;
                int size = ScalePx(loose ? 34 : Group.IconSize);
                using (var preview = new DragPreview(app.Icons, picked, size, iconTint))
                {
                    dragPreview = preview;
                    try
                    {
                        preview.Begin();
                        DoDragDrop(data, DragDropEffects.Copy | DragDropEffects.Move | DragDropEffects.Link);
                    }
                    finally
                    {
                        preview.End();
                        dragPreview = null;
                        if (!disposingWindow && !ClientRectangle.Contains(PointToClient(Cursor.Position))) ScheduleHover(false);
                    }
                }
                var drop = wallpaperDrop;
                wallpaperDrop = null;
                if (drop.HasValue) app.PlaceLoose(picked, drop.Value);
                else if (loose) app.RefreshContents();
                return;
            }
            Cursor = !loose && !Frame.Locked && !IsCollapsed && Grip.Contains(e.Location) ? Cursors.SizeNWSE : Cursors.Default;
            int hit = HitItem(e.Location);
            if (hit != hovered)
            {
                hovered = hit;
                tip.SetToolTip(this, hit >= 0 ? items[hit].Name : null);
            }
            Invalidate();
        }

        protected override void OnQueryContinueDrag(QueryContinueDragEventArgs e)
        {
            base.OnQueryContinueDrag(e);
            if (!DraggingFile || (e.Action == DragAction.Continue && !e.EscapePressed)) return;
            dragPreview.End();
            if (e.EscapePressed || e.Action != DragAction.Drop) return;
            var point = Cursor.Position;
            if (!app.IsWallpaperDrop(point)) return;
            wallpaperDrop = point;
            e.Action = DragAction.Cancel;
        }

        protected override void OnGiveFeedback(GiveFeedbackEventArgs e)
        {
            base.OnGiveFeedback(e);
            if (!DraggingFile || !app.IsWallpaperDrop(Cursor.Position)) return;
            e.UseDefaultCursors = false;
            Cursor.Current = Cursors.Default;
        }

        protected override void OnMouseUp(MouseEventArgs e)
        {
            base.OnMouseUp(e);
            if (e.Button != MouseButtons.Left) return;
            EndGesture();
            if (handledMouseDown || loose) return;
            if (MenuButton.Contains(e.Location)) ShowMenu(new Point(MenuButton.Left, MenuButton.Bottom), -1);
            else if (CollapseButton.Contains(e.Location)) app.ToggleCollapsed(this);
            else if (!IsCollapsed && Group.IsPortal && FolderButton.Contains(e.Location)) app.Open(Folder);
            else if (!IsCollapsed && Group.IsPortal && BackButton.Contains(e.Location)) GoBack();
            else
            {
                var tab = TabAt(e.Location);
                if (tab != null) app.SelectTab(tab);
            }
        }

        private void EndGesture()
        {
            MouseAction action = mouseAction;
            mouseAction = MouseAction.None;
            pressedItem = null;
            if (Capture) Capture = false;
            if ((action == MouseAction.Move || action == MouseAction.Resize) && Frame.Bounds != initialBounds)
            {
                app.UpdateQuickHideTargets();
                app.Save();
            }
        }

        protected override void OnMouseCaptureChanged(EventArgs e)
        {
            base.OnMouseCaptureChanged(e);
            if (!disposingWindow && !Capture && mouseAction != MouseAction.None) EndGesture();
        }

        protected override void OnMouseLeave(EventArgs e)
        {
            base.OnMouseLeave(e);
            if (disposingWindow) return;
            mouseInside = false;
            if (loose) selectedPath = null;
            ApplyOpacity();
            ScheduleHover(false);
            hovered = -1;
            tip.SetToolTip(this, null);
            Invalidate();
        }

        protected override void OnMouseEnter(EventArgs e)
        {
            base.OnMouseEnter(e);
            if (disposingWindow) return;
            mouseInside = true;
            ApplyOpacity();
            ScheduleHover(true);
        }

        protected override void OnMouseDoubleClick(MouseEventArgs e)
        {
            base.OnMouseDoubleClick(e);
            if (e.Button != MouseButtons.Left || DraggingFile || disposingWindow) return;
            int hit = HitItem(e.Location);
            if (hit >= 0) OpenItem(items[hit]);
            else if (!loose && e.Y < Header && !CollapseButton.Contains(e.Location) && !MenuButton.Contains(e.Location))
                app.ToggleCollapsed(this);
        }

        private void OpenItem(FileEntry item)
        {
            if (item.IsRecycleBin) { app.OpenSystemFolder(FileEntry.RecycleBinFolder); return; }
            if (!Group.IsPortal || !item.IsDirectory) { app.Open(item.Path); return; }
            previousFolders.Push(folder);
            folder = item.Path;
            ChangeFolder();
        }

        private void GoBack()
        {
            if (previousFolders.Count == 0) return;
            folder = previousFolders.Pop();
            ChangeFolder();
        }

        private void ChangeFolder()
        {
            firstRow = 0;
            selectedPath = null;
            SetItems(new FileEntry[0], true, false);
            app.RefreshContents();
        }

        internal void RevealSearchResult(PanelSearchResult result)
        {
            if (Group.IsPortal && !String.Equals(Folder, result.Folder, StringComparison.OrdinalIgnoreCase))
            {
                previousFolders.Clear();
                if (!String.Equals(Group.Folder, result.Folder, StringComparison.OrdinalIgnoreCase))
                    previousFolders.Push(Group.Folder);
                folder = result.Folder;
                pendingReveal = result.Entry.Path;
                ChangeFolder();
                return;
            }
            RevealSelection(result.Entry.Path);
        }

        private void RevealSelection(string path)
        {
            selectedPath = path;
            int index = IndexOfSelection();
            if (index < 0) return;
            int row = index / Columns;
            if (!loose) firstRow = Math.Max(0, Math.Min(LastRow, row));
            Invalidate();
        }

        protected override void OnMouseWheel(MouseEventArgs e)
        {
            base.OnMouseWheel(e);
            firstRow = Math.Max(0, Math.Min(LastRow, firstRow - Math.Sign(e.Delta)));
            hovered = -1;
            Invalidate();
        }

        protected override bool IsInputKey(Keys keyData)
        {
            var key = keyData & Keys.KeyCode;
            return key == Keys.Up || key == Keys.Down || key == Keys.Left || key == Keys.Right || base.IsInputKey(keyData);
        }

        protected override void OnKeyDown(KeyEventArgs e)
        {
            base.OnKeyDown(e);
            if (IsCollapsed) return;
            if (e.KeyCode == Keys.Back && Group.IsPortal) { GoBack(); e.Handled = true; return; }
            if (items.Length == 0) return;
            int index = IndexOfSelection();
            if (e.KeyCode == Keys.Enter && index >= 0) { OpenItem(items[index]); e.Handled = true; return; }
            int delta;
            switch (e.KeyCode)
            {
                case Keys.Left: delta = -1; break;
                case Keys.Right: delta = 1; break;
                case Keys.Up: delta = -Columns; break;
                case Keys.Down: delta = Columns; break;
                default: return;
            }
            index = index < 0 ? 0 : Math.Min(items.Length - 1, Math.Max(0, index + delta));
            selectedPath = items[index].Path;
            int row = index / Columns;
            firstRow = Math.Min(Math.Max(row - VisibleRows + 1, firstRow), row);
            e.Handled = true;
            Invalidate();
        }

        private void ShowMenu(Point point, int index)
        {
            var menu = Theme.Menu();
            if (index >= 0 && items[index].IsRecycleBin)
                menu.Items.Add("打开回收站", null, delegate { app.OpenSystemFolder(FileEntry.RecycleBinFolder); });
            else if (index >= 0)
            {
                var item = items[index];
                menu.Items.Add(Group.IsPortal && item.IsDirectory ? "在面板中打开" : "打开", null, delegate { OpenItem(item); });
                menu.Items.Add("在资源管理器中定位", null, delegate { app.Reveal(item.Path); });
                menu.Items.Add("复制路径", null, delegate { app.CopyPath(item.Path); });
                if (!Group.IsPortal)
                {
                    var move = new ToolStripMenuItem("归入分组") { ForeColor = Theme.Text };
                    foreach (var target in app.Layout.Groups.Where(group => !group.IsPortal && group.Id != Group.Id))
                    {
                        var selected = target;
                        move.DropDownItems.Add(selected.Title, null, delegate { app.Assign(selected, new[] { item.Path }); }).ForeColor = Theme.Text;
                    }
                    if (move.DropDownItems.Count != 0) menu.Items.Add(move);
                    if (app.IsManuallyPlaced(item.Path))
                        menu.Items.Add(app.IsDesktopItem(item.Path) ? "恢复自动分类" : "取消固定", null, delegate { app.ClearPlacement(item.Path); });
                }
            }
            else
            {
                menu.Items.Add("编辑面板…", null, delegate { app.EditGroup(Group); });
                menu.Items.Add(Frame.Collapsed ? "展开" : "折叠", null, delegate { app.ToggleCollapsed(this); });
                menu.Items.Add(Frame.Locked ? "解锁位置" : "锁定位置", null, delegate { Frame.Locked = !Frame.Locked; Invalidate(); app.Save(); });
                menu.Items.Add("在此面板搜索…", null, delegate { app.ShowSearch(Group); });
                menu.Items.Add("显示在哪些页面…", null, delegate { app.EditPageMembership(Group); });
                AddTabMenu(menu);
                menu.Items.Add(new ToolStripSeparator());
                menu.Items.Add("管理桌面…", null, delegate { app.ShowSettings(); });
            }
            contextOpen = true;
            menu.Closed += delegate
            {
                contextOpen = false;
                if (!disposingWindow && !ClientRectangle.Contains(PointToClient(Cursor.Position))) ScheduleHover(false);
                menu.Dispose();
            };
            menu.Show(PointToScreen(point));
        }

        private void AddTabMenu(ContextMenuStrip menu)
        {
            var tabs = app.Layout.Tabs(Group);
            if (tabs.Length > 1)
            {
                var open = new ToolStripMenuItem("切换标签") { ForeColor = Theme.Text };
                foreach (var tab in tabs)
                    open.DropDownItems.Add(tab.Title, null, delegate { app.SelectTab(tab); }).ForeColor = Theme.Text;
                menu.Items.Add(open);
                menu.Items.Add("拆出当前标签", null, delegate { app.SeparateTab(Group); });
            }
            var other = app.Layout.Groups.Where(group => group.TabHostId == null && group != Frame).ToArray();
            if (other.Length == 0) return;
            var merge = new ToolStripMenuItem("与其他面板合并为标签") { ForeColor = Theme.Text };
            foreach (var target in other)
                merge.DropDownItems.Add(target.Title, null, delegate { app.MergeTabs(Group, target); }).ForeColor = Theme.Text;
            menu.Items.Add(merge);
        }

        protected override void OnDragEnter(DragEventArgs e)
        {
            dropPaths = ReadDropPaths(e);
            UpdateDrop(e, false);
        }

        private static string[] ReadDropPaths(DragEventArgs e)
        {
            if (e.Data == null || !e.Data.GetDataPresent(DataFormats.FileDrop, false)) return new string[0];
            var paths = e.Data.GetData(DataFormats.FileDrop, false) as string[];
            if (paths == null) return new string[0];
            var full = paths.Select(FileEntry.FullPath).ToArray();
            return full.Any(path => path == null) ? new string[0] : full;
        }

        protected override void OnDragOver(DragEventArgs e) { UpdateDrop(e, false); }
        private void UpdateDrop(DragEventArgs e, bool perform)
        {
            var point = PointToClient(new Point(e.X, e.Y));
            var target = TabAt(point) ?? Group;
            var paths = dropPaths ?? ReadDropPaths(e);
            lastDragOver = perform ? null : e;
            dragScroll.Enabled = !perform && paths.Length > 0 && DragScrollStep(point) != 0;
            int hit = HitItem(point);
            string destination = ShellDestination(point, target, hit);
            var mark = Rectangle.Empty;
            e.Effect = DragDropEffects.None;
            if (destination != null)
            {
                if (ShellDrop.CanDrop(destination, paths, e.KeyState))
                {
                    e.Effect = shellDrop.Hover(Handle, destination, e);
                    if (perform && e.Effect != DragDropEffects.None) e.Effect = shellDrop.Drop(e);
                }
                else shellDrop.Dispose();
            }
            else
            {
                shellDrop.Dispose();
                if (!loose && !target.IsPortal && paths.Length > 0 &&
                    paths.All(path => !String.Equals(path, System.IO.Path.GetPathRoot(path), StringComparison.OrdinalIgnoreCase)))
                {
                    e.Effect = (e.AllowedEffect & DragDropEffects.Link) != 0 ? DragDropEffects.Link : e.AllowedEffect & DragDropEffects.Copy;
                    int slot = -1;
                    if (target == Group && !IsCollapsed && point.Y >= GridTop && (hit >= 0 || DraggingFile || Group.Sort == SortOrder.Manual))
                        slot = InsertionSlot(point, out mark);
                    if (slot >= 0 && DraggingFile && (e.AllowedEffect & DragDropEffects.Move) != 0)
                        e.Effect = DragDropEffects.Move;
                    if (perform && e.Effect != DragDropEffects.None)
                    {
                        app.Assign(target, paths, target == Group ? items : null, slot);
                        app.SelectTab(target);
                    }
                }
            }
            bool active = e.Effect != DragDropEffects.None;
            string highlight = active ? destination : null;
            if (!active) mark = Rectangle.Empty;
            if (dropHover == active && highlight == dropItemPath && mark == insertionMark) return;
            dropHover = active;
            dropItemPath = highlight;
            insertionMark = mark;
            Invalidate();
        }

        private string ShellDestination(Point point, DesktopGroup target, int index)
        {
            if (index >= 0 && (items[index].IsRecycleBin || items[index].IsDirectory))
            {
                if (loose || Group.IsPortal) return items[index].Path;
                var cell = Cell(index);
                bool icons = Group.View == PanelView.Icons;
                int offset = icons ? point.X - cell.Left : point.Y - cell.Top;
                int length = icons ? cell.Width : cell.Height;
                if (offset >= length / 4 && offset < length - length / 4) return items[index].Path;
            }
            return target.IsPortal ? (target == Group ? Folder : target.Folder) : null;
        }

        private int InsertionSlot(Point point, out Rectangle mark)
        {
            int row = Math.Max(0, Math.Min(VisibleRows - 1, (point.Y - GridTop) / CellHeight));
            int column = Math.Max(0, Math.Min(Columns - 1, (point.X - ScalePx(9)) / CellWidth));
            int index = (firstRow + row) * Columns + column;
            bool icons = Group.View == PanelView.Icons;
            var cell = Cell(index);
            bool after = icons ? point.X >= cell.Left + cell.Width / 2 : point.Y >= cell.Top + cell.Height / 2;
            if (point.Y >= GridTop + VisibleRows * CellHeight)
            {
                index = (firstRow + VisibleRows) * Columns - 1;
                after = true;
            }
            if (index >= items.Length) { index = Math.Max(0, items.Length - 1); after = items.Length > 0; }
            if (items.Length > 0 && items[index].IsRecycleBin) after = true;
            cell = Cell(index);
            int inset = ScalePx(4), width = ScalePx(2);
            mark = icons
                ? new Rectangle(after ? cell.Right - width : cell.Left, cell.Top + inset, width, cell.Height - 2 * inset)
                : new Rectangle(cell.Left + inset, after ? cell.Bottom - width : cell.Top, cell.Width - 2 * inset, width);
            mark.Intersect(new Rectangle(0, GridTop, Width, GridBottom - GridTop));
            return after ? index + 1 : index;
        }

        private int DragScrollStep(Point point)
        {
            if (loose || IsCollapsed || !ClientRectangle.Contains(point) || point.Y < GridTop) return 0;
            int edge = Math.Min(ScalePx(16), CellHeight / 3);
            if (point.Y < GridTop + edge && firstRow > 0) return -1;
            return point.Y >= GridBottom - edge && firstRow < LastRow ? 1 : 0;
        }

        protected override void OnDragLeave(EventArgs e)
        {
            shellDrop.Dispose();
            dropHover = false;
            dropItemPath = null;
            insertionMark = Rectangle.Empty;
            dropPaths = null;
            lastDragOver = null;
            dragScroll.Stop();
            if (!ClientRectangle.Contains(PointToClient(Cursor.Position))) ScheduleHover(false);
            Invalidate();
        }

        protected override void OnDragDrop(DragEventArgs e)
        {
            try { UpdateDrop(e, true); }
            finally { OnDragLeave(EventArgs.Empty); }
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing && !disposingWindow)
            {
                disposingWindow = true;
                hoverTimer.Dispose();
                dragScroll.Dispose();
                shellDrop.Dispose();
                tip.Dispose();
                DisposeFonts();
                if (iconTint != null) iconTint.Dispose();
            }
            base.Dispose(disposing);
        }
    }
}
