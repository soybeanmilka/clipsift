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

/// 统计文本中各"可疑字符"的出现次数。
///
/// 给 GUI 的"检查报告"用：让人一眼看出这段文本脏在哪，而不是只看到结果。
///
/// ## 排序为什么按严重程度、不按次数
///
/// 次数多的字符不一定更麻烦。全角逗号 `，` 在中文正文里完全正常，一次出现
/// 四个也无所谓；而全角减号 `－` 只出现一次就足以让 Python 报 SyntaxError。
/// 按次数排会把这些"一个就出人命"的字符压到列表底下，用户滚都懒得滚。
///
/// 所以排序键依次是：毁灭性（会让程序/查询直接报错）> 次数 > 码点。
pub fn inspect(text: &str) -> Vec<(char, usize)> {
    use std::collections::HashMap;
    let mut counts: HashMap<char, usize> = HashMap::new();
    for c in text.chars() {
        if chars::is_suspicious(c) {
            *counts.entry(c).or_insert(0) += 1;
        }
    }
    let mut v: Vec<(char, usize)> = counts.into_iter().collect();
    v.sort_by(|a, b| {
        severity(b.0)
            .cmp(&severity(a.0))
            .then(b.1.cmp(&a.1))
            .then(a.0.cmp(&b.0))
    });
    v
}

/// 一个可疑字符有多"要命"。数字越大越靠前。
///
/// 分三档，判断依据是"这个字符留在代码里会怎样"：
/// - 3 = 直接报错：隐形字符让编译器 `stray '\342'`，全角运算符/字母让语法分析直接挂
/// - 2 = 静默出错：弯引号、破折号让字符串比较不相等，程序能跑但结果是错的
/// - 1 = 只是难看：伪空格、全角标点在正文里不影响运行
fn severity(c: char) -> u8 {
    // 隐形字符永远第一档：它们连"看得见"都没有，用户根本无从排查。
    if chars::is_removable_invisible(c) {
        return 3;
    }
    match chars::fullwidth_to_ascii(c) {
        // 全角字母数字：变量名对不上、语法直接挂。
        Some(a) if a.is_ascii_alphanumeric() => 3,
        // 全角运算符：`－` 写进算式就是 SyntaxError。这一档必须和标点分开，
        // 否则"4 个全角逗号"会盖过"1 个全角减号"，把真正会炸的那行藏到列表底下。
        Some(a) if is_operator(a) => 3,
        // 全角标点：中文正文里是正常的，只有放进代码才是错。
        Some(_) => 2,
        None => {
            if chars::is_replaceable_confusable(c) {
                2
            } else {
                1
            }
        }
    }
}

/// 这个半角字符在代码里是"算数/语法"而不是"标符号"。
fn is_operator(c: char) -> bool {
    matches!(
        c,
        '-' | '+'
            | '*'
            | '/'
            | '\\'
            | '='
            | '<'
            | '>'
            | '&'
            | '|'
            | '!'
            | '^'
            | '~'
            | '%'
            | '`'
            | '?'
    )
}

pub use rules::{apply, Plan, Report, Rule};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspect_reports_fullwidth_characters() {
        // 全角减号是"一个就出人命"的字符，哪怕规则默认没开也得报出来。
        let found = inspect("x = \u{FF0D}1");
        assert_eq!(found, vec![('\u{FF0D}', 1)]);
    }

    #[test]
    fn inspect_puts_dangerous_chars_before_frequent_harmless_ones() {
        // 4 个全角逗号（正文里无所谓）不该盖过 1 个全角减号（会报语法错）。
        let text = "\u{FF0C}\u{FF0C}\u{FF0C}\u{FF0C}\u{FF0D}";
        let order: Vec<char> = inspect(text).into_iter().map(|(c, _)| c).collect();
        let minus = order.iter().position(|&c| c == '\u{FF0D}').unwrap();
        let comma = order.iter().position(|&c| c == '\u{FF0C}').unwrap();
        assert!(
            minus < comma,
            "全角减号应排在全角逗号前面，实际顺序 {order:?}"
        );
    }

    #[test]
    fn inspect_puts_invisible_chars_first() {
        let order: Vec<char> = inspect("abc\u{FF0C}\u{200B}")
            .into_iter()
            .map(|(c, _)| c)
            .collect();
        assert_eq!(order.first(), Some(&'\u{200B}'), "零宽空格应排第一");
    }

    #[test]
    fn clean_text_reports_nothing() {
        assert!(inspect("fn main() { let x = 1; }").is_empty());
    }
}
