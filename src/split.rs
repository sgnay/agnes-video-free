//! 分句：把故事文本切成「一句一拍」，每句对应一段视频。
//!
//! 规则（继承自上游 story-handdrawn-video）：
//! - 中文按 `。！？；` 切，英文按 `. ! ? ;` 切，标点保留在句尾（利于 TTS 自然停顿）
//! - 超长句再按逗号/连接词切（中文 softLimit 36 字，英文 120 字符）
//! - 自然段用空行分隔，每行独立处理

use crate::models::Lang;

/// 中文句末标点。
const ZH_TERMINATORS: [char; 4] = ['。', '！', '？', '；'];
/// 英文句末标点。
const EN_TERMINATORS: [char; 4] = ['.', '!', '?', ';'];

/// 中文连接词（超长句在此切分，连接词归入后半句）。
const ZH_CONJUNCTIONS: [&str; 17] = [
    "但是", "可是", "不过", "然而", "因为", "所以", "于是", "接着", "然后", "突然", "忽然", "只见",
    "这时", "后来", "最终", "终于", "虽然",
];

/// 英文连接词（前后带空格，避免误匹配单词内部）。
const EN_CONJUNCTIONS: [&str; 20] = [
    " and ",
    " but ",
    " so ",
    " because ",
    " then ",
    " when ",
    " while ",
    " after ",
    " before ",
    " however ",
    " although ",
    " if ",
    " until ",
    " since ",
    " which ",
    " that ",
    " who ",
    " with ",
    " where ",
    " as ",
];

/// 把整篇故事切成句子列表。
pub fn split_story(text: &str, lang: Lang) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        for sentence in split_sentences(line, lang) {
            for part in enforce_soft_limit(sentence, lang) {
                let part = part.trim();
                if !part.is_empty() {
                    out.push(part.to_string());
                }
            }
        }
    }
    out
}

/// 按句末标点切成句子（标点保留在句尾）。
fn split_sentences(line: &str, lang: Lang) -> Vec<String> {
    let terminators = match lang {
        Lang::Zh => &ZH_TERMINATORS[..],
        Lang::En => &EN_TERMINATORS[..],
    };
    let mut out = Vec::new();
    let mut buf = String::new();
    for ch in line.chars() {
        buf.push(ch);
        if terminators.contains(&ch) {
            out.push(buf.trim().to_string());
            buf.clear();
        }
    }
    if !buf.trim().is_empty() {
        out.push(buf.trim().to_string());
    }
    out
}

/// 超长句按逗号/连接词再切，保证每句 ≤ softLimit。
fn enforce_soft_limit(sentence: String, lang: Lang) -> Vec<String> {
    let limit = lang.soft_limit();
    let mut parts = Vec::new();
    let mut rest = sentence;
    while rest.chars().count() > limit {
        let prefix: String = rest.chars().take(limit).collect();
        let mut cut = find_split_point(&prefix, lang).unwrap_or(prefix.len());
        // 切点太靠前会产出碎片，退回硬切
        if cut < 4 {
            cut = prefix.len();
        }
        let (head, tail) = rest.split_at(cut);
        parts.push(head.trim().to_string());
        rest = tail.trim_start().to_string();
    }
    if !rest.is_empty() {
        parts.push(rest);
    }
    parts
}

/// 在前 `limit` 个字符内找最后一个可切分点（返回字节索引）：
/// 中文在 `，、` 之后切，英文在 `,` 之后切；连接词在词首切（归入后半句）。
fn find_split_point(prefix: &str, lang: Lang) -> Option<usize> {
    let mut best: Option<usize> = None;
    match lang {
        Lang::Zh => {
            for (i, ch) in prefix.char_indices() {
                if ch == '，' || ch == '、' {
                    best = Some(i + ch.len_utf8());
                }
            }
            for conj in ZH_CONJUNCTIONS {
                if let Some(i) = prefix.rfind(conj) {
                    best = best.max(Some(i));
                }
            }
        }
        Lang::En => {
            if let Some(i) = prefix.rfind(',') {
                best = Some(i + 1);
            }
            for conj in EN_CONJUNCTIONS {
                if let Some(i) = prefix.rfind(conj) {
                    best = best.max(Some(i));
                }
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zh_splits_by_terminators_keeping_punctuation() {
        let out = split_story("下雨天。我在巷口捡到一只橘猫！", Lang::Zh);
        assert_eq!(out, vec!["下雨天。", "我在巷口捡到一只橘猫！"]);
    }

    #[test]
    fn zh_resplits_long_sentence_at_comma() {
        let long = "清晨的阳光透过窗帘洒进房间，照在桌上的那杯还冒着热气的牛奶和一本翻开的书上。";
        let out = split_story(long, Lang::Zh);
        assert_eq!(out.len(), 2, "{out:?}");
        for s in &out {
            assert!(s.chars().count() <= Lang::ZH_SOFT_LIMIT, "超长: {s}");
        }
        assert!(out[0].ends_with('，'));
    }

    #[test]
    fn zh_resplits_at_conjunction() {
        // 37 字 > 36，无逗号，含连接词「但是」（避开与「终于」同时出现）
        let long = format!(
            "{}但是总算到家了。",
            "他走了很远很远很远很远很远很远很远很远很远很远很远很远的路"
        );
        let out = split_story(&long, Lang::Zh);
        assert_eq!(out.len(), 2, "{out:?}");
        assert!(out[0].ends_with("路"));
        assert!(out[1].starts_with("但是"));
        for s in &out {
            assert!(s.chars().count() <= Lang::ZH_SOFT_LIMIT);
        }
    }

    #[test]
    fn en_splits_by_terminators() {
        let out = split_story("The little rabbit hopped. Then it ate a carrot!", Lang::En);
        assert_eq!(
            out,
            vec!["The little rabbit hopped.", "Then it ate a carrot!"]
        );
    }

    #[test]
    fn en_resplits_long_sentence() {
        let long = "The little rabbit hopped through the green meadow and the tall grass and the wild flowers and the flowing stream, but it never found the carrot field it was looking for.";
        let out = split_story(long, Lang::En);
        assert!(out.len() >= 2, "{out:?}");
        for s in &out {
            assert!(s.chars().count() <= Lang::EN_SOFT_LIMIT, "超长: {s}");
        }
    }

    #[test]
    fn blank_lines_are_skipped() {
        let out = split_story("第一句。\n\n第二句！\n", Lang::Zh);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn line_without_terminator_stays_whole() {
        let out = split_story("没有标点的一行", Lang::Zh);
        assert_eq!(out, vec!["没有标点的一行"]);
    }
}
