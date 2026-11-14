use ratatui::crossterm::event::{
  KeyCode,
  KeyCode::{Backspace, Char, Delete, Down, End, Enter, Esc, Home, Left, PageDown, PageUp, Right, Tab, Up},
  KeyEvent, KeyModifiers,
};
use tui_input::InputRequest;

use self::Key::{Alt, Code, Ctrl};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Command {
  Cancel,
  Quit,
  Choose,
  TogglePreview,

  Move(isize),
  First,
  Last,

  ScrollPreview(i16),
  TogglePreviewEnd,

  EditQuery(InputRequest),

  Copy,
  Rescan,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Key {
  Code(KeyCode),
  Ctrl(KeyCode),
  Alt(KeyCode),
}

impl Key {
  fn text(self) -> Option<char> {
    return match self {
      Code(Char(c)) => Some(c),
      _ => None,
    };
  }
}

impl From<KeyEvent> for Key {
  fn from(event: KeyEvent) -> Self {
    return match event.code {
      code if event.modifiers.contains(KeyModifiers::CONTROL) => Ctrl(code),
      code if event.modifiers.contains(KeyModifiers::ALT) => Alt(code),
      code => Code(code),
    };
  }
}

const BINDINGS: &[(&[Key], Command)] = &[
  (&[Code(Esc)], Command::Cancel),
  (&[Ctrl(Char('c'))], Command::Quit),
  (&[Code(Enter)], Command::Choose),
  (&[Code(Tab)], Command::TogglePreview),
  (&[Code(Down), Ctrl(Char('n')), Ctrl(Char('j'))], Command::Move(1)),
  (&[Code(Up), Ctrl(Char('p')), Ctrl(Char('k'))], Command::Move(-1)),
  (&[Code(PageDown)], Command::Move(10)),
  (&[Code(PageUp)], Command::Move(-10)),
  (&[Code(Home)], Command::First),
  (&[Code(End)], Command::Last),
  (&[Ctrl(Char('d'))], Command::ScrollPreview(10)),
  (&[Ctrl(Char('e'))], Command::ScrollPreview(-10)),
  (&[Ctrl(Char('g'))], Command::TogglePreviewEnd),
  (&[Code(Left)], Command::EditQuery(InputRequest::GoToPrevChar)),
  (&[Code(Right)], Command::EditQuery(InputRequest::GoToNextChar)),
  (&[Alt(Left), Ctrl(Left), Alt(Char('b'))], Command::EditQuery(InputRequest::GoToPrevWord)),
  (&[Alt(Right), Ctrl(Right), Alt(Char('f'))], Command::EditQuery(InputRequest::GoToNextWord)),
  (&[Ctrl(Char('a'))], Command::EditQuery(InputRequest::GoToStart)),
  (&[Code(Backspace)], Command::EditQuery(InputRequest::DeletePrevChar)),
  (&[Code(Delete)], Command::EditQuery(InputRequest::DeleteNextChar)),
  (&[Ctrl(Char('w')), Alt(Backspace)], Command::EditQuery(InputRequest::DeletePrevWord)),
  (&[Alt(Delete), Alt(Char('d'))], Command::EditQuery(InputRequest::DeleteNextWord)),
  (&[Ctrl(Char('u'))], Command::EditQuery(InputRequest::DeleteLine)),
  (&[Ctrl(Char('y'))], Command::Copy),
  (&[Ctrl(Char('r'))], Command::Rescan),
];

pub(crate) fn command(key: KeyEvent) -> Option<Command> {
  let key = Key::from(key);

  return BINDINGS
    .iter()
    .find(|(keys, _)| keys.contains(&key))
    .map(|(_, command)| *command)
    .or_else(|| key.text().map(|c| Command::EditQuery(InputRequest::InsertChar(c))));
}
