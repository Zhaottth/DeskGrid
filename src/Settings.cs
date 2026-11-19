using System;
using System.Collections.Generic;
using System.Drawing;
using System.IO;
using System.Linq;
using System.Windows.Forms;

namespace DeskGrid
{
    internal class DeskDialog : Form
    {
        private readonly int layoutDpi = WindowsShell.Dpi(IntPtr.Zero);

        protected DeskDialog(string title, Size size)
        {
            SuspendLayout();
            Text = title;
            AutoScaleMode = AutoScaleMode.None;
            ClientSize = new Size(Px(size.Width), Px(size.Height));
            StartPosition = FormStartPosition.CenterParent;
            FormBorderStyle = FormBorderStyle.FixedSingle;
            MaximizeBox = false;
            BackColor = Theme.Background;
            ForeColor = Theme.Text;
            Font = new Font("Microsoft YaHei UI", 9F);
        }

        private int Px(int value) { return (value * layoutDpi + 48) / 96; }

        private Rectangle Place(int x, int y, int width, int height)
        {
            return new Rectangle(Px(x), Px(y), Px(width), Px(height));
        }

        protected override void OnLoad(EventArgs e)
        {
            ResumeLayout(true);
            base.OnLoad(e);
        }

        protected Label Caption(string text, int x, int y, int width, int height, bool muted = false)
        {
            var label = new Label
            {
                Text = text, UseMnemonic = false, Bounds = Place(x, y, width, height),
                TextAlign = ContentAlignment.MiddleLeft, ForeColor = muted ? Theme.Muted : Theme.Text
            };
            Controls.Add(label);
            return label;
        }

        protected Button ButtonAt(string text, int x, int y, int width, Action click, bool primary = false)
        {
            var button = new Button
            {
                Text = text, Bounds = Place(x, y, width, 34), FlatStyle = FlatStyle.Flat,
                BackColor = primary ? Theme.Accents[0] : Theme.Surface,
                ForeColor = primary ? Theme.Background : Theme.Text,
                UseVisualStyleBackColor = false, Cursor = Cursors.Hand
            };
            button.FlatAppearance.BorderColor = primary ? Theme.Accents[0] : Theme.Border;
            button.FlatAppearance.MouseOverBackColor = primary ? Color.FromArgb(149, 205, 255) : Theme.Hover;
            button.Click += delegate { click(); };
            Controls.Add(button);
            return button;
        }

        protected TextBox Field(string value, int x, int y, int width)
        {
            var field = new TextBox
            {
                Text = value ?? "", Location = new Point(Px(x), Px(y)), Width = Px(width),
                ForeColor = Theme.Text, BackColor = Theme.Surface, BorderStyle = BorderStyle.FixedSingle
            };
            Controls.Add(field);
            return field;
        }

        protected ComboBox Choices(object[] values, int selected, int x, int y, int width)
        {
            var combo = new ComboBox
            {
                Location = new Point(Px(x), Px(y)), Width = Px(width), DropDownStyle = ComboBoxStyle.DropDownList,
                BackColor = Theme.Surface, ForeColor = Theme.Text, FlatStyle = FlatStyle.Flat
            };
            combo.Items.AddRange(values);
            if (values.Length != 0) combo.SelectedIndex = Math.Max(0, Math.Min(selected, values.Length - 1));
            Controls.Add(combo);
            return combo;
        }

        protected CheckBox Toggle(string title, int x, int y, int width)
        {
            var check = new CheckBox
            {
                Text = title, Bounds = Place(x, y, width, 26),
                ForeColor = Theme.Text, UseVisualStyleBackColor = false
            };
            Controls.Add(check);
            return check;
        }

        protected NumericUpDown Number(int value, int minimum, int maximum, int step, int x, int y, int width)
        {
            var field = new NumericUpDown
            {
                Location = new Point(Px(x), Px(y)), Width = Px(width),
                Minimum = minimum, Maximum = maximum, Increment = step, Value = value,
                ForeColor = Theme.Text, BackColor = Theme.Surface, BorderStyle = BorderStyle.FixedSingle
            };
            Controls.Add(field);
            return field;
        }

        protected CheckedListBox Checklist(int x, int y, int width, int height)
        {
            var list = new CheckedListBox
            {
                Bounds = Place(x, y, width, height), CheckOnClick = true, IntegralHeight = false,
                BackColor = Theme.Surface, ForeColor = Theme.Text, BorderStyle = BorderStyle.FixedSingle
            };
            Controls.Add(list);
            return list;
        }

        protected void FinishButtons(int x, int y, Action submit, string primaryTitle = "保存")
        {
            CancelButton = ButtonAt("取消", x, y, 84, delegate { DialogResult = DialogResult.Cancel; });
            AcceptButton = ButtonAt(primaryTitle, x + 94, y, 84, submit, true);
        }

        protected ListBox CanvasList(Rectangle rect)
        {
            var list = new ListBox
            {
                Bounds = Place(rect.X, rect.Y, rect.Width, rect.Height), BorderStyle = BorderStyle.FixedSingle,
                BackColor = Theme.Surface, ForeColor = Theme.Text, DrawMode = DrawMode.OwnerDrawFixed,
                IntegralHeight = false, ItemHeight = Px(37)
            };
            Controls.Add(list);
            return list;
        }
    }

    internal sealed class SettingsWindow : DeskDialog
    {
        private readonly DesktopApp app;
        private readonly ListBox groups;
        private readonly Button edit;
        private readonly Button remove;
        private readonly Button defaultGroup;
        private readonly CheckBox nativeIcons;
        private readonly CheckBox start;
        private readonly ComboBox pages;
        private bool refreshing;

        internal SettingsWindow(DesktopApp app, Icon icon) : base("桌面格 · 管理桌面", new Size(624, 576))
        {
            this.app = app;
            Icon = icon;
            StartPosition = FormStartPosition.CenterScreen;
            var title = Caption("桌面格", 24, 18, 360, 38);
            title.Font = new Font(Font.FontFamily, 20F, FontStyle.Bold);
            Caption("面板与分类规则", 25, 58, 360, 23, true);
            ButtonAt("自动分类规则…", 456, 31, 144, app.EditRules);
            ButtonAt("新建分组", 24, 103, 96, delegate { app.NewGroup(false); }, true);
            ButtonAt("添加文件夹入口", 130, 103, 130, delegate { app.NewGroup(true); });
            ButtonAt("搜索面板", 270, 103, 96, delegate { app.ShowSearch(null); });
            ButtonAt("批量对齐…", 485, 103, 115, app.AlignPanels);
            Caption("当前页面", 24, 154, 88, 26);
            pages = Choices(new object[0], 0, 113, 154, 253);
            pages.AccessibleName = "当前桌面页面";
            pages.SelectedIndexChanged += delegate
            {
                var page = pages.SelectedItem as DesktopPage;
                if (!refreshing && page != null) app.SwitchPage(page.Id);
            };
            ButtonAt("管理页面…", 485, 147, 115, app.EditPages);
            groups = CanvasList(new Rectangle(24, 194, 576, 156));
            groups.DrawItem += DrawGroup;
            groups.SelectedIndexChanged += delegate { UpdateActions(); };
            groups.DoubleClick += delegate { if (Selected != null) app.EditGroup(Selected); };
            edit = ButtonAt("编辑", 24, 361, 65, delegate { if (Selected != null) app.EditGroup(Selected); });
            defaultGroup = ButtonAt("设为默认分组", 97, 361, 105, delegate { if (Selected != null) app.SetDefault(Selected); });
            remove = ButtonAt("移除", 210, 361, 66, delegate { if (Selected != null) app.RemoveGroup(Selected); });
            ButtonAt("页面可见性…", 284, 361, 113, delegate { if (Selected != null) app.EditPageMembership(Selected); });
            ButtonAt("导出布局", 427, 361, 82, app.ExportLayout);
            ButtonAt("导入布局", 518, 361, 82, app.ImportLayout);
            nativeIcons = Toggle("隐藏 Windows 原生桌面图标", 24, 417, 288);
            nativeIcons.Enabled = app.CanHideWindowsIcons;
            nativeIcons.CheckedChanged += delegate { if (!refreshing) app.SetNativeIconsHidden(nativeIcons.Checked); };
            start = Toggle("登录 Windows 时启动", 320, 417, 280);
            start.Enabled = app.CanStartWithWindows;
            start.CheckedChanged += delegate { if (!refreshing) app.SetStartWithWindows(start.Checked); };
            var quickHide = Toggle("双击桌面空白处隐藏或恢复面板", 24, 447, 576);
            quickHide.Checked = app.Layout.QuickHideOnDoubleClick;
            quickHide.CheckedChanged += delegate { if (!refreshing) app.SetDesktopQuickHide(quickHide.Checked); };
            Caption(app.HotkeyHelp, 24, 503, 576, 25, true);
            Caption(app.SearchHelp, 24, 528, 576, 25, true);
            RefreshGroups(null);
            RefreshOptions();
        }

        private DesktopGroup Selected { get { return groups.SelectedItem as DesktopGroup; } }

        internal void RefreshGroups(string selectedId)
        {
            refreshing = true;
            pages.BeginUpdate();
            try
            {
                pages.Items.Clear();
                pages.Items.AddRange(app.Layout.Pages.ToArray());
                pages.SelectedIndex = app.Layout.Pages.FindIndex(page => page.Id == app.Layout.ActivePageId);
            }
            finally
            {
                pages.EndUpdate();
                refreshing = false;
            }
            string currentId = selectedId ?? (Selected == null ? null : Selected.Id);
            groups.BeginUpdate();
            groups.Items.Clear();
            foreach (var group in app.Layout.Groups) groups.Items.Add(group);
            int index = app.Layout.Groups.FindIndex(group => group.Id == currentId);
            if (groups.Items.Count != 0)
            {
                groups.SelectedIndex = Math.Max(0, index);
                groups.TopIndex = Math.Max(0, groups.SelectedIndex - Math.Max(1, groups.ClientSize.Height / groups.ItemHeight) + 1);
            }
            groups.EndUpdate();
            UpdateActions();
        }

        internal void RefreshOptions()
        {
            refreshing = true;
            try
            {
                nativeIcons.Checked = app.Layout.HideWindowsIcons;
                start.Checked = app.StartsWithWindows();
            }
            finally { refreshing = false; }
        }

        private void UpdateActions()
        {
            if (edit == null) return;
            var group = Selected;
            edit.Enabled = group != null;
            remove.Enabled = group != null && (group.IsPortal || app.Layout.Groups.Count(item => !item.IsPortal) > 1);
            defaultGroup.Enabled = group != null && !group.IsPortal && group.Id != app.Layout.DefaultGroupId;
        }

        private void DrawGroup(object sender, DrawItemEventArgs e)
        {
            if (e.Index < 0) return;
            var group = (DesktopGroup)groups.Items[e.Index];
            Rectangle r = e.Bounds;
            bool selected = (e.State & DrawItemState.Selected) != 0;
            using (var brush = new SolidBrush(selected ? Theme.Selected : Theme.Surface)) e.Graphics.FillRectangle(brush, r);
            using (var accent = new SolidBrush(Theme.Accents[group.Accent]))
                e.Graphics.FillEllipse(accent, r.Left + r.Height / 3, r.Top + (r.Height - 9) / 2, 9, 9);
            string kind = group.IsPortal ? "文件夹入口" : group.Id == app.Layout.DefaultGroupId ? "默认分组" : "桌面分组";
            if (app.Layout.Tabs(group).Length > 1) kind = "标签 · " + kind;
            if (app.Layout.Pages.Count > 1) kind += "  /  " + app.Layout.PageNames(group);
            var flags = TextFormatFlags.SingleLine | TextFormatFlags.VerticalCenter | TextFormatFlags.NoPrefix | TextFormatFlags.EndEllipsis;
            TextRenderer.DrawText(e.Graphics, group.Title, Font,
                new Rectangle(r.Left + r.Height * 4 / 5, r.Top, r.Width / 2 - r.Height, r.Height), Theme.Text, flags);
            TextRenderer.DrawText(e.Graphics, kind, Font,
                new Rectangle(r.Left + r.Width / 2, r.Top, r.Width / 2 - r.Height / 3, r.Height), Theme.Muted, flags | TextFormatFlags.Right);
            e.DrawFocusRectangle();
        }
    }

    internal sealed class GroupEditor : DeskDialog
    {
        private readonly TextBox title;
        private readonly TextBox folder;
        private readonly ComboBox color;
        private readonly ComboBox sort;
        private readonly CheckBox locked;
        private readonly CheckBox tint;
        private readonly CheckBox dim;
        private readonly NumericUpDown opacity;
        private readonly ComboBox view;
        private readonly ComboBox iconSize;
        private readonly CheckBox hover;
        private readonly NumericUpDown expandDelay;
        private readonly NumericUpDown retractDelay;
        private readonly bool portal;
        private readonly DesktopGroup frame;

        internal GroupEditor(DesktopGroup group, bool portal, DesktopGroup frame = null)
            : base(portal ? "文件夹入口" : "桌面分组", new Size(482, portal ? 456 : 385))
        {
            this.portal = portal;
            this.frame = frame ?? group;
            MinimizeBox = false;
            ShowInTaskbar = false;
            int y = 24;
            Caption("名称", 24, y, 70, 26);
            title = Field(group.Title, 106, y + 2, 352);
            title.MaxLength = 60;
            if (portal)
            {
                y += 44;
                Caption("文件夹", 24, y, 70, 26);
                folder = Field(group.Folder, 106, y + 2, 270);
                ButtonAt("浏览…", 385, y - 3, 73, Browse);
                Caption("双击面板中的子文件夹即可浏览。", 106, y + 29, 352, 23, true);
                y += 27;
            }
            y += 43;
            Caption("颜色", 24, y, 70, 26);
            color = Choices(Theme.AccentNames, group.Accent, 106, y + 1, 118);
            Caption("排序", 251, y, 53, 26);
            var sorts = portal ? Theme.SortNames.Take((int)SortOrder.Manual).ToArray() : Theme.SortNames;
            sort = Choices(sorts, (int)group.Sort, 308, y + 1, 150);
            Caption("不透明度", 24, y + 40, 79, 26);
            opacity = Number(group.Opacity, 35, 100, 1, 106, y + 42, 88);
            Caption("%", 202, y + 40, 22, 26, true);
            tint = Toggle("图标随组着色", 251, y + 41, 207);
            tint.Checked = group.TintIcons;
            locked = Toggle("锁定面板位置和大小", 106, y + 79, 160);
            locked.Checked = this.frame.Locked;
            dim = Toggle("移开指针时淡出", 277, y + 79, 180);
            dim.Checked = group.DimWhenIdle;
            Caption("视图", 24, y + 115, 70, 26);
            view = Choices(Theme.ViewNames, (int)group.View, 106, y + 117, 118);
            Caption("图标大小", 251, y + 115, 76, 26);
            iconSize = Choices(new[] { "小  24 px", "中  32 px", "大  48 px" },
                Array.IndexOf(DesktopGroup.IconSizes, group.IconSize), 333, y + 117, 125);
            hover = Toggle("折叠后悬停展开", 106, y + 153, 235);
            hover.Checked = this.frame.HoverExpand;
            Caption("展开延迟（毫秒）", 24, y + 195, 122, 26);
            expandDelay = Number(this.frame.ExpandDelay, 0, 5000, 50, 147, y + 196, 77);
            Caption("收起延迟（毫秒）", 238, y + 195, 129, 26);
            retractDelay = Number(this.frame.RetractDelay, 0, 5000, 50, 379, y + 196, 79);
            hover.CheckedChanged += delegate { UpdateHoverFields(); };
            UpdateHoverFields();
            FinishButtons(280, y + 261, Submit);
            Shown += delegate { title.Focus(); title.SelectAll(); };
        }

        private void UpdateHoverFields()
        {
            expandDelay.Enabled = hover.Checked;
            retractDelay.Enabled = hover.Checked;
        }

        private void Browse()
        {
            using (var browser = new FolderBrowserDialog
            {
                Description = "选择要显示在桌面的文件夹",
                SelectedPath = Directory.Exists(folder.Text) ? folder.Text : "",
                ShowNewFolderButton = false
            })
            {
                if (browser.ShowDialog(this) != DialogResult.OK) return;
                folder.Text = browser.SelectedPath;
                if (String.IsNullOrWhiteSpace(title.Text)) title.Text = FolderName(browser.SelectedPath);
            }
        }

        private static string FolderName(string path)
        {
            string name = Path.GetFileName(path.TrimEnd('\\', '/'));
            return String.IsNullOrWhiteSpace(name) ? path : name;
        }

        private void Submit()
        {
            if (portal)
            {
                string path = FileEntry.FullPath(folder.Text.Trim().Trim('"'));
                if (path == null)
                {
                    MessageBox.Show(this, "请输入完整的文件夹路径，或使用「浏览」。", Text, MessageBoxButtons.OK, MessageBoxIcon.Information);
                    folder.Focus();
                    return;
                }
                folder.Text = path;
                if (String.IsNullOrWhiteSpace(title.Text)) title.Text = FolderName(path);
            }
            if (String.IsNullOrWhiteSpace(title.Text))
            {
                MessageBox.Show(this, "请输入面板名称。", Text, MessageBoxButtons.OK, MessageBoxIcon.Information);
                title.Focus();
                return;
            }
            DialogResult = DialogResult.OK;
        }

        internal void Apply(DesktopGroup group)
        {
            group.Title = title.Text.Trim();
            group.Folder = portal ? folder.Text : null;
            group.Accent = color.SelectedIndex;
            group.Sort = (SortOrder)sort.SelectedIndex;
            group.Opacity = (int)opacity.Value;
            group.TintIcons = tint.Checked;
            group.DimWhenIdle = dim.Checked;
            group.View = (PanelView)view.SelectedIndex;
            group.IconSize = DesktopGroup.IconSizes[iconSize.SelectedIndex];
            frame.Locked = locked.Checked;
            frame.HoverExpand = hover.Checked;
            frame.ExpandDelay = (int)expandDelay.Value;
            frame.RetractDelay = (int)retractDelay.Value;
        }
    }

    internal sealed class RulesWindow : DeskDialog
    {
        private sealed class RuleRow
        {
            internal DesktopRule Rule;
            public override string ToString() { return DescribeRule(Rule); }
        }

        private readonly DesktopGroup[] targets;
        private readonly ListBox rules;
        private readonly Button edit;
        private readonly Button delete;
        private readonly Button up;
        private readonly Button down;
        internal List<DesktopRule> EditedRules { get; private set; }
        internal static readonly string[] RuleNames = { "扩展名", "文件名匹配", "所有文件夹", "最近修改" };

        internal RulesWindow(DesktopLayout layout) : base("自动分类规则", new Size(636, 403))
        {
            ShowInTaskbar = false;
            MinimizeBox = false;
            targets = layout.Groups.Where(group => !group.IsPortal).ToArray();
            EditedRules = layout.Rules.ToList();
            string fallback = targets.First(group => group.Id == layout.DefaultGroupId).Title;
            Caption("从上到下使用第一条匹配规则。其余项目归入「" + fallback + "」。", 24, 20, 588, 30, true);
            rules = CanvasList(new Rectangle(24, 62, 588, 218));
            rules.DrawItem += DrawRule;
            rules.SelectedIndexChanged += delegate { UpdateActions(); };
            rules.DoubleClick += delegate { if (rules.SelectedIndex >= 0) EditRule(false); };
            ButtonAt("添加规则", 24, 293, 90, delegate { EditRule(true); }, true);
            edit = ButtonAt("编辑", 125, 293, 73, delegate { EditRule(false); });
            delete = ButtonAt("删除", 208, 293, 73, RemoveRule);
            up = ButtonAt("上移", 456, 293, 73, delegate { MoveRule(-1); });
            down = ButtonAt("下移", 539, 293, 73, delegate { MoveRule(1); });
            FinishButtons(434, 345, delegate { DialogResult = DialogResult.OK; });
            RefreshRules(0);
        }

        private void RefreshRules(int index)
        {
            rules.BeginUpdate();
            rules.Items.Clear();
            foreach (var rule in EditedRules) rules.Items.Add(new RuleRow { Rule = rule });
            if (rules.Items.Count > 0)
            {
                rules.SelectedIndex = Math.Min(index, rules.Items.Count - 1);
                rules.TopIndex = Math.Max(0, rules.SelectedIndex - Math.Max(1, rules.ClientSize.Height / rules.ItemHeight) + 1);
            }
            rules.EndUpdate();
            UpdateActions();
        }

        private void UpdateActions()
        {
            if (edit == null) return;
            int i = rules.SelectedIndex;
            edit.Enabled = i >= 0;
            delete.Enabled = i >= 0;
            up.Enabled = i > 0;
            down.Enabled = i >= 0 && i < rules.Items.Count - 1;
        }

        private void EditRule(bool add)
        {
            int i = rules.SelectedIndex;
            if (!add && i < 0) return;
            var initial = add ? new DesktopRule { Kind = RuleKind.Extension, TargetId = targets[0].Id } : EditedRules[i];
            using (var editor = new RuleEditor(initial, targets))
            {
                if (editor.ShowDialog(this) != DialogResult.OK) return;
                if (add) { i = EditedRules.Count; EditedRules.Add(editor.Rule); }
                else EditedRules[i] = editor.Rule;
            }
            RefreshRules(i);
        }

        private void RemoveRule()
        {
            int i = rules.SelectedIndex;
            if (i < 0) return;
            EditedRules.RemoveAt(i);
            RefreshRules(i);
        }

        private void MoveRule(int delta)
        {
            int i = rules.SelectedIndex;
            if (i < 0 || i + delta < 0 || i + delta >= EditedRules.Count) return;
            var rule = EditedRules[i];
            EditedRules.RemoveAt(i);
            EditedRules.Insert(i + delta, rule);
            RefreshRules(i + delta);
        }

        private void DrawRule(object sender, DrawItemEventArgs e)
        {
            if (e.Index < 0) return;
            var rule = ((RuleRow)rules.Items[e.Index]).Rule;
            var target = targets.First(group => group.Id == rule.TargetId);
            var r = e.Bounds;
            using (var background = new SolidBrush((e.State & DrawItemState.Selected) != 0 ? Theme.Selected : Theme.Surface))
                e.Graphics.FillRectangle(background, r);
            var flags = TextFormatFlags.SingleLine | TextFormatFlags.VerticalCenter | TextFormatFlags.NoPrefix | TextFormatFlags.EndEllipsis;
            int margin = r.Height / 3;
            int right = r.Width / 3;
            TextRenderer.DrawText(e.Graphics, DescribeRule(rule), Font,
                new Rectangle(r.Left + margin, r.Top, r.Width - right - margin, r.Height), Theme.Text, flags);
            TextRenderer.DrawText(e.Graphics, "→  " + target.Title, Font,
                new Rectangle(r.Right - right, r.Top, right - margin, r.Height),
                Theme.Accents[target.Accent], flags | TextFormatFlags.Right);
            e.DrawFocusRectangle();
        }

        private static string DescribeRule(DesktopRule rule)
        {
            string detail = rule.Kind == RuleKind.Folder ? "" : "  ·  " + rule.Value + (rule.Kind == RuleKind.ModifiedWithinDays ? " 天" : "");
            return RuleNames[(int)rule.Kind] + detail;
        }
    }

    internal sealed class RuleEditor : DeskDialog
    {
        private readonly ComboBox kind;
        private readonly ComboBox target;
        private readonly TextBox value;
        private readonly Label hint;
        private readonly DesktopGroup[] targets;
        internal DesktopRule Rule { get; private set; }

        internal RuleEditor(DesktopRule original, DesktopGroup[] targets) : base("编辑规则", new Size(486, 248))
        {
            ShowInTaskbar = false;
            MinimizeBox = false;
            this.targets = targets;
            Caption("条件", 24, 23, 82, 24);
            kind = Choices(RulesWindow.RuleNames, (int)original.Kind, 111, 23, 351);
            Caption("匹配值", 24, 64, 82, 24);
            value = Field(original.Value, 111, 66, 351);
            value.MaxLength = 500;
            hint = Caption("", 111, 95, 351, 25, true);
            Caption("目标分组", 24, 142, 82, 24);
            target = Choices(targets.Select(group => (object)group.Title).ToArray(),
                Array.FindIndex(targets, group => group.Id == original.TargetId), 111, 141, 351);
            kind.SelectedIndexChanged += delegate { UpdateHint(); };
            FinishButtons(284, 192, Submit);
            UpdateHint();
        }

        private void UpdateHint()
        {
            value.Enabled = kind.SelectedIndex != (int)RuleKind.Folder;
            switch ((RuleKind)kind.SelectedIndex)
            {
                case RuleKind.Name: hint.Text = "例如 截图*.png，支持 * 和 ? 通配符"; break;
                case RuleKind.ModifiedWithinDays: hint.Text = "输入天数，例如 7"; break;
                case RuleKind.Extension: hint.Text = "用分号分隔，例如 png; jpg; webp"; break;
                default: hint.Text = "匹配桌面上的所有文件夹"; break;
            }
        }

        private void Submit()
        {
            var rule = new DesktopRule
            {
                Kind = (RuleKind)kind.SelectedIndex, Value = value.Enabled ? value.Text.Trim() : null,
                TargetId = targets[target.SelectedIndex].Id
            };
            if (!rule.IsValid())
            {
                MessageBox.Show(this, "请输入有效的匹配值。", Text, MessageBoxButtons.OK, MessageBoxIcon.Information);
                value.Focus();
                return;
            }
            Rule = rule;
            DialogResult = DialogResult.OK;
        }
    }
}
