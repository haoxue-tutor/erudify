// Models + Sentences + Word List -> Prompt
// Prompt -> [Response]
// Model + Response -> Model
// _ -> Model // Initial model
// [Response] -> Model

use std::ops::Deref;
use std::path::{Path, PathBuf};

use anes::*;
use bytes::buf::Reader;
use bytes::{Buf, Bytes};
use clap::{Parser, Subcommand};
use itertools::Either;
use openai_dive::v1::api::Client;
use openai_dive::v1::models::TTSEngine;
use openai_dive::v1::resources::audio::{
    AudioSpeechParameters, AudioSpeechResponseFormat, AudioVoice,
};
use ordered_float::OrderedFloat;
use rodio::{Decoder, OutputStream, Sink, Source};

use std::error::Error;

use std::fs::File;
use std::io::{BufReader, Read, Write};

use haoxue_dict::Dictionary;

mod convert;
use convert::Exercise;

mod train;
use train::train;

#[derive(Parser, Clone)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Clone)]
enum Command {
    Convert {
        sentence_file: PathBuf,
        #[arg(long)]
        lax_segmentation: bool,
        #[arg(long)]
        strict_pinyin: bool,
    },
    Train {
        exercise_file: PathBuf,
    },
    Audio {
        exercise_file: PathBuf,
    },
    Tile {
        word_file: PathBuf,
        #[arg(long)]
        exercise_files: Vec<PathBuf>,
        // Optional parameter for assumed words.
        #[arg(long)]
        assumed_file: Option<PathBuf>,
        #[arg(long, value_enum)]
        output_format: OutputFormat,
        #[arg(long)]
        frequency_sort: bool,
    },
}

#[derive(clap::ValueEnum, Clone, Default, Debug)]
enum OutputFormat {
    #[default]
    Human,
    CSV,
    YAML,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    match cli.command {
        Command::Convert {
            sentence_file,
            lax_segmentation,
            strict_pinyin,
        } => {
            let sentences = std::fs::read_to_string(sentence_file).unwrap();
            let mut rest = sentences.as_str();
            while !rest.trim().is_empty() {
                if let Some((exercise, new_rest)) =
                    Exercise::parse(rest, !lax_segmentation, !strict_pinyin)
                {
                    println!("{}", serde_yaml::to_string(&[exercise]).unwrap());
                    rest = new_rest;
                } else {
                    panic!("Failed to parse at:\n{}", rest.trim());
                }
            }
        }
        Command::Train { exercise_file } => {
            // Chinese: 我是学生。
            // Pinyin:  wǒ shì xuéshēng.
            // English: I am a student.
            // Answer:  wǒ

            let mut file = File::open(exercise_file)?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;

            let exercises: Vec<Exercise> = serde_yaml::from_str(&contents)?;

            train(exercises)?;
        }
        Command::Audio { exercise_file } => {
            let mut file = File::open(exercise_file)?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;

            let exercises: Vec<Exercise> = serde_yaml::from_str(&contents)?;

            let api_key = std::env::var("OPENAI_API_KEY").expect("$OPENAI_API_KEY is not set");
            let client = Client::new(api_key);

            let (_stream, stream_handle) = OutputStream::try_default().unwrap();
            let sink = Sink::try_new(&stream_handle).unwrap();

            for exercise in exercises {
                validate_audio(
                    &client,
                    &sink,
                    &exercise.chinese(),
                    Some(&exercise.pinyin()),
                )
                .await;
                validate_audio(&client, &sink, &exercise.english, None).await;
            }
        }
        Command::Tile {
            word_file,
            exercise_files,
            assumed_file,
            output_format,
            frequency_sort,
        } => {
            let dict = Dictionary::new();

            let mut exercises: Vec<Exercise> = vec![];
            for exercise_file in exercise_files {
                let contents = std::fs::read_to_string(exercise_file)?;
                exercises.extend(serde_yaml::from_str::<Vec<Exercise>>(&contents)?);
            }

            let mut words = load_words(&dict, word_file)?;
            if frequency_sort {
                words.sort_by(|a, b| dict.frequency(b).total_cmp(&dict.frequency(a)));
            }
            let assumed_words = if let Some(assumed_file) = assumed_file {
                load_words(&dict, assumed_file)?
            } else {
                vec![]
            };
            let mut course = Course::new(
                words
                    .clone()
                    .into_iter()
                    .chain(assumed_words.into_iter())
                    .collect(),
            );
            for word in words {
                let mut costs = exercises
                    .iter()
                    .filter(|e| e.words().iter().any(|w| w.as_str() == word))
                    .map(|e| (e, course.exercise_cost(e)))
                    .collect::<Vec<_>>();
                costs.sort_by(|a, b| a.1.cmp(&b.1));
                match output_format {
                    OutputFormat::Human => {
                        println!("{}", word);
                        if costs.is_empty() {
                            anes::execute!(
                                std::io::stdout(),
                                SetForegroundColor(Color::Red),
                                "  No exercises.\n",
                                ResetAttributes,
                            )?;
                        } else if costs[0].1.n_novel_words == 0 {
                            let e = costs[0].0;
                            execute!(
                                std::io::stdout(),
                                SetForegroundColor(Color::Green),
                                "  Free: ",
                                ResetAttributes,
                            )?;
                            println!("{}", e.english);
                        } else {
                            execute!(
                                std::io::stdout(),
                                SetForegroundColor(Color::Yellow),
                                "  Costly\n",
                                ResetAttributes,
                            )?;
                            for cost in costs.iter().take(5) {
                                let e = cost.0;
                                println!("  {} {:?}", e.english, cost.1);
                            }
                        }
                    }
                    OutputFormat::CSV => {
                        for cost in costs.iter().take(1) {
                            if cost.1.n_novel_words > 0 {
                                break;
                            }
                            print!(
                                "{}/{}/{}\t",
                                cost.1.n_novel_words,
                                cost.1.n_future_words,
                                cost.1.n_extraneous_words
                            );
                            println!("{}\t{}\t{}", word, cost.0.english, cost.0.chinese());
                            course.push_exercise(cost.0.clone());
                        }
                    }
                    OutputFormat::YAML => {
                        for cost in costs.iter().take(1) {
                            if cost.1.n_novel_words > 0 {
                                break;
                            }
                            println!("{}", serde_yaml::to_string(&[cost.0]).unwrap());
                            course.push_exercise(cost.0.clone());
                        }
                    }
                }
            }

            // let contents = std::fs::read_to_string(exercise_file)?;
            // let exercises: Vec<Exercise> = serde_yaml::from_str(&contents)?;
        }
    }
    Ok(())
}

async fn validate_audio(client: &Client, sink: &Sink, text: &str, hint: Option<&str>) {
    let audio_file = audio_file_name(text);
    while !audio_file.exists() {
        println!("Text: {}", text);
        if let Some(hint) = hint {
            println!("Hint: {}", hint);
        }
        let parameters = AudioSpeechParameters {
            model: TTSEngine::Tts1.to_string(),
            input: text.to_string(),
            voice: AudioVoice::Nova,
            response_format: Some(AudioSpeechResponseFormat::Mp3),
            speed: Some(1.0),
        };

        let response = client.audio().create_speech(parameters).await.unwrap();

        // response.save(audio_file).await.unwrap();
        {
            let file = BufReader::new(std::io::Cursor::new(response.bytes.to_vec()));
            // Decode that sound file into a source
            let source = Decoder::new(file).unwrap();
            // Play the sound directly on the device
            sink.append(source);
        }
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        if input == "y\n" {
            // sink.stop();
            // sink.clear();
            std::fs::write(&audio_file, response.bytes).unwrap();
        }
    }
}

fn audio_file_name(text: &str) -> PathBuf {
    PathBuf::from(format!(
        "audio/{}.mp3",
        text.chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>()
            .to_lowercase()
            .replace(" ", "_")
    ))
}

fn load_words(dict: &Dictionary, file: PathBuf) -> anyhow::Result<Vec<String>> {
    let contents = std::fs::read_to_string(file)?;
    let entries = dict.segment(&contents);

    Ok(entries
        .into_iter()
        .filter_map(Either::left)
        .map(|e| e.simplified().to_string())
        .collect::<Vec<_>>())
}

#[derive(Debug, Ord, PartialOrd, PartialEq, Eq)]
struct ExerciseCost {
    // Freq cost of the least used new word.
    word_freq_cost: OrderedFloat<f64>,
    // Number of new words _not_ in course and _not_ in seen exercises.
    n_novel_words: usize,
    // Number of new words _not_ in seen exercises but _in_ course.
    n_future_words: usize,
    // Number of words _not_ in course but _in_ seen exercises.
    n_extraneous_words: usize,
    // Number of words in the exercise.
    n_total_words: usize,
}

// Each exercise has a set of words. The "cost" of an exercise is the maximum cost of any word in
// the exercise.
struct Course {
    course_exercises: Vec<Exercise>,
    word_list: Vec<String>,
}

impl Course {
    fn new(words: Vec<String>) -> Self {
        Course {
            course_exercises: vec![],
            word_list: words,
        }
    }

    fn push_exercise(&mut self, exercise: Exercise) {
        self.course_exercises.push(exercise);
    }

    fn exercise_cost(&self, exercise: &Exercise) -> ExerciseCost {
        let seen_words = self
            .course_exercises
            .iter()
            .flat_map(|e| e.words())
            .collect::<Vec<_>>();

        let exercise_words = exercise.words();
        let novel_words = exercise_words
            .iter()
            .filter(|w| !seen_words.contains(w))
            .filter(|w| !self.word_list.contains(w))
            .count();
        let future_words = exercise_words
            .iter()
            .filter(|w| !seen_words.contains(w))
            .filter(|w| self.word_list.contains(w))
            .count();
        let extraneous_words = exercise_words
            .iter()
            .filter(|w| seen_words.contains(w))
            .filter(|w| !self.word_list.contains(w))
            .count();
        ExerciseCost {
            word_freq_cost: OrderedFloat(0_f64),
            n_novel_words: novel_words,
            n_future_words: future_words,
            n_extraneous_words: extraneous_words,
            n_total_words: exercise.chinese().chars().count(),
        }
    }
}
