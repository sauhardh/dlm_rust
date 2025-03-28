use color_eyre;

use color_eyre::eyre::Ok;
use ratatui::crossterm::event::{self, KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Position};
use ratatui::style::{Color, Modifier, Style, Styled, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table, Tabs};
use ratatui::{DefaultTerminal, Frame};
use serde::Deserialize;
use serde::Serialize;
use strum;
use strum::IntoEnumIterator;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;
use std::time::Instant;

use crate::CommandArgument;
use crate::SingleDownload;

#[derive(Default, Debug, Clone)]
struct DownloadingTable {
    #[allow(dead_code)]
    id: u64,
    name: String,
    progress: usize,
    status: String,
    total_length: usize,
}

impl DownloadingTable {
    pub fn build(
        id: u64,
        name: String,
        progress: usize,
        status: String,
        total_length: usize,
    ) -> Self {
        Self {
            id,
            name,
            progress,
            status,
            total_length,
        }
    }
}

#[derive(
    Default,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    strum::EnumIter,
    strum::Display,
    strum::FromRepr,
    Serialize,
    Deserialize,
)]
pub enum CommandTab {
    #[default]
    Download,
    Pause,
    Resume,
    Cancel,
}

impl CommandTab {
    fn next(self) -> Self {
        let mut iter = Self::iter().cycle();
        iter.find(|&tab| tab == self);
        iter.next().unwrap_or(self)
    }

    fn previous(self) -> Self {
        let mut iter = Self::iter().cycle();
        iter.find(|tab| tab.next() == self).unwrap()
    }
}

enum Event {
    Input(event::KeyEvent),
    DownloadUpdate(SingleDownload),
    Resize,
    Tick,
}

fn handle_event(update_tx: UnboundedSender<Event>) {
    let tick_rate = Duration::from_millis(200);
    tokio::spawn(async move {
        let mut last_tick = Instant::now();

        loop {
            let timeout = tick_rate.saturating_sub(last_tick.elapsed());
            if event::poll(timeout).unwrap() {
                match event::read().unwrap() {
                    event::Event::Key(key) => update_tx.send(Event::Input(key)).unwrap(),
                    event::Event::Resize(_, _) => update_tx.send(Event::Resize).unwrap(),
                    _ => {}
                }
            }

            if last_tick.elapsed() >= tick_rate {
                if let Err(_) = update_tx.send(Event::Tick) {
                    return;
                }
                last_tick = Instant::now();
            }
        }
    });
}

pub async fn run_tui(
    command_tx: UnboundedSender<CommandArgument>,
    mut realtime_rx: UnboundedReceiver<SingleDownload>,
) -> color_eyre::Result<()> {
    color_eyre::install()?;
    let terminal = ratatui::init();

    let mut app = App::new();

    let (update_tx, update_rx) = mpsc::unbounded_channel::<Event>();
    handle_event(update_tx.clone());

    let tx = update_tx.clone();

    tokio::spawn(async move {
        loop {
            if let Some(progress) = realtime_rx.recv().await {
                tx.send(Event::DownloadUpdate(progress)).unwrap();
            };
        }
    });

    let app_result = app.run(terminal, command_tx, update_rx).await;

    ratatui::restore();
    app_result
}

#[derive(Clone)]
struct HandleInput {
    input_value: String,
    character_idx: usize,
    messages: Option<Vec<String>>,
    id: Option<usize>,
}

impl HandleInput {
    const fn new() -> Self {
        Self {
            input_value: String::new(),
            character_idx: 0,
            messages: None,
            id: None,
        }
    }

    fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.input_value.chars().count())
    }

    fn move_cursor_left(&mut self) {
        let cursor_moved_left = self.character_idx.saturating_sub(1);
        self.character_idx = self.clamp_cursor(cursor_moved_left);
    }

    fn move_cursor_right(&mut self) {
        let cursor_moved_right = self.character_idx.saturating_add(1);
        self.character_idx = self.clamp_cursor(cursor_moved_right);
    }

    fn enter_char(&mut self, new_char: char) {
        let index = self.byte_index();
        self.input_value.insert(index, new_char);
        self.move_cursor_right();
    }

    fn byte_index(&self) -> usize {
        self.input_value
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.character_idx)
            .unwrap_or(self.input_value.len())
    }

    fn reset_cursor(&mut self) {
        self.character_idx = 0;
    }

    fn submit_message(&mut self, selected_tab: CommandTab) -> (Option<Vec<String>>, Option<usize>) {
        if selected_tab == CommandTab::Download {
            let data = self
                .input_value
                .split_whitespace()
                .map(String::from)
                .collect();
            self.messages = Some(data);
        } else {
            self.id = Some(self.input_value.trim().parse().unwrap())
        }

        self.input_value.clear();
        self.reset_cursor();

        (self.messages.clone(), self.id)
    }

    fn delete_char(&mut self) {
        let is_not_cursor_leftmost = self.character_idx != 0;
        if is_not_cursor_leftmost {
            let current_index = self.character_idx;
            let from_left_to_current_index = current_index - 1;

            let before_char_to_delete = self.input_value.chars().take(from_left_to_current_index);
            let after_char_to_delete = self.input_value.chars().skip(current_index);

            self.input_value = before_char_to_delete.chain(after_char_to_delete).collect();
            self.move_cursor_left();
        }
    }
}

#[derive(Clone)]
struct App {
    input: HandleInput,
    selected_tab: CommandTab,
    table_data: Arc<RwLock<HashMap<u64, DownloadingTable>>>,
    begin_time: Instant,
}

impl App {
    fn new() -> Self {
        Self {
            input: HandleInput::new(),
            selected_tab: CommandTab::Download,
            table_data: Arc::new(RwLock::new(HashMap::new())),
            begin_time: Instant::now(),
        }
    }

    pub fn draw_tui(&mut self, terminal: &mut DefaultTerminal) {
        if let Err(e) = terminal.draw(|frame| self.draw(frame)) {
            eprintln!("The error e is :{e:#?}");
        };
    }

    pub async fn run(
        &mut self,
        mut terminal: DefaultTerminal,
        command_tx: UnboundedSender<CommandArgument>,
        mut update_rx: UnboundedReceiver<Event>,
    ) -> color_eyre::Result<()> {
        loop {
            self.draw_tui(&mut terminal);
            match update_rx.recv().await.unwrap() {
                Event::Input(key) => match (key.code, key.modifiers) {
                    (KeyCode::Esc, _) => {
                        return Ok(());
                    }
                    (KeyCode::Backspace, _) => self.input.delete_char(),
                    (KeyCode::Left, _) => self.input.move_cursor_left(),
                    (KeyCode::Right, _) => self.input.move_cursor_right(),
                    (KeyCode::Char('l'), KeyModifiers::CONTROL) => {
                        self.selected_tab = self.selected_tab.next()
                    }
                    (KeyCode::Char('h'), KeyModifiers::CONTROL) => {
                        self.selected_tab = self.selected_tab.previous()
                    }
                    (KeyCode::Enter, _) => {
                        let (message, id) = self.input.submit_message(self.selected_tab);
                        let command = CommandArgument {
                            command: self.selected_tab,
                            urls: message,
                            id,
                        };

                        // Send the Input Commands to the Server for download
                        if let Err(e) = command_tx.send(command) {
                            eprintln!("Failed to send error command: {e}");
                        };
                    }
                    (KeyCode::Char(to_insert), _) => {
                        self.input.enter_char(to_insert);
                    }

                    _ => {}
                },
                Event::Resize => {
                    terminal.autoresize()?;
                }

                Event::Tick => {}
                Event::DownloadUpdate(progress) => {
                    let mut table = self.table_data.write().unwrap();
                    table.insert(
                        progress.id as u64,
                        DownloadingTable::build(
                            progress.id as u64,
                            progress.url,
                            progress.progress,
                            progress.state,
                            progress.total_length,
                        ),
                    );
                }
            }
        }
    }

    pub fn draw(&mut self, frame: &mut Frame<'_>) {
        let size = frame.area();

        // If terminal is to small
        if size.width < 50 || size.height < 12 {
            let warning = Paragraph::new("Terminal too small - please resize")
                .block(Block::default().borders(Borders::ALL))
                .style(Style::default().fg(Color::Red).bold());
            frame.render_widget(warning, frame.area());
            return;
        }

        let vertical = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(2),
        ]);

        // Area
        let [tab_area, help_area_one, help_area_two, input_area, output_area] =
            vertical.areas(frame.area());

        // Tabs
        let tabs = self.add_tabs();
        frame.render_widget(tabs, tab_area);

        // Paragraph Info top
        let msg_one = vec![
            "Press ".into(),
            "q".bold().underlined(),
            " / ".into(),
            "Esc".bold().underlined(),
            " to quit".into(),
        ];
        frame.render_widget(self.info_paragraph(msg_one), help_area_one);

        // Paragraph Info below
        let msg_two = vec![
            "CTRL + l".bold().underlined(),
            " Scroll Mode to Left. ".into(),
            "CTRL + h".bold().underlined(),
            " Scroll Mode to Right. ".into(),
        ];
        frame.render_widget(self.info_paragraph(msg_two), help_area_two);

        //Input Mode
        frame.set_cursor_position(Position::new(
            input_area.x + self.input.character_idx as u16 + 1,
            input_area.y + 1,
        ));

        let input = self.input_paragraph();
        frame.render_widget(input, input_area);

        // Table
        let header = Row::new(vec!["ID", "Name", "Progress", "Status"]).style(
            Style::default()
                .fg(Color::LightMagenta)
                .add_modifier(Modifier::BOLD),
        );

        let table_data = self.table_data.read().unwrap();
        let rows: Vec<Row> = table_data
            .iter()
            .map(|(id, data)| {
                // Determine the style based on status
                let status_style = match data.status.as_str() {
                    "Downloading" => Style::default()
                        .fg(Color::LightBlue)
                        .add_modifier(Modifier::BOLD),
                    "Completed" => Style::default()
                        .fg(Color::LightGreen)
                        .add_modifier(Modifier::BOLD),
                    "Paused" => Style::default()
                        .fg(Color::LightYellow)
                        .add_modifier(Modifier::BOLD),
                    "Canceled" => Style::default()
                        .fg(Color::LightRed)
                        .add_modifier(Modifier::BOLD),
                    _ => Style::default(),
                };

                // Create cells with proper styling
                Row::new(vec![
                    Cell::from(id.to_string()),
                    Cell::from(data.name.to_string()),
                    Cell::from(self.progress_bar(data.progress, data.total_length)),
                    Cell::from(Span::styled(data.status.to_string(), status_style)),
                ])
            })
            .collect();

        let table = Table::new(
            rows,
            [
                Constraint::Percentage(5),
                Constraint::Percentage(30),
                Constraint::Percentage(55),
                Constraint::Percentage(10),
            ],
        )
        .header(header)
        .block(
            Block::default().borders(Borders::NONE).title(Span::styled(
                "",
                Style::default()
                    .fg(Color::LightBlue)
                    .add_modifier(Modifier::BOLD),
            )),
        );

        frame.render_widget(table, output_area);
    }

    #[inline]
    fn progress_bar(&self, progress: usize, total_length: usize) -> Line<'_> {
        if total_length != 0 {
            let percent = (progress as f64 / 100.0) as f64;
            let filled = (percent * 20.0).round() as usize;

            Line::from(vec![
                Span::raw("["),
                Span::styled("█".repeat(filled), Style::default().fg(Color::Magenta)),
                Span::raw(" ".repeat(20 - filled)),
                Span::raw("]"),
                Span::raw(format!("{:>3.0}%", percent * 100.0)),
            ])
        } else {
            let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let frame_idx =
                (self.begin_time.elapsed().as_millis() / 100) % spinner_frames.len() as u128;

            Line::from(vec![
                Span::raw(spinner_frames[frame_idx as usize]),
                Span::raw(" "),
                Span::raw(format!("{:.1}", progress as f64)),
                Span::raw(" "),
            ])
        }
    }

    #[inline]
    fn add_tabs(&self) -> Tabs<'static> {
        // Tab Mode
        let tab_titles = CommandTab::iter().map(|tab| {
            let style = if tab == self.selected_tab {
                Style::default()
                    .fg(Color::Magenta)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD | Modifier::ITALIC)
            } else {
                Style::default().fg(Color::White)
            };
            return Span::styled(tab.to_string(), style);
        });
        Tabs::new(tab_titles)
            .padding("  ", "  ")
            .select(self.selected_tab as usize)
    }

    #[inline]
    fn info_paragraph(&self, msg: Vec<Span<'static>>) -> Paragraph<'static> {
        let style_one = Style::default().add_modifier(Modifier::RAPID_BLINK);
        let text = Text::from(Line::from(msg))
            .patch_style(style_one)
            .fg(Color::Yellow);

        Paragraph::new(text)
    }

    #[inline]
    fn input_paragraph(&self) -> Paragraph {
        let input_value = if self.input.input_value.is_empty() {
            match self.selected_tab {
                CommandTab::Download => "➤ Enter URL ",
                _ => "➤ Enter ID ",
            }
        } else {
            self.input.input_value.as_str()
        };

        Paragraph::new(input_value).style(Style::default()).block(
            Block::bordered()
                .border_style(Style::new())
                .title(Span::styled(
                    self.selected_tab.to_string(),
                    Style::default().fg(Color::LightBlue).bold(),
                ))
                .border_type(BorderType::Rounded),
        )
    }
}
