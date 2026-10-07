//! Lua declares every key binding; Rust only parses sequences and counts.
use anyhow::{Context, Result, bail};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mlua::Lua;
use std::{cell::RefCell, path::Path, rc::Rc};

#[derive(Clone, PartialEq, Eq)]
struct Stroke {
    code: KeyCode,
    modifiers: KeyModifiers,
}

impl Stroke {
    fn from_event(event: KeyEvent) -> Self {
        Self {
            code: event.code,
            modifiers: event.modifiers & (KeyModifiers::CONTROL | KeyModifiers::ALT),
        }
    }

    fn event(&self) -> KeyEvent {
        KeyEvent::new(self.code, self.modifiers)
    }
}

#[derive(Clone)]
pub enum Mapping {
    Command(String),
    Keys(Vec<KeyEvent>),
}

#[derive(Clone)]
struct Binding {
    mode: char,
    lhs: Vec<Stroke>,
    rhs: Mapping,
}

pub enum Match {
    Unmapped,
    Retry(KeyEvent),
    Pending,
    Mapped(Mapping, usize, bool),
}

#[derive(Default)]
pub struct Keymaps {
    bindings: Vec<Binding>,
    pending: Vec<Stroke>,
    count: usize,
    count_enabled: bool,
    mode: char,
}

impl Keymaps {
    pub fn load(path: &Path) -> Result<Self> {
        let source = match std::fs::read_to_string(path) {
            Ok(source) => source,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(error).with_context(|| format!("Нет файла биндов {}", path.display()));
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Не удалось прочитать {}", path.display()));
            }
        };
        let lua = Lua::new();
        let globals = lua.globals();
        let vim = lua.create_table().map_err(lua_error)?;
        let g = lua.create_table().map_err(lua_error)?;
        g.set("mapleader", " ").map_err(lua_error)?;
        vim.set("g", g.clone()).map_err(lua_error)?;
        let keymap = lua.create_table().map_err(lua_error)?;
        let bindings = Rc::new(RefCell::new(Vec::<Binding>::new()));
        let count_enabled = Rc::new(RefCell::new(false));
        let registered = bindings.clone();
        keymap
            .set(
                "set",
                lua.create_function(move |_, (mode, lhs, rhs): (String, String, String)| {
                    if mode != "n" && mode != "i" {
                        return Err(mlua::Error::external("режим должен быть 'n' или 'i'"));
                    }
                    let leader: String = g.get("mapleader")?;
                    let lhs = parse_keys(&lhs, &leader).map_err(mlua::Error::external)?;
                    if lhs.is_empty() {
                        return Err(mlua::Error::external("пустое сочетание клавиш"));
                    }
                    let rhs = if let Some(command) = rhs
                        .strip_prefix(':')
                        .and_then(|s| s.strip_suffix("<CR>").or_else(|| s.strip_suffix("<Enter>")))
                    {
                        if command.trim().is_empty() {
                            return Err(mlua::Error::external("пустая команда"));
                        }
                        Mapping::Command(command.to_owned())
                    } else {
                        let keys = parse_keys(&rhs, &leader).map_err(mlua::Error::external)?;
                        if keys.is_empty() {
                            return Err(mlua::Error::external("пустая правая часть бинда"));
                        }
                        Mapping::Keys(keys.iter().map(Stroke::event).collect())
                    };
                    let mut bindings = registered.borrow_mut();
                    if bindings.len() >= 256 {
                        return Err(mlua::Error::external("не больше 256 биндов"));
                    }
                    if let Some(existing) = bindings.iter_mut().find(|entry| {
                        entry.mode == mode.chars().next().unwrap() && entry.lhs == lhs
                    }) {
                        existing.rhs = rhs;
                    } else {
                        bindings.push(Binding {
                            mode: mode.chars().next().unwrap(),
                            lhs,
                            rhs,
                        });
                    }
                    Ok(())
                })
                .map_err(lua_error)?,
            )
            .map_err(lua_error)?;
        let count_setting = count_enabled.clone();
        keymap
            .set(
                "counts",
                lua.create_function(move |_, (mode, enabled): (String, bool)| {
                    if mode != "n" {
                        return Err(mlua::Error::external(
                            "счётчик доступен только в normal mode",
                        ));
                    }
                    *count_setting.borrow_mut() = enabled;
                    Ok(())
                })
                .map_err(lua_error)?,
            )
            .map_err(lua_error)?;
        vim.set("keymap", keymap).map_err(lua_error)?;
        globals.set("vim", vim).map_err(lua_error)?;
        lua.load(&source)
            .set_name(path.display().to_string())
            .exec()
            .map_err(|error| anyhow::anyhow!("Ошибка в {}: {error}", path.display()))?;
        let bindings = bindings.borrow().clone();
        let count_enabled = *count_enabled.borrow();
        for binding in &bindings {
            if bindings.iter().any(|other| {
                other.mode == binding.mode
                    && other.lhs.len() > binding.lhs.len()
                    && other.lhs.starts_with(&binding.lhs)
            }) {
                bail!("Один бинд является префиксом другого; задайте разные сочетания");
            }
        }
        Ok(Self {
            bindings,
            pending: Vec::new(),
            count: 0,
            count_enabled,
            mode: 'n',
        })
    }

    pub fn clear(&mut self) {
        self.pending.clear();
        self.count = 0;
    }

    pub fn pending(&self) -> String {
        let count = if self.count > 0 {
            self.count.to_string()
        } else {
            String::new()
        };
        let keys: String = self
            .pending
            .iter()
            .map(|stroke| match stroke.code {
                KeyCode::Char(ch) => ch.to_string(),
                _ => format!("<{:?}>", stroke.code),
            })
            .collect();
        format!("{count}{keys}")
    }

    pub fn feed(&mut self, mode: char, event: KeyEvent) -> Match {
        if self.mode != mode {
            self.clear();
            self.mode = mode;
        }
        if event.code == KeyCode::Esc && (!self.pending.is_empty() || self.count > 0) {
            self.clear();
            return Match::Pending;
        }
        if self.count_enabled
            && mode == 'n'
            && self.pending.is_empty()
            && let KeyCode::Char(ch @ '0'..='9') = event.code
            && (ch != '0' || self.count > 0)
        {
            self.count = (self.count * 10 + ch as usize - '0' as usize).min(999_999);
            return Match::Pending;
        }
        self.pending.push(Stroke::from_event(event));
        if let Some(binding) = self
            .bindings
            .iter()
            .find(|entry| entry.mode == mode && entry.lhs == self.pending)
        {
            let rhs = binding.rhs.clone();
            let n = self.count.max(1);
            let explicit = self.count > 0;
            self.clear();
            return Match::Mapped(rhs, n, explicit);
        }
        if self
            .bindings
            .iter()
            .any(|entry| entry.mode == mode && entry.lhs.starts_with(&self.pending))
        {
            return Match::Pending;
        }
        let retry = self.pending.len() > 1;
        self.clear();
        if retry {
            Match::Retry(event)
        } else {
            Match::Unmapped
        }
    }
}

fn lua_error(error: mlua::Error) -> anyhow::Error {
    anyhow::anyhow!(error.to_string())
}

fn parse_keys(source: &str, leader: &str) -> Result<Vec<Stroke>> {
    if source.len() > 1024 {
        bail!("слишком длинное сочетание клавиш");
    }
    let mut keys = Vec::new();
    let mut rest = source;
    while !rest.is_empty() {
        if keys.len() >= 128 {
            bail!("не больше 128 клавиш в сочетании");
        }
        let (stroke, tail) = if let Some(token) = rest.strip_prefix('<') {
            let end = token.find('>').context("незакрытая клавиша <...>")?;
            let name = &token[..end];
            let tail = &token[end + 1..];
            if name.eq_ignore_ascii_case("leader") {
                if leader.to_ascii_lowercase().contains("<leader>") {
                    bail!("mapleader не может содержать <leader>");
                }
                keys.extend(parse_keys(leader, " ")?);
                rest = tail;
                continue;
            }
            (parse_named(name)?, tail)
        } else {
            let ch = rest.chars().next().unwrap();
            (
                Stroke {
                    code: KeyCode::Char(ch),
                    modifiers: KeyModifiers::NONE,
                },
                &rest[ch.len_utf8()..],
            )
        };
        keys.push(stroke);
        rest = tail;
    }
    Ok(keys)
}

fn parse_named(name: &str) -> Result<Stroke> {
    let mut name = name;
    let mut modifiers = KeyModifiers::NONE;
    let mut shifted = false;
    loop {
        if name.len() > 2 && name.as_bytes()[1] == b'-' {
            match name.as_bytes()[0].to_ascii_lowercase() {
                b'c' => modifiers |= KeyModifiers::CONTROL,
                b'a' | b'm' => modifiers |= KeyModifiers::ALT,
                b's' => shifted = true,
                _ => bail!("неизвестный модификатор клавиши"),
            }
            name = &name[2..];
        } else {
            break;
        }
    }
    let code = match name.to_ascii_lowercase().as_str() {
        "space" => KeyCode::Char(' '),
        "cr" | "enter" | "return" => KeyCode::Enter,
        "esc" => KeyCode::Esc,
        "tab" if shifted => KeyCode::BackTab,
        "tab" => KeyCode::Tab,
        "bs" | "backspace" => KeyCode::Backspace,
        "del" | "delete" => KeyCode::Delete,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pagedown" | "pgdn" => KeyCode::PageDown,
        "pageup" | "pgup" => KeyCode::PageUp,
        _ if name.chars().count() == 1 => KeyCode::Char(if shifted {
            name.chars().next().unwrap().to_ascii_uppercase()
        } else {
            name.chars().next().unwrap()
        }),
        _ => bail!("неизвестная клавиша <{name}>"),
    };
    Ok(Stroke { code, modifiers })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn shipped_config_is_the_only_normal_mode_key_source() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("keymaps.lua");
        std::fs::write(&path, include_bytes!("../../../config/keymaps.lua")).unwrap();
        let mut keymaps = Keymaps::load(&path).unwrap();

        assert!(matches!(
            keymaps.feed('n', key(KeyCode::Char(' '))),
            Match::Pending
        ));
        assert!(matches!(
            keymaps.feed('n', key(KeyCode::Char('n'))),
            Match::Mapped(Mapping::Command(command), 1, false) if command == "next"
        ));
        assert!(matches!(
            keymaps.feed('n', key(KeyCode::Char('x'))),
            Match::Unmapped
        ));
    }
}
