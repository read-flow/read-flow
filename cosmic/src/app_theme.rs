// SPDX-License-Identifier: AGPL-3.0-or-later

//! Pure mapping from [`ThemeSettings`] to a libcosmic [`cosmic::Theme`].
//!
//! Per-app theme overrides: the functions here never touch global COSMIC
//! configuration — they only build a `ThemeType::Custom` theme for this app.
//!
//! @feature: app.theme_overrides

use std::sync::Arc;

use cosmic::cosmic_theme;
use cosmic::cosmic_theme::CornerRadii;
use cosmic::cosmic_theme::FontConfig;
use cosmic::cosmic_theme::Spacing;
use cosmic::cosmic_theme::ThemeBuilder;
use cosmic::cosmic_theme::palette::Srgba;
use cosmic::cosmic_theme::palette::color_difference::Wcag21RelativeContrast;
use cosmic::iced::Color;
use read_flow_core::settings::FrostedStrength;
use read_flow_core::settings::ThemeDensity;
use read_flow_core::settings::ThemeRoundness;
use read_flow_core::settings::ThemeSettings;
use read_flow_core::settings::ThemeVariant;

/// Minimum WCAG 2.1 relative contrast for normal text (AA).
const MIN_CONTRAST: f32 = 4.5;

/// Build the custom app theme for the given variant, or `None` when
/// overrides are disabled. Density, roundness, and frosted glass are shared
/// across variants; accent and background colors are per-variant.
pub fn build_theme(t: &ThemeSettings, variant: ThemeVariant) -> Option<cosmic::Theme> {
    if !t.enabled {
        return None;
    }

    let profile = t.variant(variant);
    let mut builder = match variant {
        ThemeVariant::Dark => ThemeBuilder::dark(),
        ThemeVariant::Light => ThemeBuilder::light(),
    };

    if let Some(accent) = profile.accent.as_deref().and_then(parse_hex) {
        builder = builder.accent(accent.color);
    }
    if let Some(bg) = profile.background.as_deref().and_then(parse_hex) {
        builder = builder.bg_color(bg);
    }
    if let Some(container) = profile.container_background.as_deref().and_then(parse_hex) {
        builder = builder.primary_container_bg(container);
    }

    builder = builder
        .spacing(Spacing::from(map_density(t.density)))
        .corner_radii(CornerRadii::from(map_roundness(t.roundness)));

    // Fonts are part of the theme: start from the system theme's fonts and
    // only swap the family when an override is configured.
    let system = match variant {
        ThemeVariant::Dark => cosmic::theme::system_dark(),
        ThemeVariant::Light => cosmic::theme::system_light(),
    };
    builder = builder
        .font(with_family(&system.cosmic().font, resolve_font_family(t)))
        .mono_font(with_family(
            &system.cosmic().mono_font,
            resolve_monospace_font_family(t),
        ));

    // Frosted glass needs compositor blur (COSMIC on Wayland); elsewhere it
    // would only make the window translucent without the blur, so skip it.
    if cfg!(target_os = "linux") {
        builder.frosted_windows = t.frosted;
        builder.frosted = map_frosted_strength(t.frosted_strength);
    }

    Some(cosmic::Theme::custom(Arc::new(builder.build())))
}

/// The variant (light or dark) the system is currently in.
pub fn current_system_variant() -> ThemeVariant {
    if cosmic::theme::is_dark() {
        ThemeVariant::Dark
    } else {
        ThemeVariant::Light
    }
}

/// The theme the app should use right now for `variant`: the custom theme
/// when enabled, otherwise the system preference.
pub fn effective_theme(t: &ThemeSettings, variant: ThemeVariant) -> cosmic::Theme {
    build_theme(t, variant).unwrap_or_else(cosmic::theme::system_preference)
}

fn map_density(density: ThemeDensity) -> cosmic_theme::Density {
    match density {
        ThemeDensity::Compact => cosmic_theme::Density::Compact,
        ThemeDensity::Standard => cosmic_theme::Density::Standard,
        ThemeDensity::Spacious => cosmic_theme::Density::Spacious,
    }
}

fn map_roundness(roundness: ThemeRoundness) -> cosmic_theme::Roundness {
    match roundness {
        ThemeRoundness::Round => cosmic_theme::Roundness::Round,
        ThemeRoundness::SlightlyRound => cosmic_theme::Roundness::SlightlyRound,
        ThemeRoundness::Square => cosmic_theme::Roundness::Square,
    }
}

fn map_frosted_strength(strength: FrostedStrength) -> cosmic_theme::BlurStrength {
    match strength {
        FrostedStrength::Low => cosmic_theme::BlurStrength::Low,
        FrostedStrength::Medium => cosmic_theme::BlurStrength::Medium,
        FrostedStrength::High => cosmic_theme::BlurStrength::High,
    }
}

/// Parse `#RRGGBB` or `#RRGGBBAA` into an [`Srgba`].
pub fn parse_hex(s: &str) -> Option<Srgba> {
    let hex = s.strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    let (r, g, b) = (byte(0)?, byte(2)?, byte(4)?);
    let a = match hex.len() {
        6 => 255,
        8 => byte(6)?,
        _ => return None,
    };
    Some(Srgba::new(
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        f32::from(a) / 255.0,
    ))
}

/// Format a widget [`Color`] back to `#RRGGBB` (or `#RRGGBBAA` when
/// translucent) for storage in the TOML settings.
pub fn color_to_hex(c: Color) -> String {
    let channel = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let (r, g, b, a) = (channel(c.r), channel(c.g), channel(c.b), channel(c.a));
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

/// COSMIC's named accent presets for the given variant. The `&'static str`
/// is a stable key used to look up the localized color name.
pub fn accent_presets(variant: ThemeVariant) -> Vec<(&'static str, Srgba)> {
    let palette = match variant {
        ThemeVariant::Dark => ThemeBuilder::dark().palette,
        ThemeVariant::Light => ThemeBuilder::light().palette,
    };
    let inner = palette.as_ref();
    vec![
        ("blue", inner.accent_blue),
        ("indigo", inner.accent_indigo),
        ("purple", inner.accent_purple),
        ("pink", inner.accent_pink),
        ("red", inner.accent_red),
        ("orange", inner.accent_orange),
        ("yellow", inner.accent_yellow),
        ("green", inner.accent_green),
        ("warm-grey", inner.accent_warm_grey),
    ]
}

/// The configured interface font family, validated against the installed
/// families. `None` when overrides are disabled, unset, or not installed.
fn resolve_font_family(t: &ThemeSettings) -> Option<&'static str> {
    if !t.enabled {
        return None;
    }
    let wanted = t.interface_font.as_deref()?;
    crate::fonts::fonts()
        .into_iter()
        .find(|family| *family == wanted)
}

/// The configured monospace font family, validated against the installed
/// families. `None` when overrides are disabled, unset, or not installed.
fn resolve_monospace_font_family(t: &ThemeSettings) -> Option<&'static str> {
    if !t.enabled {
        return None;
    }
    let wanted = t.monospace_font.as_deref()?;
    crate::fonts::fonts()
        .into_iter()
        .find(|family| *family == wanted)
}

/// `base` with its family replaced by `family`, keeping weight and style.
fn with_family(base: &FontConfig, family: Option<&str>) -> FontConfig {
    let mut font = base.clone();
    if let Some(name) = family {
        font.family = name.to_string();
    }
    font
}

/// True when the two colors fail WCAG 2.1 AA contrast for normal text.
pub fn contrast_warning(bg: Srgba, fg: Srgba) -> bool {
    bg.color.relative_contrast(fg.color) < MIN_CONTRAST
}

/// The derived foreground (text) color of the built theme, for contrast
/// checks against custom backgrounds.
pub fn theme_on_bg(theme: &cosmic::Theme) -> Srgba {
    theme.cosmic().on_bg_color()
}

#[cfg(test)]
mod tests {
    use assert4rs::Assert;

    use super::*;

    fn enabled() -> ThemeSettings {
        ThemeSettings {
            enabled: true,
            ..ThemeSettings::default()
        }
    }

    #[test]
    fn disabled_settings_build_no_theme() {
        Assert::that(build_theme(&ThemeSettings::default(), ThemeVariant::Light))
            .named("light")
            .is_none();
        Assert::that(build_theme(&ThemeSettings::default(), ThemeVariant::Dark))
            .named("dark")
            .is_none();
    }

    #[test]
    fn dark_variant_builds_a_dark_theme() {
        let theme = build_theme(&enabled(), ThemeVariant::Dark).unwrap();
        Assert::that(theme.cosmic().is_dark).is(true);
    }

    #[test]
    fn light_variant_builds_a_light_theme() {
        let theme = build_theme(&enabled(), ThemeVariant::Light).unwrap();
        Assert::that(theme.cosmic().is_dark).is(false);
    }

    #[test]
    fn accent_hex_is_applied() {
        let mut settings = enabled();
        settings.variant_mut(ThemeVariant::Light).accent = Some("#ff0000".into());
        let theme = build_theme(&settings, ThemeVariant::Light).unwrap();
        let accent = theme.cosmic().accent_color();
        Assert::that(accent.red > 0.8 && accent.green < 0.4 && accent.blue < 0.4).is(true);
    }

    #[test]
    fn light_and_dark_accents_are_independent() {
        let mut settings = enabled();
        settings.variant_mut(ThemeVariant::Light).accent = Some("#ff0000".into());
        settings.variant_mut(ThemeVariant::Dark).accent = Some("#00ff00".into());

        let light = build_theme(&settings, ThemeVariant::Light)
            .unwrap()
            .cosmic()
            .accent_color();
        let dark = build_theme(&settings, ThemeVariant::Dark)
            .unwrap()
            .cosmic()
            .accent_color();
        Assert::that(light.red > 0.8 && light.green < 0.4).is(true);
        Assert::that(dark.green > 0.8 && dark.red < 0.4).is(true);
    }

    #[test]
    fn compact_density_shrinks_spacing() {
        let theme = build_theme(
            &ThemeSettings {
                density: ThemeDensity::Compact,
                ..enabled()
            },
            ThemeVariant::Light,
        )
        .unwrap();
        Assert::that(theme.cosmic().spacing.space_m).is(16);
    }

    #[test]
    fn square_roundness_flattens_corners() {
        let theme = build_theme(
            &ThemeSettings {
                roundness: ThemeRoundness::Square,
                ..enabled()
            },
            ThemeVariant::Light,
        )
        .unwrap();
        Assert::that(theme.cosmic().corner_radii.radius_s).is_eq_to([2.0; 4]);
    }

    #[test]
    fn frosted_toggle_sets_frosted_windows_on_linux_only() {
        let theme = build_theme(
            &ThemeSettings {
                frosted: true,
                frosted_strength: FrostedStrength::High,
                ..enabled()
            },
            ThemeVariant::Light,
        )
        .unwrap();
        Assert::that(theme.cosmic().frosted_windows).is(cfg!(target_os = "linux"));
        if cfg!(target_os = "linux") {
            Assert::that(theme.cosmic().frosted).is(cosmic_theme::BlurStrength::High);
        }
    }

    #[test]
    fn parse_hex_accepts_rgb_and_rgba() {
        let c = parse_hex("#ff8800").unwrap();
        Assert::that((c.color.red - 1.0).abs() < 0.005).is(true);
        Assert::that((c.alpha - 1.0).abs() < 0.005).is(true);

        let c = parse_hex("#00000080").unwrap();
        Assert::that((c.alpha - 0.502).abs() < 0.005).is(true);
    }

    #[test]
    fn parse_hex_rejects_invalid_input() {
        Assert::that(parse_hex("ff8800")).is_none();
        Assert::that(parse_hex("#ff88")).is_none();
        Assert::that(parse_hex("#zzxxyy")).is_none();
    }

    #[test]
    fn color_to_hex_round_trips() {
        let hex = color_to_hex(Color::from_rgb8(0x12, 0x34, 0x56));
        Assert::that(hex.as_str()).is("#123456");

        let translucent = color_to_hex(Color::from_rgba8(0x12, 0x34, 0x56, 0.5));
        Assert::that(translucent.starts_with("#123456")).is(true);
        Assert::that(translucent.len()).is(9);
    }

    #[test]
    fn accent_presets_expose_nine_named_colors() {
        let presets = accent_presets(ThemeVariant::Dark);
        Assert::that(&presets).has_length(9);
        Assert::that(presets[0].0).is("blue");
    }

    #[test]
    fn contrast_black_on_white_passes_grey_on_grey_fails() {
        let white = Srgba::new(1.0, 1.0, 1.0, 1.0);
        let black = Srgba::new(0.0, 0.0, 0.0, 1.0);
        let grey = Srgba::new(0.5, 0.5, 0.5, 1.0);
        Assert::that(contrast_warning(white, black)).is(false);
        Assert::that(contrast_warning(grey, grey)).is(true);
    }

    #[test]
    fn unknown_interface_font_resolves_to_none() {
        let settings = ThemeSettings {
            interface_font: Some("No Such Font Family 12345".into()),
            ..enabled()
        };
        Assert::that(resolve_font_family(&settings)).is_none();
    }

    #[test]
    fn unset_interface_font_resolves_to_none() {
        Assert::that(resolve_font_family(&enabled())).is_none();
    }

    #[test]
    fn disabled_overrides_resolve_no_font() {
        let installed = crate::fonts::fonts();
        let Some(family) = installed.first().copied() else {
            return; // no fonts installed in this environment
        };
        let settings = ThemeSettings {
            enabled: false,
            interface_font: Some(family.to_string()),
            ..ThemeSettings::default()
        };
        Assert::that(resolve_font_family(&settings)).is_none();
    }

    #[test]
    fn interface_font_override_sets_theme_font_family() {
        let installed = crate::fonts::fonts();
        let Some(family) = installed.first().copied() else {
            return; // no fonts installed in this environment
        };
        let settings = ThemeSettings {
            interface_font: Some(family.to_string()),
            ..enabled()
        };
        let theme = build_theme(&settings, ThemeVariant::Dark).expect("theme");
        Assert::that(theme.cosmic().font.family.as_str()).is(family);
    }

    #[test]
    fn unset_fonts_keep_system_theme_fonts() {
        let theme = build_theme(&enabled(), ThemeVariant::Dark).expect("theme");
        let system = cosmic::theme::system_dark();
        Assert::that(theme.cosmic().font == system.cosmic().font).is(true);
        Assert::that(theme.cosmic().mono_font == system.cosmic().mono_font).is(true);
    }

    #[test]
    fn unknown_monospace_font_resolves_to_none() {
        let settings = ThemeSettings {
            monospace_font: Some("No Such Font Family 12345".into()),
            ..enabled()
        };
        Assert::that(resolve_monospace_font_family(&settings)).is_none();
    }

    #[test]
    fn unset_monospace_font_resolves_to_none() {
        Assert::that(resolve_monospace_font_family(&enabled())).is_none();
    }

    #[test]
    fn monospace_font_override_sets_theme_mono_font_family() {
        let installed = crate::fonts::fonts();
        let Some(family) = installed.first().copied() else {
            return; // no fonts installed in this environment
        };
        let settings = ThemeSettings {
            monospace_font: Some(family.to_string()),
            ..enabled()
        };
        let theme = build_theme(&settings, ThemeVariant::Light).expect("theme");
        Assert::that(theme.cosmic().mono_font.family.as_str()).is(family);
    }
}
