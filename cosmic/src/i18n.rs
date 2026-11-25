// SPDX-License-Identifier: AGPL-3.0-or-later

//! Provides localization support for this crate.

use std::sync::LazyLock;

use i18n_embed::DefaultLocalizer;
use i18n_embed::LanguageLoader;
use i18n_embed::Localizer;
use i18n_embed::fluent::FluentLanguageLoader;
use i18n_embed::fluent::fluent_language_loader;
use i18n_embed::unic_langid::LanguageIdentifier;
use rust_embed::RustEmbed;

/// Applies the requested language(s) to requested translations from the `fl!()` macro.
pub fn init(requested_languages: &[LanguageIdentifier]) {
    if let Err(why) = localizer().select(requested_languages) {
        tracing::warn!("error while loading fluent localizations: {why}");
    }
}

/// Explicit language choices offered in the UI.
pub const LANGUAGE_OPTIONS: &[&str] = &["en", "nl", "fr"];

/// Fallback-ordered languages to request: the explicit override if set and valid,
/// otherwise the OS-reported preferred languages (works on Linux, macOS, and Windows,
/// with or without a COSMIC session — it just reads the platform locale).
pub fn effective_languages(explicit_override: Option<&str>) -> Vec<LanguageIdentifier> {
    match explicit_override.and_then(|code| code.parse().ok()) {
        Some(language) => vec![language],
        None => i18n_embed::DesktopLanguageRequester::requested_languages(),
    }
}

// Get the `Localizer` to be used for localizing this library.
#[must_use]
pub fn localizer() -> Box<dyn Localizer> {
    Box::from(DefaultLocalizer::new(&*LANGUAGE_LOADER, &Localizations))
}

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

pub static LANGUAGE_LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
    let loader: FluentLanguageLoader = fluent_language_loader!();

    loader
        .load_fallback_language(&Localizations)
        .expect("Error while loading fallback language");

    loader
});

/// Request a localized string by ID from the i18n/ directory.
#[macro_export]
macro_rules! fl {
    ($message_id:literal) => {{
        i18n_embed_fl::fl!($crate::i18n::LANGUAGE_LOADER, $message_id)
    }};

    ($message_id:literal, $($args:expr),*) => {{
        i18n_embed_fl::fl!($crate::i18n::LANGUAGE_LOADER, $message_id, $($args), *)
    }};
}

#[cfg(test)]
mod tests {
    use i18n_embed::LanguageLoader;
    use i18n_embed::fluent::fluent_language_loader;
    use i18n_embed::unic_langid::LanguageIdentifier;
    use rstest::rstest;

    use super::Localizations;

    /// The catalog search-URL placeholder must show the `{searchTerms}` token,
    /// because that is the only token `build_search_url` replaces.
    #[rstest]
    #[case("en")]
    #[case("fr")]
    #[case("nl")]
    fn catalog_search_url_placeholder_shows_search_terms_token(#[case] code: &str) {
        let language: LanguageIdentifier = code.parse().expect("valid language identifier");
        let loader = fluent_language_loader!();
        loader
            .load_languages(&Localizations, &[language])
            .expect("load language");

        let placeholder = loader.get("settings-online-library-catalog-search-url-placeholder");

        assert!(
            placeholder.contains("{searchTerms}"),
            "placeholder for {code} was {placeholder:?}"
        );
    }
}
