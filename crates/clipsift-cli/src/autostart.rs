//! Windows 开机自启。
//!
//! 用的是 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`，不是启动文件夹的
//! `.lnk`，理由有三条：
//!
//! 1. **不需要管理员权限**。HKCU 是当前用户自己的 hive，写自己的启动项天经地义；
//!    HKLM 那个才要提权，我们不碰。
//! 2. **不依赖 COM**。创建 `.lnk` 得走 `WScript.Shell` 或者手写 IShellLink COM，
//!    任一环节坏掉就是"看起来设了其实没生效"。写一个注册表字符串没有中间层。
//! 3. **用户仍然能管**。任务管理器的"启动"标签页会把 Run 项列出来，可以禁用，
//!    和我们自己写的 `autostart disable` 是同一个效果。
//!
//! 一个坑：`clipsift.exe` 是 CONSOLE 子系统程序（PE OptionalHeader 里
//! `Subsystem = 3`）。直接从 Run 项启动会弹一个黑窗口。所以自启项里带
//! `watch --background`，让守护进程起来后自己把控制台窗口藏掉。

use std::path::Path;

/// 注册表里这个值的名字。用产品名而不是 exe 名，用户一眼认得出是谁。
pub const REG_VALUE_NAME: &str = "ClipSift";

/// `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// 拼出要写进 Run 项的命令行。
///
/// 路径**必须**带引号：Windows 的 `Program Files`、以及用户名里带空格的情况
/// 太常见了，不加引号会被按空格切开，变成"找不到 clipsift.exe"。
pub fn command_line(exe: &Path, extra_args: &[String]) -> String {
    let mut s = format!("\"{}\"", exe.display());
    s.push_str(" watch --background");
    for a in extra_args {
        s.push(' ');
        // 参数里也可能有空格（比如自定义配置文件路径），照样加引号。
        if a.contains(' ') {
            s.push('"');
            s.push_str(a);
            s.push('"');
        } else {
            s.push_str(a);
        }
    }
    s
}

/// 判断一条 Run 项是不是我们写的。
///
/// 只比对"可执行文件名 + watch 子命令"，不比对完整路径：用户可能换了安装位置，
/// 旧路径已经失效，那也应该能被 `autostart disable` 清掉，而不是因为"路径不匹配"
/// 就假装不存在、留下一枚指向空气的启动项。
pub fn is_ours(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("clipsift") && lower.contains("watch")
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
        HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ,
    };

    /// 把 `&str` 转成 Windows 要的以 NUL 结尾的 UTF-16。
    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn open_run_key(access: u32) -> Result<HKEY, String> {
        let sub = wide(RUN_KEY);
        let mut key: HKEY = std::ptr::null_mut();
        // 0 表示"用默认的访问权限"。Run 键通常早就存在，不用 CreateKey。
        let rc = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, sub.as_ptr(), 0, access, &mut key) };
        if rc != ERROR_SUCCESS {
            return Err(format!(
                "打不开注册表键 HKCU\\{RUN_KEY}（错误码 {rc}）。\
                 这个键一般由系统自带，如果被删了，先手动跑一次任意一个开机自启软件把它建回来。"
            ));
        }
        Ok(key)
    }

    /// 写入（或覆盖）自启项。
    pub fn enable(cmd: &str) -> Result<(), String> {
        let key = open_run_key(KEY_SET_VALUE)?;
        let name = wide(REG_VALUE_NAME);
        // REG_SZ 的长度写**字节数**，且要包含结尾的 NUL。
        let data: Vec<u16> = wide(cmd);
        let bytes = (data.len() * std::mem::size_of::<u16>()) as u32;
        let rc = unsafe {
            RegSetValueExW(
                key,
                name.as_ptr(),
                0,
                REG_SZ,
                data.as_ptr() as *const u8,
                bytes,
            )
        };
        unsafe { RegCloseKey(key) };
        if rc != ERROR_SUCCESS {
            return Err(format!("写注册表失败（错误码 {rc}）"));
        }
        Ok(())
    }

    /// 读当前自启项。`Ok(None)` 表示没设。
    pub fn query() -> Result<Option<String>, String> {
        let key = open_run_key(KEY_QUERY_VALUE)?;
        let name = wide(REG_VALUE_NAME);

        // 先问长度。数据可能被别的程序改过，所以按"两次调用"的标准姿势来。
        let mut len: u32 = 0;
        let rc = unsafe {
            RegQueryValueExW(
                key,
                name.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut len,
            )
        };
        if rc == ERROR_FILE_NOT_FOUND {
            unsafe { RegCloseKey(key) };
            return Ok(None);
        }
        if rc != ERROR_SUCCESS {
            unsafe { RegCloseKey(key) };
            return Err(format!("读注册表失败（错误码 {rc}）"));
        }

        let mut buf = vec![0u8; len as usize];
        let mut len2 = len;
        let rc = unsafe {
            RegQueryValueExW(
                key,
                name.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                buf.as_mut_ptr(),
                &mut len2,
            )
        };
        unsafe { RegCloseKey(key) };
        if rc != ERROR_SUCCESS {
            return Err(format!("读注册表失败（错误码 {rc}）"));
        }

        // 缓冲区是字节，UTF-16 按 u16 重新解释，去掉结尾 NUL。
        let u16s: Vec<u16> = buf
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        Ok(Some(String::from_utf16_lossy(&u16s)))
    }

    /// 删掉自启项。本来就没有也算成功（幂等）。
    pub fn disable() -> Result<(), String> {
        let key = open_run_key(KEY_SET_VALUE)?;
        let name = wide(REG_VALUE_NAME);
        let rc = unsafe { RegDeleteValueW(key, name.as_ptr()) };
        unsafe { RegCloseKey(key) };
        if rc == ERROR_SUCCESS || rc == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(format!("删注册表项失败（错误码 {rc}）"))
        }
    }

    /// 藏掉当前进程的控制台窗口。
    ///
    /// 只在 `watch --background` 时调用。手动 `clipsift watch` 不能藏，
    /// 否则用户在控制台里看不到任何输出、也找不到怎么关。
    pub fn hide_console_window() -> Result<(), String> {
        use windows_sys::Win32::System::Console::GetConsoleWindow;
        use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};

        let hwnd = unsafe { GetConsoleWindow() };
        if hwnd.is_null() {
            // 没有控制台（比如从 GUI 宿主里启动），没什么可藏的，不算错。
            return Ok(());
        }
        unsafe { ShowWindow(hwnd, SW_HIDE) };
        Ok(())
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    const MSG: &str = "开机自启目前只支持 Windows";

    pub fn enable(_cmd: &str) -> Result<(), String> {
        Err(MSG.to_string())
    }

    pub fn query() -> Result<Option<String>, String> {
        Err(MSG.to_string())
    }

    pub fn disable() -> Result<(), String> {
        Err(MSG.to_string())
    }

    pub fn hide_console_window() -> Result<(), String> {
        Err(MSG.to_string())
    }
}

pub use imp::{disable, enable, hide_console_window, query};

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(s)
    }

    #[test]
    fn command_line_quotes_the_exe_path() {
        let cmd = command_line(&p(r"C:\Program Files\ClipSift\clipsift.exe"), &[]);
        // 路径带空格，必须整体加引号
        assert_eq!(
            cmd,
            r#""C:\Program Files\ClipSift\clipsift.exe" watch --background"#
        );
    }

    #[test]
    fn command_line_appends_extra_args() {
        let cmd = command_line(
            &p(r"C:\tools\clipsift.exe"),
            &["-c".into(), r"C:\my cfg.json".into()],
        );
        assert_eq!(
            cmd,
            r#""C:\tools\clipsift.exe" watch --background -c "C:\my cfg.json""#
        );
    }

    #[test]
    fn is_ours_matches_clipsift_watch() {
        assert!(is_ours(r#""C:\a\clipsift.exe" watch --background"#));
        assert!(is_ours(
            r#""D:\Other Place\ClipSift.exe" watch --background"#
        ));
        // 大小写不同也算（Windows 路径大小写不敏感）
        assert!(is_ours(r#""C:\A\CLIPSIFT.EXE" WATCH --background"#));
    }

    #[test]
    fn is_ours_rejects_unrelated_entries() {
        assert!(!is_ours(r#""C:\other\somewatch.exe" --watch"#));
        assert!(!is_ours(r#""C:\a\clipsift.exe" once"#));
        assert!(!is_ours(""));
    }
}
