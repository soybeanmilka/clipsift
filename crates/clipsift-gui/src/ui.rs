//! 界面布局。
//!
//! 分成四块，从上到下按"最要紧的信息"排：
//! 1. 顶部条——标题、主题切换
//! 2. 状态卡——守护在不在跑、统计数字、开机自启
//! 3. 规则卡——点一下就改，改完立刻生效
//! 4. 试衣间卡——贴一段文本，当场看净化前后和删了什么
//!
//! 视觉上每个功能块都套一层 `theme::card_frame`，卡片之间留白，
//! 这样即使窗口拉大也不会糊成一整片。

use crate::state::{self, Event, Watcher};
use crate::theme::{self, Mode, Palette};
use crate::tray::{self, Tray, TrayAction};
use clipsift_core::{apply, inspect, Plan};
use eframe::egui;
use std::time::{Duration, Instant};

pub struct App {
    plan: Plan,
    watcher: Option<Watcher>,
    mode: Mode,
    /// `apply` 改的是 Style，当前帧已经在画了，所以切主题要延到下一帧开头。
    pending_theme: bool,
    /// 窗口被藏到后台了（点 × 而不是退出）。
    hidden: bool,
    /// 用户明确要退出。只有它为 true 时才让 close 真的走。
    quitting: bool,
    /// 右下角通知区的图标。建不出来就是 None，界面照常。
    tray: Option<Tray>,
    /// 托盘心跳线程起过没有（只起一次，见 `tray::spawn_heartbeat`）。
    heartbeat_on: bool,

    // 统计
    cleaned_total: usize,
    chars_removed_total: usize,
    last_clean_at: Option<Instant>,
    last_error: Option<String>,

    // 试衣间
    sample: String,
    sample_result: Option<SampleResult>,

    // 开机自启（仅 Windows 用得上，其他平台显示为不可用）
    autostart_state: AutostartState,
}

#[derive(Debug, Clone, PartialEq)]
enum AutostartState {
    Enabled,
    Disabled,
    Unsupported,
}

struct SampleResult {
    out: String,
    removed: Vec<(u32, usize)>,
}

impl App {
    /// 默认主题。只在这里定义一次，main 启动时从这儿取。
    /// 两处各写一个常量的话，写岔了就是"App 以为浅色、样式却是深色"，
    /// 文字会贴在背景上看不见。
    pub const DEFAULT_MODE: Mode = Mode::Dark;

    pub fn new() -> Self {
        let mut app = Self {
            plan: Plan::default(),
            watcher: None,
            mode: Self::DEFAULT_MODE,
            pending_theme: false,
            hidden: false,
            quitting: false,
            // 必须在主线程（也就是这里）创建：托盘要挂在窗口的消息循环上，
            // 换到别的线程建出来的托盘菜单点了没反应。
            tray: Tray::new(),
            heartbeat_on: false,
            cleaned_total: 0,
            chars_removed_total: 0,
            last_clean_at: None,
            last_error: None,
            sample: DEFAULT_SAMPLE.to_string(),
            sample_result: None,
            autostart_state: read_autostart_state(),
        };
        // 先把默认样例算一遍：界面一打开就该看见"发现几个、净化成什么样"，
        // 而不是等用户点一下才有内容。
        app.refresh_sample();
        // 默认就把守护开起来。双击打开这个程序的人要的是"从此复制就干净"，
        // 不是先研究一遍界面再手动点一下开始——那一步是设计失误，不是功能。
        app.start();
        app
    }

    fn palette(&self) -> Palette {
        Palette::new(self.mode)
    }

    /// 每帧开头把后台线程攒的事件收掉。立即模式下没有其他地方适合做这件事。
    fn pump_events(&mut self, ctx: &egui::Context) {
        let Some(w) = &self.watcher else { return };
        let events = w.drain_events();
        if events.is_empty() {
            return;
        }
        // 有事件就说明界面需要重画，否则一直在等用户输入，统计数字不更新。
        ctx.request_repaint();

        for e in events {
            match e {
                Event::Cleaned { removed } => {
                    self.cleaned_total += 1;
                    self.chars_removed_total += removed;
                    self.last_clean_at = Some(Instant::now());
                    self.last_error = None;
                }
                Event::ReadFailed(msg) | Event::WriteFailed(msg) => {
                    // 剪贴板被别的程序短暂独占是常态，不弹窗打断人，只在状态条上留一句。
                    self.last_error = Some(msg);
                }
            }
        }
    }

    fn start(&mut self) {
        self.watcher = Some(state::start_watch(self.plan.clone(), 200));
        self.last_error = None;
        // 重新开始计数，免得"运行时长"和统计对不上。
        self.cleaned_total = 0;
        self.chars_removed_total = 0;
        self.last_clean_at = None;
    }

    fn stop(&mut self) {
        self.watcher = None;
    }

    /// 处理一批托盘动作。
    ///
    /// 为什么不在托盘回调里直接改 App：回调是在 Windows 消息分发时触发的，
    /// 那一刻 egui 可能正处在某一帧中间，直接改状态容易改到一半的界面。
    /// 所以回调只负责把动作塞进队列，这里在每帧开头统一执行。
    fn handle_tray(&mut self, ctx: &egui::Context, actions: Vec<TrayAction>) {
        for action in actions {
            crate::debug_log(&format!("[ui] 处理托盘动作 {action:?}"));
            match action {
                TrayAction::Show => {
                    // 顺序不能反，也不能省：被 `Visible(false)` 藏起来的窗口，
                    // 只发 Visible(true) 在 winit 里不一定真的 show（它内部有
                    // "已经是可见的就不重复显示"的短路），所以先 Minimized(false)。
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    self.hidden = false;
                }
                TrayAction::StartWatch => self.start(),
                TrayAction::StopWatch => self.stop(),
                TrayAction::CleanOnce => self.clean_once(),
                TrayAction::Quit => {
                    self.quitting = true;
                    self.stop();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    /// 把窗口藏到托盘。点 × 和点「收进托盘」都会走到这里。
    fn hide_to_tray(&mut self, ctx: &egui::Context) {
        // 藏之前确保守护在跑：初始状态本来就开着，但用户可能手动停过，
        // 这时候点 × 不该变成"程序还活着但什么都不做"。
        self.start();
        self.hidden = true;
        crate::debug_log(&format!("[ui] 收进托盘 tray={}", self.tray.is_some()));
        if self.tray.is_some() {
            // 有托盘：窗口彻底藏起来，任务栏上也不留按钮。
            // 恢复靠托盘的左键/「显示窗口」，不会找不到。
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        } else {
            // 没有托盘（非 Windows，或者托盘建不出来）：只能收进任务栏，
            // 否则 `Visible(false)` 会让任务栏图标也消失，用户再也点不回来。
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
    }

    fn clean_once(&mut self) {
        match state::clipboard::read() {
            Ok(None) => {
                self.last_error = Some("剪贴板里没有文本".to_string());
            }
            Ok(Some(text)) => {
                let (out, report) = apply(&text, &self.plan);
                if out == text {
                    self.last_error = None;
                    self.last_clean_at = Some(Instant::now());
                    return;
                }
                match state::clipboard::write(&out) {
                    Ok(()) => {
                        let n: usize = report.removed.iter().map(|(_, c)| c).sum();
                        self.cleaned_total += 1;
                        self.chars_removed_total += n;
                        self.last_clean_at = Some(Instant::now());
                        self.last_error = None;
                    }
                    Err(e) => self.last_error = Some(e),
                }
            }
            Err(e) => self.last_error = Some(e),
        }
    }

    /// 重算试衣间结果。
    fn refresh_sample(&mut self) {
        let (out, report) = apply(&self.sample, &self.plan);
        let removed: Vec<(u32, usize)> = inspect(&self.sample)
            .into_iter()
            .map(|(c, n)| (c as u32, n))
            .collect();
        // report 里的 removed 是"应用规则时实际删掉的"，inspect 是"扫出来的可疑字符"。
        // 展示用后者更直观：关掉某条规则后，那个字符仍然出现在"发现"里但不处理。
        let _ = report;
        self.sample_result = Some(SampleResult { out, removed });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 开发期截图（只在 debug 构建里存在，见 shot.rs）。不设环境变量时是空操作。
        #[cfg(debug_assertions)]
        crate::shot::tick(ctx);

        if tray::debug_enabled() {
            static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n % 30 == 0 {
                crate::debug_log(&format!(
                    "[ui] 第 {n} 帧 hidden={} 重绘请求={:?}",
                    self.hidden,
                    ctx.cumulative_pass_nr()
                ));
            }
        }

        self.pump_events(ctx);

        // 托盘点击 / 菜单选择。必须在 pump_events 之后、画界面之前处理，
        // 这样这一帧画出来的就是处理后的状态，不会闪一帧旧数据。
        // 日志走 crate::debug_log（自己写文件），不依赖 stderr。
        let dbg = tray::debug_enabled();
        let actions = self.tray.as_ref().map(|t| t.poll(dbg)).unwrap_or_default();
        if !actions.is_empty() {
            ctx.request_repaint();
            self.handle_tray(ctx, actions);
        }
        // 托盘菜单项的启用状态、悬停提示跟着界面状态走。
        if let Some(t) = &self.tray {
            t.sync(self.watcher.is_some(), self.cleaned_total);
        }
        // 托盘事件要靠 Windows 消息循环分发，而消息循环只在有新帧时才被驱动。
        // 窗口藏起来之后 eframe 会跳过所有排定的重绘（request_repaint_after 失效），
        // 所以用一个独立线程按节奏催帧，见 tray::spawn_heartbeat 的注释。
        if self.tray.is_some() && !self.heartbeat_on {
            self.heartbeat_on = true;
            tray::spawn_heartbeat(ctx.clone());
        }
        // 没托盘时靠「窗口失焦→重新点回来」判断窗口是否回来了。
        // 有托盘的话这条路走不通：窗口可能被别的程序盖住，因为「重新获得焦点」
        // 并不等于「从托盘唤回来的」，误判会让 hidden 状态自己也乱。
        if self.hidden
            && self.tray.is_none()
            && ctx.input(|i| i.viewport().focused.unwrap_or(false))
        {
            self.hidden = false;
        }

        // 点右上角 × 默认不退出，而是藏起来继续在后台守着剪贴板。
        // 用户装这个工具就是为了"一直在"，关个窗口就把守护停了等于没装。
        // 真要退出走界面上的「退出」或托盘菜单，那条路设了 quitting 就不会被拦。
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_to_tray(ctx);
        }
        // 窗口藏起来之后，egui 默认不再重绘，后台线程发来的事件就没机会被收走，
        // 统计数字会停在藏起来那一刻。所以隐藏期间仍然按较低的频率推重绘。
        if self.hidden {
            ctx.request_repaint_after(Duration::from_millis(1000));
        }

        // 守护在跑的时候，状态条要显示秒数，得有个东西推着重画。
        if self.watcher.is_some() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }

        // 主题切换是"下一帧才生效"的：apply 改的是 Style，
        // 而当前帧已经开始画了。手动记一个待切换标志，画完再套。
        if self.pending_theme {
            self.pending_theme = false;
            theme::apply(ctx, self.mode);
        }

        let p = self.palette();

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(p.bg)
                    .inner_margin(egui::Margin::same(14)),
            )
            .show(ctx, |ui| {
                self.header(ui, &p);
                ui.add_space(10.0);

                // 状态卡一直在顶部，不参与滚动——它是"现在什么情况"，永远该看得见。
                theme::card_frame(&p).show(ui, |ui| {
                    self.status_card(ui, &p);
                });
                ui.add_space(10.0);

                // 剩下的高度左右分：左边规则窄一点，右边试衣间宽一点。
                // 竖排会让试衣间被挤到窗口外面，用户得滚才看得见，太浪费。
                let avail = ui.available_height();
                // 卡片内边距 12 上下各一次，滚动区只能吃剩下的部分。
                let inner_h = (avail - 26.0).max(120.0);
                let total = ui.available_width();
                // 两栏宽度必须正好把 total 分完：以前右栏是"左栏占 45%，
                // 剩下的都给右栏"，可一旦左栏被内容撑宽，剩下的就是负数，
                // 右栏连同内容一起被推出窗口右缘 —— 看起来就是"右边被切掉一截"。
                let (left_w, right_w) = column_widths(total);

                ui.horizontal_top(|ui| {
                    // 两栏都用一个定高的槽：Frame 撑死也就 inner_h + 24，
                    // 内容再多由里面的 ScrollArea 吃掉。不这么钉死，
                    // 内容长的一边会把 Frame 撑高，另一边就留一大块空白。
                    column_card(ui, &p, left_w, avail, inner_h, "rules_scroll", |ui| {
                        self.rules_card(ui, &p);
                    });
                    ui.add_space(COLUMN_GAP);
                    column_card(ui, &p, right_w, avail, inner_h, "play_scroll", |ui| {
                        self.playground_card(ui, &p);
                    });
                });
            });
    }
}

/// 两栏之间的空隙。
const COLUMN_GAP: f32 = 10.0;

/// 一栏窄过这个宽度就摆不下东西了（中文标签一行都放不下）。
const MIN_COLUMN_W: f32 = 180.0;

/// 左栏再宽也不超过这个值——规则卡不需要那么宽，宽了纯浪费试衣间的空间。
const MAX_LEFT_COLUMN_W: f32 = 360.0;

/// 左右两栏各分多宽。
///
/// 硬约束是 `left + COLUMN_GAP + right == total`：只要这条件成立，
/// 右栏就永远在窗口里面，不可能被切。原来的写法是"左栏 45%、
/// 右栏 = 剩下的，最少 200"，那个 `max(200)` 在总宽不够时会让两栏加起来
/// 比可用宽度还宽，于是右栏冲出窗口。
fn column_widths(total: f32) -> (f32, f32) {
    let total = total.max(0.0);
    // 连两栏都放不下就只留一栏，不然两边都是没法用的窄条。
    if total <= COLUMN_GAP + 2.0 * MIN_COLUMN_W {
        return (total, 0.0);
    }
    let left = (total * 0.45).clamp(MIN_COLUMN_W, MAX_LEFT_COLUMN_W);
    let right = total - left - COLUMN_GAP;
    if right >= MIN_COLUMN_W {
        (left, right)
    } else {
        // 左栏偏宽挤掉了右栏：宁可对半分，也不要一边富一边穷。
        let each = (total - COLUMN_GAP) / 2.0;
        (each, each)
    }
}

/// 画一张「定宽定高」的卡片：外层 `card_frame` + 内部竖向滚动。
///
/// 宽度用 `set_min_width` / `set_max_width` 双向钉死，而不是只 `set_width`：
/// 只设下限的话，内容（比如不换行的长提示、`Grid` 的第二列）比槽宽时
/// Frame 会跟着长胖，把隔壁那栏顶出窗口。钉住上限后，超宽内容只能换行。
fn column_card(
    ui: &mut egui::Ui,
    p: &Palette,
    width: f32,
    height: f32,
    inner_h: f32,
    id: &str,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    if width <= 0.0 {
        return;
    }
    // card_frame 的 margin 是 12，左右各一份。
    let inner_w = (width - 24.0).max(60.0);
    ui.allocate_ui_with_layout(
        egui::vec2(width, height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_min_width(width);
            ui.set_max_width(width);
            theme::card_frame(p).show(ui, |ui| {
                ui.set_min_width(inner_w);
                ui.set_max_width(inner_w);
                egui::ScrollArea::vertical()
                    .id_salt(id)
                    .auto_shrink([false, false])
                    .max_height(inner_h)
                    .show(ui, add_contents);
            });
        },
    );
}

/// 规则卡里的一行：复选框一行，说明折到下一行。
///
/// 为什么不再"复选框 + 说明"并排：说明文字长短差很多（最短 8 个字、
/// 最长 30 个字），并排就得给说明留固定宽度，窄窗口下只能换行成两行，
/// 宽窗口下又空一大截。更糟的是原来用 `egui::Grid`——它的列宽只增不减，
/// 长说明直接把卡片撑出窗口。现在统一"复选框在上、说明在下"，
/// 说明按卡片宽度自动换行，窗口拉多窄都不会溢出。
fn rule_row(
    ui: &mut egui::Ui,
    p: &Palette,
    add_label: impl FnOnce(&mut egui::Ui),
    hint: &str,
    mono: bool,
) {
    add_label(ui);
    let mut rich = egui::RichText::new(hint).small().color(p.text_dim);
    if mono {
        rich = rich.monospace();
    }
    ui.horizontal(|ui| {
        // 缩进到复选框文字的左边缘，看着还是一组的。
        ui.add_space(18.0);
        ui.add(egui::Label::new(rich).wrap());
    });
    ui.add_space(2.0);
}

impl App {
    fn header(&mut self, ui: &mut egui::Ui, p: &Palette) {
        // 那句说明有 18 个汉字，窗口窄的时候要是还硬塞在标题那一行，
        // 就会顶到右边的按钮上（两个都哭，谁也看不清）。宽度够并排，
        // 不够就折到第二行。
        let subtitle = "复制粘贴时自动清掉零宽空格、伪空格、弯引号";
        // 并排大约要 700px：标题 + 版本号 + 说明 + 三个按钮。留到 860 是余量，
        // 默认 760 宽的窗口里并排会紧贴着「浅色」，不如干脆折成一行。
        let wide = ui.available_width() >= 860.0;

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("ClipSift")
                    .text_style(egui::TextStyle::Name("Title".into()))
                    .strong()
                    .color(p.text),
            );
            ui.label(
                egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .small()
                    .color(p.text_faint),
            );
            if wide {
                ui.label(egui::RichText::new(subtitle).small().color(p.text_dim));
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("退出").clicked() {
                    // 这条是唯一能真的退出的路：先把守护停了再放行 close。
                    self.quitting = true;
                    self.stop();
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                // 有托盘就说"收进托盘"，没有才说"任务栏"——按钮上写的就是点下去发生的事。
                let hide_label = if self.tray.is_some() {
                    "收进托盘"
                } else {
                    "收进任务栏"
                };
                if ui.button(hide_label).clicked() {
                    self.hide_to_tray(ui.ctx());
                }
                ui.separator();
                if ui.button(self.mode.toggle().label()).clicked() {
                    self.mode = self.mode.toggle();
                    self.pending_theme = true;
                }
            });
        });
        if !wide {
            ui.label(egui::RichText::new(subtitle).small().color(p.text_dim));
        }
    }

    fn status_card(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let running = self.watcher.is_some();

        ui.horizontal(|ui| {
            // 状态灯：跑起来是绿的，顺便给光圈，静止时是灰的。
            let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
            let c = if running { p.success } else { p.text_faint };
            ui.painter().circle_filled(rect.center(), 5.0, c);
            if running {
                ui.painter().circle_stroke(
                    rect.center(),
                    7.0,
                    egui::Stroke::new(1.0_f32, c.gamma_multiply(0.35)),
                );
            }

            if running {
                ui.label(egui::RichText::new("守护中").strong().color(p.success));
                let uptime = self
                    .watcher
                    .as_ref()
                    .map(|w| w.uptime())
                    .unwrap_or_default();
                ui.label(egui::RichText::new(fmt_duration(uptime)).color(p.text_dim));
            } else {
                ui.label(egui::RichText::new("未运行").strong().color(p.text_dim));
                ui.label(
                    egui::RichText::new("点下面的「开始守护」，之后复制进剪贴板的内容会自动清干净")
                        .small()
                        .color(p.text_faint),
                );
            }

            // 托盘建不出来时说一声，用户才知道为什么点 × 之后只剩任务栏按钮。
            if self.tray.is_none() {
                ui.label(
                    egui::RichText::new("（托盘不可用）")
                        .small()
                        .color(p.text_faint),
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.autostart_state == AutostartState::Unsupported {
                    ui.add_enabled(
                        false,
                        egui::Checkbox::new(&mut false, "开机自启（仅 Windows）"),
                    );
                } else {
                    let mut checked = self.autostart_state == AutostartState::Enabled;
                    if ui.checkbox(&mut checked, "开机自启").clicked() {
                        self.toggle_autostart();
                    }
                }
            });
        });

        ui.add_space(6.0);

        // 三个统计数字横排，比一行小字堆着好看。
        // 用 wrapped 版本：窗口窄的时候宁可换行，也别把「最后一次」挤出卡片。
        ui.horizontal_wrapped(|ui| {
            stat(ui, p, &self.cleaned_total.to_string(), "已净化");
            ui.add_space(18.0);
            stat(ui, p, &self.chars_removed_total.to_string(), "清掉字符");
            ui.add_space(18.0);
            let when = self
                .last_clean_at
                .map(|t| format!("{}前", fmt_duration(t.elapsed())))
                .unwrap_or_else(|| "—".into());
            stat(ui, p, &when, "最后一次");
        });

        if let Some(err) = &self.last_error {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(format!("⚠ {err}")).color(p.warning));
        }

        if !self.plan.is_idempotent() {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("⚠ 启用了「合并硬回车」，它不是幂等的，守护模式下别开")
                    .color(p.warning),
            );
        }

        ui.add_space(10.0);
        // 三个按钮在窗口窄的时候会整行超出去，wrapped 让它折行。
        ui.horizontal_wrapped(|ui| {
            let running = self.watcher.is_some();
            let label = if running {
                "停止守护"
            } else {
                "开始守护"
            };
            if theme::primary_button(ui, p, label, true) {
                if running {
                    self.stop();
                } else {
                    self.start();
                }
            }
            if ui.button("净化剪贴板一次").clicked() {
                self.clean_once();
            }
            if ui.button("剪贴板内容读进试衣间").clicked() {
                if let Ok(Some(t)) = state::clipboard::read() {
                    self.sample = t;
                    self.refresh_sample();
                }
            }
        });
    }

    fn rules_card(&mut self, ui: &mut egui::Ui, p: &Palette) {
        ui.label(egui::RichText::new("规则").strong());
        ui.label(
            egui::RichText::new("点了立刻生效；守护正在跑的时候改规则也会马上换用新规则。")
                .small()
                .color(p.text_dim),
        );
        ui.add_space(6.0);

        let before = self.plan.clone();

        // 原来这里是个 `egui::Grid`，两列排"复选框 + 说明"。Grid 的第二列
        // 不会自动换行，长说明按原始宽度把整张卡撑胖，右边那栏就被顶出窗口。
        // 现在每行自己量：说明文字塞得下就并排，塞不下就折到复选框下面一行。
        // 窗口拉宽拉窄都不会溢出。
        rule_row(
            ui,
            p,
            |ui| {
                ui.checkbox(&mut self.plan.invisible, "隐形字符");
            },
            "零宽空格、bidi 覆盖符、隐形标签",
            false,
        );

        rule_row(
            ui,
            p,
            |ui| {
                ui.checkbox(&mut self.plan.normalize_spaces, "伪空格");
            },
            "不间断空格、全角空格 → 普通空格",
            false,
        );

        rule_row(
            ui,
            p,
            |ui| {
                ui.checkbox(&mut self.plan.confusables, "弯引号 / 破折号");
            },
            "\u{2018}abc\u{2019} \u{201C}abc\u{201D} \u{2014} \u{2013} \u{2026}",
            true,
        );

        rule_row(
            ui,
            p,
            |ui| {
                ui.checkbox(&mut self.plan.fullwidth_alnum, "全角字母数字转半角");
            },
            "ＡＢＣ１２３－（）→ ABC123-()",
            true,
        );

        // 单独一档：中文正文里 ！？这些本来就是全角，转成半角反而难看。
        rule_row(
            ui,
            p,
            |ui| {
                ui.checkbox(&mut self.plan.fullwidth_punct, "全角标点转半角");
            },
            "，。！？→ ,.!?（中文正文一般别开）",
            false,
        );

        rule_row(
            ui,
            p,
            |ui| {
                ui.checkbox(&mut self.plan.collapse_whitespace, "压缩多余空格");
            },
            "连续空格合并成一个，行首缩进原样保留",
            false,
        );

        rule_row(
            ui,
            p,
            |ui| {
                ui.checkbox(&mut self.plan.trim_trailing, "去掉行尾空格");
            },
            "每行末尾的空白删掉",
            false,
        );

        rule_row(
            ui,
            p,
            |ui| {
                ui.checkbox(&mut self.plan.strip_control, "控制字符");
            },
            "删掉不可见的控制字符",
            false,
        );

        rule_row(
            ui,
            p,
            |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.plan.merge_hard_returns, "合并硬回车");
                    ui.label(egui::RichText::new("⚠").color(p.warning));
                });
            },
            "单换行变空格，适合把 PDF 正文拉成一段；重复处理结果会变",
            false,
        );

        ui.add_space(6.0);
        if ui.button("恢复默认").clicked() {
            let custom = std::mem::take(&mut self.plan.custom);
            self.plan = Plan {
                custom,
                ..Plan::default()
            };
        }

        // 改了就重算预览；守护开着的话重启一次，让新规则立刻生效。
        if self.plan != before {
            self.refresh_sample();
            if self.watcher.is_some() {
                self.start();
            }
        }
    }

    fn playground_card(&mut self, ui: &mut egui::Ui, p: &Palette) {
        ui.label(egui::RichText::new("试一下").strong());
        ui.label(
            egui::RichText::new("贴一段文本进来，改动左边规则能立刻看到差别。")
                .small()
                .color(p.text_dim),
        );
        ui.add_space(6.0);

        // 输入框占满宽度，不再外面套 ScrollArea（里面限高，外面再套会双重滚动）。
        // 高度按内容自增，最少 5 行——固定高度会在演示文本较长时把尾巴藏起来，
        // 用户以为「就这些」，其实下面还有两行脏字符没看见。
        let resp = ui.add(
            egui::TextEdit::multiline(&mut self.sample)
                .desired_width(f32::INFINITY)
                .desired_rows(11)
                .hint_text("在这里粘贴……"),
        );
        if resp.changed() {
            self.refresh_sample();
        }

        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("读剪贴板填进来").clicked() {
                if let Ok(Some(t)) = state::clipboard::read() {
                    self.sample = t;
                    self.refresh_sample();
                }
            }
            if ui.button("清空").clicked() {
                self.sample.clear();
                self.refresh_sample();
            }
            if ui.button("重新算一遍").clicked() {
                self.refresh_sample();
            }
        });

        if let Some(r) = &self.sample_result {
            ui.add_space(10.0);

            if r.removed.is_empty() {
                ui.label(egui::RichText::new("没发现可疑字符。").color(p.success));
            } else {
                let total: usize = r.removed.iter().map(|(_, n)| n).sum();
                ui.label(
                    egui::RichText::new(format!("发现 {total} 个可疑字符"))
                        .strong()
                        .color(p.warning),
                );
                ui.add_space(6.0);

                // 每行做成「原文这是什么 → 会变成什么」的对照。
                // 光列码点没用——用户要的是"它到底会被改成啥"，
                // 尤其是零宽字符这种画不出来的，只写 `U+200B` 等于什么都没说。
                for (cp, n) in &r.removed {
                    let ch = char::from_u32(*cp).unwrap_or('\u{FFFD}');
                    ui.horizontal_wrapped(|ui| {
                        // 左：原字符。画不出来的用码点顶替，但加个框表示"这里有东西"。
                        let raw_label = if is_invisible(*cp) {
                            format!("〔{:04X}〕", cp)
                        } else {
                            ch.to_string()
                        };
                        ui.label(
                            egui::RichText::new(raw_label)
                                .monospace()
                                .color(p.warning)
                                .background_color(p.control_bg),
                        );
                        ui.label(egui::RichText::new("→").small().color(p.text_faint));

                        // 中：处理后的样子（删除还是替换成什么）。
                        // 弯引号这类字符在等宽字体里长得和直引号几乎一样，
                        // 所以换成之后还要标一眼原字符的描述，否则看不出区别。
                        let after = match replacement_of(ch) {
                            Some(a) => a.to_string(),
                            None => "（删除）".to_string(),
                        };
                        ui.label(egui::RichText::new(after).monospace().color(p.success));

                        ui.label(egui::RichText::new(note(*cp)).small().color(p.text_dim));
                        ui.label(
                            egui::RichText::new(format!("× {n}"))
                                .small()
                                .color(p.text_faint),
                        );
                    });
                }
            }

            ui.add_space(10.0);
            ui.label(egui::RichText::new("净化后").strong());
            // 只读框。用 TextEdit 而不是 Label，是为了让它能选中、能 Ctrl+C 复制走。
            let mut preview = r.out.clone();
            ui.add(
                egui::TextEdit::multiline(&mut preview)
                    .desired_width(f32::INFINITY)
                    .desired_rows(5)
                    .interactive(true),
            );
            let out = r.out.clone();
            if ui.button("把结果写回剪贴板").clicked() {
                match state::clipboard::write(&out) {
                    Ok(()) => self.last_error = None,
                    Err(e) => self.last_error = Some(e),
                }
            }
        }
    }

    fn toggle_autostart(&mut self) {
        #[cfg(windows)]
        {
            let target = self.autostart_state != AutostartState::Enabled;
            let res = if target {
                autostart_enable()
            } else {
                autostart_disable()
            };
            match res {
                Ok(()) => {
                    self.autostart_state = if target {
                        AutostartState::Enabled
                    } else {
                        AutostartState::Disabled
                    };
                }
                Err(e) => self.last_error = Some(e),
            }
        }
        #[cfg(not(windows))]
        {
            self.last_error = Some("开机自启目前只支持 Windows".to_string());
        }
    }
}

/// 启动时读一次注册表，决定复选框的初始状态。
///
/// 必须"读一次"，不能默认当成关：用户上次开过自启，重开界面应当还是勾上的。
fn read_autostart_state() -> AutostartState {
    #[cfg(windows)]
    {
        match crate::reg::query_run_value(crate::reg::VALUE_NAME) {
            Some(cmd) if cmd.contains("clipsift") => AutostartState::Enabled,
            _ => AutostartState::Disabled,
        }
    }
    #[cfg(not(windows))]
    {
        AutostartState::Unsupported
    }
}

/// 把"当前这个 exe + watch --background"写进 Run 项。
///
/// 用 `--background` 是为了开机时别弹黑框：CLI 的 watch 命令带这个开关会
/// 调 ShowWindow(SW_HIDE) 把自己藏起来，只留托盘外的常驻进程。
fn autostart_enable() -> Result<(), String> {
    #[cfg(windows)]
    {
        let exe = std::env::current_exe().map_err(|e| format!("找不到自己的路径：{e}"))?;
        let exe = exe.to_string_lossy();
        // 路径里有空格必须加引号，否则 Windows 启动时会当成"参数"切分。
        let cmd = format!("\"{exe}\" watch --background");
        crate::reg::set_run_value(crate::reg::VALUE_NAME, &cmd)
    }
    #[cfg(not(windows))]
    {
        Err("开机自启目前只支持 Windows".to_string())
    }
}

fn autostart_disable() -> Result<(), String> {
    #[cfg(windows)]
    {
        crate::reg::delete_run_value(crate::reg::VALUE_NAME)
    }
    #[cfg(not(windows))]
    {
        Err("开机自启目前只支持 Windows".to_string())
    }
}

/// 一个小统计块：大数字 + 小标签，竖着排。
fn stat(ui: &mut egui::Ui, p: &Palette, value: &str, label: &str) {
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new(value)
                .text_style(egui::TextStyle::Name("Subtitle".into()))
                .strong()
                .color(p.text),
        );
        ui.label(egui::RichText::new(label).small().color(p.text_faint));
    });
}

/// 默认演示用的样例。
///
/// 挑这段是刻意的：它四种毛病各来一个，而且净化前后肉眼能比出差别——
/// - 【零宽空格】表格的列名被拆开，`na​me` 看着和 `name` 一样但字符串比较不相等
/// - 【伪空格】SQL 里那个"空格"其实是 U+00A0，亲手敲的空格跟它不匹配
/// - 【弯引号】代码里的引号是中文输入法出来的，编译器报 stray '\342'
/// - 【全角减号】`－1` 那个减号是全角的，Python 会报语法错误
///
/// 用户一打开就能看见"原来我复制的东西里有这些"，比读十行说明有效。
const DEFAULT_SAMPLE: &str = "\
import pandas as pd\n\
\n\
# 从飞书表格粘过来的表头，列名里藏了零宽空格\n\
df = pd.DataFrame({\u{201C}na\u{200B}me\u{201D}: [\u{201C}Alice\u{201D}], \u{201C}age\u{201D}: [30]})\n\
\n\
# 看上去都是 name，比较却不相等\n\
print(df.columns[0] == \u{2018}name\u{2019})\n\
\n\
# 后面这个「空格」是不间断空格，不是普通空格\n\
df = df[df[\u{201C}age\u{201D}] > 18]\u{00A0}\n\
\n\
# 这个减号是全角的，Python 会报 SyntaxError\n\
x = \u{FF0D}1\n\
\n\
print(df.loc[0, \u{201C}na\u{200B}me\u{201D}])\n\
";

fn fmt_duration(d: Duration) -> String {
    let s = d.as_secs();
    if s < 60 {
        format!("{s} 秒")
    } else if s < 3600 {
        format!("{} 分 {} 秒", s / 60, s % 60)
    } else {
        format!("{} 小时 {} 分", s / 3600, (s % 3600) / 60)
    }
}

/// 这个码点在屏幕上是不是"看不见"。
///
/// 看不见的字符直接画出来等于留一个空白，用户不知道那里有东西，
/// 所以界面上要退化成码点标注。
fn is_invisible(cp: u32) -> bool {
    matches!(cp,
        0x00AD | 0x180E
        | 0x200B..=0x200F
        | 0x202A..=0x202E
        | 0x2060..=0x2064
        | 0x2066..=0x2069
        | 0x061C | 0xFEFF | 0xFFF9..=0xFFFB
        | 0xE0000..=0xE007F
    )
}

/// 这个字符会被换成什么；`None` 表示会被直接删掉。
///
/// 顺序必须和 `rules::Plan::steps()` 一致（先查不可见、再查伪空格、
/// 再查同形字、最后查全角），否则界面上标的箭头会和实际结果对不上。
fn replacement_of(c: char) -> Option<char> {
    if clipsift_core::chars::is_fake_space(c) {
        // 伪空格被 normalize_spaces 换成普通空格，视觉上就是"一个空格"。
        return Some(' ');
    }
    if let Some(a) = clipsift_core::chars::confusable_pair(c) {
        return Some(a);
    }
    clipsift_core::chars::fullwidth_to_ascii(c)
}

fn note(cp: u32) -> &'static str {
    match cp {
        0x00A0 => "不间断空格",
        0x2007 | 0x2009 | 0x202F => "各种宽度空格",
        0x200B => "零宽空格",
        0x200C => "零宽非连接符",
        0x200D => "零宽连接符",
        0x200E | 0x200F => "方向标记",
        0x061C => "阿拉伯文方向标记",
        0x201C | 0x201D => "弯双引号",
        0x2018 | 0x2019 => "弯单引号",
        0x2013 => "en dash",
        0x2014 => "em dash",
        0x2026 => "省略号",
        0x202A..=0x202E => "bidi 覆盖符",
        0x2066..=0x2069 => "bidi 隔离符",
        0x2060..=0x2064 => "不可见运算符",
        0xFEFF => "BOM / 零宽不换行空格",
        0x3000 => "全角空格",
        0xFF01..=0xFF5E => "全角字符",
        0xE0000..=0xE007F => "隐形标签字符",
        _ => "可疑字符",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 左栏 + 空隙 + 右栏 必须正好等于可用宽度。
    ///
    /// 这条是「右栏被切掉」那个 bug 的回归测试：只要三者加起来比可用宽度多，
    /// 右栏就会跑到窗口外面去。
    #[test]
    fn columns_fill_available_width_exactly() {
        let widths = [
            0.0_f32, 120.0, 300.0, 360.0, 370.0, 400.0, 460.0, 500.0, 592.0, 700.0, 760.0, 982.0,
            1440.0, 3000.0,
        ];
        for total in widths {
            let (left, right) = column_widths(total);
            assert!(left >= 0.0, "total={total} left={left}");
            assert!(right >= 0.0, "total={total} right={right}");
            if right > 0.0 {
                let sum = left + COLUMN_GAP + right;
                assert!(
                    (sum - total).abs() < 0.01,
                    "total={total} 分出来的宽度是 {sum}（left={left} right={right}）"
                );
            } else {
                // 太窄只留一栏，这时候那一栏得吃满整行。
                assert!((left - total).abs() < 0.01, "total={total} left={left}");
            }
        }
    }

    /// 默认窗口（760）和最小窗口（620）下，右栏都得有能用的宽度。
    #[test]
    fn both_columns_are_usable_at_supported_sizes() {
        for window in [760.0_f32, 620.0] {
            // CentralPanel 左右各 14px 外边距。
            let total = window - 28.0;
            let (left, right) = column_widths(total);
            assert!(
                left >= MIN_COLUMN_W,
                "窗口 {window} 时左栏只有 {left}，放不下规则"
            );
            assert!(
                right >= 200.0,
                "窗口 {window} 时右栏只有 {right}，试衣间会被挤没了"
            );
            assert!(right > left, "窗口 {window} 时右栏（试衣间）应该比左栏宽");
        }
    }

    /// 宽窗口下左栏不能无限变大，否则试衣间被浪费。
    #[test]
    fn left_column_is_capped() {
        let (left, _) = column_widths(4000.0);
        assert!(left <= MAX_LEFT_COLUMN_W);
    }
}
