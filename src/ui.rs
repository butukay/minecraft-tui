use super::{App, theme::Palette};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, HighlightSpacing, List, ListItem, Paragraph, Wrap,
    },
};

pub(super) fn ui(frame: &mut Frame, app: &mut App) {
    let palette = &app.settings.theme.palette();
    let area = frame.area();
    frame.render_widget(Block::default().style(base(palette)), area);
    if area.width < 28 || area.height < 10 {
        frame.render_widget(
            Paragraph::new("Make the terminal a little larger\nto view your instances.")
                .style(muted(palette))
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

    render_header(frame, regions[0], app, palette);
    if app.search_mode {
        render_search(frame, regions[1], app, palette);
    }

    if app.details_mode && content.width >= 88 {
        let panes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(regions[2]);
        render_instances(frame, panes[0], app, palette);
        let detail_area = Rect {
            x: panes[1].x.saturating_add(2),
            width: panes[1].width.saturating_sub(2),
            ..panes[1]
        };
        render_details(frame, detail_area, app, palette);
    } else if app.details_mode {
        render_details(frame, regions[2], app, palette);
    } else {
        render_instances(frame, regions[2], app, palette);
    }
    render_footer(frame, regions[3], app, palette);

    if app.settings_open {
        render_settings(frame, area, app, palette);
    } else if app.help_open {
        render_help(frame, area, palette);
    }
}

fn base(palette: &Palette) -> Style {
    Style::default().fg(palette.text).bg(palette.background)
}

fn muted(palette: &Palette) -> Style {
    let style = base(palette).fg(if palette.terminal {
        palette.text
    } else {
        palette.muted
    });
    if palette.terminal {
        style.add_modifier(Modifier::DIM)
    } else {
        style
    }
}

fn faint(palette: &Palette) -> Style {
    let style = base(palette).fg(if palette.terminal {
        palette.text
    } else {
        palette.faint
    });
    if palette.terminal {
        style.add_modifier(Modifier::DIM)
    } else {
        style
    }
}

fn selected(palette: &Palette) -> Style {
    if palette.terminal {
        base(palette).add_modifier(Modifier::REVERSED)
    } else {
        base(palette).bg(palette.selected)
    }
}

fn row_muted(style: Style, palette: &Palette) -> Style {
    let style = style.fg(if palette.terminal {
        palette.text
    } else {
        palette.muted
    });
    if palette.terminal {
        style.add_modifier(Modifier::DIM)
    } else {
        style
    }
}

fn accent(palette: &Palette) -> Style {
    base(palette).fg(palette.accent)
}

fn strong(palette: &Palette) -> Style {
    base(palette).add_modifier(Modifier::BOLD)
}

fn render_header(frame: &mut Frame, area: Rect, app: &App, palette: &Palette) {
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
                Span::styled("▪ ", accent(palette)),
                Span::styled("minecraft", strong(palette)),
            ]),
            Line::from(Span::styled("  Instance library", muted(palette))),
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
            Line::from(Span::styled(format!("{count} {label}"), muted(palette))),
            Line::from(Span::styled(mode, faint(palette))),
        ])
        .alignment(Alignment::Right),
        columns[1],
    );
}

fn section(title: String, palette: &Palette) -> Block<'static> {
    Block::default()
        .borders(Borders::TOP)
        .border_style(faint(palette))
        .title(Line::from(Span::styled(
            format!(" {title} "),
            muted(palette),
        )))
}

fn render_search(frame: &mut Frame, area: Rect, app: &App, palette: &Palette) {
    let bar = Paragraph::new(Line::from(vec![
        Span::styled("/ ", accent(palette)),
        Span::styled(&app.search_query, strong(palette)),
        Span::styled("_", accent(palette)),
    ]))
    .block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(faint(palette)),
    );
    frame.render_widget(bar, area);
}

fn render_instances(frame: &mut Frame, area: Rect, app: &mut App, palette: &Palette) {
    let count = app.filtered_instances.len();
    let title = if app.search_mode {
        format!("RESULTS  {count}")
    } else {
        "INSTANCES".to_owned()
    };
    let block = section(title, palette).title(
        Line::from(Span::styled(
            format!(" sort: {} ", app.sort_mode.display()),
            muted(palette),
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
                Line::from(Span::styled(title, strong(palette))),
                Line::default(),
                Line::from(Span::styled(hint, muted(palette))),
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
            let is_selected = app.list_state.selected() == Some(index);
            let row = if is_selected {
                selected(palette)
            } else {
                base(palette)
            };
            let mut title = vec![Span::styled(
                &instance.name,
                row.add_modifier(if is_selected {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
            )];
            if app.running_instances.contains(&instance.path) {
                title.push(Span::styled("  ● running", row.fg(palette.accent)));
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
                Line::from(Span::styled(
                    metadata.join("  ·  "),
                    row_muted(row, palette),
                )),
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
        .highlight_style(selected(palette));
    frame.render_stateful_widget(list, inner, &mut app.list_state);
}

fn render_details(frame: &mut Frame, area: Rect, app: &App, palette: &Palette) {
    let block = section("DETAILS".to_owned(), palette);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(instance) = app
        .list_state
        .selected()
        .and_then(|index| app.filtered_instances.get(index))
    else {
        frame.render_widget(
            Paragraph::new("\nSelect an instance to see its details.").style(muted(palette)),
            inner,
        );
        return;
    };

    let mut lines = vec![
        Line::default(),
        Line::from(Span::styled(&instance.name, strong(palette))),
        Line::from(Span::styled(
            if app.running_instances.contains(&instance.path) {
                "● running"
            } else {
                "Ready to play"
            },
            accent(palette),
        )),
        Line::default(),
        detail(
            "Minecraft",
            instance.mc_version.as_deref().unwrap_or("Unknown"),
            palette,
        ),
        detail(
            "Loader",
            instance.mod_loader.as_deref().unwrap_or("Vanilla"),
            palette,
        ),
        detail("Mods", &instance.mod_count.to_string(), palette),
        Line::default(),
        detail(
            "Playtime",
            instance.time_played.as_deref().unwrap_or("Not played yet"),
            palette,
        ),
        detail(
            "Last played",
            instance.last_played.as_deref().unwrap_or("Never"),
            palette,
        ),
    ];
    if area.height >= 17 {
        lines.extend([
            Line::default(),
            Line::from(Span::styled("FOLDER", faint(palette))),
            Line::from(Span::styled(
                instance.path.display().to_string(),
                muted(palette),
            )),
        ]);
    }
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        inner.inner(Margin::new(1, 0)),
    );
}

fn detail(label: &str, value: &str, palette: &Palette) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<13}"), muted(palette)),
        Span::styled(value.to_owned(), base(palette)),
    ])
}

fn key(label: &str, action: &str, palette: &Palette) -> Vec<Span<'static>> {
    let action_style = Style::default()
        .fg(if palette.terminal {
            palette.text
        } else {
            palette.muted
        })
        .add_modifier(if palette.terminal {
            Modifier::DIM
        } else {
            Modifier::empty()
        });
    vec![
        Span::styled(label.to_owned(), Style::default().fg(palette.accent)),
        Span::styled(format!(" {action}  "), action_style),
    ]
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App, palette: &Palette) {
    let mut hints = Vec::new();
    if app.search_mode {
        hints.extend(key("↑↓", "select", palette));
        hints.extend(key("enter", "launch", palette));
        hints.extend(key("esc", "clear search", palette));
    } else if area.width < 45 {
        hints.extend(key("enter", "launch", palette));
        hints.extend(key("?", "keys", palette));
    } else if area.width < 65 {
        hints.extend(key("enter", "launch", palette));
        hints.extend(key(",", "settings", palette));
        hints.extend(key("?", "keys", palette));
        hints.extend(key("q", "quit", palette));
    } else {
        hints.extend(key("↑↓", "select", palette));
        hints.extend(key("enter", "launch", palette));
        hints.extend(key("/", "search", palette));
        hints.extend(key(
            "i",
            if app.details_mode {
                "library"
            } else {
                "details"
            },
            palette,
        ));
        hints.extend(key(",", "settings", palette));
        hints.extend(key("?", "keys", palette));
        hints.extend(key("q", "quit", palette));
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
            Line::from(Span::styled(status, muted(palette))),
            Line::default(),
            Line::from(hints),
        ]),
        area,
    );
}

fn modal(
    frame: &mut Frame,
    area: Rect,
    width: u16,
    height: u16,
    title: &str,
    palette: &Palette,
) -> Rect {
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
        .border_style(faint(palette))
        .style(base(palette).bg(palette.surface))
        .title(Line::from(Span::styled(
            format!(" {title} "),
            accent(palette),
        )));
    let inner = block.inner(rect).inner(Margin::new(2, 1));
    frame.render_widget(block, rect);
    inner
}

fn render_settings(frame: &mut Frame, area: Rect, app: &App, palette: &Palette) {
    let inner = modal(frame, area, 68, 21, "Settings", palette);
    let width = inner.width as usize;
    let setting_row = |index: usize, label: &str, value: &str, enabled: bool| {
        let is_selected = app.settings_selected == index;
        let style = if is_selected {
            selected(palette)
        } else {
            base(palette).bg(palette.surface)
        };
        let value_color = if is_selected && palette.terminal {
            palette.text
        } else if enabled {
            palette.accent
        } else {
            palette.muted
        };
        let spacing = width.saturating_sub(label.chars().count() + value.chars().count() + 7);
        Line::from(vec![
            Span::styled(
                if is_selected { " › " } else { "   " },
                style.fg(value_color),
            ),
            Span::styled(label.to_owned(), style),
            Span::styled(" ".repeat(spacing), style),
            Span::styled(format!(" {value} "), style.fg(value_color)),
        ])
    };
    let hide_state = if app.settings.hide_launcher {
        "on "
    } else {
        "off"
    };
    let details_state = if app.details_mode { "on " } else { "off" };
    let theme_value = format!("‹ {} ›", app.settings.theme.display());
    let description = match app.settings_selected {
        0 => {
            "Hide Prism's loading and console windows.\nClose Prism before use. Sign-in and error dialogs\nare hidden too; turn this off if launch stalls."
        }
        1 => {
            "Keep version, mods and playtime close at hand.\nOn smaller terminals, details fill the library."
        }
        _ => {
            "Terminal follows your terminal's colors.\nSage, Violet and Paper use their own palettes.\nUse left / right or enter to change the theme."
        }
    };
    if inner.height < 15 || inner.width < 48 {
        let regions = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(2),
            ])
            .split(inner);
        frame.render_widget(
            Paragraph::new(vec![
                setting_row(0, "Hide launcher", hide_state, app.settings.hide_launcher),
                setting_row(1, "Show details", details_state, app.details_mode),
                setting_row(2, "Theme", &theme_value, true),
            ]),
            regions[0],
        );
        let description = app
            .status
            .as_deref()
            .unwrap_or(match app.settings_selected {
                0 => "Hidden mode also hides sign-in.\nClose Prism before use.",
                1 => "Details replace the library\non smaller terminals.",
                _ => "Terminal follows your colors.\n← / → change theme.",
            });
        frame.render_widget(
            Paragraph::new(description)
                .style(muted(palette).bg(palette.surface))
                .wrap(Wrap { trim: false }),
            regions[2],
        );
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "↑↓ select · space change",
                    accent(palette).bg(palette.surface),
                )),
                Line::from(Span::styled(
                    "←→ change · esc close",
                    muted(palette).bg(palette.surface),
                )),
            ]),
            regions[3],
        );
        return;
    }
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(6),
            Constraint::Min(0),
            Constraint::Length(2),
            Constraint::Length(1),
        ])
        .split(inner);
    frame.render_widget(
        Paragraph::new("Launch and library preferences").style(muted(palette).bg(palette.surface)),
        regions[0],
    );
    frame.render_widget(
        Paragraph::new(vec![
            setting_row(
                0,
                "Hide launcher window",
                hide_state,
                app.settings.hide_launcher,
            ),
            Line::default(),
            setting_row(1, "Show instance details", details_state, app.details_mode),
            Line::default(),
            setting_row(2, "Theme", &theme_value, true),
        ]),
        regions[1],
    );
    frame.render_widget(
        Paragraph::new(description)
            .style(muted(palette).bg(palette.surface))
            .wrap(Wrap { trim: false }),
        regions[2],
    );
    frame.render_widget(
        Paragraph::new(app.status.as_deref().unwrap_or("Saved automatically"))
            .style(faint(palette).bg(palette.surface))
            .wrap(Wrap { trim: false }),
        regions[3],
    );
    frame.render_widget(
        Paragraph::new(Line::from(
            [
                key("↑↓", "select", palette),
                key("←→ / enter", "change", palette),
                key("esc", "close", palette),
            ]
            .concat(),
        ))
        .style(base(palette).bg(palette.surface)),
        regions[4],
    );
}

fn render_help(frame: &mut Frame, area: Rect, palette: &Palette) {
    let inner = modal(frame, area, 59, 21, "Keyboard shortcuts", palette);
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
                    .map(|text| Line::from(Span::styled(text, muted(palette).bg(palette.surface))))
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
            Span::styled(
                format!("{shortcut:<16}"),
                accent(palette).bg(palette.surface),
            ),
            Span::styled(action, base(palette).bg(palette.surface)),
        ]));
    }
    lines.extend([
        Line::default(),
        Line::from(Span::styled(
            "Press any key to close",
            muted(palette).bg(palette.surface),
        )),
    ]);
    frame.render_widget(
        Paragraph::new(lines)
            .style(base(palette).bg(palette.surface))
            .wrap(Wrap { trim: false }),
        inner,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Color};

    fn render_buffer(app: &mut App, width: u16, height: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| ui(frame, app)).unwrap();
        terminal.backend().buffer().clone()
    }

    fn render(app: &mut App, width: u16, height: u16) -> String {
        buffer_text(&render_buffer(app, width, height))
    }

    fn buffer_text(buffer: &Buffer) -> String {
        buffer
            .content()
            .chunks(buffer.area.width as usize)
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
        assert!(text.contains("space change"));
        assert!(text.contains("esc close"));
        app.status = Some("Could not save settings: read-only".to_owned());
        let text = render(&mut app, 40, 16);
        assert!(text.contains("Could not save settings:"));
        assert!(text.contains("read-only"));
        assert!(text.contains("Hide launcher"));
        assert!(text.contains("Show details"));
        assert!(text.contains("Theme"));
        assert!(text.contains("Terminal"));
        assert!(text.contains("space change"));
        assert!(text.contains("esc close"));
        app.settings_open = false;
        app.details_mode = true;
        let text = render(&mut app, 80, 24);
        assert!(text.contains("DETAILS"));
        assert!(!text.contains("INSTANCES"));
    }

    #[test]
    fn terminal_theme_inherits_colors_in_every_view() {
        let mut app = App::demo();
        assert_eq!(app.settings.theme, Theme::Terminal);
        for view in 0..5 {
            app.search_mode = view == 1;
            app.details_mode = view == 2;
            app.settings_open = view == 3;
            app.help_open = view == 4;
            let buffer = render_buffer(&mut app, 110, 32);
            assert!(
                buffer.content().iter().all(|cell| {
                    cell.bg == Color::Reset && !matches!(cell.fg, Color::Rgb(_, _, _))
                }),
                "terminal colors must be inherited in view {view}"
            );
        }
        app.help_open = false;
        let buffer = render_buffer(&mut app, 110, 32);
        assert!(
            buffer
                .content()
                .iter()
                .any(|cell| cell.modifier.contains(Modifier::REVERSED))
        );
        assert!(
            buffer
                .content()
                .iter()
                .any(|cell| cell.modifier.contains(Modifier::DIM))
        );
    }

    #[test]
    fn selected_themes_apply_to_search_details_and_overlays() {
        let mut app = App::demo();
        let terminal = render_buffer(&mut app, 110, 32);
        for theme in Theme::ALL {
            app.settings.theme = theme;
            let palette = theme.palette();
            for view in 0..5 {
                app.search_mode = view == 1;
                app.details_mode = view == 2;
                app.settings_open = view == 3;
                app.settings_selected = 2;
                app.help_open = view == 4;
                let buffer = render_buffer(&mut app, 110, 32);
                assert!(
                    buffer.content().iter().all(|cell| {
                        [palette.background, palette.surface, palette.selected].contains(&cell.bg)
                    }),
                    "unexpected background in {theme:?}, view {view}"
                );
                assert!(
                    buffer
                        .content()
                        .iter()
                        .any(|cell| cell.fg == palette.accent)
                );
                assert!(
                    buffer.content().iter().all(|cell| {
                        [palette.text, palette.muted, palette.faint, palette.accent]
                            .contains(&cell.fg)
                    }),
                    "unexpected foreground in {theme:?}, view {view}"
                );
                if theme != Theme::Terminal {
                    assert_ne!(buffer, terminal);
                    assert_eq!(buffer[(0, 0)].bg, palette.background);
                }
                if view == 3 {
                    assert!(buffer_text(&buffer).contains(theme.display()));
                    assert!(buffer_text(&buffer).contains("Theme"));
                }
            }
        }
    }
}
