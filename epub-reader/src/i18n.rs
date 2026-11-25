// SPDX-License-Identifier: MIT

//! Localization for the standalone EPUB reader.

use std::sync::LazyLock;

use i18n_embed::DefaultLocalizer;
use i18n_embed::LanguageLoader;
use i18n_embed::Localizer;
use i18n_embed::fluent::FluentLanguageLoader;
use i18n_embed::fluent::fluent_language_loader;
use i18n_embed::unic_langid::LanguageIdentifier;
use rust_embed::RustEmbed;

/// Applies the requested languages to strings requested through `fl!()`.
pub fn init(requested_languages: &[LanguageIdentifier]) {
    if let Err(why) = localizer().select(requested_languages) {
        tracing::warn!("error while loading fluent localizations: {why}");
    }
}

fn localizer() -> Box<dyn Localizer> {
    Box::from(DefaultLocalizer::new(&*LANGUAGE_LOADER, &Localizations))
}

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

pub(crate) static LANGUAGE_LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
    let loader: FluentLanguageLoader = fluent_language_loader!();

    loader
        .load_fallback_language(&Localizations)
        .expect("Error while loading fallback language");

    loader
});

#[cfg(test)]
mod tests {
    use i18n_embed::unic_langid::LanguageIdentifier;

    use super::init;

    fn language(code: &str) -> LanguageIdentifier {
        code.parse().expect("valid language identifier")
    }

    #[test]
    fn messages_follow_the_requested_language() {
        init(&[language("fr")]);
        assert_eq!(crate::fl!("menu-file"), "Fichier");
        assert_eq!(crate::fl!("app-title"), "Lecteur EPUB");

        init(&[language("nl")]);
        assert_eq!(crate::fl!("menu-file"), "Bestand");
        assert_eq!(crate::fl!("app-title"), "EPUB-lezer");

        init(&[language("en")]);
        assert_eq!(crate::fl!("menu-file"), "File");
    }
}

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
