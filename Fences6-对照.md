# 桌面格 1.2 与 Stardock Fences 6

2026 年 11 月 19 日核对。以 Stardock 的 Fences 6 发布资料、帮助中心和 6.x 更新记录为准。桌面格功能以当前源码与 Windows 测试为准。 [Fences 6 发布资料](https://www.stardock.com/press/software/fences6/documents/fences_6_1.0_press_release.pdf) · [Fences 6 帮助中心](https://support.stardock.com/space/SHC/2814443558/Fences%2B6%2B%2BUI) · [产品更新记录](https://www.stardock.com/products/fences/history)

| 功能 | 桌面格 1.2 | 与 Fences 6 的差距 |
| --- | --- | --- |
| 桌面分组 | 桌面层面板、拖动标题栏、右下角缩放、锁定、拖放归类和标签组 | Fences 6 可以在桌面圈选区域建组、拖边缩放。桌面格从托盘或管理窗口建组，图标在独立显示层渲染。 |
| 文件夹入口 | 浏览子目录、返回、打开资源管理器，支持图标、列表和详细信息视图，接受原生文件拖放 | Fences 6 的入口更接近资源管理器，包含更多显示选项。桌面格提供基本项目菜单，从资源管理器可以访问完整文件菜单。 |
| 桌面页面 | 新建、改名、快捷切页，一组可同时出现在多个选定页面 | Fences 6 支持鼠标拖屏幕边缘、触摸手势以及拖着图标切页。桌面格使用快捷键和菜单，跨页共用同一面板的位置。 |
| 折叠与隐藏 | 手动折叠、悬停延时展开和收起、展开时抬到同层面板上方、双击空白隐藏、快捷键置顶 | Fences 6 有屏幕边缘自动折叠等触发方式。桌面格的空白双击在原生桌面图标隐藏时开启，原生图标可见时用托盘或快捷键。 |
| 布局管理 | 拖动时边缘吸附、批量对齐、统一宽度、XML 导入导出和上一份配置备份 | Fences 6 提供多显示器配置切换后的布局记忆和带预览的快照历史。桌面格当前把面板限制在可用工作区，尚无按显示器配置保存独立布局、自动历史列表或拖动参考线。 |
| 自动分类与外观 | 扩展名、文件名通配符、文件夹、修改时间规则，支持优先级和手动覆盖。外观可调颜色、不透明度、图标色、大小和淡出 | Fences 6 有更丰富的规则条件、标签外观和壁纸融合设置。桌面格使用半透明色板和淡出效果。 |
| 企业管理 | 用户本机配置与导入导出 | Fences 6 发布资料还列有企业布局管理能力。 |

对照细节见 [分组操作](https://support.stardock.com/space/SHC/2814443582/Fences%2B6%2B%2BUI%3A%2BWelcome)、[文件夹入口](https://support.stardock.com/space/SHC/1459421201/Fences%2BUI%3A%2BFolder%2BPortals)、[桌面页面](https://support.stardock.com/space/SHC/2814444135/Fences%2B6%2B%2BUI%3A%2BPages)、[折叠](https://support.stardock.com/space/SHC/2814444059)、[布局快照](https://support.stardock.com/space/SHC/1331331082)和[多显示器布局说明](https://forums.stardock.com/514099/question-about-fences-4-now-6-features-and-bug-fixes)。

## 本次采纳的社区需求

| 社区反馈 | 桌面格中的实现 |
| --- | --- |
| [论坛用户希望知道文件在哪个分组](https://forums.stardock.com/517253/fences-search-feature) | `Ctrl+Alt+F` 在所有页面和面板中按名称搜索。结果显示归属，`Ctrl+Enter` 切页并选中项目。文件夹入口的搜索范围为根目录和当前浏览目录。 |
| [Steam 用户希望主页面与工作或游戏页面共用同一组](https://steamcommunity.com/app/3165690/discussions/0/693120026285217390/) | 在面板菜单勾选多个页面。各页面看到同一组内容，更新一次即可。 |
| [论坛长期用户建议独立图标大小、磁吸与批量对齐](https://forums.stardock.com/542664/suggestions-for-improving-fences-long-time-user-feedback) | 每组可选 24、32、48 px。拖动面板有磁吸，管理窗口支持六种边缘或居中对齐，也能统一宽度。 |
| [论坛用户要求自定义收起延迟](https://forums.stardock.com/541384/feedback-feature-requests-for-stardock-fences)和 [Reddit 用户要求展开时盖过其他面板](https://www.reddit.com/r/software/comments/1xr7xvm/i_made_glassdesk_a_free_opensource_glass_desktop/) | 在面板设置中分别配置悬停展开和收起延迟。临时展开时面板会提到同层其他面板前面。 |

## 拖放

拖到普通分组空白处或普通标签会重新归类。拖到文件夹图标、文件夹入口或资源管理器会交给 Windows 执行文件操作。按住 Ctrl 拖放可请求复制。拖到回收站图标会交给 Windows 的回收站处理，具体结果遵循系统的文件和卷设置。
