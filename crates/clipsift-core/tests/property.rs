//! 属性测试：不依赖 fuzz crate，用一个确定性伪随机发生器批量造文本。
//!
//! 要验证的性质只有三条，但每条都对"所有可能输入"成立才有意义：
//!
//! 1. **幂等**：`f(f(x)) == f(x)`。这是守护进程的生命线——剪贴板监听会反复触发，
//!    不幂等的话文本会被越洗越烂。
//! 2. **不崩**：任何字符序列都不能 panic（包括孤立代理项无法构造，但控制字符、
//!    私有区、Emoji ZWJ 序列都能）。
//! 3. **UTF-8 合法**：输出仍是合法 `String`（Rust 类型系统保证，但这里顺带断言
//!    不产生 U+FFFD 替换字符，说明没有把多字节字符切坏）。

use clipsift_core::{apply, Plan};

/// xorshift64* —— 32 行确定性伪随机，够用且零依赖。
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.max(1))
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[(self.next() % items.len() as u64) as usize]
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// 构造"最脏"的字符池：ASCII、中文、各种隐形字符、伪空格、同形字、控制字符。
fn dirty_pool() -> Vec<char> {
    let mut v: Vec<char> = "abcXYZ019 ,.!?-_'\"\t\n\r<>{}()".chars().collect();
    v.extend("中文测试段落。，、！？".chars());
    v.extend([
        '\u{200B}',
        '\u{200C}',
        '\u{200D}',
        '\u{2060}',
        '\u{FEFF}',
        '\u{180E}',
        '\u{200E}',
        '\u{200F}',
        '\u{061C}',
        '\u{202A}',
        '\u{202B}',
        '\u{202C}',
        '\u{202D}',
        '\u{202E}',
        '\u{2066}',
        '\u{2067}',
        '\u{2068}',
        '\u{2069}',
        '\u{00A0}',
        '\u{3000}',
        '\u{2003}',
        '\u{2009}',
        '\u{202F}',
        '\u{205F}',
        '\u{2018}',
        '\u{2019}',
        '\u{201C}',
        '\u{201D}',
        '\u{2013}',
        '\u{2014}',
        '\u{2212}',
        '\u{2026}',
        '\u{00B7}',
        '\u{FF21}',
        '\u{FF01}',
        '\u{FF41}',
        '\u{00AD}',
        '\u{0000}',
        '\u{0001}',
        '\u{001B}',
        '\u{001C}',
        '\u{E0041}',
        '\u{E007A}',
        '\u{FFF9}',
    ]);
    v
}

fn random_text(rng: &mut Rng, pool: &[char], max_len: usize) -> String {
    let len = rng.below(max_len);
    (0..len).map(|_| *rng.pick(pool)).collect()
}

fn all_plans() -> Vec<Plan> {
    let mut v = Vec::new();
    for invisible in [true, false] {
        for spaces in [true, false] {
            for confusables in [true, false] {
                for fullwidth in [true, false] {
                    for merge in [true, false] {
                        v.push(Plan {
                            invisible,
                            normalize_spaces: spaces,
                            confusables,
                            fullwidth_ascii: fullwidth,
                            merge_hard_returns: merge,
                            ..Plan::default()
                        });
                    }
                }
            }
        }
    }
    v
}

#[test]
fn idempotent_over_10000_random_inputs() {
    let pool = dirty_pool();
    let mut rng = Rng::new(0x5EED_1234_ABCD);
    let plans = all_plans();
    let mut skipped = 0usize;
    let mut checked = 0usize;
    for i in 0..10_000 {
        let text = random_text(&mut rng, &pool, 48);
        let plan = &plans[i % plans.len()];
        // merge_hard_returns 的定义与幂等互斥（见 rules::merge_hard_returns 文档），
        // 它的正确行为由 merge_is_one_way_by_design 单独守。
        if !plan.is_idempotent() {
            skipped += 1;
            continue;
        }
        checked += 1;
        let once = apply(&text, plan).0;
        let twice = apply(&once, plan).0;
        assert_eq!(
            once, twice,
            "第 {i} 例幂等性失败\n输入: {:?}\n一次: {:?}\n两次: {:?}\nplan: {:?}",
            text, once, twice, plan
        );
    }
    // 别让跳过把测试偷偷变成空转
    assert!(checked > 4_000, "实际校验的样本太少：{checked}");
    assert_eq!(checked + skipped, 10_000);
}

/// 守护进程用的就是默认配置，它**必须**是幂等的——单独钉一条，不依赖上面的抽样。
#[test]
fn default_plan_used_by_daemon_is_idempotent() {
    assert!(Plan::default().is_idempotent());
    let pool = dirty_pool();
    let mut rng = Rng::new(0xC1A9_B0A2D);
    for i in 0..10_000 {
        let text = random_text(&mut rng, &pool, 64);
        let once = apply(&text, &Plan::default()).0;
        let twice = apply(&once, &Plan::default()).0;
        assert_eq!(once, twice, "第 {i} 例：默认配置不幂等\n输入: {:?}", text);
    }
}

/// 段合并是**一次性变换**：这条测试把"它不幂等"钉成契约，
/// 免得以后有人"顺手修好了"。
#[test]
fn merge_is_one_way_by_design() {
    let plan = Plan {
        merge_hard_returns: true,
        ..Plan::default()
    };
    assert!(!plan.is_idempotent());

    // 两个换行 = 段落边界 → 压成一个换行
    let once = apply("a\n\nb", &plan).0;
    assert_eq!(once, "a\nb");
    // 再跑一遍，那个"孤零零的换行"会被当成折行吃掉
    let twice = apply(&once, &plan).0;
    assert_eq!(twice, "a b");
    // 关键点：第三轮之后不再变化。说明它是**收敛**的一次性变换，
    // 不是振荡，也不是越洗越烂到把内容吃光。
    let thrice = apply(&twice, &plan).0;
    assert_eq!(thrice, twice, "合并硬回车应在一轮内收敛");
}

/// 行之间的伪空格不能挡住段落识别。
///
/// 真实数据里"空行"经常长这样：`换行 + 不间断空格 + 换行`（PDF / Word / 网页表格
/// 都这么输出）。如果只数连续的 `\n`，这种空行会被当成两次折行，整段塌成一行。
#[test]
fn merge_sees_through_pseudo_blank_lines() {
    let plan = Plan {
        normalize_spaces: false, // 故意不提前归一，看 merge 自己扛不扛
        merge_hard_returns: true,
        ..Plan::default()
    };
    // NBSP 夹在两个换行之间 = 空行 = 段落边界
    assert_eq!(apply("a\n\u{00A0}\nb", &plan).0, "a\nb");
    // 多个空格夹着也算
    assert_eq!(apply("a\n \t \nb", &plan).0, "a\nb");
    // 对照：只有 1 个换行时，仍然是折行（不能因为吞了空格就当段落）
    assert_eq!(apply("a\n\u{00A0}b", &plan).0, "a b");
}

#[test]
fn never_panics_and_no_replacement_char_leak() {
    let pool = dirty_pool();
    let mut rng = Rng::new(0x00DE_ADBE_EF99);
    for _ in 0..5_000 {
        let text = random_text(&mut rng, &pool, 64);
        for plan in all_plans() {
            let out = apply(&text, &plan).0;
            // U+FFFD 说明有地方把多字节字符切坏了——Rust 的 String 不会，
            // 但自定义规则里的正则如果乱来就有可能，所以守着这条线。
            assert!(
                !out.contains('\u{FFFD}'),
                "输出里出现替换字符，输入 {:?}",
                text
            );
        }
    }
}

#[test]
fn empty_and_whitespace_only_inputs() {
    let plan = Plan::default();
    for s in [
        "",
        " ",
        "\n",
        "\r\n",
        "\t\t",
        "   \n   ",
        "\u{200B}",
        "\u{FEFF}\u{200B}\u{200C}",
    ] {
        let out = apply(s, &plan).0;
        assert!(!out.contains('\u{FFFD}'));
    }
}

#[test]
fn realistic_cpp_paste_from_website() {
    // 模拟真实场景：从网页复制一段 C++ 代码，每行末尾带零宽空格，
    // 缩进是不间断空格，引号是弯引号。
    let dirty = "class Person {\u{200B}\n\u{00A0}\u{00A0}public:\u{200B}\n\u{00A0}\u{00A0}\u{00A0}\u{00A0}Person() : name_(\u{201C}Unknown\u{201D}), age_(0) {}\u{200B}\n};\u{200B}";
    let plan = Plan {
        trim_trailing: false, // 代码缩进不能动
        ..Plan::default()
    };
    let (out, report) = apply(dirty, &plan);

    assert!(!out.contains('\u{200B}'));
    assert!(!out.contains('\u{00A0}'));
    assert!(out.contains("name_(\"Unknown\")"));
    assert!(out.contains("class Person {"));
    // 行数不变——净化不该改变代码结构
    assert_eq!(dirty.lines().count(), out.lines().count());
    assert!(report.removed.iter().any(|(c, n)| *c == 0x200B && *n == 4));
}
