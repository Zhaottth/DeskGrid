using System;
using System.Collections.Generic;
using System.Drawing;
using System.Linq;
using System.Windows.Forms;

namespace DeskGrid
{
    internal sealed class WorkspaceWindow : DeskDialog
    {
        private readonly ListBox pages;
        private readonly TextBox title;
        private readonly Button remove;
        internal List<DesktopPage> EditedPages { get; private set; }
        internal string SelectedId { get { return ((DesktopPage)pages.SelectedItem).Id; } }

        internal WorkspaceWindow(IEnumerable<DesktopPage> source, string activeId) : base("桌面页面", new Size(500, 388))
        {
            ShowInTaskbar = false;
            MinimizeBox = false;
            EditedPages = source.Select(page => new DesktopPage { Id = page.Id, Title = page.Title }).ToList();
            Caption("选择要打开的页面", 24, 19, 452, 26);
            pages = CanvasList(new Rectangle(24, 55, 452, 151));
            pages.DrawItem += delegate(object sender, DrawItemEventArgs e)
            {
                if (e.Index >= 0) DrawListText(e, pages.Items[e.Index].ToString(), Font);
            };
            title = Field("", 24, 229, 240);
            title.MaxLength = 40;
            title.AccessibleName = "页面名称";
            ButtonAt("添加", 274, 224, 62, Add);
            ButtonAt("重命名", 345, 224, 62, Rename);
            remove = ButtonAt("移除", 416, 224, 60, Remove);
            Caption("在面板菜单中选择「显示在哪些页面」，可把面板放到多个页面。", 24, 277, 452, 29, true);
            pages.SelectedIndexChanged += delegate
            {
                var selected = pages.SelectedItem as DesktopPage;
                title.Text = selected == null ? "" : selected.Title;
            };
            FinishButtons(298, 330, delegate { DialogResult = DialogResult.OK; }, "保存并打开");
            LoadRows(EditedPages.FindIndex(page => page.Id == activeId));
        }

        protected override void OnShown(EventArgs e)
        {
            base.OnShown(e);
            pages.Focus();
        }

        private void LoadRows(int index)
        {
            pages.Items.Clear();
            pages.Items.AddRange(EditedPages.ToArray());
            pages.SelectedIndex = Math.Max(0, Math.Min(index, pages.Items.Count - 1));
            remove.Enabled = EditedPages.Count > 1;
        }

        private string ValidTitle(DesktopPage except)
        {
            string value = title.Text.Trim();
            if (value.Length != 0 && !EditedPages.Any(item => item != except && String.Equals(item.Title, value, StringComparison.OrdinalIgnoreCase)))
                return value;
            MessageBox.Show(this, "请输入一个未使用的页面名称。", Text, MessageBoxButtons.OK, MessageBoxIcon.Information);
            title.Focus();
            return null;
        }

        private void Add()
        {
            string value = ValidTitle(null);
            if (value == null) return;
            EditedPages.Add(new DesktopPage { Id = Guid.NewGuid().ToString("N"), Title = value });
            LoadRows(EditedPages.Count - 1);
        }

        private void Rename()
        {
            var current = (DesktopPage)pages.SelectedItem;
            string value = ValidTitle(current);
            if (value == null) return;
            current.Title = value;
            LoadRows(pages.SelectedIndex);
        }

        private void Remove()
        {
            if (EditedPages.Count < 2) return;
            if (MessageBox.Show(this, "移除此页面？只在该页面显示的面板会归到列表中的第一个剩余页面。", Text,
                MessageBoxButtons.OKCancel, MessageBoxIcon.Question, MessageBoxDefaultButton.Button2) != DialogResult.OK) return;
            int index = pages.SelectedIndex;
            EditedPages.RemoveAt(index);
            LoadRows(index);
        }

        internal static void DrawListText(DrawItemEventArgs e, string title, Font font)
        {
            var bounds = e.Bounds;
            using (var brush = new SolidBrush((e.State & DrawItemState.Selected) != 0 ? Theme.Selected : Theme.Surface))
                e.Graphics.FillRectangle(brush, bounds);
            TextRenderer.DrawText(e.Graphics, title, font, Rectangle.Inflate(bounds, -bounds.Height / 3, 0), Theme.Text,
                TextFormatFlags.SingleLine | TextFormatFlags.VerticalCenter | TextFormatFlags.NoPrefix | TextFormatFlags.EndEllipsis);
            e.DrawFocusRectangle();
        }
    }

    internal sealed class PageMembershipWindow : DeskDialog
    {
        private readonly CheckedListBox pages;
        internal List<string> SelectedIds { get; private set; }

        internal PageMembershipWindow(DesktopGroup root, IEnumerable<DesktopPage> source)
            : base("显示在哪些页面", new Size(452, 344))
        {
            ShowInTaskbar = false;
            MinimizeBox = false;
            Caption("选择「" + root.Title + "」出现的页面", 24, 17, 404, 28);
            pages = Checklist(24, 55, 404, 184);
            foreach (var page in source) pages.Items.Add(page, root.PageIds.Contains(page.Id));
            ButtonAt("全选", 24, 252, 72, delegate
            {
                for (int i = 0; i < pages.Items.Count; i++) pages.SetItemChecked(i, true);
            });
            FinishButtons(250, 283, Save);
        }

        private void Save()
        {
            SelectedIds = pages.CheckedItems.Cast<DesktopPage>().Select(page => page.Id).ToList();
            if (SelectedIds.Count > 0) DialogResult = DialogResult.OK;
            else MessageBox.Show(this, "请选择至少一个页面。", Text, MessageBoxButtons.OK, MessageBoxIcon.Information);
        }
    }

    internal enum PanelAlignment { Left, HorizontalCenter, Right, Top, VerticalCenter, Bottom, Width }

    internal sealed class AlignmentWindow : DeskDialog
    {
        private readonly DesktopGroup[] groups;
        private readonly ComboBox reference;
        private readonly ComboBox operation;
        private readonly CheckedListBox targets;
        internal DesktopGroup Reference { get { return (DesktopGroup)reference.SelectedItem; } }
        internal PanelAlignment Operation { get { return (PanelAlignment)operation.SelectedIndex; } }
        internal DesktopGroup[] Targets { get; private set; }

        internal AlignmentWindow(DesktopGroup[] groups) : base("批量对齐面板", new Size(478, 407))
        {
            this.groups = groups;
            ShowInTaskbar = false;
            MinimizeBox = false;
            Caption("基准面板", 24, 19, 90, 28);
            reference = Choices(groups, 0, 119, 22, 335);
            Caption("对齐方式", 24, 59, 90, 28);
            operation = Choices(new[] { "左边缘", "水平居中", "右边缘", "上边缘", "垂直居中", "下边缘", "相同宽度" },
                0, 119, 61, 335);
            Caption("选择要调整的面板", 24, 104, 430, 25);
            targets = Checklist(24, 139, 430, 199);
            reference.SelectedIndexChanged += delegate { PopulateTargets(); };
            PopulateTargets();
            FinishButtons(276, 351, Apply, "对齐");
        }

        private void PopulateTargets()
        {
            targets.Items.Clear();
            foreach (var group in groups.Where(group => group != Reference && !group.Locked))
                targets.Items.Add(group, true);
        }

        private void Apply()
        {
            Targets = targets.CheckedItems.Cast<DesktopGroup>().ToArray();
            if (Targets.Length == 0)
            {
                MessageBox.Show(this, "请选择至少一个可移动的面板。", Text, MessageBoxButtons.OK, MessageBoxIcon.Information);
                return;
            }
            DialogResult = DialogResult.OK;
        }

        internal static Rectangle Align(Rectangle reference, Rectangle target, PanelAlignment alignment)
        {
            switch (alignment)
            {
                case PanelAlignment.Left: target.X = reference.Left; break;
                case PanelAlignment.HorizontalCenter: target.X = reference.X + (reference.Width - target.Width) / 2; break;
                case PanelAlignment.Right: target.X = reference.Right - target.Width; break;
                case PanelAlignment.Top: target.Y = reference.Top; break;
                case PanelAlignment.VerticalCenter: target.Y = reference.Y + (reference.Height - target.Height) / 2; break;
                case PanelAlignment.Bottom: target.Y = reference.Bottom - target.Height; break;
                case PanelAlignment.Width: target.Width = reference.Width; break;
            }
            return target;
        }
    }

    internal sealed class PanelSearchResult
    {
        internal readonly DesktopGroup Group;
        internal readonly FileEntry Entry;
        internal readonly string Folder;

        internal PanelSearchResult(DesktopGroup group, FileEntry entry, string folder)
        {
            Group = group;
            Entry = entry;
            Folder = folder;
        }
    }

    internal sealed class SearchWindow : DeskDialog
    {
        private readonly DesktopApp app;
        private readonly TextBox query;
        private readonly ListBox results;
        private readonly Label hint;
        private readonly Label scopeLabel;
        private readonly Button open;
        private readonly Button locate;
        private List<PanelSearchResult> source = new List<PanelSearchResult>();
        private DesktopGroup scope;
        private const int MaxResults = 250;

        private sealed class SearchRow
        {
            internal PanelSearchResult Result;
            internal string Description;
            public override string ToString() { return Result.Entry.Name + " · " + Description; }
        }

        internal SearchWindow(DesktopApp app, DesktopGroup scope) : base("桌面格 · 搜索面板", new Size(690, 478))
        {
            this.app = app;
            StartPosition = FormStartPosition.CenterScreen;
            MinimizeBox = false;
            TopMost = true;
            var title = Caption("搜索面板", 24, 17, 340, 37);
            title.Font = new Font(Font.FontFamily, 17F, FontStyle.Bold);
            scopeLabel = Caption("", 25, 58, 510, 24, true);
            ButtonAt("搜索全部", 554, 52, 112, delegate { SetScope(null); });
            query = Field("", 24, 99, 642);
            query.AccessibleName = "搜索文件、文件夹和快捷方式";
            hint = Caption("", 24, 130, 640, 26, true);
            results = CanvasList(new Rectangle(24, 164, 642, 239));
            results.DrawItem += DrawResult;
            results.SelectedIndexChanged += delegate { UpdateActions(); };
            results.DoubleClick += delegate { ActivateResult(false); };
            open = ButtonAt("打开", 582, 420, 84, delegate { ActivateResult(false); }, true);
            locate = ButtonAt("定位到面板", 470, 420, 102, delegate { ActivateResult(true); });
            Caption("Enter 打开  ·  Ctrl+Enter 定位", 24, 422, 270, 28, true);
            AcceptButton = open;
            query.TextChanged += delegate { Filter(); };
            SetScope(scope);
            UpdateActions();
        }

        internal void SetScope(DesktopGroup group)
        {
            scope = group;
            scopeLabel.Text = group == null ? "所有页面和面板" : "当前面板 · " + group.Title;
            Filter();
            query.Focus();
            query.SelectAll();
        }

        internal void UpdateSource(List<PanelSearchResult> refreshed)
        {
            source = refreshed;
            if (scope != null && !app.Layout.Groups.Contains(scope)) { SetScope(null); return; }
            if (query.TextLength != 0) Filter();
        }

        protected override void OnShown(EventArgs e)
        {
            base.OnShown(e);
            query.Focus();
        }

        private void Filter()
        {
            string phrase = query.Text.Trim();
            string[] words = phrase.Split((char[])null, StringSplitOptions.RemoveEmptyEntries);
            var matching = source.Where(item => words.Length > 0 && (scope == null || item.Group == scope) &&
                words.All(word => item.Entry.Name.IndexOf(word, StringComparison.CurrentCultureIgnoreCase) >= 0))
                .OrderByDescending(item => item.Entry.Name.StartsWith(phrase, StringComparison.CurrentCultureIgnoreCase))
                .ThenBy(item => item.Entry.Name, StringComparer.CurrentCultureIgnoreCase).ToArray();
            var rows = matching.Take(MaxResults).Select(item => new SearchRow
            {
                Result = item,
                Description = item.Group == null ? "桌面独立图标" : item.Group.Title + "  /  " + app.Layout.PageNames(item.Group)
            }).ToArray();
            hint.Text = words.Length == 0 ? "输入文件、文件夹或快捷方式的名称" :
                matching.Length == 0 ? "没有找到匹配项目" :
                matching.Length > MaxResults ? "显示前 " + MaxResults + " 项，共 " + matching.Length + " 项" : "找到 " + matching.Length + " 项";
            if (results.Items.Cast<SearchRow>().SequenceEqual(rows, SearchRowComparer.Instance)) return;

            var selected = results.SelectedItem as SearchRow;
            int previousTop = results.TopIndex;
            results.BeginUpdate();
            results.Items.Clear();
            results.Items.AddRange(rows);
            if (rows.Length != 0)
            {
                int index = Array.FindIndex(rows, row => SearchRowComparer.Instance.Equals(row, selected));
                results.SelectedIndex = Math.Max(0, index);
                if (index >= 0 && previousTop <= index) results.TopIndex = Math.Min(previousTop, rows.Length - 1);
            }
            results.EndUpdate();
            UpdateActions();
        }

        private sealed class SearchRowComparer : IEqualityComparer<SearchRow>
        {
            internal static readonly SearchRowComparer Instance = new SearchRowComparer();
            public bool Equals(SearchRow x, SearchRow y)
            {
                return x != null && y != null && x.Result.Group == y.Result.Group &&
                    x.Result.Entry.Path == y.Result.Entry.Path && x.Description == y.Description;
            }
            public int GetHashCode(SearchRow row) { return row.Result.Entry.Path.GetHashCode(); }
        }

        private void UpdateActions()
        {
            if (open == null) return;
            open.Enabled = locate.Enabled = results.SelectedIndex >= 0;
        }

        private void ActivateResult(bool locateInPanel)
        {
            var row = results.SelectedItem as SearchRow;
            if (row == null) return;
            Close();
            if (locateInPanel) app.LocateSearchResult(row.Result);
            else app.OpenSearchResult(row.Result);
        }

        protected override bool ProcessCmdKey(ref Message message, Keys keyData)
        {
            if (keyData == Keys.Escape) { Close(); return true; }
            if (keyData == (Keys.Control | Keys.Enter)) { ActivateResult(true); return true; }
            if (keyData == Keys.Enter && results.Focused) { ActivateResult(false); return true; }
            if (query.Focused && results.Items.Count > 0 && (keyData == Keys.Down || keyData == Keys.Up))
            {
                int next = results.SelectedIndex + (keyData == Keys.Down ? 1 : -1);
                results.SelectedIndex = Math.Min(results.Items.Count - 1, Math.Max(0, next));
                return true;
            }
            return base.ProcessCmdKey(ref message, keyData);
        }

        private void DrawResult(object sender, DrawItemEventArgs e)
        {
            if (e.Index < 0) return;
            var row = (SearchRow)results.Items[e.Index];
            var rect = e.Bounds;
            using (var brush = new SolidBrush((e.State & DrawItemState.Selected) != 0 ? Theme.Selected : Theme.Surface))
                e.Graphics.FillRectangle(brush, rect);
            int icon = rect.Height * 3 / 5;
            int x = rect.Left + rect.Height / 3;
            int description = rect.Width / 3;
            app.Icons.Draw(e.Graphics, row.Result.Entry,
                new Rectangle(x, rect.Y + (rect.Height - icon) / 2, icon, icon), null);
            var flags = TextFormatFlags.SingleLine | TextFormatFlags.VerticalCenter | TextFormatFlags.NoPrefix | TextFormatFlags.EndEllipsis;
            TextRenderer.DrawText(e.Graphics, row.Result.Entry.Name, Font,
                new Rectangle(x + icon + rect.Height / 4, rect.Top, rect.Width - description - (x - rect.Left) - icon - rect.Height / 3, rect.Height),
                Theme.Text, flags);
            TextRenderer.DrawText(e.Graphics, row.Description, Font,
                new Rectangle(rect.Right - description, rect.Top, description - rect.Height / 3, rect.Height),
                Theme.Muted, flags | TextFormatFlags.Right);
            e.DrawFocusRectangle();
        }
    }
}
