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

echo 是一款用 Rust 編寫的原生桌面音樂播放器和 Spotify 用戶端。echo 將您的整個 Spotify 庫——喜歡的歌曲、播放清單、專輯以及追蹤的藝術家——加上本地音樂檔案匯集到一個快速、鍵盤友善的應用程式中，提供完整的播放控制、同步歌詞和動態主題。同一安裝套件還附帶一個終端機用戶端（`spotify`），在您想待在終端機裡時隨時可用。

![echo 桌面應用程式](assets/echo-desktop.png)

## 功能特性

- **原生桌面應用程式**：基於 GPUI（Zed 的 UI 框架）建置，介面快速且由 GPU 加速，可在 Windows、macOS 和 Linux 上執行。既可以用滑鼠操作，也可以完全用鍵盤驅動。
- **完整的音樂庫**：一個側邊欄收納播放清單、專輯和追蹤的藝術家，並支援熱門曲目、最近播放和熱門藝術家檢視。
- **全面的播放控制**：在正在播放列中即可播放/暫停、上一曲/下一曲、跳轉進度、隨機播放、重複、音量、佇列和裝置切換。
- **同步歌詞**：時間同步的歌詞可直接顯示在播放列中，或以全螢幕檢視呈現。
- **最新動態**：來自您追蹤藝術家的近期專輯與單曲資訊流，最多每 6 小時重新整理一次。
- **極速喜歡的歌曲**：採用全域快取架構。您整個喜歡的歌曲庫會快取在本地（`~/.config/echo/cache.json`），實現零延遲、無速率限制的捲動瀏覽，即使有數千首儲存的曲目也毫無壓力。
- **庫管理**：建立、重新命名、刪除播放清單並將其組織到資料夾中；在自己的播放清單中重新排列曲目順序。
- **本地音樂支援**：掃描本地音樂資料夾，播放本地檔案，建立也可引用 Spotify 曲目的本地播放清單。
- **搜尋**：快速全域搜尋（`ctrl-k`），涵蓋 Spotify 目錄和已掃描的本地曲目。
- **動態主題**：內建多套主題，並支援即時主題編輯——參見[主題](#主題)。
- **終端機用戶端**：同一安裝也會將功能完備的 `spotify` TUI 加入您的 `PATH`——參見[終端機用戶端 (TUI)](#終端機用戶端-tui)。

## 設定

1. **Spotify Premium**：需要使用 Spotify Premium 帳戶才能透過 Spotify Web API 進行播放控制。
2. **Spotify 開發者應用**：
   - 前往 [Spotify 開發者儀表板](https://developer.spotify.com/dashboard/)。
   - 建立一個應用並取得您的 `Client ID` 和 `Client Secret`。
   - 將 `http://127.0.0.1:8888/callback` 新增到應用的 Redirect URIs 中。
   - echo 還使用 `http://127.0.0.1:8989/login` 進行內部第一方 Spotify 工作階段。

### 安裝

一行指令即可完成安裝：桌面應用程式**與** `spotify` 終端機指令。

**Linux 與 macOS**

```bash
curl -fsSL https://github.com/and2049/echo/releases/latest/download/install.sh | sh
```

**Windows**（PowerShell）

```powershell
irm https://github.com/and2049/echo/releases/latest/download/install.ps1 | iex
```

兩者皆不需系統管理員權限，都會將 `spotify` 加入 `PATH`（安裝後請開啟新終端機），並把桌面應用程式加入開始功能表、Launchpad 或應用程式選單。

| 平台 | 安裝位置 |
| --- | --- |
| Windows | `%LOCALAPPDATA%\Programs\echo`（透過發佈的 MSI 安裝，x64） |
| macOS | `/Applications/echo.app`，並將 `spotify` 連結至 `~/.local/bin`（Apple Silicon） |
| Linux | `~/.local/share/echo`，並將兩個指令連結至 `~/.local/bin`（x86_64） |

指定版本或解除安裝：

```bash
curl -fsSL https://github.com/and2049/echo/releases/latest/download/install.sh | sh -s -- --version 0.4.6
curl -fsSL https://github.com/and2049/echo/releases/latest/download/install.sh | sh -s -- --uninstall
```

```powershell
& ([scriptblock]::Create((irm https://github.com/and2049/echo/releases/latest/download/install.ps1))) -Version 0.4.6
& ([scriptblock]::Create((irm https://github.com/and2049/echo/releases/latest/download/install.ps1))) -Uninstall
```

解除安裝不會刪除 `~/.config/echo` 中的設定。

在 Linux 上，桌面應用程式相依於數個系統程式庫——Debian/Ubuntu 下：

```bash
sudo apt-get install libasound2 libdbus-1-3 libssl3 \
  libfontconfig1 libxkbcommon0 libxkbcommon-x11-0 libwayland-client0 libx11-xcb1
```

桌面環境通常已經帶有其中大部分。算繪優先使用 Vulkan，並可回退到 OpenGL，因此
`libvulkan1` 與顯示卡驅動程式值得安裝，但並非必要。

#### 更新

首次安裝之後，echo 可自我更新——不需重新安裝，也不需系統管理員權限：

```bash
spotify upgrade          # 更新至最新版本
spotify upgrade --check  # 僅檢查是否有可用更新
spotify upgrade 0.4.6    # 更新至指定版本
```

桌面應用程式也可透過**設定 → 更新 → 檢查更新**完成同樣操作。兩者都會就地取代執行檔與內建主題，並提示重新啟動。

### 從原始碼建置

複製倉庫並使用 Cargo 建置：

**Linux 相依性**（Ubuntu/Debian）：

```bash
sudo apt-get install -y --no-install-recommends \
  libasound2-dev libdbus-1-dev pkg-config libssl-dev \
  libfontconfig-dev libwayland-dev libx11-xcb-dev libxkbcommon-x11-dev
```

```bash
git clone https://github.com/and2049/echo.git
cd echo
cargo build --release -p echo-desktop -p spotify-tui
```

執行桌面應用程式或終端機用戶端：

```bash
./target/release/echo-desktop   # 桌面應用程式
./target/release/spotify        # 終端機用戶端
```

裸的 `cargo build --release`（或 `cargo run`）只建置 `spotify` 終端機用戶端——它是工作區的預設成員——因此要建置桌面應用程式請額外指定 `-p echo-desktop`。

> 終端機指令名為 `spotify`——之前的名稱與 shell 內建指令 `echo` 衝突。設定與快取仍位於 `~/.config/echo/`，因此現有環境在改名後可繼續使用。在 Windows 上，如果官方 Spotify 用戶端的目錄恰好在您的 `PATH` 中，請確保 `%USERPROFILE%\.cargo\bin`（或您安裝此執行檔的位置）排在前面。

首次執行時，echo 會提示您輸入 `Client ID` 和 `Client Secret`，然後開啟瀏覽器以透過 Spotify 進行身分驗證。

## 桌面應用程式

桌面應用程式支援滑鼠操作——點擊播放清單、藝術家或曲目即可開啟，使用正在播放列中的控制項，還可拖曳曲目來重排自己的播放清單。它也完全可以用鍵盤驅動。任何時候按 `?` 可查看應用程式內快捷鍵面板，按 `ctrl-,` 開啟設定，按 `t` 切換主題。快捷鍵與下方的終端機用戶端一致。

**導覽**：`j` / `k` / `↓` / `↑` 移動，`gg` / `G` 跳到開頭 / 結尾，`ctrl-b` / `ctrl-f` 翻頁，`ctrl-u` / `ctrl-d` 翻半頁，`enter` / `z` 開啟，`h` / `esc` 返回，`←` / `→` 在窗格間移動焦點，`tab` 切換分頁，`gc` 跳轉到正在播放的曲目。

**播放**：`space` 播放/暫停，`[` / `]`（或 `ctrl-←` / `ctrl-→`）上一曲 / 下一曲，`,` / `.` 跳轉進度，`-` / `=` 音量，`shift-M` 靜音，`s` / `r` 隨機播放 / 重複，`shift-D` 裝置選單，`shift-L` 全螢幕歌詞，`ctrl-shift-L` 播放列內歌詞，`shift-F` 沉浸檢視。

**庫操作**：`l` 喜歡/取消喜歡，`a` 新增至播放清單，`shift-A` 曲目操作，`q` / `shift-Q` 佇列，`shift-J` / `shift-K` 在自己的播放清單中移動曲目，`dd` 刪除，`v` 選取範圍，`m` 釘選，`c` / `e` 建立 / 重新命名。

**尋找內容**：`ctrl-k` 全域搜尋，`/` 篩選目前清單，`n` / `shift-N` 下一個 / 上一個符合項目，`:` 指令列，`t` 主題，`?` 說明，`ctrl-q` 結束。

`:` 指令列接受與終端機用戶端相同的指令——參見[命令](#命令)。

## 終端機用戶端 (TUI)

同一安裝還附帶 `spotify`，一個功能完備的終端機用戶端，支援在終端機中直接算繪影像。它與桌面應用程式共用 echo 的快取、設定、播放引擎和指令。

![echo 終端機用戶端](assets/echo-tui.png)

- **終端機影像支援**：直接在終端機中呈現高品質專輯封面和播放清單封面（支援 Kitty、Sixel 以及區塊式回退方案）。
- **桌面應用程式的全部功能**：相同的音樂庫、搜尋、本地音樂、播放控制和 `:` 指令，完全由鍵盤驅動。

echo 主要由鍵盤驅動。

### 全域導航
- `j` / `k` 或 `Down` / `Up`：向下 / 向上移動
- `gg` / `G`：跳到第一個 / 最後一個項目
- `Ctrl-b` / `Ctrl-f` 或 `Page Up` / `Page Down`：移動一頁
- `Ctrl-u` / `Ctrl-d`：移動半頁
- `Ctrl-l`：清除並完整重繪 TUI
- `gc`：跳轉到正在播放的曲目或其可用上下文
- `Enter` 或 `z`：選擇項目 / 開啟播放清單 / 播放曲目
- `h` / `q` / `Esc` / `Backspace`：返回 / 關閉對話方塊 / 清除搜尋
- `Tab`：切換分頁（例如，播放清單 ↔ 專輯，搜尋曲目 ↔ 搜尋專輯）
- `:`：進入命令模式
- `/`：在曲目清單中搜尋
- `f`：全域搜尋
- `n` / `N`：跳轉到清單中的下一個 / 上一個搜尋結果

### 播放控制
- `Space`：播放 / 暫停
- `]` / `>`：下一曲
- `[` / `<`：上一曲
- `,` / `.`：向後 / 向前快轉 5 秒
- `0`：跳到目前曲目開頭
- `M`（Shift + m）：靜音 / 回復之前的音量
- `s`：切換隨機播放
- `r`：切換重複模式（關閉 → 單曲循環 → 清單循環）
- `=` / `-`：音量增大 / 減小（1%）
- `+` / `_`：音量增大 / 減小（5%）
- `D`（Shift + d）：開啟裝置選擇選單
- `L`（Shift + l）：切換全螢幕同步歌詞介面
- `Ctrl + Shift + L`：切換精簡同步歌詞檢視

### 曲目與庫操作
- `l`：喜歡 / 取消喜歡所選曲目
- `A`（Shift + a）：開啟懸停曲目的操作選單（若未聚焦在曲目頁面則開啟目前播放曲目的選單）
- `p`：將剪下的播放清單貼上到資料夾中
- `a`：將所選曲目新增到播放清單 / 將所選專輯新增到庫中
- `q`：將目前懸停的曲目加入佇列
- `Q`（Shift + q）：開啟佇列檢視
- `m`：釘選 / 取消釘選播放清單
- `T`（Shift + t）：切換資料庫封面縮圖（在播放清單 / 專輯名稱旁顯示封面）
- `c`：快速建立新播放清單
- `e`：快速重新命名播放清單或資料夾
- `v`：進入視覺模式以進行多選
- `d`（雙擊）：刪除播放清單/資料夾，或從自訂播放清單中移除曲目
- `x`：剪下播放清單（以便移動到資料夾中）
- `J` / `K`（Shift + j / k，桌面版）：在自己播放清單中將所選曲目下移 / 上移（也支援拖放）；需要原始排序方式
- `R`（Shift + r）：強制重新整理

曲目操作選單會根據來源自動調整。Spotify 曲目支援複製連結、喜歡和專輯入庫等操作。本地曲目支援複製絕對路徑和在系統檔案管理員中顯示檔案。兩種來源都保留專輯/藝術家導覽、插入播放清單和加入佇列等操作（如適用）。

## 命令

在命令模式（`:`）下，您可以使用以下指令：
- `:search <query>`：搜尋曲目或專輯。
- `:newplaylist <name>`：建立新播放清單。
- `:newlocalplaylist <name>`：建立儲存在本機的本地播放清單。
- `:localpath <absolute-folder-path>`：設定本地音樂資料夾並掃描。路徑必須為絕對路徑，支援 macOS、Windows 和 Linux。
- `:rescanlocal`：重新掃描已設定的本地音樂資料夾。
- `:newfolder <name>`：建立新資料夾以組織播放清單。
- `:delfolder`：刪除目前選中的資料夾。
- `:rename <name>`：重新命名目前選中的播放清單或資料夾。
- `:sort <alpha|creator>`：對播放清單庫進行排序。
- `:sort <original|title|artist|album|duration|added|reverse>`：完全在記憶體中對目前曲目清單排序。
- `:seek <seconds|+seconds|-seconds>`：跳轉到絕對位置或按相對偏移跳轉。
- `:sleep <30m|1h|off>`：延遲後暫停播放（睡眠定時器）。
- `:mute`：靜音播放或回復之前的音量。
- `:open [spotify-url-or-uri]`：開啟 Spotify 曲目、專輯、藝術家或播放清單。不帶參數時從剪貼簿讀取。
- `:relative <on|off|toggle>`：設定曲目清單中 Vim 風格的相對行號。
- `:redraw`：在終端機輸出異常後清除並完整重繪 TUI。
- `:theme <theme_name>`：切換應用主題。
- `:lang <en|zh|zh-CN>`：切換語言。
- `:album`：跳轉到目前選中曲目所屬的專輯。
- `:queue`：開啟佇列檢視。
- `:clearqueue`：清空手動加入佇列的歌曲（僅在此裝置上播放時可用）。
- `:vis`：切換音訊視覺化器。
- `:visbins <number>`：設定音訊視覺化器頻率條數量（5-32）。
- `:pixelate <pixels>`：在專輯封面上啟用復古 8 位元像素風格。設定為 0 可停用，或例如 16 以獲得像素化效果。
- `:backdrop <lights|mesh|aurora|vinyl|nebula>`：選擇桌面應用沉浸檢視背後的動態畫面（設定中也可選擇）。
- `:thumbs [on|off]`：切換資料庫側欄中的封面縮圖。封面快取於 `~/.config/echo/thumbs/`，之後啟動時即時載入。
- `:index <number>`：設定曲目索引基數（從 1 開始或從 0 開始）。
- `:quit`、`:q`、`:qa`、`:wq`：結束應用程式。

## 自訂快捷鍵

在 `~/.config/echo/config.toml` 的 `[library]` 下新增 `keybindings` 表，可以覆蓋或新增語義化映射。支援單鍵、修飾鍵組合（如 `ctrl-f`）以及兩鍵序列。未映射的按鍵保留 echo 的預設行為。

```toml
[library.keybindings]
"s d" = "sort_duration"
"s a" = "sort_artist"
"ctrl-j" = "half_page_down"
"ctrl-k" = "half_page_up"
";" = "seek_forward"
```

可用動作為 `first`、`last`、`page_up`、`page_down`、`half_page_up`、`half_page_down`、`current_context`、`play_pause`、`next`、`previous`、`shuffle`、`repeat`、`seek_backward`、`seek_forward`、`seek_start`、`mute`、`sort_original`、`sort_title`、`sort_artist`、`sort_album`、`sort_duration`、`sort_added`、`reverse_tracks`、`redraw` 和 `toggle_thumbnails`。

曲目排序與導覽僅作用於已載入的資料，不會向 Spotify 發送請求。導覽歷史最多保留 20 個記憶體中的檢視，因此返回之前的曲目清單通常無需重新取得。

## 主題

主題位於 `themes/*.toml`，採用扁平列表格式：九個基礎顏色之後是桌面應用程式使用的十二個衍生顏色，每一項都有明確的註解說明其用途。您可以自由修改數值，或修改基礎顏色後執行 `python themes/generate_desktop.py` 重新計算衍生顏色。衍生鍵是可選的——缺少的鍵會依其註解中命名的公式計算，同時也可接受 `[desktop]` 表用於覆蓋。若要進行視覺化迭代，可執行 `python tools/theme-preview/serve.py` 在瀏覽器中開啟桌面視窗的即時模擬，每次儲存都會重新著色——無需重新建置。顏色可以從任一方向編輯：在編輯器中修改 toml 檔案，或在預覽圖例中點擊任意顏色，用挑色器調整並直接寫回檔案（其中的「重新計算衍生色」按鈕會為目前主題重新執行產生器）。

## 音訊品質

echo 以 320 kbps 串流播放並套用音量標準化，與 Spotify 桌面應用程式的預設行為一致。這些選項位於 `~/.config/echo/config.toml` 的 `[library]` 下，並在下次啟動時生效。

```toml
[library]
bitrate = 320               # 96、160 或 320
normalisation = true        # 平衡曲目之間的響度，類似 Spotify 應用程式。
normalisation_pregain = 3.0 # 標準化後加回的分貝數。若播放過小聲請調高。
```

標準化會依每首曲目的 ReplayGain 值進行衰減，現代母帶通常有幾個分貝。`normalisation_pregain` 會把這部分餘裕加回來，使播放音量與 Spotify 應用程式相當。增益作用於 librespot 的動態限制器之前，因此調高不會產生削波。設定 `normalisation = false` 可完全略過增益環節，獲得位元精確的滿幅輸出，代價是曲目之間會出現響度跳變。

音量完全在使用者端套用——Spotify 串流以滿幅抵達，由 echo 自行衰減，因此在其他 Spotify 用戶端中裝置的音量滑桿不起作用。Spotify 和本地播放使用相同的三次方音量曲線，因此無論播放哪個來源，相同的百分比聽起來一樣，且兩者在 100% 時均為單位增益。

只要裝置支援，echo 就會以立體聲 44.1 kHz（librespot 的原始取樣率，因此無需重新取樣）開啟輸出裝置。不提供 44.1 kHz 的裝置（大多數 Windows 端點預設 48 kHz）會回退到裝置自身的預設取樣率。

實際開啟的端點會寫入工作目錄下的 `echo-debug-audio-spotify.log`，本地檔案則寫入 `echo-debug-audio-local.log`：

```
device=Headphones (WH-1000XM5) channels=2 sample_rate=48000 format=F32
```

## 本地音樂

本地音樂支援與 Spotify 分開。使用 `:localpath <absolute-folder-path>` 選擇 echo 應掃描的資料夾。支援的副檔名為 `mp3`、`wav`、`flac`、`ogg`、`m4a` 和 `aac`；echo 會遞迴掃描並讀取標題、藝術家、專輯、時長和封面圖（如有）。echo 在啟動時會重新整理已設定的本地資料夾，並在執行期間監視其中的音訊/封面圖變化；`:rescanlocal` 仍可作為手動回退方案使用。

本地播放清單儲存在本地，不是 Spotify 播放清單。它們可以包含本地曲目和 Spotify 曲目引用。Spotify 播放清單不能包含本地曲目。本地隨機播放、重複、音量、佇列和播放/暫停由 echo 的本地播放引擎處理。

嵌入式封面圖會被優先使用。如果曲目沒有嵌入式封面圖，echo 會查找資料夾中的封面圖，例如 `cover.jpg`、`folder.jpg` 或 `front.png`。

## 疑難排解

- **主題顏色渲染問題 (Windows)**：在「預設值」設定檔的「外觀」設定中，停用「調整難以分辨的文字」。
- **影像無法渲染**：封面使用半格儲存格繪製，只需要終端機支援全彩——所有現代終端機都已具備。
- **快取不同步**：如果您喜歡的歌曲與其他裝置不同步，只需重新啟動 echo。它會在啟動時在背景積極地同步您的庫。
- **本地檔案遺失**：如果檔案在掃描後被刪除或移動，執行 `:rescanlocal` 以重新整理本地庫。
- **音訊聽起來像單聲道或悶悶的（藍牙耳機）**：Windows 將藍牙耳機暴露為兩個輸出裝置——立體聲的「耳機」（A2DP）端點，以及限制在 16 kHz 的單聲道「免持」（HFP）端點。每當有應用程式開啟麥克風時，Windows 就會切換到免持模式。檢查 `echo-debug-audio-spotify.log`：如果報告 `channels=1`，請結束占用麥克風的程式，並將立體聲端點設為預設輸出裝置。
- **設定檔路徑**：`~/.config/echo/config.toml`（儲存權杖和偏好設定）、`~/.config/echo/cache.json`（儲存喜歡的曲目）、`~/.config/echo/local_library.json` 和 `~/.config/echo/local_playlists.json`。
