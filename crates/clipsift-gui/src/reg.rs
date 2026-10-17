//! 开机自启用的注册表读写。
//!
//! 和 CLI 里的 `autostart.rs` 是同一个思路，但这边只服务 GUI 的复选框：
//! 写当前 exe 一条，读一条，删一条。不处理"同名项被别人占了"之类的情况——
//! GUI 场景下用户点复选框就是想让 ClipSift 自启，覆盖是符合预期的。

#[cfg(windows)]
mod imp {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
        HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ,
    };

    /// Run 项的位置。写 HKCU 而不是 HKLM，这样不用管理员权限。
    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

    /// 启动项的值名。和 CLI 的 `autostart::REG_VALUE_NAME` 保持一致，
    /// 否则命令行开的自启在 GUI 里看不见（两个程序写两个值名会互相覆盖不掉）。
    pub const VALUE_NAME: &str = "ClipSift";

    /// 转成以 NUL 结尾的 UTF-16。Win32 的宽字符 API 全要这个格式。
    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    /// 打开 Run 项。`write` 决定要读权限还是写权限。
    fn open_run_key(write: bool) -> Result<HKEY, String> {
        let sub = wide(RUN_KEY);
        let access = if write {
            KEY_SET_VALUE
        } else {
            KEY_QUERY_VALUE
        };
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: 传进去的三个指针都指向本函数栈上仍然活着的值；sub 是 NUL 结尾的
        // UTF-16，符合 API 对 lpSubKey 的要求。成功后 key 由 RegCloseKey 释放。
        let rc = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, sub.as_ptr(), 0, access, &mut key) };
        if rc != ERROR_SUCCESS {
            return Err(format!("打开注册表 Run 项失败（错误码 {rc}）"));
        }
        Ok(key)
    }

    pub fn set_run_value(name: &str, command: &str) -> Result<(), String> {
        let key = open_run_key(true)?;
        let value_name = wide(name);
        // REG_SZ 要求带结尾 NUL，字节数含 NUL。
        let data = wide(command);
        let bytes = data.len() * 2;
        // SAFETY: key 由 open_run_key 保证有效；value_name 和 data 都是本函数内
        // 有效的 NUL 结尾 UTF-16 缓冲；bytes 精确描述了 data 的字节长度。
        let rc = unsafe {
            RegSetValueExW(
                key,
                value_name.as_ptr(),
                0,
                REG_SZ,
                data.as_ptr() as *const u8,
                bytes as u32,
            )
        };
        // SAFETY: key 是上面成功打开的句柄，只关一次。
        unsafe { RegCloseKey(key) };
        if rc != ERROR_SUCCESS {
            return Err(format!("写入开机自启失败（错误码 {rc}）"));
        }
        Ok(())
    }

    pub fn delete_run_value(name: &str) -> Result<(), String> {
        let key = open_run_key(true)?;
        let value_name = wide(name);
        // SAFETY: key 有效；value_name 是有效的 NUL 结尾 UTF-16。
        let rc = unsafe { RegDeleteValueW(key, value_name.as_ptr()) };
        // SAFETY: 同上，只关一次。
        unsafe { RegCloseKey(key) };
        // 本来就不存在也算成功——用户想要的"没有自启项"这个状态已经达成了。
        if rc != ERROR_SUCCESS && rc != ERROR_FILE_NOT_FOUND {
            return Err(format!("删除开机自启失败（错误码 {rc}）"));
        }
        Ok(())
    }

    /// 读回当前自启项。存在返回 `Some(命令行)`，不存在返回 `None`。
    pub fn query_run_value(name: &str) -> Option<String> {
        let key = open_run_key(false).ok()?;
        let value_name = wide(name);

        // 先查需要多大缓冲。大小按字节算。
        let mut size: u32 = 0;
        // SAFETY: key 有效；value_name 有效；size 是出参，指向栈上变量。
        let rc = unsafe {
            RegQueryValueExW(
                key,
                value_name.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if rc != ERROR_SUCCESS || size == 0 {
            // SAFETY: key 是有效句柄，只关一次。
            unsafe { RegCloseKey(key) };
            return None;
        }

        let mut buf = vec![0u8; size as usize];
        // SAFETY: buf 长度正好是上一步查到的 size，符合 API 要求。
        let rc = unsafe {
            RegQueryValueExW(
                key,
                value_name.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                buf.as_mut_ptr(),
                &mut size,
            )
        };
        // SAFETY: 同上。
        unsafe { RegCloseKey(key) };
        if rc != ERROR_SUCCESS {
            return None;
        }

        // 按 UTF-16 解读，去掉结尾的 NUL。
        let u16_len = buf.len() / 2;
        let wide_slice: Vec<u16> = (0..u16_len)
            .map(|i| u16::from_le_bytes([buf[i * 2], buf[i * 2 + 1]]))
            .collect();
        let end = wide_slice.iter().position(|&c| c == 0).unwrap_or(u16_len);
        String::from_utf16(&wide_slice[..end]).ok()
    }
}

#[cfg(windows)]
pub use imp::*;

#[cfg(not(windows))]
pub const VALUE_NAME: &str = "ClipSift";

#[cfg(not(windows))]
pub fn set_run_value(_name: &str, _command: &str) -> Result<(), String> {
    Err("开机自启目前只支持 Windows".to_string())
}

#[cfg(not(windows))]
pub fn delete_run_value(_name: &str) -> Result<(), String> {
    Err("开机自启目前只支持 Windows".to_string())
}

#[cfg(not(windows))]
pub fn query_run_value(_name: &str) -> Option<String> {
    None
}
