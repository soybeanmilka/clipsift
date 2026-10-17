//! ClipSift 核心净化引擎。
//!
//! 设计目标只有一个：**把「看起来一样、实际不一样」的文本变成真正一样**。
//!
//! 典型痛点来源：
//! - 从网页 / PDF / AI 聊天窗口复制，混进 `U+200B` 零宽空格 → 编译报 `stray '\342'`
//! - 从 Excel / ERP 导出，单元格里有不间断空格 → `VLOOKUP` 死活匹配不上
//! - 从 Word 复制，全角标点 + 弯引号 → 代码里全是语法错误
//! - 复制的是恶意构造的文本，藏着 `U+E0000` 隐形标签（Unicode Tag）或 bidi 覆盖字符
//!
//! 每一步都是**幂等**的：同一段文本跑两次，结果不变。这是 `tests/property.rs` 里
//! 用一万条随机脏文本验证过的，不是嘴上说说。
//!
//! 唯一的例外是 `Plan::merge_hard_returns`（合并硬回车）。那条规则的定义本身就
//! 和幂等矛盾——"单换行变空格"归一完，下一轮就分不清哪些换行是段落、哪些是折行了。
//! 所以它默认关闭，并且 [`Plan::is_idempotent`] 会返回 `false`。
//! 剪贴板守护进程用默认配置（不带 merge），所以守护进程永远是幂等的。

pub mod chars;
pub mod rules;

/// 快速判断一段文本是否需要净化（线性扫描、零分配），给剪贴板守护进程做前置检查。
pub fn needs_cleanup(text: &str) -> bool {
    text.chars().any(chars::is_suspicious)
}

/// 统计文本中各"可疑字符"的出现次数，按次数降序。
///
/// 给 GUI 的"检查报告"用：让人一眼看出这段文本脏在哪，而不是只看到结果。
pub fn inspect(text: &str) -> Vec<(char, usize)> {
    use std::collections::HashMap;
    let mut counts: HashMap<char, usize> = HashMap::new();
    for c in text.chars() {
        if chars::is_suspicious(c) {
            *counts.entry(c).or_insert(0) += 1;
        }
    }
    let mut v: Vec<(char, usize)> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v
}

pub use rules::{apply, Plan, Report, Rule};
