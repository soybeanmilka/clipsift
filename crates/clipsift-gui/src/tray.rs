//! 系统托盘图标（右下角「状态栏通知区」那个小图标）。
//!
//! 为什么要有它：这个程序的正常形态是"窗口关掉、还在后台守着剪贴板"。
//! 只收进任务栏的话，用户得先找到任务栏上那个被一堆窗口盖住的按钮；
//! 有了托盘图标，窗口可以彻底藏起来，右下角常驻一个图标，左键点一下就回来。
//!
//! 几个坑记在这里，免得下次又踩：
//!
//! - **必须在窗口线程（主线程）上创建**：托盘要绑定一个窗口来收消息，
//!   从别的线程创建会拿到一个不属于当前消息循环的窗口，菜单点了没反应。
//! - 托盘图标自己的消息靠 Windows 消息循环分发。egui 平时没有用户操作就不跑帧，
//!   所以后台还有一个心跳线程按 `TRAY_POLL_INTERVAL` 的节奏喊一声 `request_repaint`，
//!   主线程的帧循环才会醒过来把托盘事件取走，见 `super::spawn_heartbeat`。
//!   **不能用 `request_repaint_after` 代替**：窗口被 `Visible(false)` 藏起来之后，
//!   eframe 会直接跳过帧（Windows 上非 0 尺寸的窗口才给发重绘），
//!   `request_repaint_after` 排的那一帧永远等不到，托盘点了也没反应。
//!
//! - Win11 默认把新注册的托盘图标塞进任务栏那个「⌃」溢出面板里。面板展开后会
//!   自己吃掉图标上的一部分鼠标消息（实测单击拿不到、右键能拿到），所以「唤回窗口」
//!   单击和双击都认——别指望单击一定送达。真要确保看得见，用户得在
//!   「任务栏设置 → 其他系统托盘图标」里把它设成常显。
//! - 托盘图标只有 16~32 像素，直接塞 256 的窗口图标进去会糊成一团。所以这里
//!   重画了一个小尺寸版本：强调色圆角方块 + 一把斜扫帚 + 三颗被扫走的白点。
//! - 菜单项被点之后事件里只带 id，菜单对象本身必须一直活着，
//!   否则菜单会跟着消失。`Tray` 结构体把这些对象都存住了。

use std::time::Duration;

/// 托盘上能发生的事。由事件回调写进队列，界面每帧取走。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    /// 把窗口叫回来（左键单击 / 菜单「显示窗口」）。
    Show,
    /// 重新开一轮守护（菜单「开始守护」）。
    StartWatch,
    /// 停掉守护（菜单「停止守护」）。
    StopWatch,
    /// 立刻净化一次剪贴板（菜单「净化剪贴板一次」）。
    CleanOnce,
    /// 真的退出程序（菜单「退出 ClipSift」）。
    Quit,
}

/// 应用名，出现在悬停提示里。
pub const APP_NAME: &str = "ClipSift";

/// 多久醒一次去收托盘事件。
///
/// 不是"托盘刷新频率"，只是让 egui 的帧循环别彻底睡着：托盘点击和菜单选择
/// 都会往窗口消息队列里塞消息，而 egui 只有在新的一帧里才会把消息读出来。
/// 120ms 对一次性点击来说感知不到延迟，又不会烧 CPU（一帧基本是空转）。
pub const TRAY_POLL_INTERVAL: Duration = Duration::from_millis(120);

/// 悬停提示的文字。
pub fn tooltip_text(running: bool, cleaned: usize) -> String {
    let state = if running { "守护中" } else { "已停止" };
    format!("{APP_NAME} · {state} · 已净化 {cleaned} 次")
}

#[cfg(windows)]
mod imp {
    use super::{tooltip_text, TrayAction};
    use eframe::egui;
    use tray_icon::{
        menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem},
        Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    };

    pub struct Tray {
        icon: TrayIcon,
        /// 菜单和菜单项都要活着：菜单项被点后事件里只带 id，
        /// 对象被 drop 掉的话菜单也从通知区域消失了。
        _menu: Menu,
        /// 每个菜单项对应什么动作。点菜单时事件里只带 id，
        /// 这里存一份 id → 动作的对照表，将来加菜单项只改这一处。
        bindings: Vec<(MenuId, TrayAction)>,
        /// 「开始守护 / 停止守护」要按运行状态灰掉一个。单独存起来是因为
        /// 它们是原生菜单项，改 enabled 要走系统调用，不能每帧都改。
        start_item: MenuItem,
        stop_item: MenuItem,
        /// 上一次设过的悬停提示，一样就不重复 set——每帧都碰系统托盘会闪。
        last_tip: std::cell::Cell<Option<u64>>,
    }

    impl Tray {
        /// 建托盘。失败（比如资源管理器没在跑、通知区域被禁用）时返回 None，
        /// 界面照常用，只是点 × 之后只能从任务栏找回来。
        pub fn new() -> Option<Self> {
            let show_item = MenuItem::new("显示窗口", true, None);
            let start_item = MenuItem::new("开始守护", true, None);
            // 打开程序时守护默认就是开着的，所以「开始守护」一开始是灰的。
            let stop_item = MenuItem::new("停止守护", false, None);
            let clean_item = MenuItem::new("净化剪贴板一次", true, None);
            let quit_item = MenuItem::new("退出 ClipSift", true, None);

            let menu = Menu::new();
            menu.append_items(&[
                &show_item,
                &PredefinedMenuItem::separator(),
                &start_item,
                &stop_item,
                &clean_item,
                &PredefinedMenuItem::separator(),
                &quit_item,
            ])
            .ok()?;

            let bindings = vec![
                (show_item.id().clone(), TrayAction::Show),
                (start_item.id().clone(), TrayAction::StartWatch),
                (stop_item.id().clone(), TrayAction::StopWatch),
                (clean_item.id().clone(), TrayAction::CleanOnce),
                (quit_item.id().clone(), TrayAction::Quit),
            ];

            let icon = TrayIconBuilder::new()
                .with_tooltip(tooltip_text(true, 0))
                .with_icon(make_icon())
                .with_menu(Box::new(menu.clone()))
                // 左键唤回窗口、右键出菜单——Windows 通知区域的习惯。
                .with_menu_on_left_click(false)
                .build()
                .ok()?;

            Some(Self {
                icon,
                _menu: menu,
                bindings,
                start_item,
                stop_item,
                last_tip: std::cell::Cell::new(None),
            })
        }

        /// 把菜单项状态和悬停提示跟界面状态同步。返回是否真的做了同步。
        pub fn sync(&self, running: bool, cleaned: usize) -> bool {
            self.start_item.set_enabled(!running);
            self.stop_item.set_enabled(running);

            let tip = tooltip_text(running, cleaned);
            let hash = fxhash(&tip);
            if self.last_tip.get() == Some(hash) {
                return false;
            }
            self.last_tip.set(Some(hash));
            let _ = self.icon.set_tooltip(Some(tip));
            true
        }

        /// 每帧调用：把托盘点击和菜单事件取出来，翻译成 `TrayAction`。
        ///
        /// `debug` 为真时把原始事件写进 `CLIPSIFT_TRAY_LOG` 指向的文件。
        /// 为什么不用 `eprintln`：release 版是 `windows_subsystem = "windows"`，
        /// 根本没有 stderr；debug 版从 PowerShell 里拉起来时，stderr 也常被
        /// 当成垃圾句柄丢掉，日志一条都出不来（踩过）。写文件才是稳的。
        pub fn poll(&self, debug: bool) -> Vec<TrayAction> {
            let mut out = Vec::new();

            let mut clicked = false;

            while let Ok(event) = TrayIconEvent::receiver().try_recv() {
                if debug {
                    crate::debug_log(&format!("[tray] 图标事件 {event:?}"));
                }
                // 单击和双击都认。只认单击的话，Windows 上双击的第二次点击
                // 会变成 WM_LBUTTONDBLCLK，白白丢掉一次；只认双击的话，
                // Win11 的溢出面板会把单击吃掉，用户点了没反应。
                match event {
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    }
                    | TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    } => clicked = true,
                    _ => {}
                }
            }

            if clicked {
                out.push(TrayAction::Show);
            }

            while let Ok(event) = MenuEvent::receiver().try_recv() {
                if debug {
                    crate::debug_log(&format!("[tray] 菜单事件 id={}", event.id.0));
                }
                if let Some((_, action)) = self.bindings.iter().find(|(id, _)| id == &event.id) {
                    out.push(*action);
                }
            }

            if debug && !out.is_empty() {
                crate::debug_log(&format!("[tray] 翻译成 {out:?}"));
            }

            out
        }
    }

    /// 画一个 32x32 的托盘图标。
    ///
    /// 设计：强调色的圆角方块上斜放一把扫帚柄 + 右下角扫把 + 左上三颗白点，
    /// 意思是"把看不见的字符扫出去"。和主界面同色系，缩到 16px 还认得出轮廓。
    pub fn make_icon() -> Icon {
        const S: usize = 32;
        let accent = egui::Color32::from_rgb(96, 205, 255);
        let ink = egui::Color32::from_rgb(16, 32, 48);
        let mut rgba = vec![0u8; S * S * 4];

        let put = |rgba: &mut Vec<u8>, x: i32, y: i32, c: egui::Color32| {
            if (0..S as i32).contains(&x) && (0..S as i32).contains(&y) {
                let i = (y as usize * S + x as usize) * 4;
                rgba[i] = c.r();
                rgba[i + 1] = c.g();
                rgba[i + 2] = c.b();
                rgba[i + 3] = c.a();
            }
        };

        // 圆角方块底：半径 7 的圆角，四个角留透明。
        let r = 7i32;
        for y in 0..S as i32 {
            for x in 0..S as i32 {
                let cx = x.clamp(r, S as i32 - 1 - r);
                let cy = y.clamp(r, S as i32 - 1 - r);
                let (dx, dy) = (x - cx, y - cy);
                if dx * dx + dy * dy <= r * r {
                    put(&mut rgba, x, y, accent);
                }
            }
        }

        // 扫帚柄：从 (9,8) 往右下的一条粗斜线。
        for t in 0..22i32 {
            let x = 9 + t * 6 / 10;
            let y = 8 + t / 2;
            for (ox, oy) in [(0, 0), (1, 0), (0, 1), (1, 1), (2, 1)] {
                put(&mut rgba, x + ox, y + oy, ink);
            }
        }
        // 扫把：右下角一团（越往下越宽），用粗块近似。
        for y in 19..26i32 {
            let half = 2 + (y - 19) / 2;
            for x in (20 - half)..=(19 + half) {
                put(&mut rgba, x, y, ink);
            }
        }

        // 被扫走的"隐形字符"：左上三颗白点。
        for (x, y) in [(7, 5), (12, 4), (5, 11)] {
            put(&mut rgba, x, y, egui::Color32::WHITE);
        }

        Icon::from_rgba(rgba, S as u32, S as u32).expect("32x32 RGBA 图标总是合法的")
    }

    /// 一个够用的 64 位字符串哈希，只为判断悬停提示变没变。
    fn fxhash(s: &str) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in s.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    }
}

#[cfg(not(windows))]
mod imp {
    use super::TrayAction;

    /// 非 Windows 平台不发托盘：Linux 上要 GTK + libappindicator 一整套系统依赖，
    /// 为一个小工具拖这些不划算；macOS 的托盘在菜单栏顶部，语义也不同。
    /// 这些平台点 × 仍然收进任务栏继续守护，行为不变。
    pub struct Tray;

    impl Tray {
        pub fn new() -> Option<Self> {
            None
        }

        pub fn sync(&self, _running: bool, _cleaned: usize) -> bool {
            false
        }

        pub fn poll(&self, _debug: bool) -> Vec<TrayAction> {
            Vec::new()
        }
    }
}

pub use imp::Tray;

/// 调试日志开关：设了 `CLIPSIFT_TRAY_LOG` 才写，而且只在 debug 构建里有代码。
///
/// 只在排查托盘问题的时候用，平时零成本：release 里整个函数体都被
/// `cfg!` 折成一个空操作。
pub fn debug_enabled() -> bool {
    cfg!(debug_assertions) && std::env::var_os("CLIPSIFT_TRAY_LOG").is_some()
}

/// 起一个心跳线程，按 `TRAY_POLL_INTERVAL` 的节奏催 egui 跑一帧。
///
/// 为什么不用 `ctx.request_repaint_after`：那只是"排一个未来的重绘"，
/// 而窗口被 `Visible(false)` 藏起来之后 eframe 会直接跳过这一帧（Windows 上
/// 只有非 0 尺寸的窗口才给发重绘），排进去的重绘永远不执行，托盘事件也就永远
/// 没人收——表现就是"点 × 之后点托盘图标没反应"。从另一个线程调
/// `request_repaint` 走的是 `PostMessage` 那条路，不依赖窗口可见，藏起来的窗口
/// 照样能醒过来。
///
/// 线程一直活到进程退出，和程序同寿命，所以不需要 join。
pub fn spawn_heartbeat(ctx: eframe::egui::Context) {
    // 已经起过就别再起，重复的线程只会白烧 CPU。
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if STARTED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    std::thread::Builder::new()
        .name("clipsift-tray-pulse".to_string())
        .spawn(move || loop {
            std::thread::sleep(TRAY_POLL_INTERVAL);
            ctx.request_repaint();
        })
        .ok();
    crate::debug_log("[tray] 心跳线程已起");
}
