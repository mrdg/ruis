use std::ops::Range;

use crate::app::{SharedState, TrackView, ViewContext};
use crate::pattern::{Position, MAX_PITCH};
use tui::layout::{Alignment, Constraint, Direction, Layout};
use tui::widgets::Paragraph;
use tui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, StatefulWidget, Widget},
};

#[derive(Clone, Default)]
pub struct EditorState {
    offset: usize,
}

pub struct Editor<'a> {
    ctx: ViewContext<'a>,
    cursor: Position,
    current_line: Option<usize>,
    lines_per_beat: usize,
    in_focus: bool,
    is_playing: bool,
}

impl<'a> Editor<'a> {
    pub fn new(cursor: Position, in_focus: bool, ctx: ViewContext<'a>) -> Self {
        let selected = ctx.selected_pattern_index();
        let active = ctx.active_pattern_index();
        let lines_per_beat = ctx.lines_per_beat() as usize;
        let is_playing = ctx.is_playing();
        let current_line = if selected == active {
            Some(ctx.current_line())
        } else {
            None
        };

        Self {
            ctx,
            current_line,
            cursor,
            lines_per_beat,
            in_focus,
            is_playing,
        }
    }

    fn render_mixer_controls(&self, track: &TrackView, area: Rect, buf: &mut Buffer) {
        let mut meter_width = 2;
        if area.width % 2 != 0 {
            meter_width += 1;
        }
        let offset = (area.width - meter_width) / 2;

        // VU meter
        let meter = Rect {
            x: area.x + offset,
            y: area.y,
            width: meter_width,
            height: area.height - 4,
        };

        let mut db = 0;
        for i in 0..meter.height {
            let rms = self.ctx.rms(track.index);
            let meter_color = |value: f32| {
                let db = db as f32;
                if value > db {
                    if value < db + 2.0 {
                        Color::Indexed(34)
                    } else if value < db + 4.0 {
                        Color::Indexed(40)
                    } else {
                        Color::Indexed(46)
                    }
                } else {
                    Color::Gray
                }
            };
            let left_color = meter_color(rms.0);
            let right_color = meter_color(rms.1);

            let channel_width = meter_width / 2;
            let meter_symbol = "▇".repeat(channel_width.into());

            let spans = Spans::from(vec![
                Span::styled(&meter_symbol, Style::default().fg(left_color)),
                Span::raw(" "),
                Span::styled(&meter_symbol, Style::default().fg(right_color)),
            ]);
            buf.set_spans(meter.x, meter.y + i, &spans, meter_width + 1);

            db -= 6;
        }

        // Volume control
        let volume_area = Rect {
            x: area.x,
            y: meter.y + meter.height,
            width: area.width,
            height: 2,
        };

        let volume = format!("{:.2}", track.volume);
        let volume = Paragraph::new(volume)
            .alignment(tui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::TOP));
        volume.render(volume_area, buf);

        let button_area = Rect {
            x: area.x,
            y: meter.y + meter.height + 2,
            width: area.width,
            height: 2,
        };

        if track.is_master {
            let block = Block::default().borders(Borders::TOP);
            block.render(button_area, buf);
            return;
        }

        let button_style = if track.muted {
            Style::default().bg(Color::DarkGray)
        } else {
            Style::default().bg(Color::Yellow).fg(Color::Black)
        };

        let button = Span::styled(format!(" {} ", track.index), button_style);
        let button = Paragraph::new(button)
            .alignment(tui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::TOP));
        button.render(button_area, buf);
    }

    fn render_track_steps(
        &self,
        area: Rect,
        buf: &mut Buffer,
        track: &TrackView,
        index: usize,
        steps: &Range<usize>,
    ) {
        // Draw track header
        let header = if track.is_master {
            String::from(" Master")
        } else {
            format!(" {}", index)
        };
        let bg_color = Color::Indexed(250);
        let header = Paragraph::new(header)
            .alignment(Alignment::Left)
            .style(Style::default().bg(bg_color).fg(Color::Black));

        let header_area = Rect { height: 1, ..area };
        header.render(header_area, buf);

        if track.is_master {
            return;
        }

        // Draw notes
        let mut y = area.top() + 1;
        for (line, note) in track.steps(steps).iter().enumerate() {
            let line = line + steps.start;
            let base_style = self.get_base_style(line, false);
            let column = index * 2;

            let highlight = note.pitch.is_some() && self.is_playing;
            let pitch_style = self.get_input_style(line, column, highlight);
            let pitch = match note.pitch {
                Some(pitch) => &NOTE_NAMES[pitch as usize],
                None => "---",
            };

            let snd_style = self.get_input_style(line, column + 1, false);
            let snd = match note.sound {
                Some(v) => format!("{:0width$}", v, width = 2),
                None => String::from("--"),
            };

            let spans = Spans::from(vec![
                Span::styled(" ", base_style),
                Span::styled(pitch, pitch_style),
                Span::styled(" ", base_style),
                Span::styled(snd, snd_style),
                Span::styled(" ", base_style),
            ]);

            buf.set_spans(area.left(), y, &spans, area.width);
            y += 1;
        }
    }

    fn get_input_style(&self, line: usize, col: usize, active: bool) -> Style {
        if self.in_focus && self.cursor.line == line && self.cursor.column == col {
            Style::default().bg(Color::Green).fg(Color::Black)
        } else {
            self.get_base_style(line, active)
        }
    }

    fn get_base_style(&self, line: usize, active: bool) -> Style {
        if self.current_line.is_some() && self.current_line.unwrap() == line && active {
            Style::default().bg(Color::Indexed(239)).fg(Color::White)
        } else if line % self.lines_per_beat == 0 {
            Style::default().bg(Color::Indexed(236))
        } else {
            Style::default()
        }
    }
}

impl<'a> StatefulWidget for &Editor<'a> {
    type State = EditorState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(75), Constraint::Percentage(25)].as_ref())
            .split(area);

        let pattern_area = sections[0];
        let mixer_area = sections[1];

        let header_height = 1;
        let height = pattern_area.height as usize - header_height - 1;
        let pattern = self.ctx.selected_pattern();
        let mut last_line = state.offset + std::cmp::min(height, pattern.length);

        if last_line > pattern.length {
            // pattern length must have been changed so reset offset
            state.offset = 0;
            last_line = state.offset + std::cmp::min(height, pattern.length);
        }

        if self.cursor.line > last_line {
            last_line = self.cursor.line + 1;
            state.offset = last_line - height;
        } else if self.cursor.line < state.offset {
            state.offset = self.cursor.line;
            last_line = state.offset + height;
        }

        let left = area.left() + 1;
        let steps = state.offset..last_line;

        // Draw the step indicator next to the pattern grid
        let style = Style::default().fg(Color::Indexed(241));
        buf.set_string(left, area.top(), format!("{:>3}", pattern.length), style);
        for (i, step) in steps.clone().enumerate() {
            let style = if self.current_line.is_some() && self.current_line.unwrap() == step {
                Style::default().bg(Color::Blue).fg(Color::White)
            } else if step % self.lines_per_beat == 0 {
                Style::default().bg(Color::Indexed(236))
            } else {
                Style::default()
            };
            buf.set_string(
                left,
                area.top() + 1 + i as u16,
                format!("{:>3}", step),
                style,
            );
        }

        let mut x = area.x + STEP_COLUMN_WIDTH as u16;
        let mut render_track = |track: &TrackView, idx: usize| {
            let mut borders = Borders::RIGHT | Borders::BOTTOM;
            let mut width = COLUMN_WIDTH + 1; // add one for border
            if idx == 0 {
                // leftmost border is part of first track width
                width += 1;
                borders |= Borders::LEFT;
            }

            // Draw pattern
            let area = Rect {
                x,
                y: area.y,
                width: width as u16,
                height: (last_line - state.offset + 2) as u16,
            };
            let block = Block::default().borders(borders);
            let inner = block.inner(area);
            block.render(area, buf);
            self.render_track_steps(inner, buf, track, idx, &steps);

            // Draw mixer channel
            let area = Rect {
                x,
                y: mixer_area.y,
                width: width as u16,
                height: mixer_area.height,
            };
            borders |= Borders::TOP;
            let block = Block::default().borders(borders);
            let inner = block.inner(area);
            block.render(area, buf);
            self.render_mixer_controls(track, inner, buf);

            x += width as u16;
        };

        for (i, track) in self.ctx.iter_tracks().enumerate() {
            render_track(&track, i);
        }
    }
}

const COLUMN_WIDTH: usize = " C#4 05 ".len();
const STEP_COLUMN_WIDTH: usize = " 256 ".len();

lazy_static! {
    static ref NOTE_NAMES: Vec<String> = {
        let names = vec![
            "C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-",
        ];
        // 0 based octave notation instead of -2 based makes notes easier to read in the editor.
        let mut notes: Vec<String> = (0..MAX_PITCH as usize)
            .map(|pitch| {
                let octave = pitch / 12;
                format!("{}{}", names[pitch % 12], octave)
            })
            .collect();

        notes.push("OFF".to_string());
        notes
    };
}