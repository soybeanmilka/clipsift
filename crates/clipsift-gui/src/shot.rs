//! 开发期用：让程序自己把当前窗口渲染成一张 PNG。
//!
//! 为什么不直接在外面用截图工具？
//! - `PrintWindow` 抓 OpenGL 表面会得到一张纯黑图；
//! - 屏幕抓取（`CopyFromScreen`）会被别的窗口挡住，还得猜窗口坐标。
//!
//! 所以走 eframe 自己的 `ViewportCommand::Screenshot`：拿到的是
//! 真正渲染出来的那一帧，尺寸、DPI、字体都跟用户看到的一致，改版式后
//! 可以直接对比前后。顺便也能拿来重新生成 README 里的界面图。
//!
//! 用法（只有 debug 构建里编译这段代码）：
//!
//! ```text
//! CLIPSIFT_SHOT=out.png CLIPSIFT_SHOT_SIZE=620x520 cargo run -p clipsift-gui
//! ```
//!
//! 不设 `CLIPSIFT_SHOT` 就完全不起作用；截完自动退出进程。

use eframe::egui;
use std::sync::atomic::{AtomicU32, Ordering};

const ENV_PATH: &str = "CLIPSIFT_SHOT";
const ENV_SIZE: &str = "CLIPSIFT_SHOT_SIZE";

/// 等这么多帧再截图：第一两帧里字体、自动换行、滚动条尺寸都还没稳定，
/// 截早了看到的不是最终版式。
const WARMUP_FRAMES: u32 = 12;

static FRAMES: AtomicU32 = AtomicU32::new(0);

/// 每帧开头调用一次。
pub fn tick(ctx: &egui::Context) {
    let Some(path) = std::env::var_os(ENV_PATH).map(std::path::PathBuf::from) else {
        return;
    };
    let frame = FRAMES.fetch_add(1, Ordering::Relaxed);

    if frame == 0 {
        // 先按请求的尺寸改窗口，这样截图就是指定尺寸下的版式。
        if let Some((w, h)) = parse_size() {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
        }
    }

    if frame < WARMUP_FRAMES {
        ctx.request_repaint();
        return;
    }

    if frame == WARMUP_FRAMES {
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        ctx.request_repaint();
        return;
    }

    // 截图的回包在下一帧的事件里，可能还要再等一两帧。
    let image = ctx.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Screenshot { image, .. } => Some(image.clone()),
            _ => None,
        })
    });

    let Some(image) = image else {
        ctx.request_repaint();
        return;
    };

    match save(&path, &image) {
        Ok(()) => println!(
            "截图已保存：{}（{}x{}）",
            path.display(),
            image.size[0],
            image.size[1]
        ),
        Err(e) => eprintln!("截图保存失败：{e}"),
    }
    std::process::exit(0);
}

fn parse_size() -> Option<(f32, f32)> {
    let raw = std::env::var(ENV_SIZE).ok()?;
    let (w, h) = raw.split_once(['x', 'X'])?;
    let w: f32 = w.trim().parse().ok()?;
    let h: f32 = h.trim().parse().ok()?;
    Some((w, h))
}

fn save(path: &std::path::Path, image: &egui::ColorImage) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(file),
        image.size[0] as u32,
        image.size[1] as u32,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;

    let mut rgba = Vec::with_capacity(image.size[0] * image.size[1] * 4);
    for px in &image.pixels {
        rgba.extend_from_slice(&px.to_array());
    }
    writer.write_image_data(&rgba).map_err(|e| e.to_string())?;
    Ok(())
}
