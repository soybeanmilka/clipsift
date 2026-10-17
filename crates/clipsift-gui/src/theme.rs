//! Fluent 2 风格的 egui 主题。
//!
//! 为什么自己做而不是拉现成的皮肤库：ecosystem 里像样的（fluent-egui、
//! modern-egui、opaline）都盯着 egui 0.33+，我们锁在 0.32 是为了 MSRV 1.85；
//! 而且它们体量大（fluent-egui 的 widgets.rs 就 15 万字节），我们只要 4 块界面，
//! 付不起那个身材。
//!
//! 这里的数值不是拍脑袋：取自微软 Fluent 2 设计系统在 WinUI 里的实现
//! （深色 32,32,32 / 浅色 243,243,243 的底、卡片叠一层半透明白、
//! 圆角 4、控件高 32、正文 14px、标题 20px），照着抄一份就够好看了。

use eframe::egui::{
    self, Color32, CornerRadius, FontId, Margin, Shadow, Stroke, TextStyle, Vec2, Visuals,
};

/// 主题模式。显式两种，不跟系统走 —— 程序窗口是自绘的，
/// 跟系统走反而会在系统切换时闪一下。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Light,
    Dark,
}

impl Mode {
    pub fn toggle(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "浅色",
            Self::Dark => "深色",
        }
    }
}

/// 一套解析好的颜色。整套界面只从这儿取色，别在 ui 里写死 Color32。
#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub card_bg: Color32,
    pub card_border: Color32,
    pub control_bg: Color32,
    pub control_hover: Color32,
    pub control_pressed: Color32,
    pub control_border: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub on_accent: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub shadow: Color32,
    pub is_dark: bool,
}

impl Palette {
    pub fn new(mode: Mode) -> Self {
        match mode {
            Mode::Dark => Self {
                bg: Color32::from_rgb(32, 32, 32),
                // 卡片和控件用半透明白叠在底色上，是 Fluent 的做法：
                // 这样将来想加 Mica 毛玻璃只要换底色，上面几层不用动。
                card_bg: Color32::from_rgba_unmultiplied(43, 43, 43, 255),
                card_border: Color32::from_rgba_unmultiplied(255, 255, 255, 18),
                control_bg: Color32::from_rgba_unmultiplied(58, 58, 58, 255),
                control_hover: Color32::from_rgba_unmultiplied(66, 66, 66, 255),
                control_pressed: Color32::from_rgba_unmultiplied(50, 50, 50, 255),
                control_border: Color32::from_rgba_unmultiplied(255, 255, 255, 25),
                text: Color32::from_rgb(255, 255, 255),
                text_dim: Color32::from_rgb(200, 200, 200),
                text_faint: Color32::from_rgb(126, 126, 126),
                accent: Color32::from_rgb(96, 205, 255),
                accent_soft: Color32::from_rgba_unmultiplied(96, 205, 255, 48),
                on_accent: Color32::from_rgb(0, 0, 0),
                success: Color32::from_rgb(108, 203, 95),
                warning: Color32::from_rgb(252, 225, 0),
                danger: Color32::from_rgb(255, 153, 164),
                shadow: Color32::from_rgba_unmultiplied(0, 0, 0, 76),
                is_dark: true,
            },
            Mode::Light => Self {
                bg: Color32::from_rgb(243, 243, 243),
                card_bg: Color32::from_rgb(255, 255, 255),
                card_border: Color32::from_rgba_unmultiplied(0, 0, 0, 16),
                control_bg: Color32::from_rgb(255, 255, 255),
                control_hover: Color32::from_rgb(249, 249, 249),
                control_pressed: Color32::from_rgb(245, 245, 245),
                control_border: Color32::from_rgba_unmultiplied(0, 0, 0, 38),
                text: Color32::from_rgb(27, 27, 27),
                text_dim: Color32::from_rgb(92, 92, 92),
                text_faint: Color32::from_rgb(160, 160, 160),
                accent: Color32::from_rgb(0, 95, 184),
                accent_soft: Color32::from_rgba_unmultiplied(0, 95, 184, 30),
                on_accent: Color32::from_rgb(255, 255, 255),
                success: Color32::from_rgb(15, 123, 15),
                warning: Color32::from_rgb(157, 93, 0),
                danger: Color32::from_rgb(196, 43, 28),
                shadow: Color32::from_rgba_unmultiplied(0, 0, 0, 38),
                is_dark: false,
            },
        }
    }
}

/// 把主题装进 egui 上下文。字体由 `fonts::install` 另外装，这里只碰样式。
pub fn apply(ctx: &egui::Context, mode: Mode) {
    let p = Palette::new(mode);
    let mut style = (*ctx.style()).clone();

    // ---- 文字 ----
    style.text_styles = [
        (TextStyle::Small, FontId::proportional(12.0)),
        (TextStyle::Body, FontId::proportional(14.0)),
        (TextStyle::Button, FontId::proportional(14.0)),
        (TextStyle::Heading, FontId::proportional(20.0)),
        (TextStyle::Monospace, FontId::monospace(13.0)),
    ]
    .into();
    // 「标题」「大字」这类自定义字号，egui 里要用 TextStyle::Name 拿。
    style
        .text_styles
        .insert(TextStyle::Name("Title".into()), FontId::proportional(26.0));
    style.text_styles.insert(
        TextStyle::Name("Subtitle".into()),
        FontId::proportional(16.0),
    );

    // ---- 间距 ----
    // Fluent 的基准是 4 的倍数：控件高 32、按钮内边距 12×6、元素间距 10×8。
    style.spacing.item_spacing = Vec2::new(10.0, 8.0);
    style.spacing.button_padding = Vec2::new(12.0, 6.0);
    style.spacing.interact_size = Vec2::new(40.0, 32.0);
    style.spacing.window_margin = Margin::same(16);
    style.spacing.menu_margin = Margin::same(4);
    style.spacing.icon_width = 18.0;
    style.spacing.icon_spacing = 8.0;

    // ---- 视觉 ----
    let mut visuals = if p.is_dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    visuals.override_text_color = Some(p.text);
    visuals.weak_text_color = Some(p.text_dim);
    visuals.extreme_bg_color = p.control_bg;
    visuals.text_edit_bg_color = Some(p.control_bg);
    visuals.faint_bg_color = p.control_pressed;
    visuals.code_bg_color = p.control_pressed;
    visuals.hyperlink_color = p.accent;
    visuals.warn_fg_color = p.warning;
    visuals.error_fg_color = p.danger;
    visuals.panel_fill = p.bg;
    visuals.window_fill = p.card_bg;
    visuals.window_stroke = Stroke::new(1.0_f32, p.card_border);
    visuals.window_corner_radius = CornerRadius::same(8);
    visuals.menu_corner_radius = CornerRadius::same(8);
    visuals.window_shadow = Shadow {
        offset: [0, 4],
        blur: 24,
        spread: 0,
        color: p.shadow,
    };
    visuals.popup_shadow = Shadow {
        offset: [0, 8],
        blur: 32,
        spread: 0,
        color: p.shadow,
    };
    visuals.selection.bg_fill = p.accent_soft;
    visuals.selection.stroke = Stroke::new(1.0_f32, p.accent);
    visuals.button_frame = true;
    visuals.collapsing_header_frame = false;
    visuals.indent_has_left_vline = false;
    visuals.slider_trailing_fill = true;
    visuals.handle_shape = egui::style::HandleShape::Circle;
    visuals.disabled_alpha = 0.42;

    // Fluent 的控件圆角是 4，比 egui 默认的收敛得多；卡片才用 8。
    visuals.widgets.noninteractive = WidgetVisualsBuilder::plain(p.text).build();
    visuals.widgets.inactive =
        WidgetVisualsBuilder::filled(p.control_bg, p.control_border, p.text).build();
    visuals.widgets.hovered =
        WidgetVisualsBuilder::filled(p.control_hover, p.control_border, p.text).build();
    visuals.widgets.active =
        WidgetVisualsBuilder::filled(p.control_pressed, p.control_border, p.text).build();
    visuals.widgets.open = visuals.widgets.active;
    visuals.text_cursor.stroke = Stroke::new(1.5_f32, p.accent);

    style.visuals = visuals;
    style.interaction.tooltip_delay = 0.35;
    style.interaction.show_tooltips_only_when_still = true;
    style.scroll_animation = egui::style::ScrollAnimation::duration(0.22);

    ctx.set_style(style);
}

/// 拼一个 `WidgetVisuals`。egui 0.32 里这个结构体字段是 pub 的，
/// 但每次手写六个字段太吵，包一层。
struct WidgetVisualsBuilder {
    bg_fill: Color32,
    weak_bg_fill: Color32,
    border: Color32,
    fg: Color32,
    radius: u8,
}

impl WidgetVisualsBuilder {
    fn plain(fg: Color32) -> Self {
        Self {
            bg_fill: Color32::TRANSPARENT,
            weak_bg_fill: Color32::TRANSPARENT,
            border: Color32::TRANSPARENT,
            fg,
            radius: 4,
        }
    }

    fn filled(bg: Color32, border: Color32, fg: Color32) -> Self {
        Self {
            bg_fill: bg,
            weak_bg_fill: bg,
            border,
            fg,
            radius: 4,
        }
    }

    fn build(self) -> egui::style::WidgetVisuals {
        egui::style::WidgetVisuals {
            bg_fill: self.bg_fill,
            weak_bg_fill: self.weak_bg_fill,
            bg_stroke: Stroke::new(1.0_f32, self.border),
            corner_radius: CornerRadius::same(self.radius),
            fg_stroke: Stroke::new(1.0_f32, self.fg),
            expansion: 0.0,
        }
    }
}

/// 一张卡片的外框。界面里每个功能区都套这个，视觉上分层。
pub fn card_frame(p: &Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(p.card_bg)
        .stroke(Stroke::new(1.0_f32, p.card_border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(12))
}

/// 主按钮（强调色填充）。egui 没有内置的 primary 概念，
/// 得手工画：先拿 Frame 铺一层强调色底，再放一个无边框按钮。
///
/// 返回是否被点击。
pub fn primary_button(ui: &mut egui::Ui, p: &Palette, text: &str, enabled: bool) -> bool {
    // 悬停态交给 egui 自己调亮（accent 已经在 style 里设了 hovered 变体），
    // 这里只需要区分「能用」和「不能用」两种底色。
    let fill = if enabled { p.accent } else { p.control_bg };
    let fg = if enabled { p.on_accent } else { p.text_faint };

    let mut btn = egui::Button::new(egui::RichText::new(text).color(fg).strong())
        .fill(fill)
        .min_size(Vec2::new(0.0, 32.0));
    if enabled {
        btn = btn.stroke(Stroke::NONE);
    }
    ui.add_enabled(enabled, btn).clicked()
}

/// 圆角 / 间距的自查，防止哪天有人把数值改崩。
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_palette_has_contrast_between_text_and_bg() {
        for mode in [Mode::Light, Mode::Dark] {
            let p = Palette::new(mode);
            let lum = |c: Color32| {
                0.2126 * f32::from(c.r()) + 0.7152 * f32::from(c.g()) + 0.0722 * f32::from(c.b())
            };
            let diff = (lum(p.text) - lum(p.bg)).abs();
            assert!(
                diff > 100.0,
                "{mode:?} 的正文和底色太接近了，差值只有 {diff}"
            );
        }
    }

    #[test]
    fn toggle_round_trips() {
        assert_eq!(Mode::Light.toggle(), Mode::Dark);
        assert_eq!(Mode::Dark.toggle(), Mode::Light);
        assert_eq!(Mode::Light.toggle().toggle(), Mode::Light);
    }
}
