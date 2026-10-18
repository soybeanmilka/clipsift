//! 任务栏按钮的显隐。
//!
//! 为什么需要这个模块：点 × 把窗口藏到托盘时，我们发的是
//! `ViewportCommand::Visible(false)`，winit 把它翻译成 `ShowWindow(SW_HIDE)`，
//! 只清 `WS_VISIBLE`。可窗口既没被最小化，ex style 里也还带着
//! `WS_EX_APPWINDOW`、没有 `WS_EX_TOOLWINDOW` —— Windows 就照样给它分配一个
//! 任务栏按钮。于是得到一个**看不见也点不动的僵尸按钮**：
//! 屏幕上没窗口，任务栏上有个按钮，点它 Windows 试图激活一个不可见窗口，
//! 什么都不会发生（用户视角就是“卡死了，关也关不掉”）。
//!
//! 唯一的出口本来是托盘菜单的「退出 ClipSift」，但 Win11 默认把新注册的托盘
//! 图标塞进任务栏那个「⌃」溢出面板，用户根本看不到图标 —— 程序就把自己锁进了
//! 只能从一个默认不可见的入口退出的状态。
//!
//! 修法：拿到真正的窗口句柄，自己改 ex style。egui 的 `ViewportCommand` 里
//! 没有 SkipTaskbar 这一项，winit 0.30 的公开 API 也没有 `set_skip_taskbar`
//!（它只在 platform_impl 内部），所以运行时动态切换只能自己来。
//! `eframe::Frame` 实现了 `raw_window_handle::HasWindowHandle`，HWND 就从这儿拿。
//!
//! 为什么用 `WS_EX_TOOLWINDOW` 而不是最小化：工具窗口不占任务栏，
//! 而且不需要动窗口的可见性语义。最小化虽然也有任务栏按钮，但那个按钮
//! 会和「收进任务栏」的降级路径混在一起，用户分不清自己是哪一种状态。

#[cfg(windows)]
mod imp {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    /// 扩展窗口样式在 `SetWindowLongPtrW` 里的索引。
    const GWL_EXSTYLE: i32 = -20;
    /// 不算任务栏的窗口。设上它 Windows 就不再给这个窗口分配任务栏按钮。
    pub(crate) const WS_EX_TOOLWINDOW: isize = 0x0000_0080;
    /// 普通应用窗口。任务栏按钮的归属就写在它身上。
    pub(crate) const WS_EX_APPWINDOW: isize = 0x0004_0000;

    /// SetWindowPos 的标志：不动位置、不动大小、不动 Z 序，但让框架重算。
    const SWP_FLAGS: u32 = 0x0002 | 0x0001 | 0x0004 | 0x0020;

    /// 从 eframe 的 Frame 里取出原生窗口句柄。
    ///
    /// 拿不到就返回 None：那时候所有调用方都应该退化成“只改可见性”的老行为，
    /// 而不是 panic——一个托盘工具不值得因为拿不到句柄就起不来。
    pub fn hwnd(frame: &eframe::Frame) -> Option<isize> {
        let handle = frame.window_handle().ok()?;
        match handle.as_raw() {
            RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
            _ => None,
        }
    }

    /// 算出目标 ex style。抽成纯函数是为了能单测——位运算写错了的话，
    /// 表现就是"任务栏按钮有时在有时不在"，这种 bug 在真机上极难复现。
    pub(crate) fn target_style(current: isize, show: bool) -> isize {
        // 两个位都要按需摆平，不能只动一个：
        // 藏窗口 = 清 APPWINDOW + 设 TOOLWINDOW（按钮消失）；
        // 唤回窗口 = 两个位都清（Windows 会给可见的普通窗口自己补 APPWINDOW）。
        // 曾经写成"只清 APPWINDOW"，结果藏过一次再唤回，窗口仍是工具窗口，
        // 任务栏按钮再也不出现——往返测试抓到的。
        let mut next = current & !WS_EX_APPWINDOW & !WS_EX_TOOLWINDOW;
        if !show {
            next |= WS_EX_TOOLWINDOW;
        }
        next
    }

    /// 设置或清除「这个窗口占任务栏按钮」。返回设置后的 ex style，失败返回 None。
    fn set_taskbar(hwnd: isize, show: bool) -> Option<isize> {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos,
        };
        unsafe {
            let ex = GetWindowLongPtrW(hwnd as *mut core::ffi::c_void, GWL_EXSTYLE);
            if ex == 0 {
                return None;
            }
            let next = target_style(ex, show);
            if next == ex {
                return Some(ex);
            }
            let prev = SetWindowLongPtrW(hwnd as *mut core::ffi::c_void, GWL_EXSTYLE, next);
            if prev == 0 {
                return None;
            }
            // 样式改了要告诉 Windows 重算，否则任务栏按钮不会立刻消失。
            SetWindowPos(
                hwnd as *mut core::ffi::c_void,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_FLAGS,
            );
            Some(next)
        }
    }

    /// 藏窗口时调用：把任务栏按钮去掉。
    pub fn hide_button(hwnd: isize) -> Option<isize> {
        set_taskbar(hwnd, false)
    }

    /// 唤回窗口时调用：把任务栏按钮还回来。
    ///
    /// 必须还——用户从托盘唤回窗口之后，程序又变成“看得见的普通应用”，
    /// 这时候任务栏上有个按钮是正常且符合预期的。
    pub fn show_button(hwnd: isize) -> Option<isize> {
        set_taskbar(hwnd, true)
    }
}

#[cfg(not(windows))]
mod imp {
    /// 非 Windows 平台没有任务栏按钮这回事，全都是空操作。
    pub fn hwnd(_frame: &eframe::Frame) -> Option<isize> {
        None
    }
    pub fn hide_button(_hwnd: isize) -> Option<isize> {
        None
    }
    pub fn show_button(_hwnd: isize) -> Option<isize> {
        None
    }
}

pub use imp::{hide_button, hwnd, show_button};

#[cfg(all(test, windows))]
mod tests {
    use super::imp::{target_style, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW};

    /// winit 0.30 给本程序的主窗口算出来的真实 ex style（实测 0x00040110）。
    const REAL: isize = 0x0004_0110;

    #[test]
    fn hiding_clears_appwindow_and_sets_toolwindow() {
        let got = target_style(REAL, false);
        assert_eq!(got & WS_EX_APPWINDOW, 0, "APPWINDOW 必须清掉");
        assert_eq!(
            got & WS_EX_TOOLWINDOW,
            WS_EX_TOOLWINDOW,
            "TOOLWINDOW 必须设上"
        );
    }

    #[test]
    fn showing_clears_both_bits() {
        // 唤回窗口时两个位都该没有：Windows 会自己给可见的普通窗口补 APPWINDOW。
        // 实测确认过——只清 TOOLWINDOW 而留着 APPWINDOW 也能显示按钮，
        // 但两个位都清掉之后 Windows 重算的结果更稳定。
        let got = target_style(REAL, true);
        assert_eq!(got & WS_EX_APPWINDOW, 0);
        assert_eq!(got & WS_EX_TOOLWINDOW, 0);
    }

    #[test]
    fn unrelated_bits_survive() {
        // WS_EX_ACCEPTFILES(0x10) 和 WS_EX_WINDOWEDGE(0x100) 是 winit 自己加的，
        // 不能被我们碰掉——碰掉了拖文件进窗口之类的特性会莫名失效。
        let got = target_style(REAL, false);
        assert_eq!(got & 0x10, 0x10, "ACCEPTFILES 要保住");
        assert_eq!(got & 0x100, 0x100, "WINDOWEDGE 要保住");
    }

    #[test]
    fn hiding_twice_is_stable() {
        // 幂等：连着点两次 × 不能让样式继续漂。
        let once = target_style(REAL, false);
        assert_eq!(target_style(once, false), once);
    }

    #[test]
    fn hide_then_show_round_trips_to_a_plain_window() {
        let hidden = target_style(REAL, false);
        let shown = target_style(hidden, true);
        assert_eq!(shown, REAL & !WS_EX_APPWINDOW);
    }
}
