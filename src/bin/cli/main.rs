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
    fn join(chinese: &str, pinyin: &str) -> Vec<Self> {
        // split chinese into chars.
        // split pinyin into words.
        // for each word, consume chinese letters
        let mut c_chars = chinese.chars();
        let mut chin = chinese;
        let tmp = pinyin.to_lowercase().replace("'", "");
        let p_words = tmp.split(|c| char::is_ascii_whitespace(&c));
        p_words
            .filter(|word| !word.is_empty())
            .map(|mut word| {
                let mut out_chinese = String::new();
                let mut out_pinyin = String::new();
                'outer: while !word.is_empty() {
                    dbg!(&word);
                    dbg!(&chin);
                    // let token = c_chars.next().unwrap().to_string();
                    // dbg!(&token);
                    let results = haoxue_dict::DICTIONARY
                        .lookup_entries(chin)
                        .collect::<Vec<_>>();
                    for entry in results.into_iter().rev() {
                        let pretty = prettify_pinyin::prettify(entry.pinyin()).replace(" ", "");
                        dbg!(&pretty);
                        dbg!(&word.strip_prefix(&pretty));
                        if let Some(new_word) = word.strip_prefix(&pretty) {
                            chin = chin.strip_prefix(entry.simplified()).unwrap();
                            out_chinese += entry.simplified();
                            out_pinyin += &pretty;
                            word = new_word;
                            continue 'outer;
                        }
                    }
                    if word.chars().all(|c| {
                        char::is_whitespace(c)
                            || char::is_ascii_punctuation(&c)
                            || char::is_ascii_digit(&c)
                    }) {
                        out_chinese += chin.chars().next().unwrap().to_string().as_str();
                        chin = str_tail(chin);
                        word = str_tail(word);
                        continue 'outer;
                    }
                    panic!("Failed to find match");
                    // out_chinese += &token;
                    // word = str_tail(word);
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

    #[test]
    fn basic_segment_3() {
        dbg!(Segment::join(
            "他答应帮忙，但是忘记了。",
            "Tā dāyìng bāngmáng, dànshì wàngjì le."
        ));
    }
}
