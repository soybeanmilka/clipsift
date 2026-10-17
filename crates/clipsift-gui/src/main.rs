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
mod state;
mod theme;
mod ui;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([760.0, 720.0])
            .with_min_inner_size([620.0, 520.0])
            .with_title("ClipSift — 剪贴板净化"),
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
