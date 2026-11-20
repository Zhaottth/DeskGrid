use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::dialog::{self, Input, PathMode};
use super::*;
use crate::i18n::Language;
use crate::layout::{Page, Rule, RuleKind, Sort, View};

const SORTS: [Sort; 5] = [
    Sort::Name,
    Sort::Type,
    Sort::Newest,
    Sort::Size,
    Sort::Manual,
];
const VIEWS: [View; 3] = [View::Icons, View::List, View::Details];
const KINDS: [RuleKind; 4] = [
    RuleKind::Extension,
    RuleKind::Name,
    RuleKind::Folder,
    RuleKind::ModifiedWithinDays,
];
fn rule_labels() -> [&'static str; 4] {
    [
        tr!("文件扩展名", "副檔名", "File extension"),
        tr!("名称通配符", "名稱萬用字元", "Name pattern"),
        tr!("文件夹", "資料夾", "Folder"),
        tr!("最近修改天数", "最近修改天數", "Days since modification"),
    ]
}

fn choice(options: &[&str], current: usize) -> Input {
    Input::Choice(
        options.iter().map(|value| (*value).to_owned()).collect(),
        current,
    )
}

fn field(value: impl Into<OsString>) -> Input {
    Input::Text(value.into())
}

fn action(label: impl Into<String>, command: Action) -> (String, Option<Action>) {
    (label.into(), Some(command))
}

fn separator() -> (String, Option<Action>) {
    (String::new(), None)
}

impl App {
    pub(super) fn manage(&mut self) {
        let mut manager = dashboard::window();
        if manager.is_invalid() {
            match dashboard::open() {
                Ok(window) => manager = window,
                Err(error) => {
                    report_error(&tr_format!(
                        "无法打开管理界面。{error}",
                        "無法開啟管理介面。{error}",
                        "Could not open the manager. {error}"
                    ));
                    return;
                }
            }
        }
        dashboard::refresh(manager, &self.layout);
        unsafe {
            let _ = ShowWindow(manager, SW_RESTORE);
            let _ = SetForegroundWindow(manager);
        }
    }

    pub(super) fn tray_menu(&self) {
        let mut items = vec![
            action(
                tr!("管理桌面格…", "管理桌面格…", "Manage DeskGrid…"),
                Action::Manage,
            ),
            action(
                tr!(
                    "搜索…    Ctrl+Alt+F",
                    "搜尋…    Ctrl+Alt+F",
                    "Search…    Ctrl+Alt+F"
                ),
                Action::Search(None),
            ),
            action(
                if self.hidden {
                    tr!("显示面板", "顯示面板", "Show panels")
                } else {
                    tr!("隐藏面板", "隱藏面板", "Hide panels")
                },
                Action::Hidden,
            ),
            action(
                if self.peek {
                    tr!("退出临时置顶", "結束暫時置頂", "Stop showing panels on top")
                } else {
                    tr!("临时置顶面板", "暫時置頂面板", "Show panels on top")
                },
                Action::Peek,
            ),
            action(tr!("刷新", "重新整理", "Refresh"), Action::Refresh),
            separator(),
        ];
        items.extend(self.layout.pages.items.iter().map(|page| {
            let marker = if page.id == self.layout.active_page_id {
                "● "
            } else {
                "    "
            };
            action(
                format!("{marker}{}", page.title),
                Action::SwitchPage(page.id.clone()),
            )
        }));
        items.extend([
            separator(),
            action(
                tr!("回收站", "資源回收筒", "Recycle Bin"),
                Action::Open(layout::RECYCLE.into()),
            ),
            action(
                tr!("此电脑", "本機", "This PC"),
                Action::Open("shell:MyComputerFolder".into()),
            ),
            action(
                tr!("退出桌面格", "結束桌面格", "Quit DeskGrid"),
                Action::Exit,
            ),
        ]);
        if let Some(command) = menu(self.hwnd, cursor(), items) {
            dispatch(command);
        }
    }

    pub(super) fn panel_menu(
        &self,
        window: HWND,
        root: &str,
        id: &str,
        point: POINT,
        item: Option<Entry>,
    ) {
        let mut items = Vec::new();
        if let Some(item) = item {
            items.push(action(
                tr!("打开", "開啟", "Open"),
                if item.folder
                    && self
                        .layout
                        .group(id)
                        .is_some_and(|group| group.folder.is_some())
                {
                    Action::Navigate(id.into(), item.path.clone())
                } else {
                    Action::Open(item.path.clone())
                },
            ));
            if !item.is_recycle() && !item.is_namespace() {
                items.extend([
                    action(
                        tr!(
                            "在资源管理器中定位",
                            "在檔案總管中定位",
                            "Show in File Explorer"
                        ),
                        Action::Reveal(item.path.clone()),
                    ),
                    action(
                        tr!("复制路径", "複製路徑", "Copy path"),
                        Action::Copy(item.path.clone()),
                    ),
                ]);
            }
            if !item.is_recycle() {
                if self
                    .layout
                    .placements
                    .items
                    .iter()
                    .any(|p| path_key(&p.path) == item.key)
                {
                    items.push(action(
                        tr!(
                            "取消手动归类或固定",
                            "取消手動分類或釘選",
                            "Clear assignment or pin"
                        ),
                        Action::Clear(item.path.clone()),
                    ));
                }
                items.push(separator());
                items.extend(
                    self.layout
                        .groups
                        .items
                        .iter()
                        .filter(|group| group.folder.is_none())
                        .map(|group| {
                            action(
                                tr_format!("归入 {}", "移至 {}", "Move to {}", group.title),
                                Action::Assign(
                                    group.id.clone(),
                                    vec![item.path.clone()],
                                    Rc::default(),
                                    None,
                                ),
                            )
                        }),
                );
            }
        } else if let (Some(group), Some(frame)) = (self.layout.group(id), self.layout.group(root))
        {
            items.extend([
                action(
                    tr!("编辑面板…", "編輯面板…", "Edit panel…"),
                    Action::EditGroup(id.into()),
                ),
                action(
                    tr!("显示在哪些页面…", "顯示在哪些頁面…", "Show on pages…"),
                    Action::Membership(root.into()),
                ),
                action(
                    tr!(
                        "与其他面板合并为标签…",
                        "與其他面板合併為分頁標籤…",
                        "Dock as tab…"
                    ),
                    Action::Dock(id.into()),
                ),
                action(
                    if frame.collapsed {
                        tr!("展开", "展開", "Expand")
                    } else {
                        tr!("折叠", "摺疊", "Collapse")
                    },
                    Action::Collapse(root.into()),
                ),
                action(
                    tr!("搜索此面板…", "搜尋此面板…", "Search this panel…"),
                    Action::Search(Some(id.into())),
                ),
            ]);
            let tabs: Vec<_> = self.layout.tabs(frame).collect();
            if tabs.len() > 1 {
                items.push(action(
                    tr!(
                        "将当前标签拆出为面板",
                        "將目前的分頁標籤拆成面板",
                        "Detach tab as panel"
                    ),
                    Action::Detach(id.into()),
                ));
                items.push(separator());
                items.extend(tabs.into_iter().map(|tab| {
                    action(
                        tr_format!("切换到 {}", "切換至 {}", "Switch to {}", tab.title),
                        Action::Activate(tab.id.clone()),
                    )
                }));
            }
            if group.folder.is_none() && self.layout.default_group_id != id {
                items.push(action(
                    tr!("设为默认分组", "設為預設群組", "Set as default group"),
                    Action::DefaultGroup(id.into()),
                ));
            }
            items.push(separator());
            items.push(action(
                tr!("移除面板…", "移除面板…", "Remove panel…"),
                Action::RemoveGroup(id.into()),
            ));
        }
        if items.is_empty() {
            return;
        }
        items.extend([
            separator(),
            action(
                tr!("管理桌面格…", "管理桌面格…", "Manage DeskGrid…"),
                Action::Manage,
            ),
        ]);
        if let Some(command) = menu(window, point, items) {
            dispatch(command);
        }
    }

    pub(super) fn new_bounds(&self) -> Bounds {
        let point = cursor();
        let work = work_area(None, Some(point));
        let offset = px(
            (self.layout.groups.items.len() % 8) as i32 * 28,
            dpi(self.hwnd),
        );
        Bounds {
            left: work.left + 48 + offset,
            top: work.top + 42 + offset,
            width: 390,
            height: 270,
        }
        .fit(work, (192, 125))
    }

    pub(super) fn edit_group(&mut self, id: Option<&str>, portal: bool) {
        let (mut group, frame, folder_panel) = if let Some(id) = id {
            let Some(group) = self.layout.group(id) else {
                return;
            };
            (
                group.clone(),
                self.layout.root(group).clone(),
                group.folder.is_some(),
            )
        } else {
            let group = Group::new(
                if portal {
                    tr!("文件夹入口", "資料夾入口", "Folder portal")
                } else {
                    tr!("新建分组", "新增群組", "New group")
                },
                (self.layout.groups.items.len() % 8) as i32,
                self.new_bounds(),
                &self.layout.active_page_id,
            );
            (group.clone(), group, portal)
        };
        let sort = SORTS
            .iter()
            .position(|sort| *sort == group.sort)
            .unwrap_or_default();
        let view = VIEWS
            .iter()
            .position(|view| *view == group.view)
            .unwrap_or_default();
        let size = [24, 32, 48]
            .iter()
            .position(|&size| size == group.icon_size)
            .unwrap_or(1);
        let sort_labels = [
            tr!("名称", "名稱", "Name"),
            tr!("类型", "類型", "Type"),
            tr!("最新修改", "最近修改", "Last modified"),
            tr!("大小", "大小", "Size"),
            tr!("手动", "手動", "Manual"),
        ];
        let sort_count = if folder_panel { 4 } else { sort_labels.len() };
        let mut fields = vec![
            (tr!("名称", "名稱", "Name"), field(&group.title)),
            (
                tr!("颜色", "色彩", "Color"),
                choice(
                    &[
                        tr!("蓝", "藍", "Blue"),
                        tr!("绿", "綠", "Green"),
                        tr!("橙", "橘", "Orange"),
                        tr!("紫", "紫", "Purple"),
                        tr!("粉", "粉紅", "Pink"),
                        tr!("青", "青", "Cyan"),
                        tr!("黄", "黃", "Yellow"),
                        tr!("灰", "灰", "Gray"),
                    ],
                    group.accent as usize,
                ),
            ),
            (
                tr!("视图", "檢視", "View"),
                choice(
                    &[
                        tr!("图标", "圖示", "Icons"),
                        tr!("列表", "清單", "List"),
                        tr!("详细信息", "詳細資料", "Details"),
                    ],
                    view,
                ),
            ),
            (
                tr!("图标大小", "圖示大小", "Icon size"),
                choice(&["24", "32", "48"], size),
            ),
            (
                tr!("排序", "排序", "Sort by"),
                choice(&sort_labels[..sort_count], sort),
            ),
            (
                tr!(
                    "不透明度（35–100）",
                    "不透明度（35–100）",
                    "Opacity (35–100)"
                ),
                field(group.opacity.to_string()),
            ),
            (
                tr!(
                    "锁定面板位置和大小",
                    "鎖定面板位置和大小",
                    "Lock panel position and size"
                ),
                Input::Check(frame.locked),
            ),
            (
                tr!(
                    "图标随组着色",
                    "圖示套用群組色彩",
                    "Tint icons with the group color"
                ),
                Input::Check(group.tint_icons),
            ),
            (
                tr!(
                    "移开指针时淡出",
                    "移開游標時淡出",
                    "Dim when the pointer leaves"
                ),
                Input::Check(group.dim_when_idle),
            ),
            (
                tr!(
                    "折叠后悬停展开",
                    "摺疊時將游標停留即可展開",
                    "Expand collapsed panels on hover"
                ),
                Input::Check(frame.hover_expand),
            ),
            (
                tr!("展开延迟（毫秒）", "展開延遲（毫秒）", "Expand delay (ms)"),
                field(frame.expand_delay.to_string()),
            ),
            (
                tr!(
                    "收起延迟（毫秒）",
                    "收合延遲（毫秒）",
                    "Collapse delay (ms)"
                ),
                field(frame.retract_delay.to_string()),
            ),
        ];
        if folder_panel {
            fields.insert(
                1,
                (
                    tr!("文件夹", "資料夾", "Folder"),
                    Input::Folder(
                        group
                            .folder
                            .as_ref()
                            .map(|path| path.as_os_str().to_owned())
                            .unwrap_or_default(),
                    ),
                ),
            );
        }
        let title = if id.is_some() {
            tr!("编辑面板", "編輯面板", "Edit panel")
        } else if folder_panel {
            tr!("添加文件夹入口", "新增資料夾入口", "Add folder portal")
        } else {
            tr!("新建分组", "新增群組", "New group")
        };
        loop {
            let Some((_, values)) =
                dialog::form(self.owner(), title, fields, &[tr!("保存", "儲存", "Save")])
            else {
                return;
            };
            let name = values[0].text().to_string_lossy().trim().to_owned();
            let v = &values[1 + usize::from(folder_panel)..];
            let number = |i: usize| v[i].text().to_string_lossy().trim().parse::<i32>().ok();
            let numbers = number(4).zip(number(9)).zip(number(10));
            let folder = if folder_panel {
                full_path(Path::new(values[1].text()))
            } else {
                None
            };
            let error = if name.is_empty() {
                Some(tr!(
                    "请输入面板名称。",
                    "請輸入面板名稱。",
                    "Enter a panel name."
                ))
            } else if folder_panel && folder.is_none() {
                Some(tr!(
                    "请选择目录或输入完整的文件夹路径。",
                    "請選取資料夾或輸入完整路徑。",
                    "Select a folder or enter its full path."
                ))
            } else if !numbers.is_some_and(|((opacity, expand), retract)| {
                (35..=100).contains(&opacity)
                    && (0..=5000).contains(&expand)
                    && (0..=5000).contains(&retract)
            }) {
                Some(tr!(
                    "请输入 35–100 的不透明度和 0–5000 毫秒的悬停延迟。",
                    "請輸入 35–100 的不透明度與 0–5000 毫秒的停留延遲。",
                    "Enter an opacity of 35–100 and delays of 0–5000 ms."
                ))
            } else {
                None
            };
            if let Some(error) = error {
                alert(self.owner(), error, MB_OK | MB_ICONWARNING);
                fields = vec![
                    (tr!("名称", "名稱", "Name"), field(values[0].text())),
                    (
                        tr!("颜色", "色彩", "Color"),
                        choice(
                            &[
                                tr!("蓝", "藍", "Blue"),
                                tr!("绿", "綠", "Green"),
                                tr!("橙", "橘", "Orange"),
                                tr!("紫", "紫", "Purple"),
                                tr!("粉", "粉紅", "Pink"),
                                tr!("青", "青", "Cyan"),
                                tr!("黄", "黃", "Yellow"),
                                tr!("灰", "灰", "Gray"),
                            ],
                            v[0].choice(),
                        ),
                    ),
                    (
                        tr!("视图", "檢視", "View"),
                        choice(
                            &[
                                tr!("图标", "圖示", "Icons"),
                                tr!("列表", "清單", "List"),
                                tr!("详细信息", "詳細資料", "Details"),
                            ],
                            v[1].choice(),
                        ),
                    ),
                    (
                        tr!("图标大小", "圖示大小", "Icon size"),
                        choice(&["24", "32", "48"], v[2].choice()),
                    ),
                    (
                        tr!("排序", "排序", "Sort by"),
                        choice(&sort_labels[..sort_count], v[3].choice()),
                    ),
                    (
                        tr!(
                            "不透明度（35–100）",
                            "不透明度（35–100）",
                            "Opacity (35–100)"
                        ),
                        field(v[4].text()),
                    ),
                    (
                        tr!(
                            "锁定面板位置和大小",
                            "鎖定面板位置和大小",
                            "Lock panel position and size"
                        ),
                        Input::Check(v[5].checked()),
                    ),
                    (
                        tr!(
                            "图标随组着色",
                            "圖示套用群組色彩",
                            "Tint icons with the group color"
                        ),
                        Input::Check(v[6].checked()),
                    ),
                    (
                        tr!(
                            "移开指针时淡出",
                            "移開游標時淡出",
                            "Dim when the pointer leaves"
                        ),
                        Input::Check(v[7].checked()),
                    ),
                    (
                        tr!(
                            "折叠后悬停展开",
                            "摺疊時將游標停留即可展開",
                            "Expand collapsed panels on hover"
                        ),
                        Input::Check(v[8].checked()),
                    ),
                    (
                        tr!("展开延迟（毫秒）", "展開延遲（毫秒）", "Expand delay (ms)"),
                        field(v[9].text()),
                    ),
                    (
                        tr!(
                            "收起延迟（毫秒）",
                            "收合延遲（毫秒）",
                            "Collapse delay (ms)"
                        ),
                        field(v[10].text()),
                    ),
                ];
                if folder_panel {
                    fields.insert(
                        1,
                        (
                            tr!("文件夹", "資料夾", "Folder"),
                            Input::Folder(values[1].text().to_owned()),
                        ),
                    );
                }
                continue;
            }
            let ((opacity, expand), retract) = numbers.unwrap();
            if group.folder != folder {
                self.folders.remove(&group.id);
            }
            if group.title != name {
                group.preset = None;
                group.title = name;
            }
            group.folder = folder;
            group.accent = v[0].choice().min(7) as i32;
            group.view = VIEWS.get(v[1].choice()).copied().unwrap_or_default();
            group.icon_size = [24, 32, 48].get(v[2].choice()).copied().unwrap_or(32);
            group.sort = SORTS.get(v[3].choice()).copied().unwrap_or_default();
            group.opacity = opacity;
            group.tint_icons = v[6].checked();
            group.dim_when_idle = v[7].checked();
            let set_frame = |root: &mut Group| {
                root.locked = v[5].checked();
                root.hover_expand = v[8].checked();
                root.expand_delay = expand;
                root.retract_delay = retract;
            };
            if frame.id == group.id {
                set_frame(&mut group);
            } else if let Some(frame) = self.layout.group_mut(&frame.id) {
                set_frame(frame);
            }
            if let Some(id) = id {
                *self.layout.group_mut(id).unwrap() = group;
            } else {
                self.layout.groups.items.push(group);
            }
            self.changed();
            return;
        }
    }

    pub(super) fn remove_group(&mut self, id: &str) {
        let Some(group) = self.layout.group(id) else {
            return;
        };
        if group.folder.is_none()
            && self
                .layout
                .groups
                .items
                .iter()
                .filter(|group| group.folder.is_none())
                .count()
                == 1
        {
            alert(
                self.owner(),
                tr!(
                    "请保留至少一个普通分组。",
                    "請保留至少一個一般群組。",
                    "Keep at least one regular group."
                ),
                MB_OK | MB_ICONWARNING,
            );
            return;
        }
        if alert(
            self.owner(),
            &tr_format!(
                "移除面板「{}」？",
                "移除面板「{}」？",
                "Remove panel “{}”?",
                group.title
            ),
            MB_YESNO | MB_ICONQUESTION,
        ) != IDYES
        {
            return;
        }
        self.layout.detach(id);
        self.layout.groups.items.retain(|group| group.id != id);
        self.layout.rules.items.retain(|rule| rule.target_id != id);
        self.layout
            .placements
            .items
            .retain(|placement| placement.loose || placement.group_id != id);
        self.folders.remove(id);
        if self.layout.default_group_id == id {
            self.layout.default_group_id = self
                .layout
                .groups
                .items
                .iter()
                .find(|group| group.folder.is_none())
                .unwrap()
                .id
                .clone();
        }
        self.changed();
    }

    pub(super) fn dock(&mut self, id: &str) {
        let Some(group) = self.layout.group(id) else {
            return;
        };
        let root = self.layout.root(group);
        let candidates: Vec<_> = self
            .layout
            .groups
            .items
            .iter()
            .filter(|group| group.tab_host_id.is_none() && root.id != group.id)
            .collect();
        if candidates.is_empty() {
            return;
        }
        let labels = candidates.iter().map(|group| group.title.clone()).collect();
        if let Some((_, values)) = dialog::form(
            self.owner(),
            tr!("合并为标签", "合併為分頁標籤", "Dock as tab"),
            vec![(
                tr!("目标面板", "目標面板", "Target panel"),
                Input::Choice(labels, 0),
            )],
            &[tr!("合并", "合併", "Dock")],
        ) && let Some(group) = candidates.get(values[0].choice())
        {
            let target = group.id.clone();
            self.layout.dock(id, &target);
            self.changed();
        }
    }

    pub(super) fn membership(&mut self, id: &str) {
        let mut selected = 0;
        loop {
            let Some(group) = self.layout.group(id) else {
                return;
            };
            let pages = &self.layout.pages.items;
            let labels = pages
                .iter()
                .map(|page| {
                    let prefix = if group.page_ids.items.contains(&page.id) {
                        "✓ "
                    } else {
                        "    "
                    };
                    format!("{prefix}{}", page.title)
                })
                .collect();
            let Some((button, values)) = dialog::form(
                self.owner(),
                tr!("显示在哪些页面", "顯示在哪些頁面", "Show on pages"),
                vec![(tr!("页面", "頁面", "Page"), Input::Choice(labels, selected))],
                &[
                    tr!("显示", "顯示", "Show"),
                    tr!("取消显示", "停止顯示", "Stop showing"),
                ],
            ) else {
                break;
            };
            selected = values[0].choice();
            let Some(page) = pages.get(selected).map(|page| page.id.clone()) else {
                break;
            };
            let group = self.layout.group_mut(id).unwrap();
            if button == 0 && !group.page_ids.items.contains(&page) {
                group.page_ids.items.push(page);
                self.changed();
            } else if button == 1 && group.page_ids.items.contains(&page) {
                if group.page_ids.items.len() == 1 {
                    alert(
                        self.owner(),
                        tr!(
                            "面板至少应显示在一个页面。",
                            "面板至少應顯示於一個頁面。",
                            "A panel must appear on at least one page."
                        ),
                        MB_OK | MB_ICONWARNING,
                    );
                } else {
                    group.page_ids.items.retain(|id| id != &page);
                    self.changed();
                }
            }
        }
    }

    pub(super) fn new_page(&mut self) -> bool {
        let Some(title) = self.page_title(None) else {
            return false;
        };
        self.layout.create_page(title);
        self.hidden = false;
        self.changed();
        true
    }

    fn page_title(&self, page: Option<&Page>) -> Option<String> {
        let mut title = page.map_or_else(String::new, |page| page.title.clone());
        let prompt = if page.is_some() {
            tr!("重命名页面", "重新命名頁面", "Rename page")
        } else {
            tr!("新建页面", "新增頁面", "New page")
        };
        loop {
            let (_, values) = dialog::form(
                self.owner(),
                prompt,
                vec![(tr!("名称", "名稱", "Name"), field(&title))],
                &[tr!("保存", "儲存", "Save")],
            )?;
            title = values[0].text().to_string_lossy().trim().to_owned();
            if !title.is_empty()
                && !self.layout.pages.items.iter().any(|other| {
                    page.is_none_or(|page| other.id != page.id)
                        && other.title.eq_ignore_ascii_case(&title)
                })
            {
                return Some(title);
            }
            alert(
                self.owner(),
                tr!(
                    "请输入未使用的页面名称。",
                    "請輸入尚未使用的頁面名稱。",
                    "Enter an unused page name."
                ),
                MB_OK | MB_ICONWARNING,
            );
        }
    }

    pub(super) fn pages(&mut self) {
        loop {
            let pages = &self.layout.pages.items;
            let initial = pages
                .iter()
                .position(|p| p.id == self.layout.active_page_id)
                .unwrap_or(0);
            let labels = pages.iter().map(|page| page.title.clone()).collect();
            let Some((button, values)) = dialog::form(
                self.owner(),
                tr!("管理页面", "管理頁面", "Manage pages"),
                vec![(tr!("页面", "頁面", "Page"), Input::Choice(labels, initial))],
                &[
                    tr!("切换", "切換", "Switch"),
                    tr!("新建页面", "新增頁面", "New page"),
                    tr!("重命名", "重新命名", "Rename"),
                    tr!("移除", "移除", "Remove"),
                ],
            ) else {
                break;
            };
            let index = values[0].choice();
            let Some(page) = pages.get(index) else { break };
            match button {
                0 => {
                    self.switch_page(page.id.clone());
                    break;
                }
                1 => {
                    if self.new_page() {
                        break;
                    }
                }
                2 => {
                    if let Some(name) = self.page_title(Some(page)) {
                        self.layout.pages.items[index].title = name;
                        self.changed();
                    }
                }
                3 if pages.len() > 1 => {
                    if alert(
                        self.owner(),
                        &tr_format!(
                            "移除页面「{}」？",
                            "移除頁面「{}」？",
                            "Remove page “{}”?",
                            page.title
                        ),
                        MB_YESNO | MB_ICONQUESTION,
                    ) == IDYES
                    {
                        self.layout.pages.items.remove(index);
                        self.layout.normalize_pages();
                        self.changed();
                    }
                }
                3 => {
                    alert(
                        self.owner(),
                        tr!(
                            "请保留至少一个页面。",
                            "請保留至少一個頁面。",
                            "Keep at least one page."
                        ),
                        MB_OK | MB_ICONWARNING,
                    );
                }
                _ => break,
            }
        }
    }

    pub(super) fn rules(&mut self) {
        let mut index = 0;
        loop {
            let rules = &self.layout.rules.items;
            let labels = rules
                .iter()
                .map(|rule| {
                    let kind = KINDS.iter().position(|kind| *kind == rule.kind).unwrap();
                    let title = self
                        .layout
                        .group(&rule.target_id)
                        .map_or("", |group| &group.title);
                    format!("{} {} → {title}", rule_labels()[kind], rule.value)
                })
                .collect();
            let Some((button, values)) = dialog::form(
                self.owner(),
                tr!(
                    "分类规则（从上到下匹配）",
                    "分類規則（由上而下比對）",
                    "Sorting rules (first match wins)"
                ),
                vec![(tr!("规则", "規則", "Rule"), Input::Choice(labels, index))],
                &[
                    tr!("编辑", "編輯", "Edit"),
                    tr!("添加", "新增", "Add"),
                    tr!("上移", "上移", "Move up"),
                    tr!("下移", "下移", "Move down"),
                    tr!("移除", "移除", "Remove"),
                ],
            ) else {
                break;
            };
            index = values[0].choice();
            match button {
                0 | 1 => {
                    let initial = if button == 0 {
                        let Some(rule) = rules.get(index) else {
                            continue;
                        };
                        rule.clone()
                    } else {
                        Rule::default()
                    };
                    let Some(rule) = self.rule_form(initial) else {
                        continue;
                    };
                    if button == 1 {
                        self.layout.rules.items.push(rule);
                        index = self.layout.rules.items.len() - 1;
                    } else {
                        self.layout.rules.items[index] = rule;
                    }
                    self.changed();
                }
                2 if index > 0 && index < rules.len() => {
                    self.layout.rules.items.swap(index, index - 1);
                    index -= 1;
                    self.changed();
                }
                3 if index.saturating_add(1) < rules.len() => {
                    self.layout.rules.items.swap(index, index + 1);
                    index += 1;
                    self.changed();
                }
                4 if index < rules.len() => {
                    self.layout.rules.items.remove(index);
                    index = index.min(self.layout.rules.items.len().saturating_sub(1));
                    self.changed();
                }
                _ => {}
            }
        }
    }

    fn rule_form(&self, mut rule: Rule) -> Option<Rule> {
        let groups: Vec<_> = self
            .layout
            .groups
            .items
            .iter()
            .filter(|group| group.folder.is_none())
            .collect();
        loop {
            let labels = groups.iter().map(|group| group.title.clone()).collect();
            let target = groups
                .iter()
                .position(|group| group.id == rule.target_id)
                .unwrap_or(0);
            let kind = KINDS
                .iter()
                .position(|kind| *kind == rule.kind)
                .unwrap_or(0);
            let (_, values) = dialog::form(
                self.owner(),
                tr!("编辑分类规则", "編輯分類規則", "Edit sorting rule"),
                vec![
                    (
                        tr!("匹配方式", "比對方式", "Match type"),
                        choice(&rule_labels(), kind),
                    ),
                    (
                        tr!(
                            "值（如 pdf、报告*、7）",
                            "值（例如 pdf、報告*、7）",
                            "Value (e.g. pdf, report*, 7)"
                        ),
                        field(&rule.value),
                    ),
                    (
                        tr!("目标分组", "目標群組", "Target group"),
                        Input::Choice(labels, target),
                    ),
                ],
                &[tr!("保存", "儲存", "Save")],
            )?;
            rule = Rule {
                kind: *KINDS.get(values[0].choice())?,
                value: values[1].text().to_string_lossy().trim().to_owned(),
                target_id: groups.get(values[2].choice())?.id.clone(),
            };
            if rule.valid() {
                return Some(rule);
            }
            alert(
                self.owner(),
                tr!(
                    "请输入有效的扩展名、名称通配符或天数。",
                    "請輸入有效的副檔名、名稱萬用字元或天數。",
                    "Enter a valid extension, name pattern, or number of days."
                ),
                MB_OK | MB_ICONWARNING,
            );
        }
    }

    pub(super) fn align(&mut self) {
        let roots: Vec<_> = self
            .layout
            .groups
            .items
            .iter()
            .filter(|group| {
                group.tab_host_id.is_none()
                    && group.page_ids.items.contains(&self.layout.active_page_id)
            })
            .collect();
        if roots.len() < 2 {
            alert(
                self.owner(),
                tr!(
                    "此页面至少需要两个独立面板才能对齐。",
                    "此頁面至少需要兩個獨立面板才能對齊。",
                    "This page needs at least two panels to align."
                ),
                MB_OK | MB_ICONINFORMATION,
            );
            return;
        }
        let names: Vec<_> = roots.iter().map(|group| group.title.clone()).collect();
        let targets = std::iter::once(
            tr!(
                "当前页面的其他面板",
                "目前頁面的其他面板",
                "Other panels on this page"
            )
            .to_owned(),
        )
        .chain(names.iter().cloned())
        .collect();
        let Some((_, values)) = dialog::form(
            self.owner(),
            tr!("对齐面板", "對齊面板", "Align panels"),
            vec![
                (
                    tr!("参考面板", "參考面板", "Reference panel"),
                    Input::Choice(names, 0),
                ),
                (
                    tr!("要调整的面板", "要調整的面板", "Panels to adjust"),
                    Input::Choice(targets, 0),
                ),
                (
                    tr!("对齐方式", "對齊方式", "Alignment"),
                    choice(
                        &[
                            tr!("左侧", "左側", "Left"),
                            tr!("水平居中", "水平置中", "Center horizontally"),
                            tr!("右侧", "右側", "Right"),
                            tr!("顶部", "頂端", "Top"),
                            tr!("垂直居中", "垂直置中", "Center vertically"),
                            tr!("底部", "底端", "Bottom"),
                            tr!("等宽", "等寬", "Match width"),
                        ],
                        0,
                    ),
                ),
            ],
            &[tr!("应用", "套用", "Apply")],
        ) else {
            return;
        };
        let Some(reference) = roots.get(values[0].choice()) else {
            return;
        };
        let reference_bounds = self.fitted(reference.bounds(), (192, 125));
        let target = values[1].choice();
        let changes: Vec<_> = roots
            .iter()
            .enumerate()
            .filter(|(i, group)| {
                group.id != reference.id && !group.locked && (target == 0 || target == *i + 1)
            })
            .map(|(_, group)| {
                let mut bounds = self.fitted(group.bounds(), (192, 125));
                match values[2].choice() {
                    0 => bounds.left = reference_bounds.left,
                    1 => {
                        bounds.left =
                            reference_bounds.left + (reference_bounds.width - bounds.width) / 2
                    }
                    2 => bounds.left = reference_bounds.right() - bounds.width,
                    3 => bounds.top = reference_bounds.top,
                    4 => {
                        bounds.top =
                            reference_bounds.top + (reference_bounds.height - bounds.height) / 2
                    }
                    5 => bounds.top = reference_bounds.bottom() - bounds.height,
                    _ => bounds.width = reference_bounds.width,
                }
                (group.id.clone(), self.fitted(bounds, (192, 125)))
            })
            .collect();
        if changes.is_empty() {
            return;
        }
        for (id, bounds) in changes {
            self.layout.group_mut(&id).unwrap().set_bounds(bounds);
        }
        self.changed();
    }

    pub(super) fn options(&mut self) {
        let starts = autostart();
        let language = Language::ALL
            .iter()
            .position(|language| *language == self.layout.language)
            .unwrap_or(0);
        if let Some((_, values)) = dialog::form(
            self.owner(),
            tr!("桌面格设置", "桌面格設定", "DeskGrid settings"),
            vec![
                (
                    tr!("界面语言", "介面語言", "Language"),
                    choice(&Language::LABELS, language),
                ),
                (
                    tr!(
                        "只在桌面格面板中显示桌面项目",
                        "只在桌面格面板中顯示桌面項目",
                        "Show desktop items only in DeskGrid panels"
                    ),
                    Input::Check(self.layout.hide_windows_icons),
                ),
                (
                    tr!(
                        "双击桌面空白处隐藏或恢复面板",
                        "按兩下桌面空白處可隱藏或顯示面板",
                        "Double-click empty desktop space to toggle panels"
                    ),
                    Input::Check(self.layout.quick_hide_on_double_click),
                ),
                (
                    tr!(
                        "登录 Windows 时启动",
                        "登入 Windows 時啟動",
                        "Start when I sign in to Windows"
                    ),
                    Input::Check(starts),
                ),
            ],
            &[tr!("保存", "儲存", "Save")],
        ) {
            if starts != values[3].checked()
                && let Err(error) = set_autostart(values[3].checked())
            {
                alert(
                    self.owner(),
                    &tr_format!(
                        "无法修改登录启动设置。{error}",
                        "無法修改登入啟動設定。{error}",
                        "Could not change the startup setting. {error}"
                    ),
                    MB_OK | MB_ICONWARNING,
                );
            }
            if let Some(&language) = Language::ALL.get(values[0].choice()) {
                self.layout.set_language(language);
                crate::i18n::set(language);
            }
            self.layout.hide_windows_icons = values[1].checked();
            self.layout.quick_hide_on_double_click = values[2].checked();
            self.changed();
            self.tray_icon = None;
            self.tray();
        }
    }

    pub(super) fn import(&mut self) {
        let Some(file) = dialog::choose_path(self.owner(), PathMode::Import) else {
            return;
        };
        match layout::read(&file) {
            Ok(mut layout) => {
                if alert(
                    self.owner(),
                    tr!(
                        "使用选中的快照替换当前桌面格布局？",
                        "使用選取的快照取代目前的桌面格版面？",
                        "Replace the current DeskGrid layout with this snapshot?"
                    ),
                    MB_YESNO | MB_ICONQUESTION,
                ) != IDYES
                {
                    return;
                }
                layout.set_language(self.layout.language);
                self.layout = layout;
                self.folders.clear();
                self.changed();
            }
            Err(error) => {
                alert(
                    self.owner(),
                    &tr_format!(
                        "无法导入布局。{error}",
                        "無法匯入版面。{error}",
                        "Could not import the layout. {error}"
                    ),
                    MB_OK | MB_ICONWARNING,
                );
            }
        }
    }

    pub(super) fn export(&self) {
        let Some(file) = dialog::choose_path(self.owner(), PathMode::Export) else {
            return;
        };
        let key = |path: &Path| {
            let canonical = fs::canonicalize(path)
                .or_else(|_| {
                    let parent = path.parent().map(fs::canonicalize).transpose()?;
                    Ok::<PathBuf, std::io::Error>(
                        parent
                            .unwrap_or_default()
                            .join(path.file_name().unwrap_or_default()),
                    )
                })
                .unwrap_or_else(|_| path.to_owned());
            path_key(&canonical)
        };
        if [key(&self.path), key(&layout::sidecar(&self.path, ".bak"))].contains(&key(&file)) {
            alert(
                self.owner(),
                tr!(
                    "请选择用于保存布局快照的文件。",
                    "請選取要儲存版面快照的檔案。",
                    "Choose a file for the layout snapshot."
                ),
                MB_OK | MB_ICONWARNING,
            );
        } else if let Err(error) = layout::write(&file, &self.layout, false) {
            alert(
                self.owner(),
                &tr_format!(
                    "无法导出布局。{error}",
                    "無法匯出版面。{error}",
                    "Could not export the layout. {error}"
                ),
                MB_OK | MB_ICONWARNING,
            );
        }
    }
}
