mod confirm;
mod input;
mod select;

use cliclack::{Theme, ThemeState};
pub use confirm::PromptConfirm;
use console::Style;
pub use input::Prompt;
pub use select::PromptSelect;

pub struct CliclackTheme;

impl Theme for CliclackTheme {
    fn bar_color(&self, state: &ThemeState) -> Style {
        match state {
            ThemeState::Active => Style::new().cyan(),
            ThemeState::Error(_) => Style::new().yellow(),
            _ => Style::new().cyan().dim(),
        }
    }
}

pub fn init_prompt_theme() {
    cliclack::set_theme(CliclackTheme);
}
