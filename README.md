# ClipSift

> 从复制粘贴里筛掉隐形字符：零宽空格、伪空格、弯引号、全角字母……

**中文说明见 [README.zh-CN.md](README.zh-CN.md)** | [English](README.md)

**复制网页/AI 聊天/PDF 里的 C++ 代码，编译报 `stray '\342' in program`？**
**从飞书文档复制 Python 代码，`IndentationError`？**
**Excel 导出的表格 `VLOOKUP` 死活匹配不上那几个看起来一模一样的数字？**

不是你的错，是文本里混进了看不见的字符。ClipSift 负责把它筛掉。

```
复制 → 自动净化 → 粘贴
       ^^^^^^^^
       ClipSift 在这里干活
```

## 为什么需要它

`U+200B` 零宽空格（ZWSP）、`U+00A0` 不间断空格（NBSP）、`U+2028` 行分隔符、
`U+FEFF` BOM、`U+201C/201D` 弯引号……这些字符**肉眼看和正常文本完全一样**，
但它们不是同一个字节。

| 场景 | 表现 | 真实原因 |
| --- | --- | --- |
| 网页复制 C++ 到 IDE | `error: stray '\342' in program` | 行里混进了 `U+200B` |
| 飞书/Notion 复制 Python | `IndentationError: unexpected indent` | 缩进是 NBSP 而不是空格 |
| Word 复制代码 | 满屏语法错误 | 中文全角标点 + 弯引号 |
| Excel/ERP 导出 CSV | `VLOOKUP` 匹配不上 | 单元格尾部有 `U+00A0` |
| 命令行莫名报错 | `command not found` 但看着一模一样 | 复制了 `U+00A0` 当空格 |
| 被恶意文本钓鱼 | 源码里藏着看不见的指令 | `U+E0000` Unicode Tag / bidi 覆盖 |

编程助手、AI 聊天窗口、飞书文档的富文本剪贴板，是这类字符的高发区。

- 按报错原文查 → [常见报错速查表](docs/troubleshooting.md)
- 飞书这个具体场景 → [从飞书复制代码为什么老是报错](docs/feishu-copy-paste.md)

## 安装

### 方式一：下载可执行文件（推荐）

去 [Releases](https://github.com/soybeanmilka/clipsift/releases) 下载对应平台的文件，
解压后把 `clipsift` 放到 `PATH` 里的任意目录（Windows 推荐 `C:\Users\<你>\.cargo\bin`
或任意你习惯的位置）。

Windows 单文件 exe，无需安装任何运行库。

### 方式二：从源码编译

```bash
cargo install --path crates/clipsift-cli
```

需要 Rust 1.74+。

## 用法

### 最常用：常驻后台，自动净化剪贴板

```bash
clipsift watch
```

跑起来后不用管它，你**每次复制**它都会检查一遍剪贴板，脏了就原地洗干净。
之后再粘贴就是干净的。Ctrl+C 退出。

开机自启（Windows）：把快捷方式丢进
`shell:startup`（Win+R 输入即可打开该文件夹）。

### 一次性净化文件

```bash
# 管道
cat dirty.txt | clipsift > clean.txt

# 或者直接指定文件
clipsift -f dirty.txt -o clean.txt
```

### 先看看脏在哪

```bash
clipsift inspect < dirty.txt
```

```
码点         字符       次数       备注
--------------------------------------------------------
U+200B     ·        3        零宽空格
U+00A0     ·        2        伪空格
U+201C     ·        1        弯引号
U+201D     ·        1        弯引号

共 7 处可疑字符。
```

### 净化顺便看统计

```bash
clipsift --report < dirty.txt
```

```
清除 7 个隐形字符，减少 4 字符
  U+200B  3 次  零宽空格
  U+00A0  2 次  伪空格
  U+201C  1 次  弯引号
  U+201D  1 次  弯引号
```

`--report` 只打印报告，不输出正文。想同时要正文就别加这个参数。

### 只净化一次当前剪贴板

```bash
clipsift once
```

## 规则

默认规则都经过验证，能覆盖 95% 的场景。

| 键名 | 名称 | 默认 | 干什么 |
| --- | --- | --- | --- |
| `invisible` | 删除隐形字符 | 开 | 删 `U+200B/200C/200D/2060/FEFF`、bidi 控制符、Unicode Tag |
| `spaces` | 伪空格归一 | 开 | `U+00A0`、`U+2000-200A`、`U+3000` 等 → 普通空格 |
| `confusables` | 同形字转 ASCII | 开 | 弯引号 `“”‘’`、破折号 `—–`、省略号 `…` → ASCII |
| `fullwidth` | 全角转半角 | 关 | `！`→`!`、`Ａ`→`A`（会误伤中文排版，默认关） |
| `merge` | 合并硬回车 | 关 | PDF 抠正文用，**非幂等**，见下 |
| `collapse-ws` | 压缩连续空白 | 开 | 行内多个空格 → 一个（**不动缩进宽度**） |
| `trim` | 去行尾空白 | 开 | 删每行末尾的空格/tab |
| `control` | 删控制字符 | 开 | 删 `\0` 等，保留 `\n\r\t` |

### 关掉某条规则

```bash
clipsift --off fullwidth --off merge < dirty.txt
clipsift --list-rules        # 看所有键名
```

### 配置文件

```bash
clipsift init > clipsift.json   # 生成默认配置
# 改完再喂回去
clipsift -c clipsift.json < dirty.txt
```

```json
{
  "invisible": true,
  "normalize_spaces": true,
  "confusables": true,
  "fullwidth_ascii": false,
  "merge_hard_returns": false,
  "collapse_whitespace": true,
  "trim_trailing": true,
  "strip_control": true,
  "custom": []
}
```

自定义正则规则：

```json
{
  "custom": [
    { "name": "去多余空行", "pattern": "\\n{3,}", "replacement": "\n\n", "enable": true }
  ]
}
```

注意：键名拼错会**报错**而不是静默忽略。

## 设计取舍

### 缩进是数据，不是空白

`collapse-ws` 只压**行内**的连续空白，绝不碰行首缩进，连缩进**宽度**都保住。
早期版本顺手 trim 了每行两侧，结果从网页复制来的 Python 代码全部顶格——
那是把功能做成了 bug。

### 「合并硬回车」为什么不幂等

`merge` 的定义本身就和幂等矛盾：空行 = 段落边界（压成一个 `\n`），
单换行 = 排版折行（变成空格）。它自己产生的 `\n` 在第二轮会被当成折行吃掉。

```
"a⏎⏎b"  --merge-->  "a⏎b"  --merge-->  "a b"
```

所以它默认关闭，`clipsift watch` 用它时会警告。它是**一次性变换**，
只适合"把 PDF 正文摊平成一段"这种场景。

### 会「删东西」的规则排在会「整形」的规则前面

删掉一个控制字符会让两个原本不相邻的字符贴在一起，凭空造出新的"行尾空格"。
如果删字符的规则排在最后，这些新暴露的空白要等到**下一轮**才被处理——
不幂等。所以 `strip_control` 排在 `collapse-ws`/`trim` 之前。

### 守护进程用轮询而不是事件钩子

Windows 的 `AddClipboardFormatListener` 需要窗口句柄 + 消息循环，
为了监听剪贴板拉一个隐藏窗口不值得。200ms 轮询人感知不到延迟，
CPU 开销在万分之一量级。

## 验证

```bash
cargo test
```

- 25 个单元测试覆盖每条规则和它们的组合
- 12 个单元测试覆盖配置文件与 `--off` 开关
- 7 个属性测试，含**一万条随机脏文本验证幂等性**
  （1 万条里跳过 merge 相关的部分，非 merge 计划要求 `apply(apply(x)) == apply(x)`）
- `cargo clippy --all-targets -- -D warnings` 无警告
- 端到端验证过：文件级两次净化 SHA256 一致、剪贴板守护进程自动生效

## 与其他方案对比

| 方案 | 自动净化 | 支持正则 | 跨平台 | 说明 |
| --- | --- | --- | --- | --- |
| **ClipSift** | ✅ 常驻监听 | ✅ | CLI 跨平台，监听目前限 Windows | 单文件、无依赖、规则透明 |
| Gremlins（VS Code 插件） | ❌ 只标红 | ❌ | ✅ | 只显示不修改 |
| Invisible AI Character Detector | ❌ 手动命令 | ❌ | ✅ | VS Code 内手动触发 |
| Ditto | ✅ 需配正则 | ✅ | Windows | 老牌剪贴板管理器，配置略重 |
| PureText | ✅ | ❌ | Windows | 只去富文本格式，不去零宽字符 |
| 手动 `sed` | ❌ | ✅ | ✅ | 每次都得记得跑 |

## 许可

MIT
