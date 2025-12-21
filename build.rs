fn main() {
    // Slint 的編譯設定
    slint_build::compile("ui/appwindow.slint").unwrap();

    // [新增] Windows 圖示設定
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("img/icon.ico"); // 確保這裡檔名對應您的 ico 檔
        res.compile().unwrap();
    }
}