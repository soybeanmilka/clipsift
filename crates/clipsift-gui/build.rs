//! 构建脚本：把 assets/clipsift.ico 嵌进 exe 的资源段。
//!
//! 不做这一步的话，exe 在资源管理器里显示的是 Windows 默认的白色窗口图标，
//! 拖到桌面上一眼看不出是哪个程序。窗口内部的 `with_icon` 只影响任务栏和
//! 标题栏，文件本身的图标必须走 PE 资源。

fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/clipsift.ico");
        res.set("ProductName", "ClipSift");
        res.set("FileDescription", "ClipSift — 剪贴板净化");
        res.set("LegalCopyright", "MIT License");
        // 失败也不该挡住编译：图标只是锦上添花，没有它程序照样跑。
        let _ = res.compile();
    }

    println!("cargo:rerun-if-changed=../../assets/clipsift.ico");
}
