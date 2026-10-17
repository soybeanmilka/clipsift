//! ClipSift 图形界面。
//!
//! 这个 crate 存在的理由很简单：命令行已经能做所有事了，但每次改规则都要
//! 查 `--list-rules` 找键名再拼参数，状态也没有任何地方能一眼看见。GUI 只做
//! 三件事——**看见状态、点一下改规则、当场试一遍效果**。

// 双击 exe 不要弹黑框。Windows 上不带这个属性的程序会先开一个控制台窗口，
// 对普通用户来说是"这软件有问题"的第一印象。debug 构建保留控制台，
// 因为调试的时候还得靠 println 看输出。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//!
//! 界面是 egui（立即模式）。立即模式的好处是没有任何状态同步问题：每帧都把
//! 整个界面重画一遍，界面就是数据的投影，不需要手动维护控件和数据的一致性。

mod fonts;
mod reg;
#[cfg(debug_assertions)]
mod shot;
mod state;
mod theme;
mod ui;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let icon = load_icon();
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([760.0, 720.0])
        .with_min_inner_size([620.0, 520.0])
        .with_title("ClipSift — 剪贴板净化");
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "ClipSift",
        options,
        Box::new(|cc| {
            // 顺序有讲究：字体先装（里面顺带把 CJK 排进 monospace family），再套主题。
            fonts::install(&cc.egui_ctx);
            theme::apply(&cc.egui_ctx, ui::App::DEFAULT_MODE);
            Ok(Box::new(ui::App::new()))
        }),
    )
}

/// 窗口/任务栏图标。编译时内嵌进 exe，所以换机器也带着图标。
fn load_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../../../assets/clipsift.ico");
    let (w, h, png) = ico_largest_png(bytes)?;
    // ICO 里存的是 PNG，得先解成像素。egui 的 image loader 能解 PNG，
    // 但那是给界面上显示图片用的；这里要的是原始 RGBA。
    let image = decode_png_rgba(png)?;
    Some(egui::IconData {
        rgba: image,
        width: w,
        height: h,
    })
}

/// 最小的 PNG 解码：只处理我们自己生成的这张（8 位 RGBA、无隔行）。
fn decode_png_rgba(png: &[u8]) -> Option<Vec<u8>> {
    use std::io::Cursor;
    let decoder = png::Decoder::new(Cursor::new(png));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    match info.color_type {
        png::ColorType::Rgba => Some(buf),
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity(buf.len() / 3 * 4);
            for px in buf.chunks_exact(3) {
                out.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
            Some(out)
        }
        _ => None,
    }
}

/// 从 ICO 里挑出分辨率最大的一张 PNG。没找到就返回 None（界面照样能用，
/// 只是任务栏显示默认图标）。
fn ico_largest_png(ico: &[u8]) -> Option<(u32, u32, &[u8])> {
    if ico.len() < 6 || ico[0] != 0 || ico[1] != 0 || ico[2] != 1 || ico[3] != 0 {
        return None;
    }
    let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
    let mut best: Option<(u32, u32, usize, usize)> = None;
    for i in 0..count {
        let base = 6 + i * 16;
        if base + 16 > ico.len() {
            return None;
        }
        let entry = &ico[base..base + 16];
        // 宽高字段是 u8，0 表示 256。
        let w = if entry[0] == 0 { 256 } else { entry[0] as u32 };
        let h = if entry[1] == 0 { 256 } else { entry[1] as u32 };
        let size = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as usize;
        let offset = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as usize;
        if offset + size > ico.len() {
            return None;
        }
        if best.is_none_or(|(bw, _, _, _)| w > bw) {
            best = Some((w, h, offset, size));
        }
    }
    let (w, h, off, size) = best?;
    Some((w, h, &ico[off..off + size]))
}
