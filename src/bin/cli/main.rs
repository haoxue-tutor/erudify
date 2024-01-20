// Models + Sentences + Word List -> Prompt
// Prompt -> [Response]
// Model + Response -> Model
// _ -> Model // Initial model
// [Response] -> Model

use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
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
            let (line, rest) = input.split_once('\n')?;
            input = rest;
            let (key, value) = line.split_once(':')?;
            match key {
                "Chinese" => chinese = Some(value.to_string()),
                "Pinyin" => pinyin = Some(value.to_string()),
                "English" => english = Some(value.to_string()),
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
    // 今天 有 两 个 会议。
    fn join(chinese: &str, pinyin: &str) -> Vec<Self> {
        // split chinese into chars.
        // split pinyin into words.
        // for each word, consume chinese letters
        let mut c_chars = chinese.chars();
        let tmp = pinyin.to_lowercase();
        let p_words = tmp.split_ascii_whitespace();
        p_words
            .map(|mut word| {
                let mut out_chinese = String::new();
                let mut out_pinyin = String::new();
                'outer: while !word.is_empty() {
                    dbg!(&word);
                    let token = c_chars.next().unwrap().to_string();
                    dbg!(&token);
                    for entry in haoxue_dict::DICTIONARY.lookup_entries(&token) {
                        let pretty = prettify_pinyin::prettify(entry.pinyin());
                        dbg!(&pretty);
                        dbg!(&word.strip_prefix(&pretty));
                        if let Some(new_word) = word.strip_prefix(&pretty) {
                            out_chinese += &token;
                            out_pinyin += &pretty;
                            word = new_word;
                            continue 'outer;
                        }
                    }
                    out_chinese += &token;
                    word = str_tail(word);
                }
                Segment {
                    chinese: out_chinese,
                    pinyin: out_pinyin,
                }
            })
            .collect()
    }
}

fn str_tail(input: &str) -> &str {
    let mut n = 1;
    while !input.is_char_boundary(n) {
        n += 1;
    }
    &input[n..]
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
    command: Option<Command>,
}

#[derive(Subcommand, Clone)]
enum Command {
    Convert { sentence_file: PathBuf },
    Train,
}

fn main() {
    let cli = Cli::parse();

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
    fn basic_segment_1() {
        dbg!(Segment::join(
            "今天有两个会议。",
            "Jīntiān yǒu liǎng gè huìyì."
        ));
    }

    #[test]
    fn basic_segment_2() {
        dbg!(Segment::join("我叫David。你好。", "Wǒ jiào David. Nǐ hǎo."));
    }
}
