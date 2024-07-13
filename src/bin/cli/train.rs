use std::{
    fs::File,
    io::{self, BufReader},
};

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

use crate::convert::Exercise;

struct App {
    _audio_stream: OutputStream,
    audio_sink: Sink,
    exercises: Vec<Exercise>,
    exercise: Exercise,
    index: usize,
    input: Input,
    // show_english: bool,
    show_hint: bool,
    history: Vec<Exercise>,
}

impl App {
    fn new(mut exercises: Vec<Exercise>) -> Self {
        let exercise = exercises.pop().unwrap();
        let (stream, stream_handle) = OutputStream::try_default().unwrap();
        App {
            _audio_stream: stream,
            audio_sink: Sink::try_new(&stream_handle).unwrap(),
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

pub fn train(mut exercises: Vec<Exercise>) -> Result<(), Box<dyn std::error::Error>> {
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    exercises.reverse();
    let app = App::new(exercises);
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
        let pinyin = prettify_pinyin::prettify(app.input.value());
        let pinyin_len = pinyin.chars().count();
        app.input = Input::new(pinyin)
            .with_cursor(cursor - (app.input.value().chars().count() - pinyin_len));

        while app.index < app.exercise.segments.len() {
            let target = &app.exercise.segments[app.index].pinyin;
            if target.to_lowercase() == app.input.value().trim().to_lowercase() {
                app.index += 1;
                app.input = Input::new("".into());
                app.show_hint = false;
            } else {
                break;
            }
        }
        if app.index >= app.exercise.segments.len() {
            {
                let clean_name = app
                    .exercise
                    .chinese()
                    .chars()
                    .filter(|c| c.is_alphanumeric())
                    .collect::<String>();
                // dbg!(&clean_name);
                let file = BufReader::new(File::open(format!("audio/{}.mp3", clean_name)).unwrap());
                // Decode that sound file into a source
                let source = Decoder::new(file).unwrap();
                app.audio_sink.append(source);
            }
            app.history.push(app.exercise.clone());
            let exercise = app.exercises.pop().unwrap();
            app.exercise = exercise;
            app.index = 0;
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let vertical = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
    ]);
    let [help_area, pinyin_area, hint_area, messages_area] = vertical.areas(f.size());

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
