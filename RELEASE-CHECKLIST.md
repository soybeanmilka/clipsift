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
