// Models + Sentences + Word List -> Prompt
// Prompt -> [Response]
// Model + Response -> Model
// _ -> Model // Initial model
// [Response] -> Model

use std::path::PathBuf;

use anes::*;
use clap::{Parser, Subcommand};
use itertools::Either;
use ordered_float::OrderedFloat;

use std::error::Error;

use std::fs::File;
use std::io::{Read, Write};

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
    },
    Train {
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
    },
}

#[derive(clap::ValueEnum, Clone, Default, Debug)]
enum OutputFormat {
    #[default]
    Human,
    CSV,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    match cli.command {
        Command::Convert { sentence_file } => {
            let sentences = std::fs::read_to_string(sentence_file).unwrap();
            let mut rest = sentences.as_str();
            while !rest.trim().is_empty() {
                if let Some((exercise, new_rest)) = Exercise::parse(rest) {
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
        Command::Tile {
            word_file,
            exercise_files,
            assumed_file,
            output_format,
        } => {
            let dict = Dictionary::new();

            let mut exercises: Vec<Exercise> = vec![];
            for exercise_file in exercise_files {
                let contents = std::fs::read_to_string(exercise_file)?;
                exercises.extend(serde_yaml::from_str::<Vec<Exercise>>(&contents)?);
            }

            let words = load_words(&dict, word_file)?;
            let assumed_words = if let Some(assumed_file) = assumed_file {
                load_words(&dict, assumed_file)?
            } else {
                vec![]
            };
            let course = Course::new(
                words
                    .clone()
                    .into_iter()
                    .chain(assumed_words.into_iter())
                    .collect(),
            );
            for word in words {
                match output_format {
                    OutputFormat::Human => println!("{}", word),
                    OutputFormat::CSV => print!("{}\t", word),
                }

                let mut costs = exercises
                    .iter()
                    .filter(|e| e.words().iter().any(|w| w.as_str() == word))
                    .map(|e| (e, course.exercise_cost(e)))
                    .collect::<Vec<_>>();
                costs.sort_by(|a, b| a.1.cmp(&b.1));
                match output_format {
                    OutputFormat::Human => {
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
                        if let Some(cost) = costs.first() {
                            println!("{}\t{}", costs[0].0.english, costs[0].0.chinese());
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

fn load_words(dict: &Dictionary, file: PathBuf) -> anyhow::Result<Vec<String>> {
    let contents = std::fs::read_to_string(file)?;
    let entries = dict.segment(&contents);

    let mut words = vec![];
    for entry in entries {
        if let Either::Left(word) = entry {
            words.push(word.simplified().to_string());
        }
    }
    Ok(words)
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
    known_exercises: Vec<Exercise>,
    course_exercises: Vec<Exercise>,
    word_list: Vec<String>,
}

impl Course {
    fn new(words: Vec<String>) -> Self {
        Course {
            known_exercises: vec![],
            course_exercises: vec![],
            word_list: words,
        }
    }

    fn exercise_cost(&self, exercise: &Exercise) -> ExerciseCost {
        let mut seen_words = self
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
            n_total_words: exercise.words().len(),
        }
    }
}
