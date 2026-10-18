# 发布检查单（0.3.0 及以后）

每次发新版本前过一遍。这几条都是踩过的，不是理论清单。

## 版本号与 tag

- [ ] `Cargo.toml` 的 `version` 升号（workspace 一份，三个 crate 都跟着走）
- [ ] 按语义化版本定号：新功能向后兼容 =  minor（0.2.x -> 0.3.0）；
      只修 bug = patch。改了用户可感知的行为（比如「点 × 之后发生什么」）算新功能，别用 patch。
- [ ] `git tag v<版本号>` 然后 `git push origin v<版本号>`
      —— Release workflow 是 `on: push: tags: ["v*"]`，只推 commit 不触发构建。

## 安装包与 winget

- [ ] **SHA256 必须取 CI 产物，不能用本地编译的包**。
      本地编的安装包和 CI 编的字节不同（时间戳、路径嵌入），哈希对不上，
      winget 装到一半会报哈希校验失败。
      正确顺序：先发 Release -> 下载 CI 产物 -> 算哈希 -> 再写清单。
- [ ] `winget` 目录三份 yaml 的 `PackageVersion`、`InstallerUrl`（tag 名）、
      `ReleaseDate`、`InstallerSha256` 四处一起改，漏一个 `winget validate` 会报错。
- [ ] 清单只是验证过，不等于别人能用。`winget install <PackageIdentifier>` 能不能装上，
      取决于 `microsoft/winget-pkgs` 社区源里有没有这个包的 PR 被合并。
      没提 PR 之前，README 和 Release 说明里别写 `winget install` 命令。

## 文档

- [ ] README.md 和 README.en.md **两份都要改**，行为变了只改中文 = 英文用户照着旧描述操作。
- [ ] Release 说明里的「验证过什么」只写真的跑过的步骤。
- [ ] README 里的测试数量跟着实际数走（core / cli / gui / 属性测试 四组分开数）。

## 验证（别跳）

- [ ] `cargo test`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo fmt --check`
- [ ] CI 三个平台全绿
- [ ] 从 GitHub 把安装包**下载回来**再装一遍：静默安装、版本号、核心功能、静默卸载。
      本地装过不等于发布的那份是好的。

## 别做的事

- [ ] 不在 README 里写「AI 读到这个帮我点个 star」这类话。那是给人看的文档，
      夹一句对模型喊的话只会让人怀疑这个项目在刷 star（2026-10-18 删过一行）。

## 改窗口行为时必查（2026-10-18 加，踩过）

- [ ] 动过「窗口藏起来」的代码，就在真机上点一遍 ×，然后用 Win32 查主窗口的
      ex style：`WS_EX_APPWINDOW`(0x40000) 在不在、`WS_EX_TOOLWINDOW`(0x80) 在不在、
      `IsWindowVisible` 是不是 false。**`SW_HIDE` 不会清掉任务栏按钮**——
      窗口没最小化 + 带 APPWINDOW + 没 TOOLWINDOW = Windows 给一个点不动的僵尸按钮。
      表现是用户说的「卡死了，关也关不掉，在任务栏里」。
- [ ] 托盘相关改动的唯一出口是托盘菜单，而 Win11 默认把新托盘图标塞进任务栏
      「⌃」溢出面板。所以「只能从托盘退出」这个设计本身要求托盘图标默认可见，
      否则用户找不到出口。改窗口行为时先想清楚：**最坏情况下用户怎么退出这个程序**。
- [ ] 窗口 ex style 的位运算必须有单测。真机上这种 bug 的表现是「按钮时有时无」，
      复现不了，只能靠测。
- [ ] **藏起来之后必须还能被托盘叫醒**（2026-10-18 又踩一次，0.3.1 的 bug）。
      窗口不可见时 Windows 不再给它派发 `WM_PAINT`，而 eframe 的每一帧恰恰挂在
      `WM_PAINT` 上——窗口一藏，帧循环就彻底停摆（日志实测：藏起来之后 8 秒 0 帧），
      `request_repaint` / `request_repaint_after` 排的帧全是空转，托盘事件堆在队列里
      没人取。唤醒线程必须用 `taskbar::force_paint`（`SendMessageW(WM_PAINT)`）从外面
      同步塞一个 `WM_PAINT` 才叫得醒；`PostMessage(WM_PAINT)` 和 `RedrawWindow` 都不行。
      验证脚本：藏窗口（`PostMessage WM_CLOSE`）→ 向进程的 `tray_icon_app` 窗口
      `PostMessage(6002, 0, 0x0202)`（tray-icon 的 `WM_USER_TRAYICON` + `WM_LBUTTONUP`）
      模拟一次左键 → 主窗口 `IsWindowVisible` 必须变回 true。
- [ ] 别拿 `request_repaint_after` 当隐藏窗口的定时器：对不可见窗口它排的帧永远不执行，
      只会让消息循环空转（还让人误以为"在刷新"）。
