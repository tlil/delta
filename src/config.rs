use std::cmp::min;
use std::path::PathBuf;
use std::process;

use console::Term;
use regex::Regex;
use structopt::{clap, StructOpt};
use syntect::highlighting::Style as SyntectStyle;
use syntect::highlighting::Theme as SyntaxTheme;
use syntect::parsing::SyntaxSet;

use crate::bat::assets::HighlightingAssets;
use crate::bat::output::PagingMode;
use crate::cli;
use crate::color;
use crate::delta::State;
use crate::env;
use crate::git_config::GitConfig;
use crate::rewrite_options;
use crate::set_options;
use crate::style::Style;
use crate::syntax_theme;

pub enum Width {
    Fixed(usize),
    Variable,
}

pub struct Config<'a> {
    pub background_color_extends_to_terminal_width: bool,
    pub commit_style: Style,
    pub decorations_width: Width,
    pub file_added_label: String,
    pub file_modified_label: String,
    pub file_removed_label: String,
    pub file_renamed_label: String,
    pub file_style: Style,
    pub hunk_header_style: Style,
    pub list_languages: bool,
    pub list_syntax_theme_names: bool,
    pub list_syntax_themes: bool,
    pub max_buffered_lines: usize,
    pub max_line_distance: f64,
    pub max_line_distance_for_naively_paired_lines: f64,
    pub minus_emph_style: Style,
    pub minus_file: Option<PathBuf>,
    pub minus_line_marker: &'a str,
    pub minus_non_emph_style: Style,
    pub minus_style: Style,
    pub navigate: bool,
    pub null_style: Style,
    pub null_syntect_style: SyntectStyle,
    pub number_minus_format: String,
    pub number_minus_format_style: Style,
    pub number_minus_style: Style,
    pub number_plus_format: String,
    pub number_plus_format_style: Style,
    pub number_plus_style: Style,
    pub paging_mode: PagingMode,
    pub plus_emph_style: Style,
    pub plus_file: Option<PathBuf>,
    pub plus_line_marker: &'a str,
    pub plus_non_emph_style: Style,
    pub plus_style: Style,
    pub show_background_colors: bool,
    pub show_line_numbers: bool,
    pub syntax_dummy_theme: SyntaxTheme,
    pub syntax_set: SyntaxSet,
    pub syntax_theme: Option<SyntaxTheme>,
    pub syntax_theme_name: String,
    pub tab_width: usize,
    pub true_color: bool,
    pub tokenization_regex: Regex,
    pub zero_style: Style,
}

impl<'a> Config<'a> {
    pub fn from_args(args: &[&str], git_config: &mut Option<GitConfig>) -> Self {
        Self::from_arg_matches(cli::Opt::clap().get_matches_from(args), git_config)
    }

    pub fn from_arg_matches(
        arg_matches: clap::ArgMatches,
        git_config: &mut Option<GitConfig>,
    ) -> Self {
        let mut opt = cli::Opt::from_clap(&arg_matches);
        set_options::set_options(&mut opt, git_config, &arg_matches);
        rewrite_options::apply_rewrite_rules(&mut opt, &arg_matches);
        Self::from(opt)
    }

    pub fn get_style(&self, state: &State) -> &Style {
        match state {
            State::CommitMeta => &self.commit_style,
            State::FileMeta => &self.file_style,
            State::HunkHeader => &self.hunk_header_style,
            _ => unreachable("Unreachable code reached in get_style."),
        }
    }
}

fn _check_validity(opt: &cli::Opt, assets: &HighlightingAssets) {
    if opt.light && opt.dark {
        eprintln!("--light and --dark cannot be used together.");
        process::exit(1);
    }
    if let Some(ref syntax_theme) = opt.syntax_theme {
        if !syntax_theme::is_no_syntax_highlighting_theme_name(&syntax_theme) {
            if !assets.theme_set.themes.contains_key(syntax_theme.as_str()) {
                return;
            }
            let is_light_syntax_theme = syntax_theme::is_light_theme(&syntax_theme);
            if is_light_syntax_theme && opt.dark {
                eprintln!(
                    "{} is a light syntax theme, but you supplied --dark. \
                     If you use --syntax-theme, you do not need to supply --light or --dark.",
                    syntax_theme
                );
                process::exit(1);
            } else if !is_light_syntax_theme && opt.light {
                eprintln!(
                    "{} is a dark syntax theme, but you supplied --light. \
                     If you use --syntax-theme, you do not need to supply --light or --dark.",
                    syntax_theme
                );
                process::exit(1);
            }
        }
    }
}

/// Did the user supply `option` on the command line?
pub fn user_supplied_option(option: &str, arg_matches: &clap::ArgMatches) -> bool {
    arg_matches.occurrences_of(option) > 0
}

pub fn unreachable(message: &str) -> ! {
    eprintln!(
        "{} This should not be possible. \
         Please report the bug at https://github.com/dandavison/delta/issues.",
        message
    );
    process::exit(1);
}

fn is_truecolor_terminal() -> bool {
    env::get_env_var("COLORTERM")
        .map(|colorterm| colorterm == "truecolor" || colorterm == "24bit")
        .unwrap_or(false)
}

impl<'a> From<cli::Opt> for Config<'a> {
    fn from(opt: cli::Opt) -> Self {
        let assets = HighlightingAssets::new();

        _check_validity(&opt, &assets);

        let paging_mode = match opt.paging_mode.as_ref() {
            "always" => PagingMode::Always,
            "never" => PagingMode::Never,
            "auto" => PagingMode::QuitIfOneScreen,
            _ => {
                eprintln!(
                "Invalid value for --paging option: {} (valid values are \"always\", \"never\", and \"auto\")",
                opt.paging_mode
            );
                process::exit(1);
            }
        };

        let true_color = match opt.true_color.as_ref() {
            "always" => true,
            "never" => false,
            "auto" => is_truecolor_terminal(),
            _ => {
                eprintln!(
                "Invalid value for --24-bit-color option: {} (valid values are \"always\", \"never\", and \"auto\")",
                opt.true_color
            );
                process::exit(1);
            }
        };

        // Allow one character in case e.g. `less --status-column` is in effect. See #41 and #10.
        let available_terminal_width = (Term::stdout().size().1 - 1) as usize;
        let (decorations_width, background_color_extends_to_terminal_width) =
            match opt.width.as_deref() {
                Some("variable") => (Width::Variable, false),
                Some(width) => {
                    let width = width.parse().unwrap_or_else(|_| {
                        eprintln!("Could not parse width as a positive integer: {:?}", width);
                        process::exit(1);
                    });
                    (Width::Fixed(min(width, available_terminal_width)), true)
                }
                None => (Width::Fixed(available_terminal_width), true),
            };

        let syntax_theme_name_from_bat_theme = env::get_env_var("BAT_THEME");
        let (is_light_mode, syntax_theme_name) = syntax_theme::get_is_light_mode_and_theme_name(
            opt.syntax_theme.as_ref(),
            syntax_theme_name_from_bat_theme.as_ref(),
            opt.light,
            &assets.theme_set,
        );

        let (
            minus_style,
            minus_emph_style,
            minus_non_emph_style,
            zero_style,
            plus_style,
            plus_emph_style,
            plus_non_emph_style,
        ) = make_hunk_styles(&opt, is_light_mode, true_color);

        let (commit_style, file_style, hunk_header_style) =
            make_commit_file_hunk_header_styles(&opt, true_color);

        let (
            number_minus_format_style,
            number_minus_style,
            number_plus_format_style,
            number_plus_style,
        ) = make_line_number_styles(
            &opt,
            hunk_header_style.decoration_ansi_term_style(),
            true_color,
        );

        let syntax_theme = if syntax_theme::is_no_syntax_highlighting_theme_name(&syntax_theme_name)
        {
            None
        } else {
            Some(assets.theme_set.themes[&syntax_theme_name].clone())
        };
        let syntax_dummy_theme = assets.theme_set.themes.values().next().unwrap().clone();

        let minus_line_marker = if opt.keep_plus_minus_markers {
            "-"
        } else {
            " "
        };
        let plus_line_marker = if opt.keep_plus_minus_markers {
            "+"
        } else {
            " "
        };

        let max_line_distance_for_naively_paired_lines =
            env::get_env_var("DELTA_EXPERIMENTAL_MAX_LINE_DISTANCE_FOR_NAIVELY_PAIRED_LINES")
                .map(|s| s.parse::<f64>().unwrap_or(0.0))
                .unwrap_or(0.0);

        let tokenization_regex = Regex::new(&opt.tokenization_regex).unwrap_or_else(|_| {
            eprintln!(
                "Invalid word-diff-regex: {}. \
                 The value must be a valid Rust regular expression. \
                 See https://docs.rs/regex.",
                opt.tokenization_regex
            );
            process::exit(1);
        });

        Self {
            background_color_extends_to_terminal_width,
            commit_style,
            decorations_width,
            file_added_label: opt.file_added_label,
            file_modified_label: opt.file_modified_label,
            file_removed_label: opt.file_removed_label,
            file_renamed_label: opt.file_renamed_label,
            file_style,
            hunk_header_style,
            list_languages: opt.list_languages,
            list_syntax_theme_names: opt.list_syntax_theme_names,
            list_syntax_themes: opt.list_syntax_themes,
            max_buffered_lines: 32,
            max_line_distance: opt.max_line_distance,
            max_line_distance_for_naively_paired_lines,
            minus_emph_style,
            minus_file: opt.minus_file.map(|s| s.clone()),
            minus_line_marker,
            minus_non_emph_style,
            minus_style,
            navigate: opt.navigate,
            null_style: Style::new(),
            null_syntect_style: SyntectStyle::default(),
            number_minus_format: opt.number_minus_format,
            number_minus_format_style,
            number_minus_style,
            number_plus_format: opt.number_plus_format,
            number_plus_format_style,
            number_plus_style,
            paging_mode,
            plus_emph_style,
            plus_file: opt.plus_file.map(|s| s.clone()),
            plus_line_marker,
            plus_non_emph_style,
            plus_style,
            show_background_colors: opt.show_background_colors,
            show_line_numbers: opt.show_line_numbers,
            syntax_dummy_theme,
            syntax_set: assets.syntax_set,
            syntax_theme,
            syntax_theme_name,
            tab_width: opt.tab_width,
            tokenization_regex,
            true_color,
            zero_style,
        }
    }
}

fn make_hunk_styles<'a>(
    opt: &'a cli::Opt,
    is_light_mode: bool,
    true_color: bool,
) -> (Style, Style, Style, Style, Style, Style, Style) {
    let minus_style = Style::from_str(
        &opt.minus_style,
        None,
        Some(color::get_minus_background_color_default(
            is_light_mode,
            true_color,
        )),
        None,
        true_color,
        false,
    );

    let minus_emph_style = Style::from_str(
        &opt.minus_emph_style,
        None,
        Some(color::get_minus_emph_background_color_default(
            is_light_mode,
            true_color,
        )),
        None,
        true_color,
        true,
    );

    let minus_non_emph_style = Style::from_str(
        &opt.minus_non_emph_style,
        minus_style.ansi_term_style.foreground,
        minus_style.ansi_term_style.background,
        None,
        true_color,
        false,
    );

    let zero_style = Style::from_str(&opt.zero_style, None, None, None, true_color, false);

    let plus_style = Style::from_str(
        &opt.plus_style,
        None,
        Some(color::get_plus_background_color_default(
            is_light_mode,
            true_color,
        )),
        None,
        true_color,
        false,
    );

    let plus_emph_style = Style::from_str(
        &opt.plus_emph_style,
        None,
        Some(color::get_plus_emph_background_color_default(
            is_light_mode,
            true_color,
        )),
        None,
        true_color,
        true,
    );

    let plus_non_emph_style = Style::from_str(
        &opt.plus_non_emph_style,
        plus_style.ansi_term_style.foreground,
        plus_style.ansi_term_style.background,
        None,
        true_color,
        false,
    );

    (
        minus_style,
        minus_emph_style,
        minus_non_emph_style,
        zero_style,
        plus_style,
        plus_emph_style,
        plus_non_emph_style,
    )
}

fn make_line_number_styles<'a>(
    opt: &'a cli::Opt,
    default_style: Option<ansi_term::Style>,
    true_color: bool,
) -> (Style, Style, Style, Style) {
    let (default_foreground, default_background) = match default_style {
        Some(default_style) => (default_style.foreground, default_style.background),
        None => (None, None),
    };

    let number_minus_format_style = Style::from_str(
        &opt.number_minus_format_style,
        default_foreground,
        default_background,
        None,
        true_color,
        false,
    );

    let number_minus_style = Style::from_str(
        &opt.number_minus_style,
        default_foreground,
        default_background,
        None,
        true_color,
        false,
    );

    let number_plus_format_style = Style::from_str(
        &opt.number_plus_format_style,
        default_foreground,
        default_background,
        None,
        true_color,
        false,
    );

    let number_plus_style = Style::from_str(
        &opt.number_plus_style,
        default_foreground,
        default_background,
        None,
        true_color,
        false,
    );

    (
        number_minus_format_style,
        number_minus_style,
        number_plus_format_style,
        number_plus_style,
    )
}

fn make_commit_file_hunk_header_styles(opt: &cli::Opt, true_color: bool) -> (Style, Style, Style) {
    (
        Style::from_str_with_handling_of_special_decoration_attributes_and_respecting_deprecated_foreground_color_arg(
            &opt.commit_style,
            None,
            None,
            Some(&opt.commit_decoration_style),
            opt.deprecated_commit_color.as_deref(),
            true_color,
            false,
        ),
        Style::from_str_with_handling_of_special_decoration_attributes_and_respecting_deprecated_foreground_color_arg(
            &opt.file_style,
            None,
            None,
            Some(&opt.file_decoration_style),
            opt.deprecated_file_color.as_deref(),
            true_color,
            false,
        ),
        Style::from_str_with_handling_of_special_decoration_attributes_and_respecting_deprecated_foreground_color_arg(
            &opt.hunk_header_style,
            None,
            None,
            Some(&opt.hunk_header_decoration_style),
            opt.deprecated_hunk_color.as_deref(),
            true_color,
            false,
        ),
    )
}

pub fn make_navigate_regexp(config: &Config) -> String {
    format!(
        "^(commit|{}|{}|{}|{})",
        config.file_modified_label,
        config.file_added_label,
        config.file_removed_label,
        config.file_renamed_label
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    use crate::color;
    use crate::tests::integration_test_utils::integration_test_utils;

    #[test]
    fn test_syntax_theme_selection() {
        #[derive(PartialEq)]
        enum Mode {
            Light,
            Dark,
        };
        for (
            syntax_theme,
            bat_theme_env_var,
            mode, // (--light, --dark)
            expected_syntax_theme,
            expected_mode,
        ) in vec![
            (None, "", None, syntax_theme::DEFAULT_DARK_THEME, Mode::Dark),
            (Some("GitHub"), "", None, "GitHub", Mode::Light),
            (Some("GitHub"), "1337", None, "GitHub", Mode::Light),
            (None, "1337", None, "1337", Mode::Dark),
            (
                None,
                "<not set>",
                None,
                syntax_theme::DEFAULT_DARK_THEME,
                Mode::Dark,
            ),
            (
                None,
                "",
                Some(Mode::Light),
                syntax_theme::DEFAULT_LIGHT_THEME,
                Mode::Light,
            ),
            (
                None,
                "",
                Some(Mode::Dark),
                syntax_theme::DEFAULT_DARK_THEME,
                Mode::Dark,
            ),
            (
                None,
                "<@@@@@>",
                Some(Mode::Light),
                syntax_theme::DEFAULT_LIGHT_THEME,
                Mode::Light,
            ),
            (None, "GitHub", Some(Mode::Light), "GitHub", Mode::Light),
            (Some("none"), "", None, "none", Mode::Dark),
            (Some("None"), "", Some(Mode::Light), "None", Mode::Light),
        ] {
            if bat_theme_env_var == "<not set>" {
                env::remove_var("BAT_THEME")
            } else {
                env::set_var("BAT_THEME", bat_theme_env_var);
            }
            let mut args = vec![];
            if let Some(syntax_theme) = syntax_theme {
                args.push("--syntax-theme");
                args.push(syntax_theme);
            }
            let is_true_color = true;
            if is_true_color {
                args.push("--24-bit-color");
                args.push("always");
            } else {
                args.push("--24-bit-color");
                args.push("never");
            }
            match mode {
                Some(Mode::Light) => {
                    args.push("--light");
                }
                Some(Mode::Dark) => {
                    args.push("--dark");
                }
                None => {}
            }
            let config = integration_test_utils::make_config(&args);
            assert_eq!(config.syntax_theme_name, expected_syntax_theme);
            if syntax_theme::is_no_syntax_highlighting_theme_name(expected_syntax_theme) {
                assert!(config.syntax_theme.is_none())
            } else {
                assert_eq!(
                    config.syntax_theme.unwrap().name.as_ref().unwrap(),
                    expected_syntax_theme
                );
            }
            assert_eq!(
                config.minus_style.ansi_term_style.background.unwrap(),
                color::get_minus_background_color_default(
                    expected_mode == Mode::Light,
                    is_true_color
                )
            );
            assert_eq!(
                config.minus_emph_style.ansi_term_style.background.unwrap(),
                color::get_minus_emph_background_color_default(
                    expected_mode == Mode::Light,
                    is_true_color
                )
            );
            assert_eq!(
                config.plus_style.ansi_term_style.background.unwrap(),
                color::get_plus_background_color_default(
                    expected_mode == Mode::Light,
                    is_true_color
                )
            );
            assert_eq!(
                config.plus_emph_style.ansi_term_style.background.unwrap(),
                color::get_plus_emph_background_color_default(
                    expected_mode == Mode::Light,
                    is_true_color
                )
            );
        }
    }
}
