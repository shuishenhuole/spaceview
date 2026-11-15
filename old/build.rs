//! Windows 资源编译:把 src/assist/icon.ico 嵌入 exe 文件图标。

fn main() {
    println!("cargo::rerun-if-changed=src/assist/icon.ico");
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("src/assist/icon.ico");
        if let Err(e) = res.compile() {
            println!("cargo::warning=嵌入 exe 图标失败: {e}");
        }
    }
}
