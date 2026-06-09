<p align="center">
    <a href="">
      <picture>
        <img src="assets\echo-rs.svg" alt="ECHO-RS">
      </picture>
    </a>
</p>
<p align="center">
    <a href="README.md">English</a> |
    <a href="README.zh.md">简体中文</a> |
    <a href="README.zht.md">繁體中文</a>
</p>

echo 是一款用 Rust 编写的终端音乐播放器和 Spotify 客户端。echo 将您的本地文件以及整个 Spotify 库、喜欢的歌曲、播放列表和播放控制直接带到终端中，并配有美观、动态的 TUI，支持原生图像渲染。

![demo](demo.png)

## 功能特性

- **终端图像支持**：直接在终端中呈现高品质专辑封面和播放列表封面（支持 Kitty、Sixel 以及块状回退方案）。
- **极速喜欢的歌曲**：采用全局缓存架构。您整个喜欢的歌曲库会缓存在本地（`~/.config/echo/cache.json`），实现零延迟、无速率限制的滚动浏览，即使有数千首保存的曲目也毫无压力。
- **库管理**：创建、重命名、删除播放列表并将其组织到文件夹中。
- **本地音乐支持**：扫描本地音乐文件夹，播放本地文件，创建也可引用 Spotify 曲目的本地播放列表。
- **响应式播放控制**：全面控制播放、队列、随机播放、重复播放和音量。
- **搜索**：快速全局搜索 Spotify 目录和已扫描的本地曲目。

## 设置

1. **Spotify Premium**：需要使用 Spotify Premium 账户才能通过 Spotify Web API 进行播放控制。
2. **Spotify 开发者应用**：
   - 前往 [Spotify 开发者仪表盘](https://developer.spotify.com/dashboard/)。
   - 创建一个应用并获取您的 `Client ID` 和 `Client Secret`。
   - 将 `http://127.0.0.1:8888/callback` 添加到应用的 Redirect URIs 中。
   - Echo 还使用 `http://127.0.0.1:8989/login` 进行内部第一方 Spotify 会话。

### 安装

下载并运行安装程序：
https://github.com/and2049/echo/releases

或者

克隆仓库并使用 Cargo 构建：

```bash
git clone https://github.com/and2049/echo.git
cd echo
cargo build --release
```

运行二进制文件：

```bash
./target/release/echo
```

首次运行时，echo 会提示您输入 `Client ID` 和 `Client Secret`，然后打开浏览器以通过 Spotify 进行身份验证。

## 导航与快捷键

echo 主要由键盘驱动。

### 全局导航
- `j` / `k` 或 `Down` / `Up`：向下 / 向上移动
- `Enter` 或 `z`：选择项目 / 打开播放列表 / 播放曲目
- `h` / `q` / `Esc` / `Backspace`：返回 / 关闭模态框 / 清除搜索
- `Tab`：切换标签页（例如，播放列表 ↔ 专辑，搜索曲目 ↔ 搜索专辑）
- `:`：进入命令模式
- `/`：在曲目列表中搜索
- `f`：全局搜索
- `n` / `N`：跳转到列表中的下一个 / 上一个搜索结果

### 播放控制
- `Space`：播放 / 暂停
- `]` / `>`：下一曲
- `[` / `<`：上一曲
- `s`：切换随机播放
- `r`：切换重复模式（关闭 → 单曲循环 → 列表循环）
- `=` / `-`：音量增大 / 减小（1%）
- `+` / `_`：音量增大 / 减小（5%）
- `D`（Shift + d）：打开设备选择菜单
- `L`（Shift + l）：切换全屏同步歌词界面
- `Ctrl + Shift + L`：切换精简同步歌词视图

### 曲目与库操作
- `l`：喜欢 / 取消喜欢所选曲目
- `A`（Shift + a）：打开悬停曲目的操作菜单（若未聚焦在曲目页面则打开当前播放曲目的菜单）
- `p`：将剪切的播放列表粘贴到文件夹中
- `a`：将所选曲目添加到播放列表 / 将所选专辑添加到库中
- `q`：将当前悬停的曲目添加到队列
- `Q`（Shift + q）：打开队列视图
- `m`：固定 / 取消固定播放列表
- `c`：快速创建新播放列表
- `e`：快速重命名播放列表或文件夹
- `v`：进入可视模式以进行多选
- `d`（双击）：删除播放列表/文件夹，或从自定义播放列表中移除曲目
- `x`：剪切播放列表（以便移动到文件夹中）
- `R`（Shift + r）：强制刷新

## 命令

在命令模式（`:`）下，您可以使用以下命令：
- `:search <query>`：搜索曲目或专辑。
- `:newplaylist <name>`：创建新播放列表。
- `:newlocalplaylist <name>`：创建存储在本机的本地播放列表。
- `:localpath <absolute-folder-path>`：设置本地音乐文件夹并扫描。路径必须为绝对路径，支持 macOS、Windows 和 Linux。
- `:rescanlocal`：重新扫描已配置的本地音乐文件夹。
- `:newfolder <name>`：创建新文件夹以组织播放列表。
- `:delfolder`：删除当前选中的文件夹。
- `:rename <name>`：重命名当前选中的播放列表或文件夹。
- `:sort <alpha|creator>`：对库进行排序。
- `:theme <theme_name>`：切换应用主题。
- `:lang <en|zh|zh-CN>`：切换语言。
- `:album`：跳转到当前选中曲目所属的专辑。
- `:queue`：打开队列视图。
- `:vis`：切换音频可视化器。
- `:visbins <number>`：设置音频可视化器频率条数量（5-32）。
- `:pixelate <pixels>`：在专辑封面上启用复古 8 位像素风格。设置为 0 可禁用，或例如 16 以获得像素化效果。
- `:index <number>`：设置曲目索引基数（从 1 开始或从 0 开始）。
- `:quit`、`:q`、`:qa`、`:wq`：退出应用。

## 本地音乐

本地音乐支持与 Spotify 分开。使用 `:localpath <absolute-folder-path>` 选择 echo 应扫描的文件夹。支持的音频扩展名为 `mp3`、`wav`、`flac`、`ogg`、`m4a` 和 `aac`；echo 会递归扫描并读取标题、艺术家、专辑、时长和封面图（如有）。Echo 在启动时会刷新已配置的本地文件夹，并在运行期间监视其中的音频/封面图变化；`:rescanlocal` 仍可作为手动回退方案使用。

本地播放列表存储在本地，不是 Spotify 播放列表。它们可以包含本地曲目和 Spotify 曲目引用。Spotify 播放列表不能包含本地曲目。本地随机播放、重复、音量、队列和播放/暂停由 echo 的本地播放引擎处理。

内嵌封面图会被优先使用。如果曲目没有内嵌封面图，echo 会查找文件夹中的封面图，例如 `cover.jpg`、`folder.jpg` 或 `front.png`。

## 故障排除

- **图像无法渲染**：确保您的终端支持 Kitty 图像协议或 Sixel 图形（例如 Kitty、WezTerm、打了补丁的 Alacritty）。如果都不支持，echo 将回退到块状渲染。
- **缓存不同步**：如果您喜欢的歌曲与其他设备不同步，只需重启 echo。它会在启动时在后台急切地同步您的库。
- **本地文件丢失**：如果文件在扫描后被删除或移动，运行 `:rescanlocal` 以刷新本地库。
- **配置文件路径**：`~/.config/echo/config.toml`（保存令牌和偏好设置）、`~/.config/echo/cache.json`（保存喜欢的曲目）、`~/.config/echo/local_library.json` 和 `~/.config/echo/local_playlists.json`。
