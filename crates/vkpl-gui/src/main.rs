use iced::{
    Alignment, Background, Border, Color, Element, Length, Task, Theme,
    widget::{Space, button, column, container, row, text},
};
use iced_exwlshell::{
    layershell::application,
    reexport::{Anchor, Layer, LayerSize},
    settings::{LayerShellSettings, StartMode},
    to_layer_message,
};

const ACCENT: Color = Color::from_rgb8(153, 111, 255);
const BACKGROUND: Color = Color::from_rgb8(20, 20, 30);
const SURFACE: Color = Color::from_rgb8(34, 34, 49);
const MUTED: Color = Color::from_rgb8(163, 163, 184);

fn main() -> Result<(), iced_exwlshell::Error> {
    application(Overlay::default, namespace, update, view)
        .style(|_, _| iced::theme::Style {
            background_color: Color::TRANSPARENT,
            text_color: Color::WHITE,
        })
        .layer_settings(LayerShellSettings {
            size: LayerSize::fill_width(72),
            anchor: Anchor::Top,
            layer: Layer::Top,
            exclusive_zone: -1,
            start_mode: StartMode::Active,
            ..Default::default()
        })
        .run()
}

fn namespace() -> String {
    "vkpl-overlay".into()
}

#[derive(Default)]
struct Overlay {
    expanded: bool,
    playing: bool,
}

#[to_layer_message]
#[derive(Debug, Clone)]
enum Message {
    Toggle,
    TogglePlayback,
    Previous,
    Next,
}

fn update(state: &mut Overlay, message: Message) -> Task<Message> {
    match message {
        Message::Toggle => state.expanded = !state.expanded,
        Message::TogglePlayback => state.playing = !state.playing,
        Message::Previous | Message::Next => {}
        _ => {}
    }
    Task::none()
}

fn view(state: &Overlay) -> Element<'_, Message> {
    let disclosure = if state.expanded { "⌃" } else { "⌄" };
    let title_bar = row![
        text("VKPL").size(18).color(ACCENT),
        text("  /  сейчас играет").size(15).color(MUTED),
        Space::new().width(Length::Fill),
        text("emokid — Разные с тобой").size(15),
        Space::new().width(Length::Fill),
        button(text(disclosure).size(20)).on_press(Message::Toggle),
    ]
    .align_y(Alignment::Center)
    .padding([14, 24]);

    let details = row![
        container(text("♫").size(42).color(ACCENT))
            .width(74)
            .height(74)
            .center(Length::Fill),
        column![
            text("Разные с тобой").size(27),
            text("emokid").size(17).color(ACCENT),
            text("━━━━━━━━━━━━━━━●━━━━━━  0:38 / 2:46").color(MUTED),
            row![
                button("‹‹").on_press(Message::Previous),
                button(if state.playing { "Ⅱ" } else { "▶" }).on_press(Message::TogglePlayback),
                button("››").on_press(Message::Next),
            ]
            .spacing(10)
        ]
        .spacing(8)
        .width(Length::Fill),
        column![
            text("ДАЛЕЕ").size(12).color(MUTED),
            text("Дождик идет — Чучело"),
            text("Я ушел гулять — Bisexual").color(MUTED),
        ]
        .spacing(6)
        .width(300),
    ]
    .align_y(Alignment::Center)
    .spacing(22)
    .padding([12, 24]);

    let content = if state.expanded {
        column![title_bar, details]
    } else {
        column![title_bar]
    };

    container(content)
        .width(Length::Fill)
        .style(|_: &Theme| container::Style {
            background: Some(Background::Color(if state.expanded {
                SURFACE
            } else {
                BACKGROUND
            })),
            border: Border {
                color: ACCENT,
                width: 1.0,
                radius: 0.into(),
            },
            ..Default::default()
        })
        .into()
}
