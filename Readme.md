# psu-controller-rs ⚡️

[](https://www.rust-lang.org/)
[](https://slint.dev/)

![alt text](img/image.png)

一個基於 **Rust** 與 **Slint** 開發的跨平台電源供應器控制軟體。透過 **SCPI (Standard Commands for Programmable Instruments)** 指令集與 Serial Port (USB/RS-232) 通訊，為硬體工程師提供輕量、高效且美觀的儀器控制介面。

## ✨ 特色功能 (Features)

  * **🚀 極速啟動 & 低資源佔用**：使用 Rust 編寫，原生編譯，無需安裝龐大的 Runtime (如 Python 或 JVM)。
  * **🎨 現代化暗黑介面 (Dark Mode)**：針對長時間工作的工程師設計，高對比配色 (High Contrast)，數據讀取清晰不刺眼。
  * **🔌 自動偵測連接埠**：啟動時自動掃描系統可用的 COM Port / TTY 裝置。
  * **🎛 完整控制功能**：
      * **電壓/電流設定**：支援 `VOLT` 與 `CURR` 指令。
      * **即時讀取**：一鍵回讀實際電流值 (`MEAS:CURR?`)。
      * **安全防護**：支援輸出開關 (`OUTP ON/OFF`) 與面板鎖定解除 (`SYST:LOC`)。
  * **🖥 響應式佈局**：基於 Slint 的儀表板設計，視窗大小調整時內容自動適配。

## 🛠 技術棧 (Tech Stack)

  * **程式語言**: [Rust](https://www.rust-lang.org/)
  * **GUI 框架**: [Slint](https://slint.dev/) (輕量級、適合嵌入式與桌面應用)
  * **序列通訊**: `serialport` crate
  * **錯誤處理**: `anyhow`

## 📦 安裝與執行 (Installation)

### 前置需求

請確保您的電腦已安裝 [Rust Toolchain](https://rustup.rs/)。

### 建置專案

1.  複製專案庫：

    ```bash
    git clone https://github.com/yingchaotw/psu-controller-rs.git
    cd psu-controller-rs
    ```

2.  執行程式：

    ```bash
    cargo run --release
    ```

> **注意**：Linux/macOS 使用者若遇到 Permission Denied 錯誤，請確保當前用戶有權限存取 USB 裝置 (例如將用戶加入 `dialout` 群組，或暫時使用 `sudo` 執行)。

## 📖 使用指南 (Usage)

1.  **連接硬體**：使用 USB 線材連接支援 SCPI 的電源供應器。
2.  **選擇 Port**：在軟體左上角的下拉選單選擇對應的 COM Port。
3.  **建立連線**：點擊 **Connect**。若連線成功，狀態燈號將轉為綠色。
4.  **設定參數**：
      * 在 **Voltage Control** 輸入目標電壓 (如 `12.0`) 並點擊 **SET Voltage**。
      * 在 **Current Monitor** 輸入限流值 (如 `1.5`) 並點擊 **SET OCP**。
5.  **開啟輸出**：點擊底部大顆的 **OUTPUT ON** 按鈕供電。
6.  **讀取數值**：點擊 **READ** 按鈕，右側將顯示電源回傳的實際電流值。
7.  **斷線**：點擊 **Disconnect**，軟體會自動送出 `SYST:LOC` 解鎖機器面板並斷開連線。

## 🔌 硬體相容性 (Hardware Compatibility)

本軟體支援大多遵循 SCPI 標準 (IEEE 488.2) 的可程式化直流電源供應器，包括但不限於：

  * **Keysight / Agilent** (E36xx 系列等)
  * **Rigol** (DP800 系列等)
  * **Siglent** (SPD 系列)
  * **GW Instek** (固緯)
  * **Keithley**

*需確認您的裝置支援透過 Serial Port (Virtual COM) 傳輸 SCPI 指令。*

## 📂 專案結構

```text
psu-controller-rs/
├── Cargo.toml              # 相依套件設定
├── build.rs                # Slint 編譯腳本
├── src/
│   └── main.rs             # Rust 主程式邏輯 (Serial 通訊處理)
└── ui/
    └── appwindow.slint     # UI 佈局與樣式定義
```

## 📝 授權 (License)

此專案採用 MIT License 授權 - 詳見 [LICENSE](https://www.google.com/search?q=LICENSE) 文件。

