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
use std::time::Duration;

const ACCENT: Color = Color::from_rgb8(153, 111, 255);
const BACKGROUND: Color = Color::from_rgb8(20, 20, 30);
const SURFACE: Color = Color::from_rgb8(34, 34, 49);
const MUTED: Color = Color::from_rgb8(163, 163, 184);
const COLLAPSED_HEIGHT: f32 = 72.0;
const EXPANDED_HEIGHT: f32 = 308.0;
const SPRING: f32 = 280.0;
const DAMPING: f32 = 19.0;

fn main() -> Result<(), iced_exwlshell::Error> {
    application(Overlay::default, namespace, update, view)
        .style(|_, _| iced::theme::Style {
            background_color: Color::TRANSPARENT,
            text_color: Color::WHITE,
        })
        .layer_settings(LayerShellSettings {
            size: LayerSize::fill_width(COLLAPSED_HEIGHT as u32),
            anchor: Anchor::Top,
            layer: Layer::Top,
            exclusive_zone: -1,
            start_mode: StartMode::Active,
            ..Default::default()
        })
        .subscription(subscription)
        .run()
}

fn namespace() -> String {
    "vkpl-overlay".into()
}

struct Overlay {
    expanded: bool,
    playing: bool,
    height: f32,
    velocity: f32,
}

impl Default for Overlay {
    fn default() -> Self {
        Self {
            expanded: false,
            playing: false,
            height: COLLAPSED_HEIGHT,
            velocity: 0.0,
        }
    }
}

#[to_layer_message]
#[derive(Debug, Clone)]
enum Message {
    Toggle,
    TogglePlayback,
    Previous,
    Next,
    Tick,
}

fn subscription(_: &Overlay) -> iced::Subscription<Message> {
    iced::time::every(Duration::from_millis(16)).map(|_| Message::Tick)
}

fn update(state: &mut Overlay, message: Message) -> Task<Message> {
    match message {
        Message::Toggle => {
            state.expanded = !state.expanded;
        }
        Message::TogglePlayback => state.playing = !state.playing,
        Message::Previous | Message::Next => {}
        Message::Tick => return spring_frame(state),
        _ => {}
    }
    Task::none()
}

fn spring_frame(state: &mut Overlay) -> Task<Message> {
    let target = if state.expanded {
        EXPANDED_HEIGHT
    } else {
        COLLAPSED_HEIGHT
    };
    let distance = target - state.height;
    let dt = 1.0 / 60.0;

    state.velocity += distance * SPRING * dt;
    state.velocity *= (-DAMPING * dt).exp();
    state.height = (state.height + state.velocity * dt).clamp(48.0, EXPANDED_HEIGHT + 36.0);

    if distance.abs() < 0.35 && state.velocity.abs() < 0.35 {
        state.height = target;
        state.velocity = 0.0;
    }

    Task::done(Message::LayoutChange {
        anchor: Anchor::Top,
        size: LayerSize::fill_width(state.height.round() as u32),
    })
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

    // The compositor clips this column to the animated layer surface. It makes
    // the player physically slide from the top edge instead of suddenly appearing.
    let content = column![title_bar, details];

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
