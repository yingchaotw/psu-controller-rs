fn main() {
    // 這行指令會告訴 Cargo：去編譯 ui/appwindow.slint 這個檔案
    slint_build::compile("ui/appwindow.slint").unwrap();
}