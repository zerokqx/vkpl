//! Named actions selected by Lua mappings or the command line.

pub enum Action {
    Move(isize),
    Scroll(isize),
    Page(isize, bool),
    First(usize),
    Last(Option<usize>),
    Screen(char),
    Align(char),
    Focus(bool),
    ToggleFocus,
    Tab(isize),
    Section(usize),
    Delete(usize),
    Add(usize),
}

pub fn from_command(name: &str, n: usize, counted: bool) -> Option<Action> {
    use Action::*;
    Some(match name {
        "move-down" => Move(n as isize),
        "move-up" => Move(-(n as isize)),
        "scroll-down" => Scroll(n as isize),
        "scroll-up" => Scroll(-(n as isize)),
        "page-down" => Page(n as isize, false),
        "page-up" => Page(-(n as isize), false),
        "half-page-down" => Page(n as isize, true),
        "half-page-up" => Page(-(n as isize), true),
        "first" => First(if counted { n - 1 } else { 0 }),
        "last" => Last(counted.then_some(n - 1)),
        "screen-top" => Screen('H'),
        "screen-middle" => Screen('M'),
        "screen-bottom" => Screen('L'),
        "align-top" => Align('t'),
        "align-middle" => Align('z'),
        "align-bottom" => Align('b'),
        "focus-left" => Focus(true),
        "focus-right" => Focus(false),
        "focus-toggle" => ToggleFocus,
        "tab-next" if counted => Section(n - 1),
        "tab-next" => Tab(1),
        "tab-prev" => Tab(-(n as isize)),
        "queue-delete" => Delete(n),
        "queue-add" => Add(n),
        _ => return None,
    })
}
