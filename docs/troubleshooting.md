# 常见报错速查：这些错可能只是复制粘贴时混进了看不见的字符

这份表按**报错原文**索引。你搜到的报错如果在这里，八成不是代码写错了。

## C / C++

| 报错 | 原因 | 处理 |
| --- | --- | --- |
| `error: stray '\342' in program` | 混进了 `U+200B` 零宽空格（UTF-8 首字节 `0xE2` = 八进制 `342`） | `clipsift` 删掉 |
| `error: stray '\302' in program` | 混进了 `U+00A0` 不间断空格（`0xC2` = `302`） | 归一成普通空格 |
| `stray '\343'` / `stray '\300'` | 全角标点 / 其他多字节字符 | 转半角 |
| `expected unqualified-id before '{' token` 且代码看着没错 | 行首有隐形字符 | 删掉 |
| `warning: missing terminating " character` | 弯引号 `“”` 被当成双引号 | 转 ASCII |

**为什么是八进制数字**：GCC 报 stray 时会把那个字节按八进制打出来。
`U+200B` 的 UTF-8 编码是 `E2 80 8B`，首个字节 `E2` 十六进制 = `342` 八进制。
所以看到 `\342` 几乎可以确诊是零宽空格。

## Python

| 报错 | 原因 | 处理 |
| --- | --- | --- |
| `IndentationError: unexpected indent` | 缩进是 `U+00A0` 而不是空格 | 归一成 ASCII 空格（宽度不变） |
| `TabError: inconsistent use of tabs and spaces` | 混了全角空格 `U+3000` | 归一 |
| `SyntaxError: invalid character '，' (U+FF0C)` | 中文全角逗号 | 转半角（`--fullwidth`） |
| `SyntaxError: invalid character '“' (U+201C)` | 弯引号 | 转 ASCII |
| `SyntaxError: invalid non-printable character U+200B` | 零宽空格 | 删掉 |

## JavaScript / TypeScript

| 报错 | 原因 | 处理 |
| --- | --- | --- |
| `SyntaxError: Invalid or unexpected token` | 字符串里有 `U+200B` | 删掉 |
| `SyntaxError: Unexpected character '\u00a0'` | 不间断空格 | 归一 |
| `TS1127: Invalid character` | 弯引号 / 全角标点 | 转 ASCII |

## Shell / 命令行

| 现象 | 原因 | 处理 |
| --- | --- | --- |
| `command not found` 但命令看着完全正确 | 空格是 `U+00A0` | 归一 |
| `bash: $'\302\240xxx': command not found` | 同上，bash 打出八进制字节 | 归一 |
| `No such file or directory` 但文件明明存在 | 路径里有隐形字符 | 删掉 |

## Excel / 数据库 / 数据

| 现象 | 原因 | 处理 |
| --- | --- | --- |
| `VLOOKUP` / `XLOOKUP` 匹配不上 | 单元格末尾有 `U+00A0` | 归一成普通空格 |
| `SUM` 结果是 0 但看着有数字 | 数字前有 `U+200B` | 删掉 |
| MySQL `Incorrect string value` | 混进了 `U+2028` 等 | 删掉 |
| 唯一索引冲突但两条记录看着一样 | 一个带 `U+200B` 一个不带 | 删掉 |

## 安全相关

| 现象 | 原因 | 处理 |
| --- | --- | --- |
| 源码审查过但行为诡异 | `U+E0000-E007F` Unicode Tag 隐形指令 | 删掉 |
| 代码逻辑和看到的顺序不一致 | bidi 覆盖符 `U+202A-202E`、`U+2066-2069`（Trojan Source） | 删掉 |
| 变量名看着一样但比较不相等 | 西里尔字母同形字 | 部分处理（见 confusables 规则） |

## 怎么确认

```bash
clipsift inspect < 有问题的文件
```

会列出每个可疑字符的码点和出现次数。如果输出"干净"，那问题不在字符上。

## 一句话

从**飞书文档、钉钉文档、Notion、Word、PDF、AI 聊天窗口**复制代码到编辑器，
是这些字符最主要的来源。富文本剪贴板为了保留格式，会带上一堆不可见元字符。
