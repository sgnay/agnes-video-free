//! 风格注册表：按 id 查找 / 枚举全部风格档案。
//!
//! 当前仅含 realistic 真实感风格族；`crayon` / `textbook`（继承自上游项目）
//! 在 M0/M3 按 PLAN.md 补齐。

pub mod realistic;

use crate::models::StyleProfile;

/// 全部可用风格档案。
pub fn all() -> Vec<StyleProfile> {
    realistic::profiles()
}

/// 按 id 查找风格档案（如 `realistic-cinematic`）。
pub fn by_id(id: &str) -> Option<StyleProfile> {
    all().into_iter().find(|s| s.id == id)
}

/// 全部风格 id（供错误提示与交互选择）。
pub fn ids() -> Vec<String> {
    all().into_iter().map(|s| s.id.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_three_realistic_styles() {
        let all = all();
        assert_eq!(all.len(), 3);

        assert!(by_id("realistic-cinematic").is_some());
        assert!(by_id("realistic-vlog").is_some());
        assert!(by_id("realistic-documentary").is_some());
        // crayon/textbook 尚未落地
        assert!(by_id("crayon").is_none());
        assert!(by_id("textbook").is_none());
    }
}
