//! Terminal UI, modelled on macmon: rounded panels with live values in the titles,
//! sparklines (or gauges) per metric, and a combined history chart.

use crate::config::{Config, View};
use crate::sampler::{Sample, Sampler};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    symbols::Marker,
    text::{Line, Span},
    widgets::{
        Axis, Block, BorderType, Chart, Dataset, Gauge, GraphType, LegendPosition, Paragraph,
        RenderDirection, Sparkline, SparklineBar,
    },
};
use std::collections::VecDeque;
use std::ffi::{CStr, c_char, c_void};
use std::sync::mpsc;
use std::time::Duration;

const COLORS: [Color; 7] =
    [Color::Green, Color::Yellow, Color::Red, Color::Blue, Color::Magenta, Color::Cyan, Color::White];
/// Secondary series colors for the history chart, in preference order.
const SERIES: [Color; 5] = [Color::Yellow, Color::Magenta, Color::Cyan, Color::Blue, Color::Red];
const WINDOWS: [u64; 4] = [60, 300, 900, 3600];

pub fn run(interval: Duration) -> Result<(), String> {
    let mut sampler = Sampler::new()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        loop {
            let started = std::time::Instant::now();
            if tx.send(sampler.sample()).is_err() {
                return;
            }
            std::thread::sleep(interval.saturating_sub(started.elapsed()));
        }
    });

    let mut app = App::new(interval);
    let mut terminal = ratatui::init();
    let result = app.event_loop(&mut terminal, rx);
    ratatui::restore();
    result
}

struct App {
    cfg: Config,
    history: VecDeque<Sample>,
    capacity: usize,
    model: String,
    energy_in_wh: f64,
    energy_sys_wh: f64,
    energy_batt_wh: f64,
}

impl App {
    fn new(interval: Duration) -> Self {
        let mut cfg = Config::load();
        cfg.color %= COLORS.len();
        cfg.window %= WINDOWS.len();
        let max_window_ms = WINDOWS[WINDOWS.len() - 1] * 1000;
        Self {
            cfg,
            history: VecDeque::new(),
            capacity: (max_window_ms / interval.as_millis().max(1) as u64) as usize + 2,
            model: hw_model(),
            energy_in_wh: 0.0,
            energy_sys_wh: 0.0,
            energy_batt_wh: 0.0,
        }
    }

    fn event_loop(
        &mut self,
        terminal: &mut ratatui::DefaultTerminal,
        rx: mpsc::Receiver<Sample>,
    ) -> Result<(), String> {
        loop {
            while let Ok(sample) = rx.try_recv() {
                self.push(sample);
            }
            terminal.draw(|f| self.render(f)).map_err(|e| e.to_string())?;

            if !event::poll(Duration::from_millis(200)).map_err(|e| e.to_string())? {
                continue;
            }
            let Event::Key(key) = event::read().map_err(|e| e.to_string())? else { continue };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Ok(()),
                KeyCode::Char('c') => self.cfg.color = (self.cfg.color + 1) % COLORS.len(),
                KeyCode::Char('v') => self.cfg.view = self.cfg.view.toggle(),
                KeyCode::Char('+') | KeyCode::Char('=') => {
                    self.cfg.window = (self.cfg.window + 1).min(WINDOWS.len() - 1)
                }
                KeyCode::Char('-') | KeyCode::Char('_') => self.cfg.window = self.cfg.window.saturating_sub(1),
                _ => continue,
            }
            self.cfg.save();
        }
    }

    fn push(&mut self, sample: Sample) {
        if let Some(prev) = self.history.back() {
            let hours = sample.timestamp_ms.saturating_sub(prev.timestamp_ms) as f64 / 3_600_000.0;
            self.energy_in_wh += prev.input.watts * hours;
            self.energy_sys_wh += prev.system_watts * hours;
            self.energy_batt_wh += prev.battery.watts * hours;
        }
        self.history.push_back(sample);
        while self.history.len() > self.capacity {
            self.history.pop_front();
        }
    }

    fn accent(&self) -> Color {
        COLORS[self.cfg.color]
    }

    fn window_secs(&self) -> u64 {
        WINDOWS[self.cfg.window]
    }

    /// Samples inside the selected history window, oldest first.
    fn in_window(&self) -> impl Iterator<Item = &Sample> {
        let now = self.history.back().map(|s| s.timestamp_ms).unwrap_or(0);
        let start = now.saturating_sub(self.window_secs() * 1000);
        self.history.iter().filter(move |s| s.timestamp_ms >= start)
    }

    fn render(&self, f: &mut Frame) {
        let accent = self.accent();
        let Some(s) = self.history.back() else {
            f.render_widget(Paragraph::new("waiting for first sample…"), f.area());
            return;
        };

        let header = match &s.adapter {
            Some(a) if s.external_connected => {
                let mut h = format!("{} W adapter · {:.1} V × {:.2} A", fmt_num(a.rated_watts), a.volts, a.amps);
                if let Some(name) = &a.name {
                    h.push_str(&format!(" ({name})"));
                }
                if a.wireless {
                    h.push_str(" · wireless");
                }
                h
            }
            _ if s.external_connected => "external power".into(),
            _ => "on battery".into(),
        };
        let header = if s.source == "smc" { header } else { format!("{header} · registry only (slow updates)") };
        let hint = format!(
            "q quit · c color · v view: {} · +/- history: {}",
            self.cfg.view.label(),
            fmt_secs(self.window_secs())
        );
        let width = f.area().width;
        let model = format!("{} · macbat v{}", self.model, env!("CARGO_PKG_VERSION"));
        let (header, model) = fit(width, &header, &model);
        let (_, hint) = fit(width, "", &hint);
        let mut outer = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(accent))
            .title_style(Style::new().fg(Color::Reset))
            .title_top(Line::from(format!(" {header} ")).bold());
        if !model.is_empty() {
            outer = outer.title_top(Line::from(format!(" {model} ")).right_aligned());
        }
        if !hint.is_empty() {
            outer = outer.title_bottom(Line::from(format!(" {hint} ")).right_aligned());
        }
        let area = outer.inner(f.area());
        f.render_widget(outer, f.area());

        let [row1, row2, row3] =
            Layout::vertical([Constraint::Fill(1), Constraint::Fill(1), Constraint::Fill(2)]).areas(area);
        let [input_area, system_area] = Layout::horizontal([Constraint::Fill(1); 2]).areas(row1);
        let [battery_area, charge_area] = Layout::horizontal([Constraint::Fill(1); 2]).areas(row2);

        self.render_input(f, input_area, s);
        self.render_system(f, system_area, s);
        self.render_battery(f, battery_area, s);
        self.render_charge(f, charge_area, s);
        self.render_history(f, row3, s);
    }

    /// The most recent samples that fit in `width` columns, newest first.
    fn recent(&self, width: u16) -> impl Iterator<Item = &Sample> {
        self.history.iter().rev().take(width as usize)
    }

    fn render_input(&self, f: &mut Frame, area: Rect, s: &Sample) {
        let accent = self.accent();
        let rated = s.adapter.as_ref().map(|a| a.rated_watts).filter(|w| *w > 0.0);
        let (value, foot_l, foot_r) = if s.external_connected {
            (
                fmt_w(s.input.watts),
                format!("{:.2} V × {:.2} A", s.input.volts, s.input.amps),
                rated.map(|r| format!("{:.0}% of {} W rating", s.input.watts / r * 100.0, fmt_num(r))).unwrap_or_default(),
            )
        } else {
            ("—".into(), "on battery".into(), String::new())
        };
        let block = panel(accent, area.width, "Power In", value, foot_l, foot_r);
        let inner = block.inner(area);
        f.render_widget(block, area);

        match self.cfg.view {
            View::Sparkline => {
                let peak = self.recent(inner.width).map(|s| s.input.watts).fold(rated.unwrap_or(1.0), f64::max);
                let data = self.recent(inner.width).map(|s| SparklineBar::from(cw(s.input.watts)));
                f.render_widget(spark(accent, data, peak), inner);
            }
            View::Gauge => {
                let full = rated.unwrap_or(s.input.watts.max(1.0));
                let label = format!("{} of {} W", fmt_w(s.input.watts), fmt_num(full));
                f.render_widget(gauge(accent, s.input.watts / full, label), inner);
            }
        }
    }

    fn render_system(&self, f: &mut Frame, area: Rect, s: &Sample) {
        let accent = self.accent();
        let (avg, max) = avg_max(self.in_window().map(|s| s.system_watts));
        let short = s.external_connected && s.battery.watts < -0.1;
        let foot_r = if short { format!("adapter short by {}", fmt_w(-s.battery.watts)) } else { String::new() };
        let (foot_l, foot_r) = fit(area.width, &format!("avg {} · max {}", fmt_w(avg), fmt_w(max)), &foot_r);
        let mut block = panel(accent, area.width, "System", fmt_w(s.system_watts), foot_l, String::new());
        if !foot_r.is_empty() {
            block = block.title_bottom(Line::from(format!(" {foot_r} ")).right_aligned().fg(Color::Red).bold());
        }
        let inner = block.inner(area);
        f.render_widget(block, area);

        match self.cfg.view {
            View::Sparkline => {
                let peak = self.recent(inner.width).map(|s| s.system_watts).fold(1.0, f64::max);
                let data = self.recent(inner.width).map(|s| SparklineBar::from(cw(s.system_watts)));
                f.render_widget(spark(accent, data, peak), inner);
            }
            View::Gauge => {
                let label = format!("{} of {} peak", fmt_w(s.system_watts), fmt_w(max));
                f.render_widget(gauge(accent, s.system_watts / max.max(0.01), label), inner);
            }
        }
    }

    fn render_battery(&self, f: &mut Frame, area: Rect, s: &Sample) {
        let accent = self.accent();
        let drain = if accent == Color::Red { Color::Yellow } else { Color::Red };
        let b = &s.battery;
        let value = match b.state {
            "charging" | "discharging" => format!("{} {}", fmt_signed_w(b.watts), b.state),
            other => other.to_string(),
        };
        let foot_r = match (b.state, b.minutes_remaining) {
            ("charging", Some(m)) => format!("full in {}", fmt_minutes(m)),
            ("discharging", Some(m)) => format!("empty in {}", fmt_minutes(m)),
            _ => String::new(),
        };
        let block = panel(accent, area.width, "Battery", value, format!("{:.2} V × {:+.2} A", b.volts, b.amps), foot_r);
        let inner = block.inner(area);
        f.render_widget(block, area);

        let color = |w: f64| if w < 0.0 { drain } else { accent };
        match self.cfg.view {
            View::Sparkline => {
                let peak = self.recent(inner.width).map(|s| s.battery.watts.abs()).fold(1.0, f64::max);
                let data = self.recent(inner.width).map(|s| {
                    SparklineBar::from(cw(s.battery.watts.abs())).style(Style::new().fg(color(s.battery.watts)))
                });
                f.render_widget(spark(accent, data, peak), inner);
            }
            View::Gauge => {
                let peak = self.in_window().map(|s| s.battery.watts.abs()).fold(0.01, f64::max);
                let label = format!("{} ({} peak)", fmt_signed_w(b.watts), fmt_w(peak));
                f.render_widget(gauge(color(b.watts), b.watts.abs() / peak, label), inner);
            }
        }
    }

    fn render_charge(&self, f: &mut Frame, area: Rect, s: &Sample) {
        let accent = self.accent();
        let b = &s.battery;
        let foot_l = match (b.remaining_mah, b.full_charge_mah) {
            (Some(r), Some(full)) => format!("{r} / {full} mAh"),
            _ => String::new(),
        };
        let mut details = Vec::new();
        if let (Some(n), Some(d)) = (b.nominal_mah, b.design_mah) {
            details.push(format!("{:.0}% of design", n as f64 / d as f64 * 100.0));
        }
        if let Some(c) = b.cycle_count {
            details.push(format!("{c} cycles"));
        }
        if let Some(t) = b.temp_c {
            details.push(format!("{t:.1} °C"));
        }
        let block = panel(accent, area.width, "Charge", format!("{:.0}%", b.percent), foot_l, details.join(" · "));
        let inner = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(gauge(accent, b.percent / 100.0, format!("{:.0}%", b.percent)), inner);
    }

    fn render_history(&self, f: &mut Frame, area: Rect, s: &Sample) {
        let accent = self.accent();
        let mut series = SERIES.iter().copied().filter(|c| *c != accent);
        let (c_sys, c_batt) = (series.next().unwrap(), series.next().unwrap());

        let window = self.window_secs() as f64;
        let now = s.timestamp_ms;
        let x = |t: u64| (t as f64 - now as f64) / 1000.0;
        let pts = |val: fn(&Sample) -> f64| -> Vec<(f64, f64)> {
            self.in_window().map(|s| (x(s.timestamp_ms), val(s))).collect()
        };
        let input = pts(|s| s.input.watts);
        let system = pts(|s| s.system_watts);
        let battery = pts(|s| s.battery.watts);

        let all = input.iter().chain(&system).chain(&battery).map(|p| p.1);
        let (lo, hi) = all.fold((0.0f64, 1.0f64), |(lo, hi), v| (lo.min(v), hi.max(v)));
        let (lo, hi) = (lo.floor(), hi.ceil());
        let zero = [(-window, 0.0), (0.0, 0.0)];

        let mut datasets = vec![];
        if lo < 0.0 {
            datasets.push(Dataset::default().marker(Marker::Braille).graph_type(GraphType::Line).data(&zero).dark_gray());
        }
        datasets.extend([
            line("in", accent, &input),
            line("sys", c_sys, &system),
            line("bat", c_batt, &battery),
        ]);

        let (avg_in, _) = avg_max(self.in_window().map(|s| s.input.watts));
        let (avg_sys, _) = avg_max(self.in_window().map(|s| s.system_watts));
        let (avg_batt, _) = avg_max(self.in_window().map(|s| s.battery.watts));
        let block = panel(
            accent,
            area.width,
            "Power History",
            format!("avg in {} · sys {} · bat {}", fmt_w(avg_in), fmt_w(avg_sys), fmt_signed_w(avg_batt)),
            format!(
                "since: in {:.2} Wh · sys {:.2} Wh · bat {:+.2} Wh",
                self.energy_in_wh, self.energy_sys_wh, self.energy_batt_wh
            ),
            String::new(),
        );

        let w = self.window_secs();
        let chart = Chart::new(datasets)
            .block(block)
            .legend_position(Some(LegendPosition::TopLeft))
            .hidden_legend_constraints((Constraint::Percentage(50), Constraint::Percentage(50)))
            .x_axis(
                Axis::default()
                    .bounds([-window, 0.0])
                    .labels([format!("-{}", fmt_secs(w)), format!("-{}", fmt_secs(w / 2)), "now".into()])
                    .dark_gray(),
            )
            .y_axis(
                Axis::default()
                    .bounds([lo, hi])
                    .labels([format!("{} W", fmt_num(lo)), format!("{} W", fmt_num((lo + hi) / 2.0)), format!("{} W", fmt_num(hi))])
                    .dark_gray(),
            );
        f.render_widget(chart, area);
    }
}

/// Fits a left and a right title into one border of a block `width` cells wide.
///
/// ratatui draws overlapping titles on top of each other, so when both don't fit, the right
/// title loses its " · "-separated parts from the end, then the left title is truncated with "…".
fn fit(width: u16, left: &str, right: &str) -> (String, String) {
    let avail = (width as usize).saturating_sub(2); // corners
    let cells = |s: &str| if s.is_empty() { 0 } else { s.chars().count() + 2 }; // " s "

    let mut parts: Vec<&str> = if right.is_empty() { vec![] } else { right.split(" · ").collect() };
    let mut right = parts.join(" · ");
    while !parts.is_empty() && cells(left) + cells(&right) > avail {
        parts.pop();
        right = parts.join(" · ");
    }
    let left = if cells(left) > avail {
        let keep = avail.saturating_sub(3);
        if keep == 0 { String::new() } else { left.chars().take(keep).chain(['…']).collect() }
    } else {
        left.to_string()
    };
    (left, right)
}

fn panel<'a>(color: Color, width: u16, title: &str, value: String, foot_l: String, foot_r: String) -> Block<'a> {
    let (title, value) = fit(width, title, &value);
    let (foot_l, foot_r) = fit(width, &foot_l, &foot_r);
    let mut block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(color))
        .title_style(Style::new().fg(Color::Reset))
        .title_top(Line::from(format!(" {title} ")));
    if !value.is_empty() {
        block = block.title_top(Line::from(format!(" {value} ")).right_aligned().add_modifier(Modifier::BOLD));
    }
    if !foot_l.is_empty() {
        block = block.title_bottom(Line::from(format!(" {foot_l} ")).dark_gray());
    }
    if !foot_r.is_empty() {
        block = block.title_bottom(Line::from(format!(" {foot_r} ")).right_aligned().dark_gray());
    }
    block
}

/// Newest-first data rendered right-to-left, so the latest sample sits at the right edge.
fn spark<'a>(color: Color, data: impl Iterator<Item = SparklineBar>, peak: f64) -> Sparkline<'a> {
    Sparkline::default()
        .data(data.collect::<Vec<_>>())
        .max(cw(peak).max(1))
        .direction(RenderDirection::RightToLeft)
        .style(Style::new().fg(color))
}

fn gauge<'a>(color: Color, ratio: f64, label: String) -> Gauge<'a> {
    let ratio = if ratio.is_finite() { ratio.clamp(0.0, 1.0) } else { 0.0 };
    Gauge::default()
        .gauge_style(Style::new().fg(color))
        .ratio(ratio)
        .label(Span::styled(label, Style::new().fg(Color::Reset).bold()))
        .use_unicode(true)
}

fn line<'a>(name: &'a str, color: Color, data: &'a [(f64, f64)]) -> Dataset<'a> {
    Dataset::default()
        .name(name)
        .marker(Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::new().fg(color))
        .data(data)
}

fn avg_max(values: impl Iterator<Item = f64>) -> (f64, f64) {
    let (sum, n, max) = values.fold((0.0, 0usize, f64::MIN), |(s, n, m), v| (s + v, n + 1, m.max(v)));
    if n == 0 { (0.0, 0.0) } else { (sum / n as f64, max) }
}

/// Watts → centiwatts, for integer sparkline bars.
fn cw(w: f64) -> u64 {
    (w.max(0.0) * 100.0).round() as u64
}

fn fmt_w(w: f64) -> String {
    if w.abs() < 10.0 { format!("{w:.2} W") } else { format!("{w:.1} W") }
}

fn fmt_signed_w(w: f64) -> String {
    let sign = if w < 0.0 { "−" } else { "+" };
    format!("{sign}{}", fmt_w(w.abs()))
}

fn fmt_num(v: f64) -> String {
    if (v - v.round()).abs() < 0.05 { format!("{v:.0}") } else { format!("{v:.1}") }
}

fn fmt_minutes(m: u32) -> String {
    format!("{}:{:02}", m / 60, m % 60)
}

fn fmt_secs(s: u64) -> String {
    match s {
        s if s >= 3600 && s % 3600 == 0 => format!("{}h", s / 3600),
        s if s >= 60 && s % 60 == 0 => format!("{}m", s / 60),
        s if s >= 60 => format!("{}m{}s", s / 60, s % 60),
        s => format!("{s}s"),
    }
}

fn hw_model() -> String {
    unsafe extern "C" {
        fn sysctlbyname(name: *const c_char, oldp: *mut c_void, oldlenp: *mut usize, newp: *mut c_void, newlen: usize) -> i32;
    }
    let mut buf = [0u8; 64];
    let mut len = buf.len();
    let ok = unsafe {
        sysctlbyname(c"hw.model".as_ptr(), buf.as_mut_ptr() as _, &mut len, std::ptr::null_mut(), 0) == 0
    };
    if !ok {
        return "Mac".into();
    }
    CStr::from_bytes_until_nul(&buf).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|_| "Mac".into())
}
