<p align="center">
    <a href="">
      <picture>
        <img src="assets\echo-rs.svg" alt="ECHO-RS">
      </picture>
    </a>
</p>
<p align="center">
    <a href="README.md">English</a> |
    <a href="README.zh.md">简体中文</a>
</p>

echo 是一款用 Rust 编写的原生桌面音乐播放器和 Spotify 客户端。echo 将您的整个 Spotify 库——喜欢的歌曲、播放列表、专辑以及关注的艺术家——加上本地音乐文件汇集到一个快速、键盘友好的应用中，提供完整的播放控制、同步歌词和动态主题。

![echo 桌面应用](assets/echo-desktop.png)

## 功能特性

- **原生桌面应用**：基于 GPUI（Zed 的 UI 框架）构建，界面快速且由 GPU 加速，可在 Windows、macOS 和 Linux 上运行。既可以用鼠标操作，也可以完全用键盘驱动。
- **完整的资料库**：一个侧栏收纳播放列表、专辑和关注的艺术家，并支持热门曲目、最近播放和热门艺术家视图。
- **全面的播放控制**：在正在播放栏中即可播放/暂停、上一曲/下一曲、跳转进度、随机播放、重复、音量、队列和设备切换。
- **同步歌词**：时间同步的歌词可直接显示在播放栏中，或以全屏视图呈现。
- **最新动态**：来自您关注艺术家的近期专辑与单曲信息流，最多每 6 小时刷新一次。
- **极速喜欢的歌曲**：您整个喜欢的歌曲库会缓存在本地（`~/.config/echo/liked_songs.json`），实现零延迟的滚动浏览，即使有数千首保存的曲目也毫无压力。保持同步通常只需一次请求；完整重读会限速进行、可断点续传，并在 Spotify 限流时自动退避。
- **库管理**：创建、重命名、删除播放列表并将其组织到文件夹中；在自己的播放列表中重新排列曲目顺序。
- **本地音乐支持**：扫描本地音乐文件夹，播放本地文件，创建也可引用 Spotify 曲目的本地播放列表。
- **搜索**：快速全局搜索（`ctrl-k`），覆盖 Spotify 目录和已扫描的本地曲目。
- **动态主题**：内置多套主题，并支持实时主题编辑——参见[主题](#主题)。

## 设置

1. **Spotify Premium**：需要使用 Spotify Premium 账户才能通过 Spotify Web API 进行播放控制。
2. **Spotify 开发者应用**：
   - 前往 [Spotify 开发者仪表盘](https://developer.spotify.com/dashboard/)。
   - 创建一个应用并获取您的 `Client ID` 和 `Client Secret`。
   - 将 `http://127.0.0.1:8888/callback` 添加到应用的 Redirect URIs 中。
   - echo 还使用 `http://127.0.0.1:8989/login` 进行内部第一方 Spotify 会话。

### 安装

**Linux 与 macOS**

```bash
curl -fsSL https://github.com/and2049/echo/releases/latest/download/install.sh | sh
```

**Windows**（PowerShell）

```powershell
irm https://github.com/and2049/echo/releases/latest/download/install.ps1 | iex
```

两者均无需管理员权限，都会把 echo 添加到开始菜单、启动台或应用程序菜单中。

| 平台 | 安装位置 |
| --- | --- |
| Windows | `%LOCALAPPDATA%\Programs\echo`（通过发布的 MSI 安装，x64） |
| macOS | `/Applications/echo.app`（Apple Silicon） |
| Linux | `~/.local/share/echo`，并将 `echo-desktop` 链接到 `~/.local/bin`（x86_64） |

指定版本或卸载：

```bash
curl -fsSL https://github.com/and2049/echo/releases/latest/download/install.sh | sh -s -- --version 0.4.6
curl -fsSL https://github.com/and2049/echo/releases/latest/download/install.sh | sh -s -- --uninstall
```

```powershell
& ([scriptblock]::Create((irm https://github.com/and2049/echo/releases/latest/download/install.ps1))) -Version 0.4.6
& ([scriptblock]::Create((irm https://github.com/and2049/echo/releases/latest/download/install.ps1))) -Uninstall
```

卸载不会删除 `~/.config/echo` 中的配置。

在 Linux 上，桌面应用依赖若干系统库——Debian/Ubuntu 下：

```bash
sudo apt-get install libasound2 libdbus-1-3 libssl3 \
  libfontconfig1 libxkbcommon0 libxkbcommon-x11-0 libwayland-client0 libx11-xcb1
```

桌面环境通常已经带有其中大部分。渲染优先使用 Vulkan，并可回退到 OpenGL，因此
`libvulkan1` 与显卡驱动值得安装，但并非必需。

#### 更新

首次安装之后，echo 可自我更新——无需重装，也无需管理员权限。新版本会在后台自动安装；可在设置中或通过 `:autoupdate off` 关闭，也可在**设置 → 更新 → 检查更新**手动检查。更新会就地替换应用与内置主题，并提示重启。

> 早期版本还附带一个 `spotify` 终端客户端，现已移除；从这些版本更新时会一并删除它及其在 `~/.local/bin` 中的链接。

### 从源码构建

克隆仓库并使用 Cargo 构建：

**Linux 依赖**（Ubuntu/Debian）：

```bash
sudo apt-get install -y --no-install-recommends \
  libasound2-dev libdbus-1-dev pkg-config libssl-dev \
  libfontconfig-dev libwayland-dev libx11-xcb-dev libxkbcommon-x11-dev
```

```bash
git clone https://github.com/and2049/echo.git
cd echo
cargo run --release
```

生成的二进制文件位于 `./target/release/echo-desktop`。

首次运行时，echo 会提示您输入 `Client ID` 和 `Client Secret`，然后打开浏览器以通过 Spotify 进行身份验证。

## 使用

echo 支持鼠标操作——点击播放列表、艺术家或曲目即可打开，使用正在播放栏中的控件，还可拖动曲目来重排自己的播放列表。它也完全可以用键盘驱动。任何时候按 `?` 可查看应用内快捷键面板，按 `ctrl-,` 打开设置，按 `t` 切换主题。

关闭窗口时 echo 会继续播放：在 Windows 和 Linux 上隐藏到托盘图标（点击图标恢复窗口，或选择退出），在 macOS 上保留在 Dock 中。可用 `:tray off` 或「设置 → 窗口」关闭此行为；`ctrl-q` 始终退出。再次启动 echo 会把正在运行的实例带到前台，而不是打开第二个。

### 导航
- `j` / `k` 或 `↓` / `↑`：向下 / 向上移动
- `gg` / `G`：跳到第一个 / 最后一个项目
- `ctrl-b` / `ctrl-f` 或 `Page Up` / `Page Down`：移动一页
- `ctrl-u` / `ctrl-d`：移动半页
- `gc`：跳转到正在播放的曲目或其上下文
- `enter` 或 `z`：打开所选项目 / 播放所选曲目
- `h` / `esc`：返回 / 关闭面板
- `←` / `→`：在侧栏与主窗格之间移动焦点；`backspace` 也可回到侧栏
- `alt-←` / `alt-→`：历史后退 / 前进
- `tab`：切换标签页（例如搜索结果、艺术家作品）
- `ctrl-h`（macOS 上为 `ctrl-shift-h`）：首页
- `ctrl-\`：显示 / 隐藏侧栏

### 播放
- `space`：播放 / 暂停
- `]` / `[`（或 `ctrl-→` / `ctrl-←`）：下一曲 / 上一曲
- `.` / `,`（或 `shift-→` / `shift-←`）：向前 / 向后跳转 5 秒
- `0`：跳到曲目开头
- `=` / `-`：音量增大 / 减小 1%；`+` / `_` 为 5%
- `shift-M`：静音 / 恢复之前的音量
- `s`：切换随机播放
- `r`：切换重复模式（关闭 → 单曲循环 → 列表循环）
- `shift-D`：设备菜单
- `shift-L`：同步歌词面板
- `ctrl-shift-L`：播放栏内的精简歌词
- `shift-F`：沉浸视图

### 库操作
- `l`：喜欢 / 取消喜欢所选曲目
- `a`：将所选曲目添加到播放列表，或将所选专辑添加到库中
- `shift-A`：所选（或正在播放）曲目的操作菜单
- `q`：将所选曲目加入队列
- `shift-Q`：打开队列
- `m`：固定 / 取消固定播放列表
- `c` / `e`：创建 / 重命名播放列表或文件夹
- `v`：可视模式，用于选择范围
- `dd`：删除播放列表或文件夹，或从自己的播放列表中移除曲目
- `shift-J` / `shift-K`：在自己的播放列表中将所选曲目下移 / 上移（也支持拖拽）；需要原始排序方式
- `shift-R`：强制刷新

### 查找内容
- `ctrl-k`：全局搜索
- `f`：从命令栏搜索
- `/`：过滤当前列表
- `n` / `shift-N`：下一个 / 上一个匹配项
- `:`：命令栏——参见[命令](#命令)

曲目操作菜单会根据来源自动调整。Spotify 曲目支持复制链接、喜欢和专辑入库等操作。本地曲目支持复制绝对路径和在系统文件管理器中显示文件。两种来源都保留专辑/艺术家导航、插入播放列表和加入队列等操作（如适用）。

## 命令

`:` 命令栏接受以下命令：
- `:search <query>`：搜索曲目或专辑。
- `:newplaylist <name>`：创建新播放列表。
- `:newlocalplaylist <name>`：创建存储在本机的本地播放列表。
- `:localpath <absolute-folder-path>`：设置本地音乐文件夹并扫描。路径必须为绝对路径，支持 macOS、Windows 和 Linux。
- `:rescanlocal`：重新扫描已配置的本地音乐文件夹。
- `:newfolder <name>`：创建新文件夹以组织播放列表。
- `:delfolder`：删除当前选中的文件夹。
- `:rename <name>`：重命名当前选中的播放列表或文件夹。
- `:sort <alpha|creator>`：对播放列表库进行排序。
- `:sort <original|title|artist|album|duration|added|reverse>`：完全在内存中对当前曲目列表排序。
- `:seek <seconds|+seconds|-seconds>`：跳转到绝对位置或按相对偏移跳转。
- `:sleep <30m|1h|off>`：延迟后暂停播放（睡眠定时器）。
- `:mute`：静音播放或恢复之前的音量。
- `:open [spotify-url-or-uri]`：打开 Spotify 曲目、专辑、艺术家或播放列表。不带参数时从剪贴板读取。
- `:relative <on|off|toggle>`：配置曲目列表中 Vim 风格的相对行号。
- `:theme <theme_name>`：切换应用主题。
- `:lang <en|zh|zh-CN>`：切换语言。
- `:album`：跳转到当前选中曲目所属的专辑。
- `:queue`：打开队列视图。
- `:clearqueue`：清空手动加入队列的歌曲（仅在此设备上播放时可用）。
- `:clearhistory`：清除本地播放记录。
- `:range <short|medium|long>`：热门曲目与热门艺术家的时间范围。
- `:spotifylogin`：重新登录 Spotify。
- `:vis`：切换音频可视化器。
- `:visbins <number>`：设置音频可视化器频率条数量（5-32）。
- `:pixelate <pixels>`：在专辑封面上启用复古 8 位像素风格。设置为 0 可禁用，或例如 16 以获得像素化效果。
- `:backdrop <lights|mesh|aurora|vinyl|nebula>`：选择沉浸视图背后的动态画面（设置中也可选择）。
- `:tray [on|off]`：关闭按钮是将 echo 隐藏到托盘还是直接退出（设置中也可选择）。
- `:autoupdate [on|off]`：自动安装新版本（设置中也可选择）。
- `:index <number>`：设置曲目索引基数（从 1 开始或从 0 开始）。
- `:quit`、`:q`、`:qa`、`:wq`：退出应用。

曲目排序与导航仅作用于已加载的数据，不会向 Spotify 发送请求。导航历史最多保留 20 个内存中的视图，因此返回之前的曲目列表通常无需重新获取。

## 主题

主题位于 `themes/*.toml`，采用扁平列表格式：九个基础颜色之后是应用使用的十二个派生颜色，每一项都有明确的注释说明其用途。您可以随意修改数值，或修改基础颜色后运行 `python themes/generate_desktop.py` 重新计算派生颜色。派生键是可选的——缺失的键会按其注释中命名的公式计算，同时也可以接受 `[desktop]` 表用于覆盖。要进行可视化迭代，可运行 `python tools/theme-preview/serve.py` 在浏览器中打开桌面窗口的实时模拟，每次保存都会重新着色——无需重新构建。颜色可以从任一方向编辑：在编辑器中修改 toml 文件，或在预览图例中点击任意颜色，用取色器调整并直接写回文件（其中的“重新计算派生色”按钮会为当前主题重新运行生成器）。

## 音频质量

echo 以 320 kbps 流媒体播放并应用音量归一化，与 Spotify 桌面应用的默认行为一致。这些选项位于 `~/.config/echo/config.toml` 的 `[library]` 下，并在下次启动时生效。

```toml
[library]
bitrate = 320               # 96、160 或 320
normalisation = true        # 平衡曲目之间的响度，类似 Spotify 应用。
normalisation_pregain = 3.0 # 归一化后加回的分贝数。若播放过小声请调高。
```

归一化会根据每首曲目的 ReplayGain 值进行衰减，现代母带通常有几个分贝。`normalisation_pregain` 会把这部分余量加回来，使播放音量与 Spotify 应用相当。增益作用于 librespot 的动态限制器之前，因此调高不会产生削波。设置 `normalisation = false` 可完全跳过增益环节，获得位精确的满幅输出，代价是曲目之间会出现响度跳变。

音量完全在客户端侧应用——Spotify 流以满幅到达，由 echo 自行衰减，因此在其他 Spotify 客户端中设备的音量滑块不起作用。Spotify 和本地播放使用相同的三次方音量曲线，因此无论播放哪个来源，相同的百分比听起来一样，且两者在 100% 时均为单位增益。

只要设备支持，echo 就会以立体声 44.1 kHz（librespot 的原始采样率，因此无需重采样）打开输出设备。不提供 44.1 kHz 的设备（大多数 Windows 端点默认 48 kHz）会回退到设备自身的默认采样率。

实际打开的端点会写入工作目录下的 `echo-debug-audio-spotify.log`，本地文件则写入 `echo-debug-audio-local.log`：

```
device=Headphones (WH-1000XM5) channels=2 sample_rate=48000 format=F32
```

## 本地音乐

本地音乐支持与 Spotify 分开。使用 `:localpath <absolute-folder-path>` 选择 echo 应扫描的文件夹。支持的音频扩展名为 `mp3`、`wav`、`flac`、`ogg`、`m4a` 和 `aac`；echo 会递归扫描并读取标题、艺术家、专辑、时长和封面图（如有）。echo 在启动时会刷新已配置的本地文件夹，并在运行期间监视其中的音频/封面图变化；`:rescanlocal` 仍可作为手动回退方案使用。

本地播放列表存储在本地，不是 Spotify 播放列表。它们可以包含本地曲目和 Spotify 曲目引用。Spotify 播放列表不能包含本地曲目。本地随机播放、重复、音量、队列和播放/暂停由 echo 的本地播放引擎处理。

内嵌封面图会被优先使用。如果曲目没有内嵌封面图，echo 会查找文件夹中的封面图，例如 `cover.jpg`、`folder.jpg` 或 `front.png`。

## 故障排除

- **缓存不同步**：其他设备上喜欢的歌曲会在启动时或打开喜欢的歌曲时出现（最多每 15 分钟检查一次）。在其他设备上取消喜欢的歌曲也会在同一时机被发现，并触发后台重读整个库；被限流的重读会从中断处继续。在 echo 关闭时删除 `~/.config/echo/liked_songs.json` 可强制完整重读。
- **本地文件丢失**：如果文件在扫描后被删除或移动，运行 `:rescanlocal` 以刷新本地库。
- **音频听起来单声道或沉闷（蓝牙耳机）**：Windows 将蓝牙耳机暴露为两个输出设备——立体声的“耳机”（A2DP）端点，以及限制在 16 kHz 的单声道“免提”（HFP）端点。每当有应用程序打开麦克风时，Windows 就会切换到免提模式。检查 `echo-debug-audio-spotify.log`：如果报告 `channels=1`，请退出占用麦克风的程序，并将立体声端点设为默认输出设备。
- **配置文件路径**：`~/.config/echo/config.toml`（保存令牌和偏好设置）、`~/.config/echo/cache.json`（保存库缓存和喜欢状态）、`~/.config/echo/liked_songs.json`（保存喜欢的歌曲列表）、`~/.config/echo/local_library.json` 和 `~/.config/echo/local_playlists.json`。
