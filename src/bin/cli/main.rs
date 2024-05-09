// Models + Sentences + Word List -> Prompt
// Prompt -> [Response]
// Model + Response -> Model
// _ -> Model // Initial model
// [Response] -> Model

use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Exercise {
    segments: Vec<Segment>,
    english: String,
}

impl Exercise {
    fn empty() -> Self {
        Exercise {
            segments: Vec::new(),
            english: String::new(),
        }
    }
    fn parse(input: &str) -> Option<(Self, &str)> {
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Segment {
    chinese: String,
    pinyin: String,
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
    fn join(chinese: &str, pinyin: &str) -> Vec<Self> {
        let mut segments: Vec<Self> = vec![];
        let pinyin = pinyin.to_lowercase().replace("'", "");
        let mut pinyin = pinyin.as_str();
        let mut chinese = chinese;
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
                    if let Some(new_pinyin) = pinyin.strip_prefix(&pretty.replace(" ", "")) {
                        segments.push(Segment {
                            chinese: entry.simplified().to_string(),
                            pinyin: pretty,
                        });
                        chinese = chinese.strip_prefix(entry.simplified()).unwrap();
                        pinyin = new_pinyin;
                        continue 'top;
                    }
                }
                panic!("Failed to align match");
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

struct Model {
    word: String,
    last_seen: DateTime<Utc>,
    next_prompt: Duration,
}

struct Response {
    chinese: String,
    pinyin: String,
    input: String,
}

#[derive(Parser, Clone)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Clone)]
enum Command {
    Convert { sentence_file: PathBuf },
    Train,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Convert { sentence_file } => {
            let sentences = std::fs::read_to_string(sentence_file).unwrap();
            let mut exercises = Vec::new();
            let mut rest = sentences.as_str();
            while !rest.trim().is_empty() {
                if let Some((exercise, new_rest)) = Exercise::parse(rest) {
                    dbg!(&exercise);
                    exercises.push(exercise);
                    rest = new_rest;
                } else {
                    panic!("Failed to parse at:\n{rest}");
                }
            }
            dbg!(exercises.len());
        }
        Command::Train => {
            todo!();
        }
    }

    // Load word models
    // Load sentences
    // Load word list

    // Find expired sentences closest to now().
    // Otherwise, find best sentence with next word from list.
    // Prompt sentence
    // Update models
    // Repeat

    // Ok(())
}

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
