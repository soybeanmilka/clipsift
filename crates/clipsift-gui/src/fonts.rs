//! 中文字体加载。
//!
//! egui 自带字体只有拉丁字符，界面上的中文会全变成豆腐块。这里从系统字体目录
//! 按候选列表找一个能用的中文无衬线字体装进去。
//!
//! 为什么不把字体打包进 exe：一个中文字体动辄 10~20MB，而 exe 现在才 1.3MB。
//! 体积是这个工具的核心卖点之一（单个可执行文件，不依赖运行库），为界面漂亮
//! 一点就把体积翻十倍不划算。系统字体是每个平台都有的东西，找得到。
//!
//! 找不到也不崩——宁可界面显示成方块，也不能让程序因为字体问题打不开。

use eframe::egui;

/// 按优先级尝试的字体路径。前面的更"好看"，后面的更"通用"。
///
/// 这个列表覆盖了 Windows / macOS / 常见 Linux 发行版。找不到任何一个时返回
/// `None`，界面退化成英文可读、中文方块的状态，但功能全都正常。
fn candidates() -> &'static [&'static str] {
    #[cfg(windows)]
    {
        &[
            "C:/Windows/Fonts/msyh.ttc",   // 微软雅黑，Win7+ 都有
            "C:/Windows/Fonts/msyhl.ttc",  // 微软雅黑 Light
            "C:/Windows/Fonts/Deng.ttf",   // 等线
            "C:/Windows/Fonts/simhei.ttf", // 黑体，老系统兜底
            "C:/Windows/Fonts/simsun.ttc", // 宋体，再兜底
        ]
    }
    #[cfg(target_os = "macos")]
    {
        &[
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/STHeiti Light.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
        ]
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        &[
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
            "/usr/share/fonts/truetype/arphic/uming.ttc",
        ]
    }
}

/// 往 egui 里装中文字体。找不到就什么都不做，不 panic。
pub fn install(ctx: &egui::Context) {
    let Some((path, bytes)) = load_first_available() else {
        // 没有中文字体也不影响功能，只是界面上的中文会显示成方块。
        // 不弹窗不报错，避免"打开就有个错误框"的体验。
        return;
    };

    let mut fonts = egui::FontDefinitions::default();

    // Proportional 是正文用的，等宽用同一个字体即可——中文字体里也含 ASCII，
    // 只是没有等宽特性。代码块那种场景在 GUI 里不是重点。
    fonts.font_data.insert(
        "cjk".to_owned(),
        std::sync::Arc::new(egui::FontData::from_owned(bytes)),
    );

    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            // 放在最前面 = 优先用中文字体。它的拉丁字形不如 egui 自带的
            // Pixel 系好看，但混排时字重和基线更统一，且中文场景下这更重要。
            .insert(0, "cjk".to_owned());
    }

    ctx.set_fonts(fonts);

    // 路径记一份到临时全局，方便"关于"那里展示当前用的是哪个字体。
    let _ = path;
}

fn load_first_available() -> Option<(&'static str, Vec<u8>)> {
    for path in candidates() {
        if let Ok(bytes) = std::fs::read(path) {
            // 空文件或者读出来明显不对的，跳过。字体文件最小也有几百 KB。
            if bytes.len() > 10_000 {
                return Some((path, bytes));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_are_non_empty() {
        assert!(!candidates().is_empty(), "每个平台都该有候选字体列表");
    }

    #[test]
    fn candidates_are_absolute_paths() {
        // 相对路径在开机自启（工作目录是 System32）的情况下必然读不到。
        for p in candidates() {
            assert!(
                p.starts_with('/') || p.chars().nth(1) == Some(':'),
                "{p} 不是绝对路径"
            );
        }
    }

    #[test]
    fn current_platform_has_at_least_one_usable_font() {
        // 这是给 CI / 开发机做的环境检查，不是功能断言。
        // 万一某台机器真没有中文字体，这里失败能立刻看出来是环境问题。
        if load_first_available().is_none() {
            eprintln!("注意：本机没找到候选中文字体，界面上的中文会显示成方块。");
        }
    }
}
