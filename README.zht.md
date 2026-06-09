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

echo 是一款用 Rust 編寫的終端音樂播放器和 Spotify 用戶端。echo 將您的本地檔案以及整個 Spotify 庫、喜歡的歌曲、播放清單和播放控制直接帶到終端中，並配備美觀、動態的 TUI，支援原生影像渲染。

![demo](demo.png)

## 功能特性

- **終端影像支援**：直接在終端中呈現高品質專輯封面和播放清單封面（支援 Kitty、Sixel 以及區塊式回退方案）。
- **極速喜歡的歌曲**：採用全域快取架構。您整個喜歡的歌曲庫會快取在本地（`~/.config/echo/cache.json`），實現零延遲、無速率限制的捲動瀏覽，即使有數千首儲存的曲目也毫無壓力。
- **庫管理**：建立、重新命名、刪除播放清單並將其組織到資料夾中。
- **本地音樂支援**：掃描本地音樂資料夾，播放本地檔案，建立也可引用 Spotify 曲目的本地播放清單。
- **回應式播放控制**：全面控制播放、佇列、隨機播放、重複播放和音量。
- **搜尋**：快速全域搜尋 Spotify 目錄和已掃描的本地曲目。

## 設定

1. **Spotify Premium**：需要使用 Spotify Premium 帳戶才能透過 Spotify Web API 進行播放控制。
2. **Spotify 開發者應用**：
   - 前往 [Spotify 開發者儀表板](https://developer.spotify.com/dashboard/)。
   - 建立一個應用並取得您的 `Client ID` 和 `Client Secret`。
   - 將 `http://127.0.0.1:8888/callback` 新增到應用的 Redirect URIs 中。
   - Echo 還使用 `http://127.0.0.1:8989/login` 進行內部第一方 Spotify 工作階段。

### 安裝

下載並執行安裝程式：
https://github.com/and2049/echo/releases

或者

複製倉庫並使用 Cargo 建置：

```bash
git clone https://github.com/and2049/echo.git
cd echo
cargo build --release
```

執行二進位檔：

```bash
./target/release/echo
```

首次執行時，echo 會提示您輸入 `Client ID` 和 `Client Secret`，然後開啟瀏覽器以透過 Spotify 進行身分驗證。

## 導航與快捷鍵

echo 主要由鍵盤驅動。

### 全域導航
- `j` / `k` 或 `Down` / `Up`：向下 / 向上移動
- `Enter` 或 `z`：選擇項目 / 開啟播放清單 / 播放曲目
- `h` / `q` / `Esc` / `Backspace`：返回 / 關閉模態框 / 清除搜尋
- `Tab`：切換標籤頁（例如，播放清單 ↔ 專輯，搜尋曲目 ↔ 搜尋專輯）
- `:`：進入命令模式
- `/`：在曲目清單中搜尋
- `f`：全域搜尋
- `n` / `N`：跳轉到清單中的下一個 / 上一個搜尋結果

### 播放控制
- `Space`：播放 / 暫停
- `]` / `>`：下一曲
- `[` / `<`：上一曲
- `s`：切換隨機播放
- `r`：切換重複模式（關閉 → 單曲循環 → 列表循環）
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
- `q`：將目前懸停的曲目新增到佇列
- `Q`（Shift + q）：開啟佇列檢視
- `m`：固定 / 取消固定播放清單
- `c`：快速建立新播放清單
- `e`：快速重新命名播放清單或資料夾
- `v`：進入視覺模式以進行多選
- `d`（雙擊）：刪除播放清單/資料夾，或從自訂播放清單中移除曲目
- `x`：剪下播放清單（以便移動到資料夾中）
- `R`（Shift + r）：強制重新整理

## 命令

在命令模式（`:`）下，您可以使用以下命令：
- `:search <query>`：搜尋曲目或專輯。
- `:newplaylist <name>`：建立新播放清單。
- `:newlocalplaylist <name>`：建立儲存在本機的本地播放清單。
- `:localpath <absolute-folder-path>`：設定本地音樂資料夾並掃描。路徑必須為絕對路徑，支援 macOS、Windows 和 Linux。
- `:rescanlocal`：重新掃描已設定的本地音樂資料夾。
- `:newfolder <name>`：建立新資料夾以組織播放清單。
- `:delfolder`：刪除目前選中的資料夾。
- `:rename <name>`：重新命名目前選中的播放清單或資料夾。
- `:sort <alpha|creator>`：對庫進行排序。
- `:theme <theme_name>`：切換應用主題。
- `:lang <en|zh|zh-CN>`：切換語言。
- `:album`：跳轉到目前選中曲目所屬的專輯。
- `:queue`：開啟佇列檢視。
- `:vis`：切換音訊視覺化器。
- `:visbins <number>`：設定音訊視覺化器頻率條數量（5-32）。
- `:pixelate <pixels>`：在專輯封面上啟用復古 8 位元像素風格。設定為 0 可停用，或例如 16 以獲得像素化效果。
- `:index <number>`：設定曲目索引基數（從 1 開始或從 0 開始）。
- `:quit`、`:q`、`:qa`、`:wq`：退出應用。

## 本地音樂

本地音樂支援與 Spotify 分開。使用 `:localpath <absolute-folder-path>` 選擇 echo 應掃描的資料夾。支援的副檔名為 `mp3`、`wav`、`flac`、`ogg`、`m4a` 和 `aac`；echo 會遞迴掃描並讀取標題、藝術家、專輯、時長和封面圖（如有）。Echo 在啟動時會重新整理已設定的本地資料夾，並在執行期間監視其中的音訊/封面圖變化；`:rescanlocal` 仍可作為手動回退方案使用。

本地播放清單儲存在本地，不是 Spotify 播放清單。它們可以包含本地曲目和 Spotify 曲目引用。Spotify 播放清單不能包含本地曲目。本地隨機播放、重複、音量、佇列和播放/暫停由 echo 的本地播放引擎處理。

嵌入式封面圖會被優先使用。如果曲目沒有嵌入式封面圖，echo 會查詢資料夾中的封面圖，例如 `cover.jpg`、`folder.jpg` 或 `front.png`。

## 疑難排解

- **影像無法渲染**：確保您的終端支援 Kitty 影像協定或 Sixel 圖形（例如 Kitty、WezTerm、打了修補程式的 Alacritty）。如果都不支援，echo 將回退到區塊式渲染。
- **快取不同步**：如果您喜歡的歌曲與其他裝置不同步，只需重新啟動 echo。它會在啟動時在背景急切地同步您的庫。
- **本地檔案遺失**：如果檔案在掃描後被刪除或移動，執行 `:rescanlocal` 以重新整理本地庫。
- **設定檔路徑**：`~/.config/echo/config.toml`（儲存權杖和偏好設定）、`~/.config/echo/cache.json`（儲存喜歡的曲目）、`~/.config/echo/local_library.json` 和 `~/.config/echo/local_playlists.json`。
