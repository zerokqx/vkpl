use crate::app::{App, Input, NAV, View};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, Gauge, List, ListItem, ListState, Paragraph, Row,
        Table, TableState, Wrap,
    },
};

// ANSI colors are resolved by the active terminal theme, while Reset preserves
// its foreground and background. No RGB palette is baked into the UI.
fn normal() -> Style {
    Style::default().fg(Color::Reset).bg(Color::Reset)
}

fn muted() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn accent() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

fn info() -> Style {
    Style::default().fg(Color::Blue)
}

fn artist() -> Style {
    Style::default().fg(Color::Magenta)
}

fn success() -> Style {
    Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD)
}

fn warning() -> Style {
    Style::default().fg(Color::Yellow)
}

fn selected() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::REVERSED | Modifier::BOLD)
        .remove_modifier(Modifier::DIM)
}

fn error() -> Style {
    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
}

fn panel(title: impl Into<String>, focus: bool) -> Block<'static> {
    Block::default()
        .title(format!(" {} ", title.into()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(if focus { accent() } else { muted() })
        .style(normal())
}
fn time(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
fn spinner(tick: usize) -> &'static str {
    ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"][tick % 10]
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    frame.render_widget(Block::default().style(normal()), area);
    if area.width < 45 || area.height < 14 {
        frame.render_widget(
            Paragraph::new("VKPL\nУвеличьте терминал до 45 × 14\nq — выход")
                .alignment(Alignment::Center),
            area,
        );
        return;
    }
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(4),
        Constraint::Length(5),
        Constraint::Length(1),
    ])
    .margin(1)
    .split(area);
    let header = Layout::horizontal([Constraint::Length(23), Constraint::Min(10)]).split(chunks[0]);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  VK", accent()),
            Span::styled(
                "PL",
                artist().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ),
            Span::styled("  / music", muted()),
        ])),
        header[0],
    );
    let query = if matches!(app.view, View::Artists | View::Artist | View::ArtistAlbums) {
        &app.artist_query
    } else {
        &app.query
    };
    frame.render_widget(
        Paragraph::new(if query.is_empty() {
            "Ваша музыка. Ваш ритм.".into()
        } else {
            format!("Поиск: {query}")
        })
        .style(muted())
        .alignment(Alignment::Right),
        header[1],
    );
    if app.zen {
        zen(frame, chunks[1], app);
    } else if area.width < 75 && app.sidebar {
        sidebar(frame, chunks[1], app);
    } else if area.width >= 75 {
        let body = Layout::horizontal([Constraint::Length(23), Constraint::Min(20)])
            .spacing(1)
            .split(chunks[1]);
        sidebar(frame, body[0], app);
        content(frame, body[1], app);
    } else {
        content(frame, chunks[1], app);
    }
    player(frame, chunks[2], app);
    let busy = app.loading || app.login_busy || app.buffering;
    let text = format!(
        " {} [{}{}]{}",
        if busy { spinner(app.tick) } else { "·" },
        if app.input.is_some() {
            "ВВОД"
        } else {
            "NORMAL"
        },
        if app.pending_keys().is_empty() {
            String::new()
        } else {
            format!(" {}", app.pending_keys())
        },
        if app.status.is_empty() {
            String::new()
        } else {
            format!("  {}", app.status)
        }
    );
    frame.render_widget(
        Paragraph::new(text).style(if app.error { error() } else { muted() }),
        chunks[3],
    );
    if let Some(input) = app.input {
        let width = area.width.saturating_sub(8).min(78);
        let popup = Rect::new((area.width - width) / 2, area.height / 2 - 2, width, 5);
        frame.render_widget(Clear, popup);
        let (title, value) = match input {
            Input::Search => ("Поиск музыки", app.input_text.clone()),
            Input::Artists => ("Поиск исполнителей", app.input_text.clone()),
            Input::Find => ("Найти в загруженном списке · n / N", app.input_text.clone()),
            Input::Command => (": команда", app.input_text.clone()),
            Input::Cookies => ("Путь к cookies.txt / cookies.json", app.input_text.clone()),
            Input::Token => (
                "Токен VK · ввод скрыт",
                "•".repeat(app.input_text.chars().count()),
            ),
        };
        let cursor = app.input_text[..app.input_cursor.min(app.input_text.len())]
            .chars()
            .count();
        let chars: Vec<_> = value.chars().collect();
        let start = cursor.saturating_sub(width.saturating_sub(5) as usize);
        let mut value: String = chars.iter().skip(start).take(cursor - start).collect();
        value.push('▏');
        value.extend(chars.iter().skip(cursor));
        let block = panel(title, true);
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(value),
                Line::from(""),
                Line::styled("Enter — подтвердить · Esc — отменить", muted()),
            ]),
            inner,
        );
    }
}

fn sidebar(frame: &mut Frame, area: Rect, app: &App) {
    let block = panel("Коллекция", app.sidebar);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let layout = Layout::vertical([Constraint::Min(9), Constraint::Length(4)])
        .margin(1)
        .split(inner);
    let icons = ["◈", "♫", "▤", "⌕", "≡", "○", "?", "♬", "∞"];
    let icon_styles = [
        accent(),
        success(),
        warning(),
        info(),
        artist(),
        warning(),
        info(),
        artist(),
        accent(),
    ];
    let items: Vec<_> = NAV
        .iter()
        .enumerate()
        .map(|(i, view)| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", i + 1), muted()),
                Span::styled(format!("{}  ", icons[i]), icon_styles[i]),
                Span::styled(
                    view.title(),
                    if *view == app.view {
                        accent()
                    } else {
                        normal()
                    },
                ),
            ]))
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(app.nav));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_style(selected())
            .highlight_symbol("▌ "),
        layout[0],
        &mut state,
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                if app.connected {
                    "● Подключено"
                } else {
                    "○ Не подключено"
                },
                if app.connected { success() } else { error() },
            ),
            Line::styled(&app.account, muted()),
            Line::from(""),
            Line::styled("?  Управление", info()),
        ])
        .wrap(Wrap { trim: true }),
        layout[1],
    );
}

fn content(frame: &mut Frame, area: Rect, app: &mut App) {
    let title = if matches!(app.view, View::Artist | View::ArtistAlbums) {
        app.artist
            .as_ref()
            .map(|a| a.name.as_str())
            .unwrap_or("Исполнитель")
    } else if app.view == View::Mix {
        app.mix.as_ref().map(|m| m.title()).unwrap_or("Микс")
    } else if app.view == View::Playlist {
        app.playlist
            .as_ref()
            .map(|p| p.title.as_str())
            .unwrap_or("Плейлист")
    } else {
        app.view.title()
    };
    let title = match app.view {
        View::Artist => format!("{title} · треки · o: релизы"),
        View::ArtistAlbums => format!("{title} · релизы · o: треки"),
        _ => title.to_owned(),
    };
    let block = panel(title, !app.sidebar);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.viewport = usize::from(inner.height.saturating_sub(2)).max(1);
    if app.view == View::Account {
        let text = vec![
            Line::from(""),
            Line::styled(
                "  Музыка начинается со входа",
                warning().add_modifier(Modifier::BOLD),
            ),
            Line::from(""),
            Line::from("  b   Открыть браузер и войти через VK ID"),
            Line::styled(
                "      QR, телефон, пароль — доступные способы покажет VK",
                muted(),
            ),
            Line::from(""),
            Line::from("  i   Импортировать cookies.txt или cookies.json"),
            Line::from("  t   Ввести готовый токен VK"),
            Line::from(""),
            Line::styled(format!("  {}", app.account), success()),
            Line::styled("  Сессия хранится локально. Выход: vkpl logout", muted()),
        ];
        text_panel(
            frame,
            inner,
            app,
            Paragraph::new(text).wrap(Wrap { trim: false }),
        );
        return;
    }
    if app.view == View::Help {
        let lines = [
            "БИНДЫ LUA",
            "Все сочетания берутся из ~/.config/vkpl/keymaps.lua.",
            "Измените файл и нажмите Space+r или :source.",
            "Ниже — действия стандартного файла; ваш конфиг может их менять.",
            "",
            "СТАНДАРТНЫЙ KEYMAP · НАВИГАЦИЯ",
            "h / l  Назад, меню / открыть выбранное",
            "j / k, ↑ / ↓  Вниз / вверх · 10j — на 10 строк",
            "gg / G  Начало / конец · 25G или :25 — строка 25",
            "Ctrl+d / u  Полэкрана · Ctrl+f / b  Экран",
            "PgDn / PgUp  Экран · Ctrl+e / y  Прокрутка на строку",
            "gH / M / gL  Верх / середина / низ видимой области",
            "zt / zz / zb  Выбранная строка сверху / по центру / снизу",
            "Tab, Ctrl+w w  Фокус меню / содержимое",
            "Ctrl+w h / l, Ctrl+h / l  Фокус слева / справа",
            "gt / gT  Следующий / предыдущий раздел",
            "g1…g9, Alt+1…9, 3gt  Открыть раздел по номеру",
            "Enter  Открыть / играть · Esc  Отмена / назад",
            "",
            "ПОИСК И КАТАЛОГ",
            "/  Поиск VK; во вкладке Исполнители — по исполнителям",
            "f  Найти в загруженном списке · n / N  Следующее / предыдущее",
            "ga / A  Исполнитель выбранного трека",
            "o  Треки / релизы на странице исполнителя",
            "g8  Исполнители · g9  Миксы · R  Обновить",
            "",
            "ПЛЕЕР И ОЧЕРЕДЬ",
            "p  Пауза · H / L  Предыдущий / следующий трек",
            "← / →  Перемотка 5 с · + / −  Громкость",
            "s  Случайный порядок · r  Повтор выкл / всё / один",
            "a / yy  Добавить в очередь · 3yy — добавить 3 трека",
            "dd / Delete  Удалить из очереди · 3dd — удалить 3",
            "v  Экран трека · Y  Текст песни · j/k — прокрутка",
            "",
            "КОМАНДЫ (: затем Enter)",
            ":search запрос · :artists имя · :mixes · :queue",
            ":home · :library · :playlists · :account · :help",
            ":play · :pause · :stop · :next · :prev · :reload · :source",
            "Lua-бинды: ~/.config/vkpl/keymaps.lua · :source — перечитать",
            ":q / :wq, q, ZZ, ZQ, Ctrl+C  Выход с сохранением",
            "",
            "ВВОД",
            "← / →, Home / End  Курсор · Ctrl+a / e  Начало / конец",
            "Ctrl+w  Удалить слово · Ctrl+u  Удалить до начала",
            "Esc / Ctrl+C  Отмена ввода",
            "",
            "Enter по треку заменяет очередь загруженным списком.",
            "G идёт в конец загруженных данных; дальше работает подгрузка.",
            "VK Микс получает новые порции во время воспроизведения.",
            "playerctl -p vkpl play-pause / stop / next / previous",
        ];
        let paragraph = Paragraph::new(
            lines
                .iter()
                .map(|line| Line::from(*line))
                .collect::<Vec<_>>(),
        )
        .wrap(Wrap { trim: false });
        text_panel(frame, inner, app, paragraph);
        return;
    }
    if app.count() == 0 {
        let message = if app.loading {
            "Загружаю музыку…"
        } else if app.error {
            "Не удалось загрузить данные. Подробности внизу экрана."
        } else if app.view == View::Artists {
            "Поиск исполнителей: нажмите / и введите имя"
        } else if app.view == View::Search {
            "Найдите любимый трек: нажмите /"
        } else if app.view == View::Queue {
            "Очередь пока пуста. Добавьте треки из библиотеки или поиска."
        } else {
            "Здесь пока нет музыки"
        };
        frame.render_widget(
            Paragraph::new(format!("\n\n{message}"))
                .alignment(Alignment::Center)
                .style(muted())
                .wrap(Wrap { trim: true }),
            inner,
        );
        return;
    }
    let rows: Vec<Row> = if app.view == View::Artists {
        app.artists
            .iter()
            .enumerate()
            .map(|(i, a)| {
                Row::new(vec![
                    Cell::from(format!("{:02}", i + 1)).style(muted()),
                    Cell::from(a.name.clone()).style(artist()),
                    Cell::from("Исполнитель · Enter").style(info()),
                    Cell::from(""),
                ])
            })
            .collect()
    } else if app.view == View::Mixes {
        app.mixes
            .iter()
            .enumerate()
            .map(|(i, m)| {
                Row::new(vec![
                    Cell::from(format!("{:02}", i + 1)).style(muted()),
                    Cell::from(m.title().to_owned()).style(accent()),
                    Cell::from(match m {
                        vk_music::Mix::Stream(_) => "Непрерывный микс".into(),
                        vk_music::Mix::Playlist(p) => format!("{} треков", p.count),
                    })
                    .style(warning()),
                    Cell::from(""),
                ])
            })
            .collect()
    } else if matches!(app.view, View::Playlists | View::ArtistAlbums) {
        (if app.view == View::ArtistAlbums {
            &app.artist_albums
        } else {
            &app.playlists
        })
        .iter()
        .enumerate()
        .map(|(i, p)| {
            Row::new(vec![
                Cell::from(format!("{:02}", i + 1)).style(muted()),
                Cell::from(p.title.clone()).style(warning()),
                Cell::from(format!("{} треков", p.count)).style(info()),
                Cell::from(""),
            ])
        })
        .collect()
    } else {
        app.visible_tracks()
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let active = app.playing.as_ref().is_some_and(|p| p.key() == t.key());
                Row::new(vec![
                    Cell::from(if active {
                        "♫".into()
                    } else if t.is_restricted() {
                        "×".into()
                    } else {
                        format!("{:02}", i + 1)
                    })
                    .style(if active {
                        success()
                    } else if t.is_restricted() {
                        error()
                    } else {
                        muted()
                    }),
                    Cell::from(t.title.clone()).style(if active { success() } else { normal() }),
                    Cell::from(t.artist.clone()).style(artist()),
                    Cell::from(time(t.duration as f64)).style(warning()),
                ])
                .style(if t.is_restricted() {
                    muted()
                } else if active {
                    accent()
                } else {
                    normal()
                })
            })
            .collect()
    };
    let table = Table::new(
        rows,
        [
            Constraint::Length(4),
            Constraint::Fill(3),
            Constraint::Fill(2),
            Constraint::Length(6),
        ],
    )
    .header(
        Row::new([
            "",
            "Название",
            if matches!(
                app.view,
                View::Playlists | View::ArtistAlbums | View::Artists | View::Mixes
            ) {
                "Состав"
            } else {
                "Исполнитель"
            },
            "Время",
        ])
        .style(info())
        .bottom_margin(1),
    )
    .row_highlight_style(selected())
    .highlight_symbol("▌ ")
    .column_spacing(1);
    app.selected = app.selected.min(app.count().saturating_sub(1));
    if app.selected < app.offset {
        app.offset = app.selected;
    }
    if app.selected >= app.offset + app.viewport {
        app.offset = app.selected + 1 - app.viewport;
    }
    let mut state = TableState::default()
        .with_offset(app.offset)
        .with_selected(Some(app.selected));
    frame.render_stateful_widget(table, inner, &mut state);
    app.offset = state.offset();
}

fn player(frame: &mut Frame, area: Rect, app: &App) {
    let block = panel("Сейчас играет", false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);
    let state = if app.buffering {
        spinner(app.tick)
    } else if app.paused {
        "Ⅱ"
    } else {
        "▶"
    };
    let title = if let Some(track) = &app.playing {
        Line::from(vec![
            Span::styled(
                format!("{state}  "),
                if app.paused { warning() } else { success() },
            ),
            Span::styled(&track.artist, artist()),
            Span::raw("  —  "),
            Span::styled(&track.title, accent()),
        ])
    } else {
        Line::styled("Выберите музыку для воспроизведения", muted())
    };
    frame.render_widget(Paragraph::new(title), rows[0]);
    let ratio = if app.duration > 0.0 && app.duration.is_finite() && app.position.is_finite() {
        (app.position / app.duration).clamp(0.0, 1.0)
    } else {
        0.0
    };
    frame.render_widget(
        Gauge::default()
            .ratio(ratio)
            .gauge_style(selected())
            .label(format!("{} / {}", time(app.position), time(app.duration)))
            .use_unicode(true),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!("Звук {:3.0}%", app.saved.volume), warning()),
            Span::styled(
                format!(
                    "     Порядок: {}",
                    if app.saved.shuffle {
                        "случайный"
                    } else {
                        "обычный"
                    }
                ),
                info(),
            ),
            Span::styled(
                format!("     Повтор: {}", app.saved.repeat.label()),
                artist(),
            ),
            Span::styled(format!("     Очередь: {}", app.saved.queue.len()), accent()),
            Span::styled(format!("     Буфер: {}", app.prefetched_count()), success()),
        ])),
        rows[2],
    );
}

fn zen(frame: &mut Frame, area: Rect, app: &mut App) {
    let block = panel("В музыке", false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines = vec![Line::from(""), Line::from(""), Line::styled("♫", accent())];
    if let Some(track) = &app.playing {
        lines.extend([
            Line::from(""),
            Line::styled(track.title.clone(), warning().add_modifier(Modifier::BOLD)),
            Line::styled(track.artist.clone(), artist()),
            Line::from(""),
        ]);
        if let Some(album) = &track.album {
            lines.push(Line::styled(album.title.clone(), info()));
        }
    }
    if let Some(lyrics) = &app.lyrics {
        lines.push(Line::from(""));
        lines.extend(lyrics.lines().map(|line| Line::from(line.to_owned())));
    }
    text_panel(
        frame,
        inner,
        app,
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
    );
}

fn text_panel(frame: &mut Frame, area: Rect, app: &mut App, paragraph: Paragraph<'_>) {
    app.viewport = usize::from(area.height).max(1);
    app.text_lines = paragraph.line_count(area.width).min(u16::MAX as usize);
    app.text_scroll = app
        .text_scroll
        .min(app.text_lines.saturating_sub(app.viewport));
    frame.render_widget(paragraph.scroll((app.text_scroll as u16, 0)), area);
}
