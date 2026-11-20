use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::directory::Entry;
use crate::i18n::Language;

pub const RECYCLE: &str = "shell:RecycleBinFolder";
pub const NAMESPACE_PREFIX: &str = "shell:";
const MAIN_PAGE: &str = "desktop";
const MAX_XML: u64 = 16 * 1024 * 1024;

pub fn new_id() -> String {
    Uuid::new_v4().simple().to_string()
}

pub fn full_path(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() || path.as_os_str().as_encoded_bytes().contains(&0) {
        return None;
    }
    let full = std::path::absolute(path).ok()?;
    #[cfg(windows)]
    let full = expand_short_path(full);
    Some(full.components().collect())
}

#[cfg(windows)]
fn expand_short_path(path: PathBuf) -> PathBuf {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::{Component, Prefix};
    use windows::Win32::Storage::FileSystem::GetLongPathNameW;
    use windows::core::PCWSTR;

    // Shell drops can report the long spelling of a DOS 8.3 alias saved in a layout.
    let mut components = path.components();
    let local_drive = matches!(components.next(), Some(Component::Prefix(prefix))
        if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)));
    let has_alias = components.any(|component| {
        component
            .as_os_str()
            .as_encoded_bytes()
            .windows(2)
            .any(|pair| pair[0] == b'~' && pair[1].is_ascii_digit())
    });
    if !local_drive || !has_alias {
        return path;
    }
    let input: Vec<_> = path.as_os_str().encode_wide().chain([0]).collect();
    let input = PCWSTR(input.as_ptr());
    let required = unsafe { GetLongPathNameW(input, None) } as usize;
    if required == 0 || required > 32_768 {
        return path;
    }
    let mut output = vec![0; required];
    let written = unsafe { GetLongPathNameW(input, Some(&mut output)) } as usize;
    if written == 0 || written >= output.len() {
        return path;
    }
    OsString::from_wide(&output[..written]).into()
}

pub fn namespace_id(path: &Path) -> Option<Uuid> {
    let id = path
        .to_str()?
        .strip_prefix("shell:::{")?
        .strip_suffix('}')?;
    if id.len() != 36 {
        return None;
    }
    Uuid::parse_str(id).ok()
}

fn item_path(path: &Path) -> Option<PathBuf> {
    if let Some(id) = namespace_id(path) {
        return Some(format!("{NAMESPACE_PREFIX}::{{{id}}}").into());
    }
    full_path(path)
}

pub fn path_key(path: &Path) -> String {
    let name = path.to_string_lossy().to_lowercase();
    if let Some(tail) = name.strip_prefix(r"\\?\unc\") {
        return format!(r"\\{tail}");
    }
    name.strip_prefix(r"\\?\").unwrap_or(&name).into()
}

fn dedup_paths(paths: &mut Vec<PathBuf>) {
    let mut seen = HashSet::new();
    paths.retain_mut(|path| {
        let Some(full) = item_path(path) else {
            return false;
        };
        *path = full;
        seen.insert(path_key(path))
    });
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bounds {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

impl Bounds {
    pub fn right(self) -> i32 {
        self.left.saturating_add(self.width)
    }

    pub fn bottom(self) -> i32 {
        self.top.saturating_add(self.height)
    }

    pub fn fit(self, work: Self, minimum: (i32, i32)) -> Self {
        let width = self.width.clamp(minimum.0.min(work.width), work.width);
        let height = self.height.clamp(minimum.1.min(work.height), work.height);
        Self {
            left: self.left.clamp(work.left, work.right() - width),
            top: self.top.clamp(work.top, work.bottom() - height),
            width,
            height,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum Sort {
    #[default]
    Name,
    Type,
    Newest,
    Size,
    Manual,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum View {
    #[default]
    Icons,
    List,
    Details,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum RuleKind {
    #[default]
    Extension,
    Name,
    Folder,
    ModifiedWithinDays,
}

macro_rules! xml_list {
    ($name:ident, $item:ty, $element:literal) => {
        #[derive(Clone, Debug, Default, Deserialize, Serialize)]
        pub struct $name {
            #[serde(default, rename = $element)]
            pub items: Vec<$item>,
        }
    };
}

xml_list!(Pages, Page, "DesktopPage");
xml_list!(Groups, Group, "DesktopGroup");
xml_list!(Rules, Rule, "DesktopRule");
xml_list!(Placements, Placement, "Placement");
xml_list!(PageIds, String, "string");
xml_list!(ManualOrder, PathBuf, "string");

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename = "DesktopLayout", rename_all = "PascalCase", default)]
pub struct Layout {
    pub pages: Pages,
    pub active_page_id: String,
    pub groups: Groups,
    pub rules: Rules,
    pub placements: Placements,
    pub default_group_id: String,
    pub language: Language,
    pub hide_windows_icons: bool,
    pub quick_hide_on_double_click: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            pages: Pages::default(),
            active_page_id: MAIN_PAGE.into(),
            groups: Groups::default(),
            rules: Rules::default(),
            placements: Placements::default(),
            default_group_id: String::new(),
            language: Language::default(),
            hide_windows_icons: false,
            quick_hide_on_double_click: true,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Page {
    pub id: String,
    pub title: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum GroupPreset {
    Apps,
    Folders,
    Documents,
    Other,
}

impl GroupPreset {
    const ALL: [Self; 4] = [Self::Apps, Self::Folders, Self::Documents, Self::Other];

    fn title(self, language: Language) -> &'static str {
        match self {
            Self::Apps => language.text("应用与快捷方式", "應用程式與捷徑", "Apps and shortcuts"),
            Self::Folders => language.text("桌面文件夹", "桌面資料夾", "Desktop folders"),
            Self::Documents => language.text("文档与媒体", "文件與媒體", "Documents and media"),
            Self::Other => language.text("其他桌面项目", "其他桌面項目", "Other desktop items"),
        }
    }
}

fn desktop_title(language: Language) -> &'static str {
    language.text("桌面", "桌面", "Desktop")
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Group {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<GroupPreset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder: Option<PathBuf>,
    pub accent: i32,
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
    pub sort: Sort,
    pub manual_order: ManualOrder,
    pub collapsed: bool,
    pub locked: bool,
    pub opacity: i32,
    pub tint_icons: bool,
    pub dim_when_idle: bool,
    pub view: View,
    pub icon_size: i32,
    pub hover_expand: bool,
    pub expand_delay: i32,
    pub retract_delay: i32,
    pub page_ids: PageIds,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_host_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_tab_id: Option<String>,
}

impl Default for Group {
    fn default() -> Self {
        Self {
            id: String::new(),
            title: String::new(),
            preset: None,
            folder: None,
            accent: 0,
            left: 0,
            top: 0,
            width: 390,
            height: 270,
            sort: Sort::Name,
            manual_order: ManualOrder::default(),
            collapsed: false,
            locked: false,
            opacity: 82,
            tint_icons: false,
            dim_when_idle: false,
            view: View::Icons,
            icon_size: 32,
            hover_expand: false,
            expand_delay: 350,
            retract_delay: 800,
            page_ids: PageIds::default(),
            tab_host_id: None,
            active_tab_id: None,
        }
    }
}

impl Group {
    pub fn new(title: &str, accent: i32, bounds: Bounds, page: &str) -> Self {
        let mut group = Self {
            id: new_id(),
            title: title.into(),
            accent,
            ..Self::default()
        };
        group.page_ids.items.push(page.into());
        group.set_bounds(bounds);
        group
    }

    pub fn bounds(&self) -> Bounds {
        Bounds {
            left: self.left,
            top: self.top,
            width: self.width,
            height: self.height,
        }
    }

    pub fn set_bounds(&mut self, bounds: Bounds) {
        self.left = bounds.left;
        self.top = bounds.top;
        self.width = bounds.width;
        self.height = bounds.height;
    }

    pub fn sort_items(&self, items: &mut [Entry]) {
        let ranks: HashMap<_, _> = self
            .manual_order
            .items
            .iter()
            .enumerate()
            .map(|(index, path)| (path_key(path), index))
            .collect();
        items.sort_by(|a, b| {
            let order = match self.sort {
                Sort::Name => b.folder.cmp(&a.folder),
                Sort::Type => b.folder.cmp(&a.folder).then(a.extension.cmp(&b.extension)),
                Sort::Newest => b.modified.cmp(&a.modified),
                Sort::Size => b.bytes.cmp(&a.bytes),
                Sort::Manual => ranks
                    .get(&a.key)
                    .copied()
                    .unwrap_or(usize::MAX)
                    .cmp(&ranks.get(&b.key).copied().unwrap_or(usize::MAX))
                    .then(b.folder.cmp(&a.folder)),
            };
            order
                .then(a.label.to_lowercase().cmp(&b.label.to_lowercase()))
                .then(a.key.cmp(&b.key))
        });
    }

    fn insert(&mut self, displayed: &[Entry], slot: usize, paths: &[PathBuf]) {
        let moving: HashSet<_> = paths.iter().map(|path| path_key(path)).collect();
        let current: Vec<_> = displayed
            .iter()
            .filter(|entry| !entry.is_recycle())
            .map(|entry| entry.path.clone())
            .collect();
        let saved = if self.sort == Sort::Manual {
            self.manual_order.items.as_slice()
        } else {
            &[]
        };
        let mut seen = moving.clone();
        let mut order: Vec<_> = saved
            .iter()
            .chain(&current)
            .filter(|path| seen.insert(path_key(path)))
            .cloned()
            .collect();
        let index = displayed
            .iter()
            .skip(slot)
            .find(|entry| !entry.is_recycle() && !moving.contains(&entry.key))
            .and_then(|next| order.iter().position(|path| path_key(path) == next.key))
            .unwrap_or(order.len());
        order.splice(index..index, paths.iter().cloned());
        let visible: HashSet<_> = current
            .iter()
            .chain(paths)
            .map(|path| path_key(path))
            .collect();
        if order
            .iter()
            .map(|path| path_key(path))
            .filter(|key| visible.contains(key))
            .eq(current.iter().map(|path| path_key(path)))
        {
            return;
        }
        self.manual_order.items = order;
        self.sort = Sort::Manual;
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Rule {
    pub kind: RuleKind,
    pub value: String,
    pub target_id: String,
}

impl Rule {
    fn extensions(&self) -> impl Iterator<Item = &str> {
        self.value
            .split(|part: char| part.is_whitespace() || ";,，；".contains(part))
            .map(|part| part.trim_start_matches(['*', '.']))
            .filter(|part| !part.is_empty())
    }

    fn days(&self) -> Option<u64> {
        self.value
            .trim()
            .parse()
            .ok()
            .filter(|days| (1..=36500).contains(days))
    }

    pub fn valid(&self) -> bool {
        match self.kind {
            RuleKind::Extension => self.extensions().next().is_some(),
            RuleKind::Name => !self.value.trim().is_empty() && self.value.chars().count() <= 160,
            RuleKind::Folder => true,
            RuleKind::ModifiedWithinDays => self.days().is_some(),
        }
    }

    fn matches(&self, entry: &Entry, now: SystemTime) -> bool {
        match self.kind {
            RuleKind::Extension => {
                !entry.folder
                    && self
                        .extensions()
                        .any(|ext| ext.to_lowercase() == entry.extension)
            }
            RuleKind::Name => wildcard(self.value.trim(), &entry.name),
            RuleKind::Folder => entry.folder,
            RuleKind::ModifiedWithinDays => self
                .days()
                .and_then(|days| now.checked_sub(Duration::from_secs(days * 86400)))
                .is_some_and(|cutoff| entry.modified.is_some_and(|modified| modified >= cutoff)),
        }
    }
}

fn wildcard(pattern: &str, name: &str) -> bool {
    let pattern: Vec<_> = pattern.to_lowercase().chars().collect();
    let name: Vec<_> = name.to_lowercase().chars().collect();
    let mut row = vec![false; name.len() + 1];
    row[0] = true;
    for symbol in pattern {
        let mut previous = row[0];
        row[0] &= symbol == '*';
        for (index, character) in name.iter().enumerate() {
            let next = row[index + 1];
            row[index + 1] = match symbol {
                '*' => row[index] || next,
                '?' => previous,
                _ => previous && symbol == *character,
            };
            previous = next;
        }
    }
    row[name.len()]
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Placement {
    pub path: PathBuf,
    pub group_id: String,
    pub pinned: bool,
    pub loose: bool,
    pub left: i32,
    pub top: i32,
}

impl Layout {
    pub fn initial(work: Bounds, dpi: i32) -> Self {
        let mut layout = Self {
            hide_windows_icons: true,
            ..Self::default()
        };
        let pad = 20 * dpi / 96;
        let left = (140 * dpi / 96).min(work.width / 8);
        let top = (68 * dpi / 96).min(work.height / 10);
        let width = (390 * dpi / 96).min((work.width - left - pad * 2) / 2);
        let height = (270 * dpi / 96).min((work.height - top - pad * 2) / 2);
        for (i, preset) in GroupPreset::ALL.into_iter().enumerate() {
            let i = i as i32;
            let mut group = Group::new(
                preset.title(layout.language),
                i,
                Bounds {
                    left: work.left + left + (i % 2) * (width + pad),
                    top: work.top + top + (i / 2) * (height + pad),
                    width,
                    height,
                },
                MAIN_PAGE,
            );
            group.preset = Some(preset);
            layout.groups.items.push(group);
        }
        layout.pages.items.push(Page {
            id: MAIN_PAGE.into(),
            title: desktop_title(layout.language).into(),
        });
        layout
            .default_group_id
            .clone_from(&layout.groups.items[3].id);
        for (i, kind, value) in [
            (1, RuleKind::Folder, ""),
            (
                0,
                RuleKind::Extension,
                "lnk; url; exe; appref-ms; msi; website; bat; cmd",
            ),
            (
                2,
                RuleKind::Extension,
                "pdf; doc; docx; ppt; pptx; xls; xlsx; csv; txt; md; rtf; png; jpg; jpeg; webp; gif; bmp; svg; heic; mp4; mov; mkv; mp3; wav",
            ),
        ] {
            layout.rules.items.push(Rule {
                kind,
                value: value.into(),
                target_id: layout.groups.items[i].id.clone(),
            });
        }
        layout
    }

    pub fn set_language(&mut self, language: Language) {
        if let Some(page) = self
            .pages
            .items
            .iter_mut()
            .find(|page| page.id == MAIN_PAGE && page.title == desktop_title(self.language))
        {
            page.title = desktop_title(language).into();
        }
        for group in &mut self.groups.items {
            if let Some(preset) = group.preset {
                group.title = preset.title(language).into();
            }
        }
        self.language = language;
    }

    pub fn draw_group(&mut self, bounds: Bounds) {
        let prefix = self.language.text("分组", "群組", "Group");
        let title = (1..)
            .map(|number| format!("{prefix} {number}"))
            .find(|title| {
                !self
                    .groups
                    .items
                    .iter()
                    .filter(|group| group.page_ids.items.contains(&self.active_page_id))
                    .any(|group| group.title.eq_ignore_ascii_case(title))
            })
            .unwrap();
        self.groups.items.push(Group::new(
            &title,
            (self.groups.items.len() % 8) as i32,
            bounds,
            &self.active_page_id,
        ));
    }

    pub fn create_page(&mut self, title: String) {
        let id = new_id();
        self.pages.items.push(Page {
            id: id.clone(),
            title,
        });
        self.active_page_id = id;
    }

    pub fn normalize(&mut self) -> io::Result<()> {
        let mut ids = HashSet::new();
        self.groups
            .items
            .retain(|group| !group.id.trim().is_empty() && ids.insert(group.id.clone()));
        self.normalize_pages();
        let groups = &mut self.groups.items;
        for group in groups.iter_mut() {
            if group.title.trim().is_empty() {
                group.title = tr!("未命名", "未命名", "Untitled").into();
            }
            group.accent = group.accent.clamp(0, 7);
            group.opacity = group.opacity.clamp(35, 100);
            group.expand_delay = group.expand_delay.clamp(0, 5000);
            group.retract_delay = group.retract_delay.clamp(0, 5000);
            if ![24, 32, 48].contains(&group.icon_size) {
                group.icon_size = 32;
            }
            if let Some(folder) = &mut group.folder {
                if folder.as_os_str().is_empty() {
                    group.folder = None;
                } else {
                    *folder = full_path(folder).ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            tr!(
                                "布局中的文件夹路径无效",
                                "版面中的資料夾路徑無效",
                                "The layout contains an invalid folder path"
                            ),
                        )
                    })?;
                    if group.sort == Sort::Manual {
                        group.sort = Sort::Name;
                    }
                }
            }
            dedup_paths(&mut group.manual_order.items);
        }
        // Resolve hosts against the original graph so damaged chains are repaired consistently.
        let roots: HashSet<_> = groups
            .iter()
            .filter(|group| group.tab_host_id.as_deref().is_none_or(str::is_empty))
            .map(|group| group.id.clone())
            .collect();
        for group in groups.iter_mut() {
            if group
                .tab_host_id
                .as_ref()
                .is_some_and(|host| host == &group.id || !roots.contains(host))
            {
                group.tab_host_id = None;
            }
        }
        let hosts: HashMap<_, _> = groups
            .iter()
            .map(|group| (group.id.clone(), group.tab_host_id.clone()))
            .collect();
        for group in groups.iter_mut() {
            if group.tab_host_id.is_some()
                || group
                    .active_tab_id
                    .as_ref()
                    .is_some_and(|tab| hosts.get(tab).and_then(Option::as_ref) != Some(&group.id))
            {
                group.active_tab_id = None;
            }
        }
        let Some(first) = groups.iter().find(|group| group.folder.is_none()) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                tr!(
                    "布局至少需要一个普通分组",
                    "版面至少需要一個一般群組",
                    "A layout needs at least one regular group"
                ),
            ));
        };
        let targets: HashSet<_> = groups
            .iter()
            .filter(|group| group.folder.is_none())
            .map(|group| &group.id)
            .collect();
        if !targets.contains(&self.default_group_id) {
            self.default_group_id.clone_from(&first.id);
        }
        self.rules
            .items
            .retain(|rule| targets.contains(&rule.target_id) && rule.valid());
        let mut paths = HashSet::new();
        self.placements.items.retain_mut(|placement| {
            if !placement.loose && !targets.contains(&placement.group_id) {
                return false;
            }
            let Some(full) = item_path(&placement.path) else {
                return false;
            };
            if namespace_id(&full).is_some() {
                if placement.loose {
                    return false;
                }
                placement.pinned = false;
            }
            placement.path = full;
            paths.insert(path_key(&placement.path))
        });
        Ok(())
    }

    pub fn normalize_pages(&mut self) {
        let mut pages = HashSet::new();
        self.pages
            .items
            .retain(|page| !page.id.trim().is_empty() && pages.insert(page.id.clone()));
        if self.pages.items.is_empty() {
            self.pages.items.push(Page {
                id: MAIN_PAGE.into(),
                title: desktop_title(self.language).into(),
            });
            pages.insert(MAIN_PAGE.into());
        }
        for page in &mut self.pages.items {
            if page.title.trim().is_empty() {
                page.title = tr!("未命名页面", "未命名頁面", "Untitled page").into();
            }
        }
        let first = &self.pages.items[0].id;
        if !pages.contains(&self.active_page_id) {
            self.active_page_id.clone_from(first);
        }
        for group in &mut self.groups.items {
            let mut seen = HashSet::new();
            group
                .page_ids
                .items
                .retain(|id| pages.contains(id) && seen.insert(id.clone()));
            if group.page_ids.items.is_empty() {
                group.page_ids.items.push(first.clone());
            }
        }
    }

    pub fn group(&self, id: &str) -> Option<&Group> {
        self.groups.items.iter().find(|group| group.id == id)
    }

    pub fn group_mut(&mut self, id: &str) -> Option<&mut Group> {
        self.groups.items.iter_mut().find(|group| group.id == id)
    }

    pub fn root<'a>(&'a self, group: &'a Group) -> &'a Group {
        group
            .tab_host_id
            .as_deref()
            .and_then(|host| self.group(host))
            .unwrap_or(group)
    }

    pub fn tabs<'a>(&'a self, root: &'a Group) -> impl Iterator<Item = &'a Group> {
        std::iter::once(root).chain(
            self.groups
                .items
                .iter()
                .filter(|tab| tab.tab_host_id.as_deref() == Some(&root.id)),
        )
    }

    pub fn dock(&mut self, source: &str, target: &str) {
        let (Some(source_group), Some(target_group)) = (self.group(source), self.group(target))
        else {
            return;
        };
        let source_root = self.root(source_group).id.clone();
        let target_root = self.root(target_group).id.clone();
        if source_root == target_root {
            return;
        }
        for group in &mut self.groups.items {
            if group.id == source
                || (source == source_root && group.tab_host_id.as_deref() == Some(source))
            {
                group.tab_host_id = Some(target_root.clone());
                group.active_tab_id = None;
            } else if group.id == source_root && group.active_tab_id.as_deref() == Some(source) {
                group.active_tab_id = None;
            } else if group.id == target_root {
                group.active_tab_id = Some(source.into());
            }
        }
    }

    pub fn detach(&mut self, id: &str) {
        let Some(tab) = self.group(id) else { return };
        let root = self.root(tab).clone();
        let Some(first) = self
            .tabs(&root)
            .find(|group| group.id != id)
            .map(|group| group.id.clone())
        else {
            return;
        };
        if root.id == id {
            for group in &mut self.groups.items {
                if group.id == first {
                    group.tab_host_id = None;
                    group.active_tab_id = root
                        .active_tab_id
                        .clone()
                        .filter(|active| active != id && active != &first);
                    group.set_bounds(root.bounds());
                    group.page_ids = root.page_ids.clone();
                    group.collapsed = root.collapsed;
                    group.locked = root.locked;
                    group.hover_expand = root.hover_expand;
                    group.expand_delay = root.expand_delay;
                    group.retract_delay = root.retract_delay;
                } else if group.tab_host_id.as_deref() == Some(id) {
                    group.tab_host_id = Some(first.clone());
                }
            }
        } else if root.active_tab_id.as_deref() == Some(id) {
            self.group_mut(&root.id).unwrap().active_tab_id = None;
        }
        let tab = self.group_mut(id).unwrap();
        tab.tab_host_id = None;
        tab.active_tab_id = None;
        tab.page_ids = root.page_ids;
    }

    pub fn activate(&mut self, id: &str) {
        let Some(group) = self.group(id) else { return };
        let root = self.root(group).id.clone();
        self.group_mut(&root).unwrap().active_tab_id = (root != id).then(|| id.to_owned());
    }

    pub fn route(
        &self,
        entries: impl IntoIterator<Item = Entry>,
        now: SystemTime,
    ) -> HashMap<String, Vec<Entry>> {
        let placements: HashMap<_, _> = self
            .placements
            .items
            .iter()
            .map(|placement| (path_key(&placement.path), placement))
            .collect();
        let mut groups: HashMap<_, Vec<_>> = self
            .groups
            .items
            .iter()
            .filter(|group| group.folder.is_none())
            .map(|group| (group.id.clone(), Vec::new()))
            .collect();
        let mut seen = HashSet::new();
        let mut recycle = None;
        for entry in entries {
            if !seen.insert(entry.key.clone()) {
                continue;
            }
            if entry.is_recycle() {
                recycle = Some(entry);
                continue;
            }
            let id = match placements.get(&entry.key) {
                Some(placement) if placement.loose => continue,
                Some(placement) => &placement.group_id,
                None => self
                    .rules
                    .items
                    .iter()
                    .find(|rule| rule.matches(&entry, now))
                    .map_or(&self.default_group_id, |rule| &rule.target_id),
            };
            if let Some(items) = groups.get_mut(id) {
                items.push(entry);
            }
        }
        for group in &self.groups.items {
            if let Some(items) = groups.get_mut(&group.id) {
                group.sort_items(items);
            }
        }
        if let Some(entry) = recycle
            && let Some(items) = groups.get_mut(&self.default_group_id)
        {
            items.insert(0, entry);
        }
        groups
    }

    pub fn assign(
        &mut self,
        id: &str,
        mut paths: Vec<PathBuf>,
        desktop: &HashSet<String>,
        display: &[Entry],
        slot: Option<usize>,
    ) {
        dedup_paths(&mut paths);
        let Some(group) = self.group_mut(id).filter(|group| group.folder.is_none()) else {
            return;
        };
        if let Some(slot) = slot {
            group.insert(display, slot, &paths);
        }
        for path in paths {
            let key = path_key(&path);
            self.placements
                .items
                .retain(|placement| path_key(&placement.path) != key);
            self.placements.items.push(Placement {
                pinned: namespace_id(&path).is_none() && !desktop.contains(&key),
                path,
                group_id: id.into(),
                ..Placement::default()
            });
        }
    }
}

pub fn read(path: &Path) -> io::Result<Layout> {
    use std::io::Read;

    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_XML + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_XML {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            tr!(
                "布局文件超过 16 MB",
                "版面檔案超過 16 MB",
                "The layout file exceeds 16 MB"
            ),
        ));
    }
    let xml = std::str::from_utf8(&bytes).map_err(invalid_xml)?;
    let xml = xml.trim_start_matches('\u{feff}');
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut root_seen = false;
    let mut depth = 0u32;
    loop {
        use quick_xml::events::Event;

        match reader.read_event().map_err(invalid_xml)? {
            Event::Start(tag)
                if depth > 0 || (!root_seen && tag.name().as_ref() == b"DesktopLayout") =>
            {
                depth += 1;
                root_seen = true;
            }
            Event::Empty(tag)
                if depth > 0 || (!root_seen && tag.name().as_ref() == b"DesktopLayout") =>
            {
                root_seen = true
            }
            Event::End(_) if depth > 0 => depth -= 1,
            Event::Decl(_) if !root_seen => {}
            Event::Comment(_) | Event::PI(_) => {}
            Event::Text(text) if depth > 0 || text.as_ref().iter().all(u8::is_ascii_whitespace) => {
            }
            Event::GeneralRef(_) | Event::CData(_) if depth > 0 => {}
            Event::Eof if root_seen && depth == 0 => break,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    tr!(
                        "无法识别布局 XML",
                        "無法辨識版面 XML",
                        "Unrecognized layout XML"
                    ),
                ));
            }
        }
    }
    let mut layout: Layout = quick_xml::de::from_str(xml).map_err(invalid_xml)?;
    layout.normalize()?;
    Ok(layout)
}

fn invalid_xml(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

pub fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    name.into()
}

pub fn write(path: &Path, layout: &Layout, rotate: bool) -> io::Result<()> {
    let xml = quick_xml::se::to_string(layout).map_err(invalid_xml)?;
    if xml.len() as u64 + 40 > MAX_XML {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            tr!(
                "布局文件超过 16 MB",
                "版面檔案超過 16 MB",
                "The layout file exceeds 16 MB"
            ),
        ));
    }
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut next = tempfile::NamedTempFile::new_in(parent)?;
    writeln!(next, "<?xml version=\"1.0\" encoding=\"utf-8\"?>")?;
    next.write_all(xml.as_bytes())?;
    next.as_file().sync_all()?;
    if rotate && path.try_exists()? {
        copy_atomic(path, &sidecar(path, ".bak"))?;
    }
    next.persist(path)?;
    Ok(())
}

pub fn copy_atomic(source: &Path, target: &Path) -> io::Result<()> {
    let parent = target
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut next = tempfile::NamedTempFile::new_in(parent)?;
    io::copy(&mut fs::File::open(source)?, &mut next)?;
    next.as_file().sync_all()?;
    next.persist(target)?;
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    const LEGACY: &str = r#"<?xml version="1.0"?>
<DesktopLayout xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <Pages><DesktopPage><Id>work</Id><Title>工作</Title></DesktopPage></Pages>
  <ActivePageId>work</ActivePageId>
  <Groups>
    <DesktopGroup><Id>documents</Id><Title>文件</Title><Left>-420</Left><Width>480</Width><Sort>Manual</Sort><ManualOrder><string>C:\Desk\B.txt</string></ManualOrder><ActiveTabId>portal</ActiveTabId></DesktopGroup>
    <DesktopGroup><Id>portal</Id><Title>入口</Title><Folder>C:\Samples\A &amp; B</Folder><TabHostId>documents</TabHostId><View>Details</View></DesktopGroup>
  </Groups>
  <Rules><DesktopRule><Kind>Extension</Kind><Value>txt;md</Value><TargetId>documents</TargetId></DesktopRule></Rules>
  <Placements><Placement><Path>C:\Work\固定.txt</Path><GroupId>documents</GroupId><Pinned>true</Pinned></Placement></Placements>
  <DefaultGroupId>documents</DefaultGroupId>
  <HideWindowsIcons>true</HideWindowsIcons>
</DesktopLayout>"#;

    #[test]
    fn legacy_layout_round_trips_and_keeps_the_previous_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("layout.xml");
        fs::write(&path, LEGACY).unwrap();
        let mut layout = read(&path).unwrap();
        assert_eq!(layout.active_page_id, "work");
        assert_eq!(layout.language, Language::SimplifiedChinese);
        assert!(layout.quick_hide_on_double_click);
        assert_eq!(layout.group("documents").unwrap().left, -420);
        assert_eq!(layout.group("documents").unwrap().opacity, 82);
        let portal = layout.group("portal").unwrap();
        assert_eq!(layout.root(portal).id, "documents");
        assert_eq!(
            portal.folder.as_deref(),
            Some(Path::new(r"C:\Samples\A & B"))
        );
        assert!(layout.placements.items[0].pinned);
        layout.language = Language::TraditionalChinese;
        layout.group_mut("portal").unwrap().title = "项目入口".into();
        write(&path, &layout, true).unwrap();
        assert_eq!(
            read(&path).unwrap().group("portal").unwrap().title,
            "项目入口"
        );
        assert_eq!(fs::read_to_string(sidecar(&path, ".bak")).unwrap(), LEGACY);
        assert_eq!(read(&path).unwrap().language, Language::TraditionalChinese);
        assert!(read(&path).unwrap().hide_windows_icons);
    }

    #[test]
    fn drawing_stays_on_the_current_page_and_explicit_creation_switches_pages() {
        let mut layout = Layout::initial(
            Bounds {
                left: -1600,
                top: 0,
                width: 1600,
                height: 900,
            },
            96,
        );
        let original_page = layout.active_page_id.clone();
        let original_groups = layout.groups.items.len();
        layout.groups.items[0].title = "Personal apps".into();
        layout.groups.items[0].preset = None;
        layout.set_language(Language::English);
        assert_eq!(layout.pages.items[0].title, "Desktop");
        assert_eq!(layout.groups.items[0].title, "Personal apps");
        assert_eq!(layout.groups.items[1].title, "Desktop folders");

        let area = Bounds {
            left: -740,
            top: 240,
            width: 320,
            height: 260,
        };
        layout.draw_group(area);
        assert_eq!(layout.pages.items.len(), 1);
        assert_eq!(layout.active_page_id, original_page);
        assert_eq!(
            layout.groups.items.last().unwrap().page_ids.items,
            std::slice::from_ref(&original_page),
        );

        layout.create_page("Work".into());
        let work_page = layout.active_page_id.clone();
        assert_ne!(work_page, original_page);
        assert!(
            !layout
                .groups
                .items
                .iter()
                .any(|group| group.page_ids.items.contains(&work_page))
        );
        let work_area = Bounds {
            left: -430,
            top: 270,
            ..area
        };
        layout.draw_group(work_area);

        let file = tempfile::tempdir().unwrap();
        let path = file.path().join("layout.xml");
        write(&path, &layout, false).unwrap();
        let saved = read(&path).unwrap();
        assert_eq!(saved.pages.items[0].id, original_page);
        assert_eq!(saved.pages.items.len(), 2);
        assert_eq!(saved.active_page_id, work_page);
        assert_eq!(saved.pages.items[1].title, "Work");
        assert_eq!(saved.groups.items.len(), original_groups + 2);
        let home_panel = &saved.groups.items[original_groups];
        assert_eq!(home_panel.page_ids.items, [original_page]);
        assert_eq!(home_panel.bounds(), area);
        let work_panel = &saved.groups.items[original_groups + 1];
        assert_eq!(work_panel.title, "Group 1");
        assert_eq!(work_panel.page_ids.items, [work_page]);
        assert_eq!(work_panel.bounds(), work_area);
    }

    #[test]
    fn system_icon_assignments_survive_reload_without_becoming_file_pins() {
        let mut layout = Layout::initial(
            Bounds {
                left: 0,
                top: 0,
                width: 1280,
                height: 800,
            },
            96,
        );
        let target = layout.groups.items[0].id.clone();
        let icon = Entry::namespace(
            "shell:::{20d04fe0-3aea-1069-a2d8-08002b30309d}".into(),
            "This PC".into(),
        );
        layout.assign(
            &target,
            vec![icon.path.clone()],
            &HashSet::new(),
            &[],
            Some(0),
        );
        let file = tempfile::tempdir().unwrap();
        let path = file.path().join("layout.xml");
        write(&path, &layout, false).unwrap();
        let mut saved = read(&path).unwrap();
        assert!(!saved.placements.items[0].pinned);
        assert_eq!(
            saved.group(&target).unwrap().manual_order.items,
            std::slice::from_ref(&icon.path)
        );
        let routed = saved.route([icon.clone()], SystemTime::now());
        assert_eq!(routed[&target], [icon]);
        saved.placements.items[0].loose = true;
        saved.normalize().unwrap();
        assert!(saved.placements.items.is_empty());
    }

    fn entry(name: &str) -> Entry {
        let path = Path::new(r"C:\Desktop").join(name);
        Entry {
            key: path_key(&path),
            path,
            name: name.into(),
            label: name.into(),
            extension: "txt".into(),
            folder: false,
            modified: None,
            bytes: None,
        }
    }

    #[test]
    fn assignment_preserves_source_files_and_recycle_order() {
        let mut layout = Layout::initial(
            Bounds {
                width: 1280,
                height: 800,
                ..Bounds::default()
            },
            96,
        );
        let target = layout.default_group_id.clone();
        let entries = [entry("A.txt"), entry("B.txt"), entry("C.txt")];
        let desktop = entries.iter().map(|item| item.key.clone()).collect();
        layout.assign(
            &target,
            entries.iter().map(|e| e.path.clone()).collect(),
            &desktop,
            &[],
            None,
        );
        let displayed = layout
            .route(
                entries.iter().cloned().chain([Entry::recycle()]),
                SystemTime::now(),
            )
            .remove(&target)
            .unwrap();
        layout.assign(
            &target,
            vec![entries[2].path.clone(), PathBuf::from(r"C:\desktop\c.TXT")],
            &desktop,
            &displayed,
            Some(0),
        );
        let next = layout
            .route(
                entries.iter().cloned().chain([Entry::recycle()]),
                SystemTime::now(),
            )
            .remove(&target)
            .unwrap();
        assert!(next[0].is_recycle());
        assert!(layout.route([], SystemTime::now())[&target].is_empty());
        assert_eq!(
            next.iter()
                .skip(1)
                .map(|e| e.label.as_str())
                .collect::<Vec<_>>(),
            ["C.txt", "A.txt", "B.txt"]
        );
        assert_eq!(layout.placements.items.len(), 3);
        assert!(
            layout
                .placements
                .items
                .iter()
                .all(|placement| !placement.pinned)
        );
    }

    #[test]
    fn detaching_a_tab_host_promotes_a_sibling() {
        let mut layout = Layout::initial(
            Bounds {
                width: 1280,
                height: 800,
                ..Bounds::default()
            },
            96,
        );
        let ids: Vec<_> = layout
            .groups
            .items
            .iter()
            .map(|group| group.id.clone())
            .collect();
        layout.dock(&ids[1], &ids[0]);
        layout.dock(&ids[2], &ids[0]);
        layout.detach(&ids[0]);
        assert_eq!(layout.root(layout.group(&ids[2]).unwrap()).id, ids[1]);
        assert_eq!(
            layout.group(&ids[1]).unwrap().active_tab_id.as_deref(),
            Some(ids[2].as_str())
        );
        assert_eq!(layout.group(&ids[0]).unwrap().tab_host_id, None);
    }

    #[test]
    fn wildcard_matches_complete_names_case_insensitively() {
        assert!(wildcard("报告*?.TXT", "报告一月.txt"));
        assert!(!wildcard("报告*?.TXT", "报告.txt"));
        assert!(!wildcard("报告*.txt", "周报-报告.txt"));
    }
}
