//! ClipSift 命令行工具 + 剪贴板守护进程。
//!
//! 两种使用方式：
//!
//! ```text
//! # 1. 一次性净化：从 stdin 读，写到 stdout（管道友好）
//! cat dirty.txt | clipsift
//! clipsift -f some.txt -o clean.txt
//!
//! # 2. 剪贴板守护：常驻后台，你每次复制它就自动净化
//! clipsift watch
//! ```
//!
//! 设计上只有一个原则：**绝不让坏输入把程序搞崩**。剪贴板守护进程崩了，
//! 用户此后所有复制都会带着脏字符，这比不装还糟。

mod config;

use clap::{Parser, Subcommand};
use clipsift_core::{apply, Plan};
use std::io::{self, Read, Write};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "clipsift",
    version,
    about = "从复制粘贴里筛掉隐形字符：零宽空格、伪空格、弯引号、全角字母……",
    long_about = "ClipSift 把「看起来一样、实际不一样」的文本变成真正一样。\n\
                  默认规则会删除零宽/隐形字符、把伪空格归一成普通空格、把弯引号\n\
                  和破折号换成 ASCII。适合从网页、PDF、Word、Excel 里复制内容后\n\
                  再粘到代码或数据库里的场景。"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// 从文件读，而不是 stdin
    #[arg(short, long, global = true)]
    file: Option<PathBuf>,

    /// 写到文件，而不是 stdout
    #[arg(short, long, global = true)]
    output: Option<PathBuf>,

    /// 用配置文件里的规则（JSON）
    #[arg(short, long, global = true, value_name = "FILE")]
    config: Option<PathBuf>,

    /// 打开合并硬回车（单换行变空格、空行留作段落）。
    /// 只适合从 PDF 抠正文；它**不是幂等的**，同一份文本别跑两遍
    #[arg(long, global = true)]
    merge: bool,

    /// 打开全角字母数字转半角
    #[arg(long, global = true)]
    fullwidth: bool,

    /// 关掉某条规则，可重复：--off spaces --off confusables
    #[arg(long, global = true, value_name = "RULE")]
    off: Vec<String>,

    /// 只打印统计报告，不输出正文
    #[arg(long, global = true)]
    report: bool,

    /// 列出所有规则
    #[arg(long, global = true)]
    list_rules: bool,

    /// 守护进程轮询间隔（毫秒），默认 200
    #[arg(long, global = true, default_value_t = 200)]
    interval: u64,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// 常驻后台，每次复制自动净化剪贴板内容
    Watch,
    /// 净化当前剪贴板一次，把结果写回剪贴板
    Once,
    /// 打印默认配置的 JSON，方便改完再 --config 喂回来
    Init,
    /// 把一段文本里的可疑字符列出来（从 stdin 或 -f 读）
    Inspect,
}

fn main() {
    let cli = Cli::parse();

    match run(cli) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("clipsift: {e}");
            std::process::exit(1);
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    if cli.list_rules {
        print_rules();
        return Ok(());
    }

    // 先定计划：默认 → 配置文件 → 命令行开关，后覆盖前。
    let mut plan = match &cli.config {
        Some(path) => config::load(path)?,
        None => Plan::default(),
    };
    if cli.merge {
        plan.merge_hard_returns = true;
    }
    if cli.fullwidth {
        plan.fullwidth_ascii = true;
    }
    for name in &cli.off {
        if !config::disable_rule(&mut plan, name) {
            return Err(format!("不认识的规则名 `{name}`，用 --list-rules 看有哪些"));
        }
    }

    match cli.command {
        Some(Command::Init) => {
            let json = serde_json::to_string_pretty(&plan).map_err(|e| e.to_string())?;
            println!("{json}");
            Ok(())
        }
        Some(Command::Inspect) => {
            let text = read_input(&cli)?;
            inspect(&text);
            Ok(())
        }
        Some(Command::Watch) => watch(plan, cli.interval),
        Some(Command::Once) => once(plan),
        None => {
            let text = read_input(&cli)?;
            let (out, report) = apply(&text, &plan);
            if cli.report {
                // --report 的意思是"只看报告，不要正文"：
                // 报告走 stdout（方便 `| head`、重定向到文件），正文丢掉。
                println!("{}", report.summary());
                if !report.removed.is_empty() {
                    for (cp, n) in &report.removed {
                        println!("  U+{cp:04X}  {n} 次  {}", note(*cp));
                    }
                }
                return Ok(());
            }
            write_output(&cli, &out)
        }
    }
}

/// 从文件或 stdin 读入全部内容。
fn read_input(cli: &Cli) -> Result<String, String> {
    match &cli.file {
        Some(path) => {
            std::fs::read_to_string(path).map_err(|e| format!("读 {} 失败：{e}", path.display()))
        }
        None => {
            let mut buf = String::new();
            io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| format!("读 stdin 失败：{e}"))?;
            Ok(buf)
        }
    }
}

fn write_output(cli: &Cli, text: &str) -> Result<(), String> {
    match &cli.output {
        Some(path) => {
            std::fs::write(path, text).map_err(|e| format!("写 {} 失败：{e}", path.display()))
        }
        None => {
            let mut out = io::stdout().lock();
            // 正文按原始字节写出，不额外加换行
            out.write_all(text.as_bytes())
                .map_err(|e| format!("写 stdout 失败：{e}"))
        }
    }
}

/// 列出规则表和默认开关。
fn print_rules() {
    use clipsift_core::Rule;
    let default_plan = Plan::default();
    let on = default_plan.steps();
    println!("{:<12} {:<20} 默认", "键名", "名称");
    println!("{}", "-".repeat(48));
    for rule in Rule::ALL {
        let enabled = if on.contains(&rule) { "开" } else { "关" };
        println!("{:<12} {:<20} {}", rule.key(), rule.label(), enabled);
    }
    println!();
    println!("自定义规则用 --config 加载 JSON 里的 custom 数组。");
}

/// 列出文本里的可疑字符，给人看"到底脏在哪"。
fn inspect(text: &str) {
    let found = clipsift_core::inspect(text);
    if found.is_empty() {
        println!("干净，没有发现可疑字符。");
        return;
    }
    println!("{:<10} {:<8} {:<8} 备注", "码点", "字符", "次数");
    println!("{}", "-".repeat(56));
    for (c, n) in &found {
        let cp = *c as u32;
        let shown = if (0x0021..=0x007E).contains(&cp) {
            format!("'{c}'")
        } else {
            "·".to_string()
        };
        println!(
            "{:<10} {:<8} {:<8} {}",
            format!("U+{cp:04X}"),
            shown,
            n,
            note(cp)
        );
    }
    println!();
    println!(
        "共 {} 处可疑字符。",
        found.iter().map(|(_, n)| n).sum::<usize>()
    );
}

/// 给常见码点一句人话说明，报告才看得懂。
fn note(cp: u32) -> &'static str {
    match cp {
        0x00A0 | 0x1680 | 0x2000..=0x200A | 0x202F | 0x205F | 0x3000 => "伪空格",
        0x200B => "零宽空格",
        0x200C => "零宽非连接符",
        0x200D => "零宽连接符",
        0x2060 => "单词连接符",
        0xFEFF => "BOM / 零宽不换行空格",
        0x00AD => "软连字符",
        0x061C | 0x200E | 0x200F | 0x202A..=0x202E | 0x2066..=0x2069 => "双向文本控制符",
        0xE0000..=0xE007F => "Unicode 隐形标签",
        0x2018..=0x201F | 0x2032 | 0x2033 | 0x2035 | 0x2036 => "弯引号",
        0x2010..=0x2015 | 0x2212 | 0x2E3A | 0x2E3B => "破折号 / 减号",
        0x2026 | 0x22EF => "省略号",
        0x00B7 => "间隔号",
        0xFF01..=0xFF5E => "全角字符",
        _ => "",
    }
}

// ======================= 剪贴板部分 =======================

/// 一次性净化：读剪贴板 → 净化 → 写回。
fn once(plan: Plan) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let _ = plan;
        Err("剪贴板功能目前只支持 Windows".to_string())
    }

    #[cfg(windows)]
    {
        let text = clipboard_read()?;
        let (out, report) = apply(&text, &plan);
        if out == text {
            println!("剪贴板已经是干净的。");
            return Ok(());
        }
        clipboard_write(&out)?;
        println!("{}", report.summary());
        Ok(())
    }
}

/// 守护进程：轮询剪贴板，发现脏文本就原地换掉。
///
/// 为什么轮询而不是监听：Windows 的剪贴板事件（`AddClipboardFormatListener`）
/// 需要窗口句柄 + 消息循环，为这点功能拉起一个隐藏窗口不值得。200ms 轮询
/// 对人是感觉不到延迟的，CPU 开销也就在万分之一量级。
fn watch(plan: Plan, interval_ms: u64) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let _ = (plan, interval_ms);
        Err("剪贴板功能目前只支持 Windows".to_string())
    }

    #[cfg(windows)]
    {
        if !plan.is_idempotent() {
            eprintln!(
                "警告：当前配置启用了「合并硬回车」，它不是幂等的。\n\
                 守护进程会反复处理同一份文本，建议关掉它（去掉 --merge）。"
            );
        }
        let interval = std::time::Duration::from_millis(interval_ms.max(50));
        println!("ClipSift 守护中（每 {interval_ms}ms 检查一次剪贴板），Ctrl+C 退出。");

        let mut last_seen = String::new();
        let mut cleaned_count = 0usize;
        loop {
            match clipboard_read() {
                Ok(text) => {
                    // 自己写回去的内容会被读回来，靠这个比对跳过，避免自我触发
                    if text == last_seen || text.is_empty() {
                        std::thread::sleep(interval);
                        continue;
                    }
                    last_seen = text.clone();
                    if !clipsift_core::needs_cleanup(&text) {
                        std::thread::sleep(interval);
                        continue;
                    }
                    let (out, report) = apply(&text, &plan);
                    if out == text {
                        std::thread::sleep(interval);
                        continue;
                    }
                    match clipboard_write(&out) {
                        Ok(()) => {
                            last_seen = out;
                            cleaned_count += 1;
                            println!("[{cleaned_count}] {}", report.summary());
                        }
                        Err(e) => eprintln!("写剪贴板失败：{e}"),
                    }
                }
                Err(_) => {
                    // 剪贴板被别的程序占着（很常见，不是错误），下一轮再试
                }
            }
            std::thread::sleep(interval);
        }
    }
}

#[cfg(windows)]
fn clipboard_read() -> Result<String, String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("打不开剪贴板：{e}"))?;
    cb.get_text().map_err(|e| format!("读剪贴板文本失败：{e}"))
}

#[cfg(windows)]
fn clipboard_write(text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("打不开剪贴板：{e}"))?;
    cb.set_text(text.to_string())
        .map_err(|e| format!("写剪贴板失败：{e}"))
}
