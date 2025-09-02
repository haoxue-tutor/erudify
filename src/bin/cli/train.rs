use std::{
    fs::File,
    io::{self, BufReader},
};

use chrono::{Duration, Utc};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    layout::Offset,
    prelude::*,
    widgets::{Block, List, ListItem, Paragraph},
};

use rodio::{Decoder, OutputStream, Sink};
use tui_input::{backend::crossterm::EventHandler, Input};

use crate::{
    convert::Exercise,
    model::{ExerciseScore, UserModel},
};

struct App {
    _audio_stream: OutputStream,
    _audio_sink: Sink,
    word_list: Vec<String>,
    model: UserModel,
    exercise_score: ExerciseScore,
    target_word: String,
    exercises: Vec<Exercise>,
    exercise: Exercise,
    index: usize,
    input: Input,
    // show_english: bool,
    show_hint: bool,
    history: Vec<Exercise>,
}

impl App {
    fn new(word_list: Vec<String>, exercises: Vec<Exercise>) -> Self {
        let model = UserModel::load().unwrap_or_default();
        let target_word = model.next_word(Utc::now(), &word_list);
        let exercise = model
            .next_exercise(Utc::now(), &exercises, &word_list, &target_word)
            .unwrap();
        let exercise_score = model.score_exercise(Utc::now(), &exercise, &word_list);
        let (stream, stream_handle) = OutputStream::try_default().unwrap();
        App {
            _audio_stream: stream,
            _audio_sink: Sink::try_new(&stream_handle).unwrap(),
            word_list,
            model,
            exercise_score,
            target_word,
            exercises,
            exercise,
            index: 0,
            input: Input::new("".into()),
            // show_english: false,
            show_hint: false,
            history: vec![],
        }
    }
}

pub fn train(
    word_list: Vec<String>,
    mut exercises: Vec<Exercise>,
) -> Result<(), Box<dyn std::error::Error>> {
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    exercises.reverse();
    let app = App::new(word_list, exercises);
    let res = run_app(&mut terminal, app);

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{err:?}");
    }

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, &app))?;

        let evt = event::read()?;
        if let Event::Key(key) = &evt {
            match key.code {
                KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                    return Ok(())
                }
                KeyCode::Esc => {
                    app.show_hint = true;
                }
                _ => {}
            }
        }
        app.input.handle_event(&evt);

        let cursor = app.input.cursor();
        let pinyin = apply_tones(app.input.value());
        let pinyin_len = pinyin.chars().count();
        app.input = Input::new(pinyin)
            .with_cursor(cursor - (app.input.value().chars().count() - pinyin_len));

        while app.index < app.exercise.segments.len() {
            let target = &app.exercise.segments[app.index];
            if target
                .pinyin
                .to_lowercase()
                .replace(char::is_whitespace, "")
                == app
                    .input
                    .value()
                    .trim()
                    .to_lowercase()
                    .replace(char::is_whitespace, "")
            {
                if !target.pinyin.is_empty() {
                    let now = Utc::now();
                    let prof = app
                        .model
                        .with_proficiency(&app.exercise.segments[app.index].chinese, now);
                    if app.show_hint {
                        // Reset memory strength
                        prof.fail(now);
                    } else {
                        // Increase memory strength
                        prof.success(now);
                    }
                    app.model.store().unwrap();
                }
                app.index += 1;
                app.input = Input::new("".into());
                app.show_hint = false;
            } else {
                break;
            }
        }
        if app.index >= app.exercise.segments.len() {
            // {
            //     let clean_name = app
            //         .exercise
            //         .chinese()
            //         .chars()
            //         .filter(|c| c.is_alphanumeric())
            //         .collect::<String>();
            //     // dbg!(&clean_name);
            //     let file = BufReader::new(File::open(format!("audio/{}.mp3", clean_name)).unwrap());
            //     // Decode that sound file into a source
            //     let source = Decoder::new(file).unwrap();
            //     app.audio_sink.append(source);
            // }
            app.model.mark_seen(&app.exercise, Utc::now());
            app.history.push(app.exercise.clone());
            app.target_word = app.model.next_word(Utc::now(), &app.word_list);
            let exercise = app
                .model
                .next_exercise(Utc::now(), &app.exercises, &app.word_list, &app.target_word)
                .unwrap();
            app.exercise_score = app
                .model
                .score_exercise(Utc::now(), &exercise, &app.word_list);
            app.exercise = exercise;
            app.index = 0;
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let vertical = Layout::vertical([
        Constraint::Length(1), // Status: target word
        Constraint::Length(1), // Exercise score
        Constraint::Length(1), // Chinese
        Constraint::Length(1), // Pinyin
        Constraint::Length(1), // Hint
        Constraint::Min(1),    // History
    ]);
    let [status_area, exercise_score_area, help_area, pinyin_area, hint_area, messages_area] =
        vertical.areas(f.size());

    let model_status = app.model.status(&app.exercises, &app.word_list, Utc::now());
    let status = Paragraph::new(format!(
        "Target word: {}, known words: {}, to review: {}, total: {}, sentences: {}/{}",
        app.target_word,
        model_status.known_words,
        model_status.words_to_review,
        model_status.total_words,
        model_status.seen_sentences,
        model_status.unlocked_sentences
    ));
    f.render_widget(status, status_area);

    let exercise_score = Paragraph::new(format!("Exercise score: {:?}", app.exercise_score));
    f.render_widget(exercise_score, exercise_score_area);

    let mut msg = vec![];
    msg.push("Chinese: ".into());
    for (nth, segment) in app.exercise.segments.iter().enumerate() {
        let span: Span = segment.chinese.clone().into();
        if nth == app.index {
            msg.push(span.bold().fg(Color::Yellow));
        } else {
            msg.push(span);
        }
    }
    let text = Text::from(Line::from(msg));
    let help_message = Paragraph::new(text);
    f.render_widget(help_message, help_area);

    let mut pinyin_msgs: Vec<Span> = vec![];
    pinyin_msgs.push("Pinyin:  ".into());
    for segment in app.exercise.segments.iter().take(app.index) {
        let span: Span = segment.pinyin.clone().replace(" ", "").into();
        pinyin_msgs.push(span.dim());
        pinyin_msgs.push(" ".into());
    }
    let pinyin_line = Line::from(pinyin_msgs);
    let pinyin_line_len = pinyin_line.width();
    f.render_widget(Text::from(pinyin_line), pinyin_area);

    let pinyin_area = pinyin_area.offset(Offset {
        x: pinyin_line_len as i32,
        y: 0,
    });
    let input = Paragraph::new(app.input.value());
    f.render_widget(input, pinyin_area);
    // Make the cursor visible and ask ratatui to put it at the specified coordinates after
    // rendering
    #[allow(clippy::cast_possible_truncation)]
    f.set_cursor(
        // Draw the cursor at the current position in the input field.
        // This position is can be controlled via the left and right arrow key
        pinyin_area.x + app.input.visual_cursor() as u16,
        // Move one line down, from the border to the input line
        pinyin_area.y,
    );

    if app.show_hint {
        let hint = app.exercise.segments[app.index].pinyin.clone();
        let hint =
            Paragraph::new(format!("Answer: {hint}")).style(Style::default().fg(Color::Yellow));
        f.render_widget(hint, hint_area);
    }

    let mut messages: Vec<ListItem> = vec![];
    for exercise in app.history.iter().rev() {
        messages.push(ListItem::new(Text::from(format!(
            "Chinese: {}",
            exercise
                .segments
                .iter()
                .map(|s| s.chinese.as_str())
                .collect::<String>()
        ))));
        messages.push(ListItem::new(Text::from(format!(
            "Pinyin:  {}",
            exercise
                .segments
                .iter()
                .map(|s| s.pinyin.replace(" ", ""))
                .collect::<Vec<_>>()
                .join(" ")
        ))));
        messages.push(ListItem::new(Text::from(format!(
            "English: {}",
            exercise.english
        ))));
        messages.push(ListItem::new(Text::from("")));
    }
    let messages = List::new(messages).block(Block::bordered().title("History"));
    f.render_widget(messages, messages_area);
}

// Apply tones to pinyin. The pinyin may cover multiple characters. Tone numbers
// 1-4 apply to the first word without an existing tone mark. Tone 5 applies to
// the last word _with_ a tone mark. For example, "xuésheng1" becomes "xuéshēng"
// but "xuésheng5" becomes "xuesheng".
fn apply_tones(pinyin: &str) -> String {
    fn has_tone_mark(s: &str) -> bool {
        const TONE_MARKS: &str = "āáǎàēéěèīíǐìōóǒòūúǔùǖǘǚǜĀÁǍÀĒÉĚÈĪÍǏÌŌÓǑÒŪÚǓÙǕǗǙǛ";
        s.chars().any(|c| TONE_MARKS.contains(c))
    }

    // Find and remove a trailing tone digit (ignore digits in the middle)
    let mut chars: Vec<char> = pinyin.chars().collect();
    let mut tone_digit: Option<char> = None;
    let mut last_non_ws_idx: Option<usize> = None;
    for i in (0..chars.len()).rev() {
        if !chars[i].is_whitespace() {
            last_non_ws_idx = Some(i);
            break;
        }
    }
    if let Some(i) = last_non_ws_idx {
        if matches!(chars[i], '1' | '2' | '3' | '4' | '5') {
            tone_digit = Some(chars[i]);
            chars.remove(i);
        }
    }
    let base: String = chars.into_iter().collect();

    // If no explicit tone digit, just prettify any existing numeric tones/marks
    let Some(d) = tone_digit else {
        return prettify_pinyin::prettify(&base);
    };

    // Split into chunks, preserving original whitespace in the chunks
    let mut chunks = split_words(&base);

    if d == '5' {
        // Apply neutral tone to the last chunk that has a tone mark
        if let Some(idx) =
            chunks
                .iter()
                .enumerate()
                .rev()
                .find_map(|(i, s)| if has_tone_mark(s) { Some(i) } else { None })
        {
            chunks[idx].push('5');
        }
    } else {
        // Apply tones 1-4 to the first chunk without an existing tone mark
        if let Some(idx) =
            chunks
                .iter()
                .enumerate()
                .find_map(|(i, s)| if !has_tone_mark(s) { Some(i) } else { None })
        {
            chunks[idx].push(d);
        } else if !chunks.is_empty() {
            // Fallback: attach to the last chunk
            let last = chunks.len() - 1;
            chunks[last].push(d);
        }
    }

    // Recombine and prettify
    let combined = chunks
        .into_iter()
        .map(|s| prettify_pinyin::prettify(&s))
        .collect::<Vec<_>>()
        .join("");
    combined
}

// Best-effort word splitting. When this function does a bad job, one can always
// separate words with a space.
//
// split_words("xuesheng") -> ["xue", "sheng"]
// split_words("nihao") -> ["ni", "hao"]
// split_words("wǎnshang") -> ["wǎn", "shang"]
// split_words("xihuan") -> ["xi", "huan"]
// split_words("wo") -> ["wo"]
// split_words("daan") -> ["daan"]
// split_words("da an") -> ["da", " an"]
fn split_words(pinyin: &str) -> Vec<String> {
    fn is_vowel(c: char) -> bool {
        // Includes base vowels and common pinyin tone-marked variants (lower/upper case)
        const VOWELS: &str = "aeiouAEIOUüÜāáǎàēéěèīíǐìōóǒòūúǔùǖǘǚǜĀÁǍÀĒÉĚÈĪÍǏÌŌÓǑÒŪÚǓÙǕǗǙǛ";
        VOWELS.contains(c)
    }

    let chars: Vec<char> = pinyin.chars().collect();
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut seen_vowel = false;

    let mut i = 0_usize;
    while i < chars.len() {
        let c = chars[i];

        // If we encounter whitespace, end the current chunk (without including the space),
        // then start a new chunk that begins with the whitespace (to preserve original spacing).
        if c.is_whitespace() {
            if !current.is_empty() {
                parts.push(std::mem::take(&mut current));
                seen_vowel = false;
            }
            // Collect one or more whitespace characters as the start of the next chunk
            current.push(c);
            i += 1;
            while i < chars.len() && chars[i].is_whitespace() {
                current.push(chars[i]);
                i += 1;
            }
            continue;
        }

        current.push(c);
        if is_vowel(c) {
            seen_vowel = true;
        }

        // Look ahead to decide if we should split here
        let next = chars.get(i + 1).copied();
        // Note: we only need to look one character ahead for our splitting heuristic.

        // Always end at end-of-input
        if next.is_none() {
            parts.push(std::mem::take(&mut current));
            break;
        }

        if seen_vowel {
            let n = next.unwrap();

            // If next is whitespace, end this chunk here.
            if n.is_whitespace() {
                parts.push(std::mem::take(&mut current));
                seen_vowel = false;
                // Do not consume next here; it will be processed in the next loop iteration
            } else {
                let n_lower = n.to_ascii_lowercase();
                let next_is_vowel = is_vowel(n);
                let next_is_apostrophe = n == '\'' || n == '’';
                // Only split if the next syllable clearly starts with a consonant initial.
                // Do not split when the next char is a vowel (e.g., "daan") or an apostrophe.
                // Also, avoid splitting before a potential coda 'n' — let it attach to the
                // current syllable (we'll split before the following onset instead).
                let next_is_consonant_onset = !next_is_vowel && n.is_alphabetic();
                // Avoid splitting the common nasal coda "ng"
                let current_ends_with_n = c.to_ascii_lowercase() == 'n';
                let next_is_g = n_lower == 'g';

                if !next_is_apostrophe
                    && next_is_consonant_onset
                    && n_lower != 'n'
                    && !(current_ends_with_n && next_is_g)
                {
                    parts.push(std::mem::take(&mut current));
                    seen_vowel = false;
                }
            }
        }

        i += 1;
    }

    if !current.is_empty() {
        parts.push(current);
    }

    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_tones_basic() {
        assert_eq!(apply_tones("wo3"), "wǒ");
        assert_eq!(apply_tones("xue2"), "xué");
        assert_eq!(apply_tones("xuesheng2"), "xuésheng");
        assert_eq!(apply_tones("xuésheng1"), "xuéshēng");
        assert_eq!(apply_tones("xuésheng5"), "xuesheng");
        assert_eq!(apply_tones("xuéshēng5"), "xuésheng");
        assert_eq!(apply_tones("xue sheng2"), "xué sheng");
        assert_eq!(apply_tones("xué sheng1"), "xué shēng");
        assert_eq!(apply_tones("xué sheng5"), "xue sheng");
        assert_eq!(apply_tones("xué shēng5"), "xué sheng");
        assert_eq!(apply_tones("hao3"), "hǎo");
        assert_eq!(apply_tones("hǎo5"), "hao");
        assert_eq!(apply_tones("ma"), "ma");
    }

    #[test]
    fn test_split_words_examples() {
        assert_eq!(split_words("xuesheng"), vec!["xue", "sheng"]);
        assert_eq!(split_words("nihao"), vec!["ni", "hao"]);
        assert_eq!(split_words("wǎnshang"), vec!["wǎn", "shang"]);
        assert_eq!(split_words("xihuan"), vec!["xi", "huan"]);
        assert_eq!(split_words("wo"), vec!["wo"]);
        assert_eq!(split_words("daan"), vec!["daan"]);
        assert_eq!(split_words("da an"), vec!["da", " an"]);
        assert_eq!(split_words("aihao"), vec!["ai", "hao"]);
        assert_eq!(split_words("xiayu"), vec!["xia", "yu"]);
        assert_eq!(split_words("shengqi"), vec!["sheng", "qi"]);
        assert_eq!(split_words("guojia"), vec!["guo", "jia"]);
    }
}
