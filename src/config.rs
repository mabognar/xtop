use rust_embed::RustEmbed;
use std::fs;
use std::path::PathBuf;
use ratatui::style::Color;
use syntect::highlighting::Theme;

// Embed the top-level "themes" directory into the binary
#[derive(RustEmbed)]
#[folder = "themes/"]
pub struct BundledThemes;

pub struct Config;

impl Config {
    /// Gets the base directory (~/.xtop)
    pub fn get_base_dir() -> Option<PathBuf> {
        // Uses the `dirs` crate to reliably find the home directory across platforms
        dirs::home_dir().map(|p| p.join(".xtop"))
    }

    /// Gets (and creates if necessary) the themes directory (~/.xtop/themes)
    pub fn get_theme_dir() -> Option<PathBuf> {
        Self::get_base_dir().map(|p| {
            let theme_path = p.join("themes");
            let _ = fs::create_dir_all(&theme_path);
            theme_path
        })
    }

    /// Writes the embedded themes to disk if the directory is empty
    pub fn initialize_themes() -> std::io::Result<()> {
        if let Some(theme_dir) = Self::get_theme_dir() {
            // Check if the directory is empty
            if fs::read_dir(&theme_dir)?.next().is_none() {
                // Iterate through the embedded files
                for file in BundledThemes::iter() {
                    if let Some(embedded_file) = BundledThemes::get(&file) {
                        let path = theme_dir.join(file.as_ref());

                        // Ensure parent directories exist (in case of subfolders in themes/)
                        if let Some(parent) = path.parent() {
                            fs::create_dir_all(parent)?;
                        }

                        // Write the embedded bytes to disk
                        fs::write(path, embedded_file.data)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Gets the path to the configuration file (~/.xtop/xtoprc)
    pub fn get_config_path() -> Option<PathBuf> {
        Self::get_base_dir().map(|p| p.join("xtoprc"))
    }

    /// Loads the configuration. Returns the saved theme name, or a default.
    pub fn load_config() -> String {
        let mut theme = String::from("Default-Dark"); // The fallback default

        if let Some(path) = Self::get_config_path() {
            if let Ok(content) = fs::read_to_string(path) {
                for line in content.lines() {
                    let parts: Vec<&str> = line.splitn(2, '=').collect();
                    if parts.len() == 2 {
                        match parts[0].trim() {
                            "theme" => theme = parts[1].trim().to_string(),
                            _ => {}
                        }
                    }
                }
            }
        }
        theme
    }

    /// Saves the current configuration to the file
    pub fn save_config(theme: &str) {
        if let Some(path) = Self::get_config_path() {
            let content = format!("theme={}\n", theme);
            let _ = fs::write(path, content);
        }
    }
}

#[derive(Clone, Copy)]
pub struct UiColors {
    pub bg: Color,
    pub fg: Color,
    pub menu_bg: Color,
    pub selected_bg: Color,
    pub accent: Color,
    pub title: Color,
    pub is_dark: bool,
}

impl UiColors {
    pub fn from_theme(theme: &Theme) -> Self {
        let raw_bg = theme.settings.background.unwrap_or(syntect::highlighting::Color { r: 0, g: 0, b: 0, a: 255 });
        let raw_fg = theme.settings.foreground.unwrap_or(syntect::highlighting::Color { r: 255, g: 255, b: 255, a: 255 });

        let bg = Color::Rgb(raw_bg.r, raw_bg.g, raw_bg.b);
        let fg = Color::Rgb(raw_fg.r, raw_fg.g, raw_fg.b);

        // determine luminance
        let is_dark = (raw_bg.r as u32 + raw_bg.g as u32 + raw_bg.b as u32) < 384;

        let ui_bg = if is_dark {
            Color::Rgb(raw_bg.r.saturating_add(20), raw_bg.g.saturating_add(20), raw_bg.b.saturating_add(20))
        } else {
            Color::Rgb(raw_bg.r.saturating_sub(20), raw_bg.g.saturating_sub(20), raw_bg.b.saturating_sub(20))
        };

        let selected_bg = if raw_bg.r < 128 {
            Color::Rgb(raw_bg.r.saturating_add(40), raw_bg.g.saturating_add(40), raw_bg.b.saturating_add(40))
        } else {
            Color::Rgb(raw_bg.r.saturating_sub(40), raw_bg.g.saturating_sub(40), raw_bg.b.saturating_sub(40))
        };


        let raw_bg = theme.settings.background.unwrap_or(syntect::highlighting::Color { r: 0, g: 0, b: 0, a: 255 });
        let raw_fg = theme.settings.foreground.unwrap_or(syntect::highlighting::Color { r: 255, g: 255, b: 255, a: 255 });

        let bg = Color::Rgb(raw_bg.r, raw_bg.g, raw_bg.b);
        let fg = Color::Rgb(raw_fg.r, raw_fg.g, raw_fg.b);

        let is_dark = (raw_bg.r as u32 + raw_bg.g as u32 + raw_bg.b as u32) < 384;

        let ui_bg = if is_dark {
            Color::Rgb(raw_bg.r.saturating_add(20), raw_bg.g.saturating_add(20), raw_bg.b.saturating_add(20))
        } else {
            Color::Rgb(raw_bg.r.saturating_sub(20), raw_bg.g.saturating_sub(20), raw_bg.b.saturating_sub(20))
        };

        let selected_bg = if raw_bg.r < 128 {
            Color::Rgb(raw_bg.r.saturating_add(40), raw_bg.g.saturating_add(40), raw_bg.b.saturating_add(40))
        } else {
            Color::Rgb(raw_bg.r.saturating_sub(40), raw_bg.g.saturating_sub(40), raw_bg.b.saturating_sub(40))
        };

        let get_theme_color = |keys: &[&str]| -> Option<Color> {
            for item in &theme.scopes {
                let scope_str = format!("{:?}", item.scope).to_lowercase();
                for key in keys {
                    if scope_str.contains(key) {
                        if let Some(c) = item.style.foreground {
                            return Some(Color::Rgb(c.r, c.g, c.b));
                        }
                    }
                }
            }
            None
        };

        // Extract the accent (Functions/Variables)
        let mut accent = get_theme_color(&["entity.name.function", "variable"])
            .unwrap_or(if is_dark { Color::Rgb(100, 200, 255) } else { Color::Rgb(20, 100, 180) });

        // Extract a new color for Titles (Keywords/Strings/Constants)
        let mut title = get_theme_color(&["keyword", "string", "constant"])
            .unwrap_or(if is_dark { Color::Rgb(200, 200, 100) } else { Color::Rgb(100, 100, 50) });

        if let Some(name) = &theme.name {
            let lower_name = name.to_lowercase();
            if lower_name.contains("catppuccin") {
                accent = if is_dark { Color::Rgb(249, 226, 175) } else { Color::Rgb(223, 142, 29) };
                title = if is_dark { Color::Rgb(166, 227, 161) } else { Color::Rgb(64, 160, 43) };
            } else if lower_name.contains("base16") {
                accent = if is_dark { Color::Rgb(250, 188, 45) } else { Color::Rgb(200, 60, 20) };
                title = if is_dark { Color::Rgb(181, 189, 104) } else { Color::Rgb(113, 140, 0) };
            }
        }

        Self { bg, fg, menu_bg: ui_bg, selected_bg, accent, title, is_dark }
    }
}

