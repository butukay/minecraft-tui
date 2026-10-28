use super::App;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, HighlightSpacing, List, ListItem, Paragraph, Wrap,
    },
};

const BACKGROUND: Color = Color::Rgb(20, 23, 22);
const SURFACE: Color = Color::Rgb(27, 31, 29);
const SELECTED: Color = Color::Rgb(39, 52, 45);
const TEXT: Color = Color::Rgb(222, 228, 222);
const MUTED: Color = Color::Rgb(147, 158, 149);
const FAINT: Color = Color::Rgb(82, 96, 87);
const ACCENT: Color = Color::Rgb(169, 202, 162);

pub(super) fn ui(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    frame.render_widget(Block::default().style(base()), area);
    if area.width < 28 || area.height < 10 {
        frame.render_widget(
            Paragraph::new("Make the terminal a little larger\nto view your instances.")
                .style(muted())
                .wrap(Wrap { trim: false }),
            area.inner(Margin::new(1, 1)),
        );
        return;
    }

    let margin = if area.width >= 80 { 3 } else { 1 };
    let content = area.inner(Margin::new(margin, 1));
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(if app.search_mode { 3 } else { 0 }),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(content);

    render_header(frame, regions[0], app);
    if app.search_mode {
        render_search(frame, regions[1], app);
    }

    if app.details_mode && content.width >= 88 {
        let panes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(regions[2]);
        render_instances(frame, panes[0], app);
        let detail_area = Rect {
            x: panes[1].x.saturating_add(2),
            width: panes[1].width.saturating_sub(2),
            ..panes[1]
        };
        render_details(frame, detail_area, app);
    } else if app.details_mode {
        render_details(frame, regions[2], app);
    } else {
        render_instances(frame, regions[2], app);
    }
    render_footer(frame, regions[3], app);

    if app.settings_open {
        render_settings(frame, area, app);
    } else if app.help_open {
        render_help(frame, area);
    }
}

fn base() -> Style {
    Style::default().fg(TEXT).bg(BACKGROUND)
}

fn muted() -> Style {
    base().fg(MUTED)
}

fn accent() -> Style {
    base().fg(ACCENT)
}

fn strong() -> Style {
    base().add_modifier(Modifier::BOLD)
}

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(10),
            Constraint::Length(if area.width < 45 { 12 } else { 25 }),
        ])
        .split(area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("▪ ", accent()),
                Span::styled("minecraft", strong()),
            ]),
            Line::from(Span::styled("  Instance library", muted())),
        ]),
        columns[0],
    );
    let count = app.instances.len();
    let label = if count == 1 { "instance" } else { "instances" };
    let mode = if app.settings.hide_launcher {
        "hidden launch"
    } else {
        "Prism Launcher"
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(format!("{count} {label}"), muted())),
            Line::from(Span::styled(mode, base().fg(FAINT))),
        ])
        .alignment(Alignment::Right),
        columns[1],
    );
}

fn section(title: String) -> Block<'static> {
    Block::default()
        .borders(Borders::TOP)
        .border_style(base().fg(FAINT))
        .title(Line::from(Span::styled(format!(" {title} "), muted())))
}

fn render_search(frame: &mut Frame, area: Rect, app: &App) {
    let bar = Paragraph::new(Line::from(vec![
        Span::styled("/ ", accent()),
        Span::styled(&app.search_query, strong()),
        Span::styled("_", accent()),
    ]))
    .block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(base().fg(FAINT)),
    );
    frame.render_widget(bar, area);
}

fn render_instances(frame: &mut Frame, area: Rect, app: &mut App) {
    let count = app.filtered_instances.len();
    let title = if app.search_mode {
        format!("RESULTS  {count}")
    } else {
        "INSTANCES".to_owned()
    };
    let block = section(title).title(
        Line::from(Span::styled(
            format!(" sort: {} ", app.sort_mode.display()),
            muted(),
        ))
        .right_aligned(),
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let inner = Rect {
        y: inner.y.saturating_add(1),
        height: inner.height.saturating_sub(1),
        ..inner
    };
    if app.filtered_instances.is_empty() {
        let (title, hint) = if app.search_mode {
            (
                "No matching instances",
                "Try another name, or press Esc to clear.",
            )
        } else {
            (
                "Your library is empty",
                "Create an instance in Prism Launcher, then press r.",
            )
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::default(),
                Line::from(Span::styled(title, strong())),
                Line::default(),
                Line::from(Span::styled(hint, muted())),
            ])
            .wrap(Wrap { trim: false }),
            inner,
        );
        return;
    }

    let compact = area.height < 15;
    let items: Vec<ListItem> = app
        .filtered_instances
        .iter()
        .enumerate()
        .map(|(index, instance)| {
            let selected = app.list_state.selected() == Some(index);
            let row = if selected {
                base().bg(SELECTED)
            } else {
                base()
            };
            let mut title = vec![Span::styled(
                &instance.name,
                row.add_modifier(if selected {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
            )];
            if app.running_instances.contains(&instance.path) {
                title.push(Span::styled("  ● running", row.fg(ACCENT)));
            }
            let mut metadata = vec![
                instance
                    .mc_version
                    .as_deref()
                    .unwrap_or("unknown version")
                    .to_owned(),
                instance
                    .mod_loader
                    .as_deref()
                    .unwrap_or("Vanilla")
                    .to_owned(),
            ];
            if inner.width > 45 {
                metadata.push(
                    instance
                        .time_played
                        .as_deref()
                        .unwrap_or("not played")
                        .to_owned(),
                );
            }
            if inner.width > 65 {
                metadata.push(
                    instance
                        .last_played
                        .as_deref()
                        .unwrap_or("never launched")
                        .to_owned(),
                );
            }
            let mut lines = vec![
                Line::from(title),
                Line::from(Span::styled(metadata.join("  ·  "), row.fg(MUTED))),
            ];
            if !compact {
                lines.push(Line::default());
            }
            ListItem::new(lines).style(row)
        })
        .collect();
    let list = List::new(items)
        .highlight_symbol("▌ ")
        .highlight_spacing(HighlightSpacing::Always)
        .highlight_style(base().fg(ACCENT).bg(SELECTED));
    frame.render_stateful_widget(list, inner, &mut app.list_state);
}

fn render_details(frame: &mut Frame, area: Rect, app: &App) {
    let block = section("DETAILS".to_owned());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(instance) = app
        .list_state
        .selected()
        .and_then(|index| app.filtered_instances.get(index))
    else {
        frame.render_widget(
            Paragraph::new("\nSelect an instance to see its details.").style(muted()),
            inner,
        );
        return;
    };

    let mut lines = vec![
        Line::default(),
        Line::from(Span::styled(&instance.name, strong())),
        Line::from(Span::styled(
            if app.running_instances.contains(&instance.path) {
                "● running"
            } else {
                "Ready to play"
            },
            accent(),
        )),
        Line::default(),
        detail(
            "Minecraft",
            instance.mc_version.as_deref().unwrap_or("Unknown"),
        ),
        detail(
            "Loader",
            instance.mod_loader.as_deref().unwrap_or("Vanilla"),
        ),
        detail("Mods", &instance.mod_count.to_string()),
        Line::default(),
        detail(
            "Playtime",
            instance.time_played.as_deref().unwrap_or("Not played yet"),
        ),
        detail(
            "Last played",
            instance.last_played.as_deref().unwrap_or("Never"),
        ),
    ];
    if area.height >= 17 {
        lines.extend([
            Line::default(),
            Line::from(Span::styled("FOLDER", base().fg(FAINT))),
            Line::from(Span::styled(instance.path.display().to_string(), muted())),
        ]);
    }
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        inner.inner(Margin::new(1, 0)),
    );
}

fn detail(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<13}"), muted()),
        Span::styled(value.to_owned(), base()),
    ])
}

fn key(label: &str, action: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled(label.to_owned(), accent()),
        Span::styled(format!(" {action}  "), muted()),
    ]
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let mut hints = Vec::new();
    if app.search_mode {
        hints.extend(key("↑↓", "select"));
        hints.extend(key("enter", "launch"));
        hints.extend(key("esc", "clear search"));
    } else if area.width < 45 {
        hints.extend(key("enter", "launch"));
        hints.extend(key("?", "keys"));
    } else if area.width < 65 {
        hints.extend(key("enter", "launch"));
        hints.extend(key(",", "settings"));
        hints.extend(key("?", "keys"));
        hints.extend(key("q", "quit"));
    } else {
        hints.extend(key("↑↓", "select"));
        hints.extend(key("enter", "launch"));
        hints.extend(key("/", "search"));
        hints.extend(key(
            "i",
            if app.details_mode {
                "library"
            } else {
                "details"
            },
        ));
        hints.extend(key(",", "settings"));
        hints.extend(key("?", "keys"));
        hints.extend(key("q", "quit"));
    }
    let status = app.status.clone().unwrap_or_else(|| {
        if app.search_mode {
            format!("{} results · type to filter", app.filtered_instances.len())
        } else if app.settings.hide_launcher {
            "Launcher windows hidden · switch off in settings for sign-in or troubleshooting"
                .to_owned()
        } else {
            "".to_owned()
        }
    });
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(status, muted())),
            Line::default(),
            Line::from(hints),
        ]),
        area,
    );
}

fn modal(frame: &mut Frame, area: Rect, width: u16, height: u16, title: &str) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(base().fg(FAINT))
        .style(base().bg(SURFACE))
        .title(Line::from(Span::styled(format!(" {title} "), accent())));
    let inner = block.inner(rect).inner(Margin::new(2, 1));
    frame.render_widget(block, rect);
    inner
}

fn render_settings(frame: &mut Frame, area: Rect, app: &App) {
    let inner = modal(frame, area, 68, 19, "Settings");
    let width = inner.width as usize;
    let setting_row = |index: usize, label: &str, enabled: bool| {
        let selected = app.settings_selected == index;
        let style = if selected {
            base().bg(SELECTED)
        } else {
            base().bg(SURFACE)
        };
        let state = if enabled { "on " } else { "off" };
        let spacing = width.saturating_sub(label.chars().count() + 9);
        Line::from(vec![
            Span::styled(if selected { " › " } else { "   " }, style.fg(ACCENT)),
            Span::styled(label.to_owned(), style),
            Span::styled(" ".repeat(spacing), style),
            Span::styled(
                format!(" {state} "),
                style.fg(if enabled { ACCENT } else { MUTED }),
            ),
        ])
    };
    let compact = inner.height < 13 || inner.width < 48;
    let mut lines = vec![
        Line::from(Span::styled(
            "Launch and library preferences",
            muted().bg(SURFACE),
        )),
        Line::default(),
        setting_row(0, "Hide launcher window", app.settings.hide_launcher),
        Line::default(),
        setting_row(1, "Show instance details", app.details_mode),
        Line::default(),
    ];
    if compact {
        let regions = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(2),
            ])
            .split(inner);
        frame.render_widget(
            Paragraph::new(vec![
                setting_row(0, "Hide launcher", app.settings.hide_launcher),
                setting_row(1, "Show details", app.details_mode),
            ]),
            regions[0],
        );
        let description = app
            .status
            .as_deref()
            .unwrap_or("Hidden mode also hides sign-in.\nClose Prism before use.");
        frame.render_widget(
            Paragraph::new(description)
                .style(muted().bg(SURFACE))
                .wrap(Wrap { trim: false }),
            regions[2],
        );
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "↑↓ select · space toggle",
                    accent().bg(SURFACE),
                )),
                Line::from(Span::styled("esc close", muted().bg(SURFACE))),
            ]),
            regions[3],
        );
        return;
    }
    if app.settings_selected == 0 {
        lines.extend([
            Line::from(Span::styled(
                "Hide Prism's loading and console windows.",
                muted().bg(SURFACE),
            )),
            Line::from(Span::styled(
                "Close Prism before use. Sign-in and error dialogs",
                muted().bg(SURFACE),
            )),
            Line::from(Span::styled(
                "are hidden too; turn this off if launch stalls.",
                muted().bg(SURFACE),
            )),
        ]);
    } else {
        lines.extend([
            Line::from(Span::styled(
                "Keep version, mods and playtime close at hand.",
                muted().bg(SURFACE),
            )),
            Line::from(Span::styled(
                "On smaller terminals, details fill the library.",
                muted().bg(SURFACE),
            )),
            Line::default(),
        ]);
    }
    lines.extend([
        Line::default(),
        Line::from(Span::styled(
            app.status.as_deref().unwrap_or("Saved automatically"),
            base().fg(FAINT).bg(SURFACE),
        )),
        Line::default(),
        Line::from(
            [
                key("↑↓", "select"),
                key("space/enter", "toggle"),
                key("esc", "close"),
            ]
            .concat(),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(lines)
            .style(base().bg(SURFACE))
            .wrap(Wrap { trim: false }),
        inner,
    );
}

fn render_help(frame: &mut Frame, area: Rect) {
    let inner = modal(frame, area, 59, 21, "Keyboard shortcuts");
    if inner.width < 40 || inner.height < 15 {
        let lines = [
            "j/k, ↑↓   select",
            "enter     launch",
            "/ search  i details",
            "o folder  s sort",
            "r refresh , settings",
            "q quit",
            "",
            "any key   close",
        ];
        frame.render_widget(
            Paragraph::new(
                lines
                    .into_iter()
                    .map(|text| Line::from(Span::styled(text, muted().bg(SURFACE))))
                    .collect::<Vec<_>>(),
            ),
            inner,
        );
        return;
    }
    let actions = [
        ("↑ / ↓  j / k", "Select instance"),
        ("enter", "Launch selected instance"),
        ("/", "Search your library"),
        ("esc", "Clear search / close panel"),
        ("i", "Toggle instance details"),
        ("o", "Open instance folder"),
        ("s", "Cycle sort order"),
        ("r", "Refresh instances"),
        (",", "Open settings"),
        ("?", "Show keyboard shortcuts"),
        ("q", "Quit"),
    ];
    let mut lines = vec![Line::default()];
    for (shortcut, action) in actions {
        lines.push(Line::from(vec![
            Span::styled(format!("{shortcut:<16}"), accent().bg(SURFACE)),
            Span::styled(action, base().bg(SURFACE)),
        ]));
    }
    lines.extend([
        Line::default(),
        Line::from(Span::styled("Press any key to close", muted().bg(SURFACE))),
    ]);
    frame.render_widget(
        Paragraph::new(lines)
            .style(base().bg(SURFACE))
            .wrap(Wrap { trim: false }),
        inner,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn render(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| ui(frame, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .chunks(width as usize)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn renders_library_search_and_settings() {
        let mut app = App::demo();
        let text = render(&mut app, 110, 32);
        assert!(text.contains("Instance library"));
        assert!(text.contains("INSTANCES"));
        app.search_mode = true;
        app.search_query = "test".to_owned();
        let text = render(&mut app, 110, 32);
        assert!(text.contains("/ test_"));
        assert!(text.contains("RESULTS"));
        app.settings_open = true;
        let text = render(&mut app, 110, 32);
        assert!(text.contains("Hide launcher window"));
        assert!(text.contains("Sign-in and error dialogs"));
    }

    #[test]
    fn fits_small_terminals_and_uses_full_width_details() {
        let mut app = App::demo();
        for (width, height) in [(20, 8), (40, 16), (80, 24), (120, 35)] {
            render(&mut app, width, height);
            app.settings_open = true;
            render(&mut app, width, height);
            app.settings_open = false;
        }
        app.settings_open = true;
        let text = render(&mut app, 40, 16);
        assert!(text.contains("space toggle"));
        assert!(text.contains("esc close"));
        app.status = Some("Could not save settings: read-only".to_owned());
        let text = render(&mut app, 40, 16);
        assert!(text.contains("Could not save settings:"));
        assert!(text.contains("read-only"));
        assert!(text.contains("Hide launcher"));
        assert!(text.contains("Show details"));
        assert!(text.contains("space toggle"));
        assert!(text.contains("esc close"));
        app.settings_open = false;
        app.details_mode = true;
        let text = render(&mut app, 80, 24);
        assert!(text.contains("DETAILS"));
        assert!(!text.contains("INSTANCES"));
    }
}
