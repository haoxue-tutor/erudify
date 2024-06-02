use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Exercise {
    pub segments: Vec<Segment>,
    pub english: String,
}

impl Exercise {
    pub fn parse(input: &str) -> Option<(Self, &str)> {
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
                segments: Segment::join(&chinese, &pinyin),
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
                for entry in results.into_iter().rev() {
                    let pretty = prettify_pinyin::prettify(entry.pinyin());
                    // dbg!(&pretty);
                    // dbg!(&pinyin);
                    if let Some(new_pinyin) =
                        pinyin.strip_prefix(&pretty.to_lowercase().replace(" ", ""))
                    {
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
