//! 字符分类表。
//!
//! 这里只干一件事：给定一个字符，回答"它属于哪类脏东西"，以及"该变成什么"。
//! 规则集中在一处，净化逻辑 [crate::rules] 只负责编排顺序。
//!
//! ## 为什么不用 `char::is_control()` 一刀切
//!
//! 教材里清不可见字符常写 `c.is_control()`。这在 Windows 上会误伤：
//! 老文件里 `U+001A`(SUB) 常见作 EOF 标记，`U+001C`~`U+001E` 是文件分隔符。
//! 更重要的是 `is_control()` **漏掉**了最关键的零宽字符族——`U+200B`、
//! `U+2060`、`U+FEFF` 在 Unicode 里属于 `Cf`(Format) 类，`is_control()` 返回
//! `false`，所以那种写法根本拦不住粘贴污染。
//!
//! 所以这里显式列举 + 按区间判断，表是唯一事实来源。

/// 需要被**直接删除**的字符（没有任何可见等价物）。
///
/// 包含：
/// - 零宽字符族：ZWSP / ZWNJ / ZWJ / WORD JOINER
/// - 双向文本控制符：LRM / RLM / ALM，以及 LRE/RLE/PDF/LRO/RLO 与嵌入隔离符
/// - BOM（`U+FEFF`，粘贴开头最常见）
/// - 蒙古文元音分隔符（`U+180E`，旧版 Unicode 里当过空格）
/// - 不可见运算符（`U+2061`~`U+2064`）
/// - 私用区中已知的隐形标签区之外的 `U+FFF9`~`U+FFFB`（注释用锚点）
pub fn is_removable_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}' // SOFT HYPHEN 软连字符：复制 PDF 时残留，断行提示，肉眼不可见
        | '\u{180E}' // MONGOLIAN VOWEL SEPARATOR
        | '\u{200B}' // ZERO WIDTH SPACE  ← 用户这次踩的坑就是它
        | '\u{200C}' // ZERO WIDTH NON-JOINER
        | '\u{200D}' // ZERO WIDTH JOINER
        | '\u{200E}' // LEFT-TO-RIGHT MARK
        | '\u{200F}' // RIGHT-TO-LEFT MARK
        | '\u{202A}' // LEFT-TO-RIGHT EMBEDDING
        | '\u{202B}' // RIGHT-TO-LEFT EMBEDDING
        | '\u{202C}' // POP DIRECTIONAL FORMATTING
        | '\u{202D}' // LEFT-TO-RIGHT OVERRIDE
        | '\u{202E}' // RIGHT-TO-LEFT OVERRIDE  ← Trojan Source 攻击核心
        | '\u{2060}' // WORD JOINER
        | '\u{2061}' // FUNCTION APPLICATION
        | '\u{2062}' // INVISIBLE TIMES
        | '\u{2063}' // INVISIBLE SEPARATOR
        | '\u{2064}' // INVISIBLE PLUS
        | '\u{2066}' // LEFT-TO-RIGHT ISOLATE
        | '\u{2067}' // RIGHT-TO-LEFT ISOLATE
        | '\u{2068}' // FIRST STRONG ISOLATE
        | '\u{2069}' // POP DIRECTIONAL ISOLATE
        | '\u{061C}' // ARABIC LETTER MARK
        | '\u{FEFF}' // ZERO WIDTH NO-BREAK SPACE / BOM
        | '\u{FFF9}'
        | '\u{FFFA}'
        | '\u{FFFB}' // INTERLINEAR ANNOTATION 锚点
    ) || is_unicode_tag(c)
}

/// Unicode Tag 区（`U+E0000`~`U+E007F`）。
///
/// 这是一类**完全隐形**的字符：浏览器、终端、编辑器、AI 助手里都不渲染，
/// 但每个 tag 字符都对应一个可打印 ASCII（码点 - 0xE0000）。
/// 于是有人可以把一整句隐藏指令塞进看起来正常的文本里。
/// 2024 年 Simon Willison 公开演示过用它在 Claude 里做提示注入。
///
/// 这类字符没有任何合法用途（除了表情旗帜序列使用 `U+E0061`~`U+E007A`，
/// 但那是 emoji 内部序列，文本编辑器里出现即为异常），因此一律删除。
pub fn is_unicode_tag(c: char) -> bool {
    matches!(c, '\u{E0000}'..='\u{E007F}')
}

/// 需要换成普通空格的"伪空格"。
///
/// 这些字符在编辑器里和空格长得一样，但比较时 `"a\u{00A0}b" != "a b"`。
/// 换成 ASCII 空格，不删除——因为它在语义上就是一个空格。
pub fn is_fake_space(c: char) -> bool {
    matches!(
        c,
        '\u{00A0}'   // NO-BREAK SPACE   ← Excel / ERP 导出最爱的
        | '\u{1680}' // OGHAM SPACE MARK
        | '\u{2000}' // EN QUAD
        | '\u{2001}' // EM QUAD
        | '\u{2002}' // EN SPACE
        | '\u{2003}' // EM SPACE
        | '\u{2004}' // THREE-PER-EM SPACE
        | '\u{2005}' // FOUR-PER-EM SPACE
        | '\u{2006}' // SIX-PER-EM SPACE
        | '\u{2007}' // FIGURE SPACE
        | '\u{2008}' // PUNCTUATION SPACE
        | '\u{2009}' // THIN SPACE
        | '\u{200A}' // HAIR SPACE
        | '\u{202F}' // NARROW NO-BREAK SPACE
        | '\u{205F}' // MEDIUM MATHEMATICAL SPACE
        | '\u{3000}' // IDEOGRAPHIC SPACE 全角空格
    )
}

/// 零宽字符（常和"不可见"混为一谈，但这里单独暴露给 report 用）。
pub fn is_zero_width(c: char) -> bool {
    matches!(
        c,
        '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}' | '\u{180E}'
    )
}

/// 是否是需要**替换**（而非删除）的可疑字符。
pub fn is_replaceable_confusable(c: char) -> bool {
    confusable_pair(c).is_some()
}

/// 是否算"可疑字符"（用于快速预检 / 检查报告）。
///
/// 这里统计的是**所有可能给人添麻烦的字符**，不管对应的规则开没开。
/// 理由：「检查报告」回答的是"这段文本脏在哪"，不是"当前配置会改哪些"——
/// 如果关掉全角转换就看不见全角的 `－`，用户根本不知道那里有个坑。
///
/// 注意这里**不包含**普通空格、tab、换行——它们不是问题。
pub fn is_suspicious(c: char) -> bool {
    is_removable_invisible(c)
        || is_fake_space(c)
        || is_replaceable_confusable(c)
        || fullwidth_to_ascii(c).is_some()
}

/// 同形字（homoglyph / confusable）映射表。
///
/// 这些字符"看起来像"ASCII，但码点不同，是复制粘贴后代码报错、搜索搜不到、
/// diff 一片红的根源。比如 Word 自动把 `"` 变成 `“”`，把 `--` 变成 `—`。
///
/// 返回 `Some(替代字符)` 表示有安全的 ASCII 等价物；`None` 表示没有。
///
/// ## 为什么不做全角字母数字
///
/// `ａ`(U+FF41) → `a` 这种转换需要扫描 `U+FF01`~`U+FF5E` 整个区间。
/// 用户的诉求里全角字母数字确实要转，但它是**可选规则**（`fullwidth_ascii`），
/// 因为这个操作对中文正文友好、对英文正文有害——取决于你复制的是什么。
/// 所以拆成独立开关，而不是塞进默认清洗里。
pub fn confusable_pair(c: char) -> Option<char> {
    let r = match c {
        // ===== 引号 =====
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' | '\u{2032}' | '\u{2035}' => '\'',
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{2033}' | '\u{2036}' => '"',
        '\u{00AB}' | '\u{00BB}' | '\u{2039}' | '\u{203A}' => '"',
        // ===== 连字符 / 减号 / 破折号 =====
        '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}'
        | '\u{2212}' | '\u{2E3A}' | '\u{2E3B}' => '-',
        // ===== 省略号 =====
        '\u{2026}' | '\u{22EF}' => '.',
        // ===== 其他容易混的 =====
        '\u{00B7}' => '.', // MIDDLE DOT，中文排版里的间隔号，代码里常是点
        '\u{2022}' => '*', // BULLET
        '\u{00D7}' => 'x', // MULTIPLICATION SIGN
        '\u{00F7}' => '/', // DIVISION SIGN
        '\u{2044}' => '/', // FRACTION SLASH
        '\u{2215}' => '/', // DIVISION SLASH
        _ => return None,
    };
    Some(r)
}

/// 全角 ASCII 区间（`U+FF01`~`U+FF5E`，即 `！` 到 `～`）→ 半角。
///
/// 映射规则是一条公式：`全角码点 - 0xFEE0 = 半角码点`。这不是我编的，
/// 是 Unicode 给全角形式（Halfwidth and Fullwidth Forms 块）设计的固定偏移。
/// 唯一例外是全角空格 `U+3000`（在旁边，走 `is_fake_space`）。
pub fn fullwidth_to_ascii(c: char) -> Option<char> {
    let cp = c as u32;
    if (0xFF01..=0xFF5E).contains(&cp) {
        char::from_u32(cp - 0xFEE0)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_width_chars_are_removable() {
        for c in ['\u{200B}', '\u{200C}', '\u{200D}', '\u{2060}', '\u{FEFF}'] {
            assert!(is_removable_invisible(c), "{} 应被识别为可删隐形字符", c);
        }
    }

    #[test]
    fn bidi_controls_are_removable() {
        // Trojan Source 攻击用到的方向覆盖符必须被拦
        for c in ['\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}'] {
            assert!(is_removable_invisible(c), "{} 应被删除", c);
        }
    }

    #[test]
    fn unicode_tag_block_is_removable() {
        assert!(is_unicode_tag('\u{E0041}')); // 隐藏的 'A'
        assert!(!is_unicode_tag('\u{E0100}')); // 变体选择符补充，不在 tag 区
        for cp in 0xE0000u32..=0xE007F {
            let c = char::from_u32(cp).unwrap();
            assert!(is_removable_invisible(c));
        }
    }

    #[test]
    fn nbsp_is_space_not_deletion() {
        assert!(is_fake_space('\u{00A0}'));
        assert!(!is_removable_invisible('\u{00A0}'));
        assert!(is_fake_space('\u{3000}'));
        assert!(is_fake_space('\u{202F}'));
    }

    #[test]
    fn normal_whitespace_is_fine() {
        for c in [' ', '\t', '\n', '\r'] {
            assert!(!is_suspicious(c), "{:?} 不该被判为可疑", c);
        }
    }

    #[test]
    fn curly_quotes_map_to_ascii() {
        assert_eq!(confusable_pair('\u{201C}'), Some('"'));
        assert_eq!(confusable_pair('\u{201D}'), Some('"'));
        assert_eq!(confusable_pair('\u{2018}'), Some('\''));
        assert_eq!(confusable_pair('\u{2014}'), Some('-'));
        assert_eq!(confusable_pair('\u{2026}'), Some('.'));
        assert_eq!(confusable_pair('a'), None);
    }

    #[test]
    fn fullwidth_offset_formula() {
        // 全角 'Ａ' U+FF21 - 0xFEE0 = 'A' U+0041
        assert_eq!(fullwidth_to_ascii('\u{FF21}'), Some('A'));
        assert_eq!(fullwidth_to_ascii('\u{FF10}'), Some('0'));
        assert_eq!(fullwidth_to_ascii('\u{FF01}'), Some('!'));
        assert_eq!(fullwidth_to_ascii('a'), None);
        // 全角空格不在这个区间，走 fake_space
        assert_eq!(fullwidth_to_ascii('\u{3000}'), None);
    }
}
