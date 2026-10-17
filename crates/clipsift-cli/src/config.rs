//! 配置文件读写。
//!
//! 配置文件就是 `Plan` 的 JSON 序列化结果，`clipsift init > clipsift.json`
//! 生成一份，改完再用 `--config clipsift.json` 喂回来。
//!
//! 两个原则：
//! 1. **读配置失败要能说清楚错在哪**，而不是抛一个 serde 的英文报错。
//! 2. **不认识的键必须报错**（`Plan` 上已经有 `deny_unknown_fields`），
//!    否则用户拼错 `merge_hard_returns` 会静默不生效——这类"我明明配了"的
//!    bug 最难查。

use clipsift_core::{Plan, Rule};
use std::path::Path;

/// 读取并解析配置文件。
pub fn load(path: &Path) -> Result<Plan, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("读配置文件 {} 失败：{e}", path.display()))?;
    let plan: Plan = serde_json::from_str(&text).map_err(|e| {
        format!(
            "配置文件 {} 解析失败：{e}\n\
             提示：可以先跑 `clipsift init > config.json` 生成一份默认配置再改。",
            path.display()
        )
    })?;
    Ok(plan)
}

/// 按规则键名关掉一条规则。返回 `false` 表示名字不认识。
///
/// 键名与 [`Rule::key`] 一致，另外兼容几个好记的别名（比如 `whitespace`）。
pub fn disable_rule(plan: &mut Plan, name: &str) -> bool {
    // 统一大小写，允许 `--off Merge` 这种写法
    let key = name.trim().to_ascii_lowercase();
    let key = match key.as_str() {
        // 别名：让人凭直觉就能写对
        "ws" | "whitespace" | "space" => "spaces",
        "quote" | "quotes" | "confusable" => "confusables",
        "halfwidth" | "full-width" => "fullwidth",
        "control-char" | "ctrl" => "control",
        "trailing" | "trailing-ws" => "trim",
        "zero-width" | "zw" | "invisible-chars" => "invisible",
        "collapse" => "collapse-ws",
        "merge-returns" | "hard-return" => "merge",
        other => other,
    };

    match key {
        "invisible" => {
            plan.invisible = false;
            true
        }
        "spaces" => {
            plan.normalize_spaces = false;
            true
        }
        "confusables" => {
            plan.confusables = false;
            true
        }
        // `fullwidth` 是老名字，一把关掉两档；也可以单独关其中一档。
        "fullwidth" => {
            plan.fullwidth_alnum = false;
            plan.fullwidth_punct = false;
            true
        }
        "fullwidth-alnum" | "fullwidth-letters" => {
            plan.fullwidth_alnum = false;
            true
        }
        "fullwidth-punct" | "fullwidth-punctuation" => {
            plan.fullwidth_punct = false;
            true
        }
        "merge" => {
            plan.merge_hard_returns = false;
            true
        }
        "collapse-ws" => {
            plan.collapse_whitespace = false;
            true
        }
        "trim" => {
            plan.trim_trailing = false;
            true
        }
        "control" => {
            plan.strip_control = false;
            true
        }
        // 自定义规则没有开关，只能靠配置文件的 enable 字段控制
        _ => false,
    }
}

/// 所有可用键名，报错时列出来给用户看。
#[allow(dead_code)]
pub fn rule_keys() -> Vec<&'static str> {
    Rule::ALL.iter().map(|r| r.key()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_rule_key_can_be_disabled() {
        // Custom 没法用 --off 关（它靠配置文件的 enable 字段），跳过
        for rule in Rule::ALL.iter().filter(|r| **r != Rule::Custom) {
            let mut plan = Plan::default();
            assert!(
                disable_rule(&mut plan, rule.key()),
                "键名 {} 应该能被关掉",
                rule.key()
            );
        }
    }

    #[test]
    fn aliases_work() {
        let mut plan = Plan::default();
        assert!(disable_rule(&mut plan, "ZERO-WIDTH"));
        assert!(!plan.invisible);
        assert!(disable_rule(&mut plan, " merge-returns "));
        assert!(!plan.merge_hard_returns);
    }

    #[test]
    fn unknown_key_is_rejected() {
        let mut plan = Plan::default();
        assert!(!disable_rule(&mut plan, "not-a-rule"));
        // 自定义规则没法用 --off 关，要写配置文件
        assert!(!disable_rule(&mut plan, "custom"));
    }

    #[test]
    fn roundtrip_json() {
        let plan = Plan {
            merge_hard_returns: true,
            ..Plan::default()
        };
        let json = serde_json::to_string(&plan).unwrap();
        let back: Plan = serde_json::from_str(&json).unwrap();
        assert!(back.merge_hard_returns);
        assert_eq!(back.steps().len(), plan.steps().len());
    }

    #[test]
    fn typo_in_json_is_rejected() {
        // 拼错的键必须报错，不能静默忽略
        let bad = r#"{"merge_hard_return": true}"#;
        let r: Result<Plan, _> = serde_json::from_str(bad);
        assert!(r.is_err(), "拼错的键名应该报错");
    }
}
