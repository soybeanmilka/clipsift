//! 净化规则与管线编排。
//!
//! 每条规则是独立的纯函数，`Plan` 决定执行哪些、以什么顺序执行。
//! 顺序是有讲究的，见 [`Plan::steps`] 的注释。
//!
//! ## 幂等性怎么保证的
//!
//! `apply(apply(x)) == apply(x)` 是剪贴板守护进程的生命线：监听会反复触发，
//! 不幂等的话文本会被越洗越烂。做法分两层：
//!
//! 1. **每条规则自己必须对"自己的输出"不再改变**。典型反例是 `trim_trailing`：
//!    它会把 `"a\n"` 变成 `"a"`，如果只去行尾空格不去末尾换行，第一轮输出
//!    `"a\nb\n"` 第二轮就变成 `"a\nb"`——长度在变，永远收敛不了。
//! 2. **跨规则的不动点**：某条规则的输出要能被后续规则原样接受。比如
//!    `collapse_whitespace` 留在每行两端的空格，`trim_trailing` 才吃得掉；
//!    反过来 `trim_trailing` 只处理行尾，两轮之间的"新暴露出来的空白"
//!    就会被下一轮吃一次——所以每条规则都要处理到**收敛为止**，而不是只跑一遍。
//!
//! 唯一做不到严格幂等的是 `merge_hard_returns`，原因见该函数文档。

use crate::chars;
use serde::{Deserialize, Serialize};

/// 一条规则的可开关配置。字段名同时用于 JSON 配置文件和命令行参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Plan {
    /// 删除零宽 / 双向控制 / BOM / Unicode Tag（默认开）
    pub invisible: bool,
    /// 伪空格（NBSP、全角空格等）归一为 ASCII 空格（默认开）
    pub normalize_spaces: bool,
    /// 弯引号、破折号、省略号等同形字换成 ASCII（默认开）
    pub confusables: bool,
    /// 全角 ASCII（`！`~`～`、`Ａ`~`ｚ`）转半角（默认关，英文正文会误伤中文标点场景）
    pub fullwidth_ascii: bool,
    /// 合并硬回车：单换行变空格、空行保留为段落（默认关，只适合从 PDF 抠出来的正文）
    ///
    /// ⚠️ 这是**唯一不做幂等保证**的规则，见 [`merge_hard_returns`] 的说明。
    /// 打开它意味着接受"这份文本只归一一次"的语义，不要重复喂给管线。
    pub merge_hard_returns: bool,
    /// 压缩连续空白（默认开；只压行内，保行首缩进宽度）
    pub collapse_whitespace: bool,
    /// 删除每行行尾空白（默认开）
    pub trim_trailing: bool,
    /// 删除控制字符（保留 `\n` `\r` `\t`）（默认开）
    pub strip_control: bool,
    /// 用户自定义正则规则（默认空）
    pub custom: Vec<CustomRule>,
}

impl Default for Plan {
    fn default() -> Self {
        Self {
            invisible: true,
            normalize_spaces: true,
            confusables: true,
            fullwidth_ascii: false,
            merge_hard_returns: false,
            collapse_whitespace: true,
            trim_trailing: true,
            strip_control: true,
            custom: Vec::new(),
        }
    }
}

/// 自定义规则：正则 → 替换串。
///
/// `enable` 字段用于 GUI 里勾选/取消而不必删规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomRule {
    pub name: String,
    pub pattern: String,
    pub replacement: String,
    #[serde(default = "yes")]
    pub enable: bool,
}

fn yes() -> bool {
    true
}

/// 规则的标识，用于报告与命令行。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    Invisible,
    NormalizeSpaces,
    Confusables,
    FullwidthAscii,
    MergeHardReturns,
    CollapseWhitespace,
    TrimTrailing,
    StripControl,
    Custom,
}

impl Rule {
    /// 人类可读名字，CLI 与 GUI 共用。
    pub fn label(self) -> &'static str {
        match self {
            Rule::Invisible => "删除隐形字符",
            Rule::NormalizeSpaces => "伪空格归一",
            Rule::Confusables => "同形字转 ASCII",
            Rule::FullwidthAscii => "全角转半角",
            Rule::MergeHardReturns => "合并硬回车",
            Rule::CollapseWhitespace => "压缩连续空白",
            Rule::TrimTrailing => "去行尾空白",
            Rule::StripControl => "删控制字符",
            Rule::Custom => "自定义规则",
        }
    }

    /// 命令行/配置文件里的键名（短横线风格）。
    pub fn key(self) -> &'static str {
        match self {
            Rule::Invisible => "invisible",
            Rule::NormalizeSpaces => "spaces",
            Rule::Confusables => "confusables",
            Rule::FullwidthAscii => "fullwidth",
            Rule::MergeHardReturns => "merge",
            Rule::CollapseWhitespace => "collapse-ws",
            Rule::TrimTrailing => "trim",
            Rule::StripControl => "control",
            Rule::Custom => "custom",
        }
    }

    /// 全部规则（不含 Custom），CLI `--list-rules` 用。
    pub const ALL: [Rule; 8] = [
        Rule::Invisible,
        Rule::NormalizeSpaces,
        Rule::Confusables,
        Rule::FullwidthAscii,
        Rule::MergeHardReturns,
        Rule::CollapseWhitespace,
        Rule::TrimTrailing,
        Rule::StripControl,
    ];
}

impl Plan {
    /// 这个配置能否保证 `apply(apply(x)) == apply(x)`。
    ///
    /// 唯一的破坏者是 [`Rule::MergeHardReturns`]。调用方（比如剪贴板守护进程）
    /// 应该在可能重复清洗同一份文本的场景下检查这个值，别把归一化跑两遍。
    pub fn is_idempotent(&self) -> bool {
        !self.merge_hard_returns
    }

    /// 按顺序返回要执行的规则。
    ///
    /// ## 为什么是这个顺序
    ///
    /// 贯穿始终的一条原则：**会"删东西"的规则必须排在会"整形"的规则前面**。
    /// 删字符会让原本不相邻的字符贴到一起，凭空造出新的空白结构；如果整形已经
    /// 跑完了，这团新产生的空白就要等到下一轮才被处理 —— 那就不幂等了。
    ///
    /// 1. **先删隐性字符**。零宽空格如果留到后面，会把"连续空白压缩"和
    ///    "行尾空白检测"都搞乱——`"a \u{200B}b"` 里的那串空白不是真的空白。
    /// 2. **删控制字符**。它是最典型的"删了才暴露问题"的规则：`"a \u{0000}\n"`
    ///    删掉 `\0` 后，那个空格就贴到换行前面了。放在空白压缩之后跑的话，
    ///    这一轮没人收拾它，下一轮才被吃掉 —— 不幂等。所以必须提到这里。
    /// 3. **再归一伪空格**。`\u{00A0}` 变成真空格后，后面的空白压缩才能看见它。
    /// 4. **同形字转 ASCII**。弯引号转直引号；只处理标点（空白类已在第 3 步处理）。
    /// 5. **全角转半角**。放这里是因为它可能产生新的 ASCII（如 `！`→`!`）。
    /// 6. **合并硬回车**。必须放在所有"按行"的规则之前，因为它改变了行结构。
    ///    注意它和幂等性互斥，见 [`Plan::is_idempotent`]。
    /// 7. **压缩连续空白**、8. **去行尾空白**。这两条一起收尾，且要跑在
    ///    所有可能改变字符邻接关系的规则之后，否则会被"偷偷改变"。
    /// 9. **自定义规则**永远最后，用户看到的是"干净文本"基础上的定制。
    pub fn steps(&self) -> Vec<Rule> {
        let mut v = Vec::new();
        if self.invisible {
            v.push(Rule::Invisible);
        }
        if self.strip_control {
            v.push(Rule::StripControl);
        }
        if self.normalize_spaces {
            v.push(Rule::NormalizeSpaces);
        }
        if self.confusables {
            v.push(Rule::Confusables);
        }
        if self.fullwidth_ascii {
            v.push(Rule::FullwidthAscii);
        }
        if self.merge_hard_returns {
            v.push(Rule::MergeHardReturns);
        }
        if self.collapse_whitespace {
            v.push(Rule::CollapseWhitespace);
        }
        if self.trim_trailing {
            v.push(Rule::TrimTrailing);
        }
        if self.custom.iter().any(|r| r.enable) {
            v.push(Rule::Custom);
        }
        v
    }
}

/// 一次净化的统计报告：哪个字符被干掉了、干掉了几个。
///
/// 这是 ClipSift 区分于同类工具的地方之一——它不只给你结果，
/// 还告诉你"原来这段文本被污染在哪"。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Report {
    /// 每个改动过的规则名 → 改动前的字符数减改动后的字符数。
    pub per_rule: Vec<(String, i64)>,
    /// 被删除的可疑字符 → 出现次数。
    pub removed: Vec<(u32, usize)>,
    pub chars_in: usize,
    pub chars_out: usize,
}

impl Report {
    /// 是否没有任何改动。
    ///
    /// 注意"有改动"不等于"文本脏"：默认计划里的 `trim_trailing` 会吃掉块尾换行，
    /// 所以任何以 `\n` 结尾的干净文本也会被判成"有改动"。真正判断脏不脏要看
    /// [`Report::removed`]（有没有可疑字符被清掉）。
    pub fn is_clean(&self) -> bool {
        self.chars_in == self.chars_out && self.removed.is_empty()
    }

    /// 有没有清掉过可疑字符——这才是"这段文本原本是脏的"的判据。
    pub fn had_suspicious_chars(&self) -> bool {
        !self.removed.is_empty()
    }

    /// 一句话摘要，CLI/GUI 直接显示。
    pub fn summary(&self) -> String {
        if self.is_clean() {
            return "干净，无需处理".to_string();
        }
        let mut parts = Vec::new();
        if !self.removed.is_empty() {
            let total: usize = self.removed.iter().map(|(_, n)| n).sum();
            parts.push(format!("清除 {} 个隐形字符", total));
        }
        if self.chars_in != self.chars_out {
            let diff = self.chars_in as i64 - self.chars_out as i64;
            // 只差空白的时候别说"减少 N 字符"，听着像是丢了内容
            let only_ws = !text_has_suspicious_gap(self);
            if diff > 0 {
                if only_ws {
                    parts.push(format!("去掉 {diff} 个多余的空白"));
                } else {
                    parts.push(format!("减少 {} 字符", diff));
                }
            } else {
                parts.push(format!("增加 {} 字符", -diff));
            }
        }
        if parts.is_empty() {
            "已替换可疑字符".to_string()
        } else {
            parts.join("，")
        }
    }
}

/// 只在"字符数变化但没删可疑字符"时用，判断这次差异是不是纯空白造成的。
///
/// 没有删可疑字符却长度变了，基本只能是空白整形（trim / collapse）干的活。
fn text_has_suspicious_gap(report: &Report) -> bool {
    report.had_suspicious_chars()
}

/// 应用整个管线。这是唯一的公开入口。
///
/// 返回 `(结果文本, 报告)`。不修改输入。
pub fn apply(text: &str, plan: &Plan) -> (String, Report) {
    let mut report = Report {
        chars_in: text.chars().count(),
        ..Default::default()
    };

    // 记录输入里的可疑字符分布（在改动之前统计）
    report.removed = crate::inspect(text)
        .into_iter()
        .map(|(c, n)| (c as u32, n))
        .collect();

    let mut cur = text.to_string();
    for rule in plan.steps() {
        let before = cur.chars().count() as i64;
        cur = match rule {
            Rule::Invisible => remove_invisible(&cur),
            Rule::NormalizeSpaces => normalize_spaces(&cur),
            Rule::Confusables => to_confusables(&cur),
            Rule::FullwidthAscii => to_fullwidth_ascii(&cur),
            Rule::MergeHardReturns => merge_hard_returns(&cur),
            Rule::CollapseWhitespace => collapse_whitespace(&cur),
            Rule::TrimTrailing => trim_trailing(&cur),
            Rule::StripControl => strip_control(&cur),
            Rule::Custom => apply_custom(&cur, &plan.custom),
        };
        let delta = cur.chars().count() as i64 - before;
        if delta != 0 {
            report.per_rule.push((rule.label().to_string(), delta));
        }
    }

    report.chars_out = cur.chars().count();
    (cur, report)
}

// ======================= 各规则实现 =======================

/// 删除零宽/双向控制/BOM/Unicode Tag。
fn remove_invisible(text: &str) -> String {
    text.chars()
        .filter(|c| !chars::is_removable_invisible(*c))
        .collect()
}

/// 伪空格 → 空格；`\u{3000}` 全角空格也在这处理。
fn normalize_spaces(text: &str) -> String {
    // 一次性把所有“长得像空格”的字符统一成空格。
    // 顺序很重要：这一步必须在 merge_hard_returns 之前跑，
    // 否则 `"\r\r−"` 里的两个回车先被识别成段落边界、后面再并入行内空白，
    // 就会产生"第二步比第一步还短"的非不动点。
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        out.push(match ch {
            '\u{00A0}' | '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}' => ' ',
            '\u{000B}' | '\u{000C}' | '\u{0085}' => '\n',
            '\u{2028}' | '\u{2029}' => '\n',
            other => other,
        });
    }
    out
}

/// 同形字 → ASCII。
fn to_confusables(text: &str) -> String {
    text.chars()
        .map(|c| chars::confusable_pair(c).unwrap_or(c))
        .collect()
}

/// 全角 ASCII → 半角。
fn to_fullwidth_ascii(text: &str) -> String {
    text.chars()
        .map(|c| chars::fullwidth_to_ascii(c).unwrap_or(c))
        .collect()
}

/// 合并硬回车。
///
/// 规则：连续 ≥2 个换行 = 段落边界（保留为 1 个换行）；单个换行 = 排版折行（变空格）。
///
/// ## 为什么它注定不幂等
///
/// 这条规则输出的 `\n` 不可能同时满足它自己的两条判断：
///
/// ```text
/// "a⏎⏎b"  第一轮：2 个换行 = 段落边界 → "a⏎b"
/// "a⏎b"   第二轮：1 个换行 = 排版折行 → "a b"   ← 自己把自己的输出吃掉了
/// ```
///
/// 这不是实现 bug，是规则的定义自相矛盾：**"段落"和"折行"的信息在纯文本里
/// 只能靠"换行个数"表达，而归一化恰恰要把这个个数改掉**。任何"单换行变空格"
/// 的实现都逃不掉这一点（换任何哨兵字符也会在下一轮被当成正文）。
///
/// 所以正确的用法是**把它当一次性变换**：同一份文本只归一一次。
/// [`Plan::is_idempotent`] 会返回 `false` 提示调用方，属性测试也据此跳过。
fn merge_hard_returns(text: &str) -> String {
    // 先把换行符统一成 \n：CR 一律视为换行（\r\n 和单独的 \r 都算一个换行），
    // 并把 NBSP 之类还没处理干净的伪空格就地归一，免得它们藏在换行之间
    // 让"连续换行"的判断看不穿（"a⏎\u{00A0}⏎b" 应该算一段空行）。
    let mut unified = String::with_capacity(text.len());
    let mut prev_cr = false;
    for ch in text.chars() {
        match ch {
            '\r' => {
                unified.push('\n');
                prev_cr = true;
            }
            '\n' => {
                // \r\n 里的 \n 已经由前一字符的 \r 生成了一个，跳过这一个
                if !prev_cr {
                    unified.push('\n');
                }
                prev_cr = false;
            }
            other => {
                // 行与行之间可能夹着 NBSP/全角空格：转成普通空格，
                // 这样下面的连续换行扫描能看到"这行其实是空的"
                unified.push(if chars::is_fake_space(other) {
                    ' '
                } else {
                    other
                });
                prev_cr = false;
            }
        }
    }

    let chars: Vec<char> = unified.chars().collect();
    let mut out = String::with_capacity(unified.len());
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] as u32 == 0x0A {
            // 扫描这一段"换行 + 夹在中间的空白"。
            //
            // 为什么要穿过空格：真实数据里空行常常是 `换行 + 不间断空格 + 换行`，
            // PDF、Word、网页表格都这么干。只数连续 `\n` 的话，`"\n \n"` 会被
            // 当成两个独立折行，整段结构塌成一行 —— 用户会看到「PDF 复制下来
            // 段落全粘在一起」。所以这里允许空格/tab 夹在换行之间，只要中间
            // 没有真正的可见字符，就算同一段空行。
            let mut j = i;
            let mut newlines = 0usize;
            let mut last_nl = i;
            while j < chars.len() {
                let c = chars[j];
                if c as u32 == 0x0A {
                    newlines += 1;
                    last_nl = j;
                    j += 1;
                } else if c == ' ' || c == '\t' {
                    j += 1;
                } else {
                    break;
                }
            }
            // 换个更贴切的判据：这段空行里**包含至少 2 个换行**才算段落边界。
            if newlines >= 2 {
                out.push('\n');
            } else {
                out.push(' ');
            }
            // 跳过整段，但把末尾的空格留着（交给后面的空白压缩收）——
            // 直接吞掉会让 `"a\n  b"` 变成 `"a b"`，丢了作者故意的缩进。
            i = last_nl + 1;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}
/// 连续空白（空格、tab、换行挤在一起）压成一个。
///
/// 注意：不会碰单个空格，也不会吃掉换行本身——只在同一行内压缩。
fn collapse_whitespace(text: &str) -> String {
    // 只做一件事：把行**内部**的连续空格/tab 压成一个空格（`"a    b"` → `"a b"`）。
    //
    // 刻意**不**去动行首行尾的空白：
    // - 行尾空白由 `trim_trailing` 专门管，职责分开，幂等性更好推。
    // - 行首空白就是**缩进**，是数据的一部分。早期版本在这里顺手 trim 了两侧，
    //   结果从网页复制来的 Python / C++ 代码全部顶格——缩进是语法，删掉就废了。
    //   同理，缩进**宽度**也要保住：`[ \t]{2,}` 一刀切会把 4 空格缩进压成 1 个，
    //   于是改用两条正则，第一条负责"行首之后"的部分。
    // - 也绝不跨行：把 `\n` 变成空格会让段落边界被自己的输出再吃一次。
    static INSIDE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static RUN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    // 行首缩进整体跳过后再压；`(?m)` 让 `^` 按行匹配
    let inside = INSIDE.get_or_init(|| regex::Regex::new(r"(?m)^([ \t]*)(.*)$").unwrap());
    let run = RUN.get_or_init(|| regex::Regex::new(r"[ \t]{2,}").unwrap());
    inside
        .replace_all(text, |caps: &regex::Captures| {
            let indent = &caps[1];
            let body = run.replace_all(&caps[2], " ");
            format!("{indent}{body}")
        })
        .into_owned()
}
/// 去每行行尾空白。
fn trim_trailing(text: &str) -> String {
    // 管线里的最后一道"整形"：去掉行尾空格/tab，**并吃掉块尾的换行**。
    //
    // 为什么要吃块尾换行：流程走到这里，`merge_hard_returns` 可能已经把一个
    // 孤零零的 `\n` 留在了末尾。若不吃掉，下一轮它会变成空格、再下一轮变成
    // 空串——输出不是不动点，幂等性破（property 测试第 260 例就是这个）。
    // 所以定义：trim 之后，末尾的所有空白（含换行）都不保留。
    let trimmed_line = |line: &str| line.trim_end_matches([' ', '\t']).to_string();
    let mut out: Vec<String> = text.split('\n').map(trimmed_line).collect();
    // 去掉末尾空行
    while out.last().map(|s| s.is_empty()).unwrap_or(false) {
        out.pop();
    }
    // 开头的空行也一并去掉（块首的空行同样没有语义）
    let mut start = 0;
    while start < out.len() && out[start].is_empty() {
        start += 1;
    }
    out[start..].join("\n")
}
/// 删控制字符，保留 `\n` `\r` `\t`。
///
/// ## 为什么它排在空白类规则**前面**
///
/// 这条规则会**删字符**，删完原本不相邻的东西就贴一起了。最典型的例子：
/// `"a \u{0000}\n"` 删掉 `\0` 之后，那个空格直接贴到换行前面。
/// 如果"去行尾空白"已经跑完了，这个空格这一轮就没被处理 —— 下一轮才被吃掉，
/// 于是 `apply(apply(x)) != apply(x)`。属性测试抓到过。
///
/// ## 只删"真正无意义"的控制字符
///
/// `char::is_control()` 会一并判掉 `\n`/`\r`/`\t`（需要白名单）。但这里还有个
/// 反直觉的坑：**`\r` 必须保留**。因为合并硬回车规则在后面才跑，它要先看到
/// `\r\n` 才能正确归一；如果这里把 `\r` 删了，`"a\r\rb"` 变成 `"a\nb"`，第二次
/// 再跑就变成 `"a b"`——不幂等。属性测试抓到过这个 bug。
///
/// `U+001C`~`U+001F` 是文件/组/记录/单元分隔符，老系统用它做字段分隔，
/// 虽属控制字符但有语义，一律保留，避免破坏数据。
fn strip_control(text: &str) -> String {
    text.chars()
        .filter(|c| {
            if !c.is_control() {
                return true;
            }
            // 保留换行族、制表符、以及有分隔语义的 1C~1F
            matches!(c, '\n' | '\r' | '\t' | '\u{001C}'..='\u{001F}')
        })
        .collect()
}

/// 用户自定义正则规则。
///
/// 正则写错不应该让整个程序崩溃——跳过并继续，这是守护进程的刚性要求：
/// 一个坏规则不能让你此后所有复制都失效。
fn apply_custom(text: &str, rules: &[CustomRule]) -> String {
    let mut cur = text.to_string();
    for rule in rules {
        if !rule.enable {
            continue;
        }
        // 用 regex 的 builder 限制大小，防御 ReDoS
        let built = regex::RegexBuilder::new(&rule.pattern)
            .size_limit(1 << 20)
            .dfa_size_limit(1 << 20)
            .build();
        if let Ok(re) = built {
            let replaced = re.replace_all(&cur, rule.replacement.as_str());
            cur = replaced.into_owned();
        }
        // 规则写错就跳过，不 panic
    }
    cur
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(text: &str, plan: &Plan) -> String {
        apply(text, plan).0
    }

    #[test]
    fn default_plan_removes_zwsp() {
        let plan = Plan::default();
        assert_eq!(clean("这是\u{200B}测试\u{FEFF}文本", &plan), "这是测试文本");
    }

    #[test]
    fn default_plan_keeps_newlines() {
        let plan = Plan::default();
        assert_eq!(clean("第一行\n第二行", &plan), "第一行\n第二行");
    }

    #[test]
    fn nbsp_becomes_plain_space() {
        let plan = Plan::default();
        assert_eq!(clean("a\u{00A0}b", &plan), "a b");
    }

    #[test]
    fn curly_quotes_fixed() {
        let plan = Plan::default();
        assert_eq!(clean("\u{201C}hi\u{201D}", &plan), "\"hi\"");
    }

    #[test]
    fn fullwidth_only_when_enabled() {
        let off = Plan {
            fullwidth_ascii: false,
            ..Plan::default()
        };
        assert_eq!(clean("\u{FF21}", &off), "\u{FF21}");
        let on = Plan {
            fullwidth_ascii: true,
            ..Plan::default()
        };
        assert_eq!(clean("\u{FF21}", &on), "A");
    }

    #[test]
    fn merge_hard_returns_single_and_paragraph() {
        let plan = Plan {
            merge_hard_returns: true,
            ..Plan::default()
        };
        let input = "第一段第一行\n第一段第二行\n\n第二段";
        assert_eq!(clean(input, &plan), "第一段第一行 第一段第二行\n第二段");
    }

    #[test]
    fn merge_hard_returns_windows_crlf() {
        let plan = Plan {
            merge_hard_returns: true,
            ..Plan::default()
        };
        let got = clean("a\r\nb\r\n\r\nc", &plan);
        assert_eq!(got, "a b\nc");
    }

    #[test]
    fn collapse_whitespace_inside_line() {
        let plan = Plan::default();
        assert_eq!(clean("a    b\t\tc", &plan), "a b c"); // 默认计划含 collapse-ws
        assert_eq!(clean("a\n\nb", &plan), "a\n\nb"); // 不碰换行
    }

    #[test]
    fn trim_trailing_whitespace() {
        let plan = Plan::default();
        assert_eq!(clean("a   \nb\t\nc  ", &plan), "a\nb\nc");
    }

    #[test]
    fn strip_control_keeps_lf_cr_tab() {
        let plan = Plan {
            collapse_whitespace: false,
            trim_trailing: false,
            ..Plan::default()
        };
        assert_eq!(clean("a\x00\x01b\tc\nd", &plan), "ab\tc\nd");
    }

    #[test]
    fn strip_control_runs_before_whitespace_rules() {
        // 删掉 \0 之后，那个空格会贴到换行前面；因为 strip_control 排在
        // collapse/trim 之前，这一轮就能收掉，不需要等第二轮。
        let plan = Plan::default();
        let once = clean("a \u{0000}\nb", &plan);
        assert_eq!(once, "a\nb");
        assert_eq!(clean(&once, &plan), once, "第二轮不应再变");
    }

    #[test]
    fn indentation_survives_whitespace_rules() {
        // 从网页复制 Python/C++ 代码时，缩进是语法的一部分。
        // 曾经 collapse_whitespace 顺手 trim 了每行两侧，结果代码全部顶格；
        // 后来改成"只压行内"却又把 4 空格缩进压成 1 个。两条都锁住。
        let plan = Plan::default();
        let src = "class A:\n    def f(self):\n        return 1\n";
        assert_eq!(
            clean(src, &plan),
            "class A:\n    def f(self):\n        return 1"
        );
        // 行内的连续空格照压不误
        assert_eq!(clean("a     b", &plan), "a b");
        // NBSP 缩进归一成 ASCII 空格后，宽度不变
        let nbsp_indent = "\u{00A0}\u{00A0}\u{00A0}\u{00A0}pass";
        assert_eq!(clean(nbsp_indent, &plan), "    pass");
    }

    #[test]
    fn unicode_tag_is_removed() {
        let plan = Plan::default();
        // U+E0041 是隐藏的 'A'
        assert_eq!(clean("hi\u{E0041}there", &plan), "hithere");
    }

    #[test]
    fn custom_rule_applies() {
        let plan = Plan {
            custom: vec![CustomRule {
                name: "去多余空行".into(),
                pattern: r"\n{3,}".into(),
                replacement: "\n\n".into(),
                enable: true,
            }],
            ..Plan::default()
        };
        let got = clean("a\n\n\n\nb", &plan);
        assert_eq!(got, "a\n\nb");
    }

    #[test]
    fn bad_custom_rule_is_skipped_not_panicked() {
        let plan = Plan {
            custom: vec![CustomRule {
                name: "坏正则".into(),
                pattern: "([unclosed".into(),
                replacement: "x".into(),
                enable: true,
            }],
            ..Plan::default()
        };
        // 不该 panic，文本原样返回（除了默认规则）
        assert_eq!(clean("abc", &plan), "abc");
    }

    #[test]
    fn report_counts_removed() {
        let (_, report) = apply("a\u{200B}b\u{200B}c", &Plan::default());
        assert!(!report.is_clean());
        assert_eq!(report.chars_in, 5);
        assert_eq!(report.chars_out, 3);
        let zwsp = report.removed.iter().find(|(c, _)| *c == 0x200B).unwrap();
        assert_eq!(zwsp.1, 2);
    }

    #[test]
    fn clean_text_reports_clean() {
        let (_, report) = apply("普通的一段中文 abc 123", &Plan::default());
        assert!(report.is_clean());
        assert_eq!(report.summary(), "干净，无需处理");
    }

    #[test]
    fn idempotent() {
        let plan = Plan::default();
        let once = clean("a\u{200B}b\u{00A0}c\u{201C}d\u{201D}", &plan);
        let twice = clean(&once, &plan);
        assert_eq!(once, twice);
    }
}
