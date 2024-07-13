use itertools::Itertools;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Exercise {
    pub segments: Vec<Segment>,
    pub english: String,
}

impl Exercise {
    pub fn parse(input: &str, strict_segmentation: bool, lax_pinyin: bool) -> Option<(Self, &str)> {
        let mut input = input.trim();
        let mut chinese = None;
        let mut pinyin = None;
        let mut english = None;
        for _ in 0..3 {
            let (line, rest) = input.split_once('\n').unwrap_or((input, ""));
            input = rest;
            let (key, value) = line.split_once(':')?;
            match key {
                "Chinese" => chinese = Some(value.trim().to_string()),
                "Pinyin" => pinyin = Some(value.trim().to_string()),
                "English" => english = Some(value.trim().to_string()),
                _ => return None,
            }
        }
        let chinese = chinese?;
        let pinyin = pinyin?;
        let english = english?;
        Some((
            Exercise {
                segments: Segment::join_with(&chinese, &pinyin, strict_segmentation, lax_pinyin),
                english,
            },
            input,
        ))
    }
    pub fn words(&self) -> Vec<&String> {
        let mut ws = self
            .segments
            .iter()
            .filter(|s| !s.pinyin.is_empty())
            .map(|s| &s.chinese)
            .collect::<Vec<_>>();
        ws.dedup();
        ws
    }

    pub fn chinese(&self) -> String {
        self.segments
            .iter()
            .map(|s| s.chinese.clone())
            .collect::<Vec<_>>()
            .join("")
    }
    pub fn pinyin(&self) -> String {
        self.segments
            .iter()
            .map(|s| s.pinyin.clone())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub chinese: String,
    pub pinyin: String,
}

impl Segment {
    // 今天有两个会议。
    // Jīntiān yǒu liǎng gè huìyì.
    // 今天     有  两    个  会议。
    //
    // Chinese: 他也不知道答案。
    // Pinyin: Tā yě bù zhīdào dá'àn.
    //
    // Chinese: 我叫David。你好。
    // Pinyin: Wǒ jiào David. Nǐ hǎo.
    fn join(orig_chinese: &str, orig_pinyin: &str) -> Vec<Self> {
        Self::join_with(orig_chinese, orig_pinyin, true, false)
    }

    fn join_with(
        orig_chinese: &str,
        orig_pinyin: &str,
        strict_segmentation: bool,
        lax_pinyin: bool,
    ) -> Vec<Self> {
        let mut segments: Vec<Self> = vec![];
        let pinyin = orig_pinyin.to_lowercase().replace("'", "");
        let mut pinyin = pinyin.as_str();
        let orig_chinese = orig_chinese.replace(' ', "");
        let mut chinese = orig_chinese.as_str();
        'top: while !chinese.is_empty() {
            pinyin = pinyin.trim_start();
            let results = haoxue_dict::DICTIONARY
                .lookup_entries(chinese)
                .collect::<Vec<_>>();
            if results.is_empty() {
                let (c, new_chinese) = str_pop(chinese).unwrap();
                match segments.last_mut() {
                    Some(s) if s.pinyin.is_empty() => {
                        s.chinese += &c.to_string();
                    }
                    _ => {
                        segments.push(Segment {
                            chinese: c.to_string(),
                            pinyin: "".to_string(),
                        });
                    }
                }
                chinese = new_chinese;
                pinyin = str_tail(&pinyin);
            } else {
                let longest_result = results
                    .iter()
                    .map(|e| e.simplified().chars().count())
                    .max()
                    .unwrap_or_default();
                let n_longest = results
                    .iter()
                    .filter(|e| e.simplified().chars().count() == longest_result)
                    .map(|e| e.pinyin())
                    .unique()
                    .count();
                for (nth, entry) in results.iter().rev().enumerate() {
                    let pretty = prettify_pinyin::prettify(entry.pinyin());
                    let pretty_compact = pretty.to_lowercase().replace(" ", "");
                    let stripped =
                        if lax_pinyin && longest_result >= 2 && n_longest == 1 && nth == 0 {
                            strip_prefix_no_tones(pinyin, &pretty_compact)
                        } else {
                            pinyin.strip_prefix(pretty_compact.as_str())
                        };
                    if let Some(new_pinyin) = stripped {
                        if strict_segmentation && new_pinyin.chars().next().unwrap().is_alphabetic()
                        {
                            panic!(
                                "Segmentation failed at {} at {pinyin}",
                                pretty.to_lowercase().replace(" ", "")
                            );
                        }
                        segments.push(Segment {
                            chinese: entry.simplified().to_string(),
                            pinyin: pretty,
                        });
                        chinese = chinese.strip_prefix(entry.simplified()).unwrap();
                        pinyin = new_pinyin;
                        continue 'top;
                    }
                }
                panic!("Failed to align match {orig_chinese} with {orig_pinyin} at {pinyin}");
            }
        }
        segments
    }
}

fn strip_prefix_no_tones<'a>(mut input: &'a str, mut prefix: &str) -> Option<&'a str> {
    while !input.is_empty() && !prefix.is_empty() {
        let (input_c, input_tail) = str_pop(input)?;
        let (prefix_c, prefix_tail) = str_pop(prefix)?;
        if strip_tone(prefix_c) != strip_tone(input_c) {
            return None;
        }
        input = input_tail;
        prefix = prefix_tail;
    }
    Some(input)
}

fn strip_tone(c: char) -> char {
    let tones = [
        ['ā', 'á', 'ǎ', 'à', 'a'],
        ['ē', 'é', 'ě', 'è', 'e'],
        ['ū', 'ú', 'ǔ', 'ù', 'u'],
        ['ī', 'í', 'ǐ', 'ì', 'i'],
        ['ō', 'ó', 'ǒ', 'ò', 'o'],
        ['ǖ', 'ǘ', 'ǚ', 'ǜ', 'ü'],
        ['Ā', 'Á', 'Ǎ', 'À', 'A'],
        ['Ē', 'É', 'Ě', 'È', 'E'],
        ['Ū', 'Ú', 'Ǔ', 'Ù', 'U'],
        ['Ī', 'Í', 'Ǐ', 'Ì', 'I'],
        ['Ō', 'Ó', 'Ǒ', 'Ò', 'O'],
        ['Ǖ', 'Ǘ', 'Ǚ', 'Ǜ', 'U'],
    ];
    for tone_set in &tones {
        if tone_set.contains(&c) {
            return tone_set[4];
        }
    }
    c
}

fn str_tail(input: &str) -> &str {
    if input.is_empty() {
        return input;
    }
    let mut n = 1;
    while !input.is_char_boundary(n) {
        n += 1;
    }
    &input[n..]
}

fn str_pop(input: &str) -> Option<(char, &str)> {
    let first = input.chars().next()?;
    let mut n = 1;
    while !input.is_char_boundary(n) {
        n += 1;
    }
    Some((first, &input[n..]))
}

// struct Model {
//     word: String,
//     last_seen: DateTime<Utc>,
//     next_prompt: Duration,
// }

// struct Response {
//     chinese: String,
//     pinyin: String,
//     input: String,
// }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn str_pop_1() {
        let (c, rest) = str_pop("abc").unwrap();
        assert_eq!(c, 'a');
        assert_eq!(rest, "bc");
    }

    #[test]
    fn str_pop_2() {
        let (c, rest) = str_pop("今天有两个会议。").unwrap();
        assert_eq!(c, '今');
        assert_eq!(rest, "天有两个会议。");
    }

    #[test]
    fn str_pop_3() {
        let (c, rest) = str_pop("ǒ jiào").unwrap();
        assert_eq!(c, 'ǒ');
        assert_eq!(rest, " jiào");
    }

    #[test]
    fn basic_segment_1() {
        dbg!(Segment::join(
            "今天有两个会议。",
            "Jīntiān yǒu liǎng gè huìyì."
        ));
    }

    #[test]
    fn basic_segment_2() {
        dbg!(Segment::join("我叫David。你好。", "Wǒ jiào David. Nǐhǎo."));
    }

    #[test]
    fn basic_segment_3() {
        dbg!(Segment::join(
            "他答应帮忙，但是忘记了。",
            "Tā dāyìng bāngmáng, dànshì wàngjì le."
        ));
    }

    #[test]
    fn basic_segment_4() {
        dbg!(Segment::join("他也不知道答案。", "Tā yě bù zhīdào dá'àn."));
    }
}
