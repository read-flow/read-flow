// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use archive_organizer::ApplicationModule;
use archive_organizer::Builder;
use cosmic::app::context_drawer;
use cosmic::cosmic_config;
use cosmic::cosmic_config::CosmicConfigEntry;
use cosmic::iced::Length;
use cosmic::iced::Subscription;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::alignment::Vertical;
use cosmic::prelude::*;
use cosmic::task;
use cosmic::widget;
use cosmic::widget::about::About;
use cosmic::widget::icon;
use cosmic::widget::menu;
use cosmic::widget::nav_bar;
use cosmic::widget::segmented_button::Entity;
use cosmic::widget::segmented_button::EntityMut;
use futures_util::SinkExt;
use i18n_embed::unic_langid::LanguageIdentifier;

use crate::config::Config;
use crate::cosmic_ext::ActionExt;
use crate::fl;
use crate::page::PageMessage;
use crate::page::PageOutput;
use crate::page::PageSelector;
use crate::page::Pages;

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const APP_ICON: &[u8] = include_bytes!("../resources/icons/hicolor/scalable/apps/icon.svg");

/// The application model stores app-specific state used to describe its interface and
/// drive its logic.
pub struct AppModel {
    /// Application state which is managed by the COSMIC runtime.
    core: cosmic::Core,
    /// Display a context drawer with the designated page if defined.
    context_page: ContextPage,
    /// The about page for this app.
    about: About,
    /// Contains items assigned to the nav bar panel.
    nav: nav_bar::Model,
    /// Mappings for nav_bar items.
    nav_mappings: HashMap<PageSelector, Entity>,
    /// Key bindings for the application's menu bar.
    key_binds: HashMap<menu::KeyBind, MenuAction>,
    // Configuration data that persists between application runs.
    config: Config,
    /// Application Module
    _application_module: ApplicationModule,
    /// Pages
    pages: Pages,
}

/// Messages emitted by the application and its widgets.
#[derive(Debug, Clone)]
pub enum Message {
    SubscriptionChannel,
    ToggleContextPage(ContextPage),
    ToggleActivePageContext,
    UpdateConfig(Config),
    LaunchUrl(String),
    Page(PageMessage),
    PageAdded(PageSelector, &'static str),
    ActivePageRemoved(PageSelector),
    SwitchLanguage(LanguageIdentifier),
}

impl From<PageOutput> for Message {
    fn from(source: PageOutput) -> Self {
        match source {
            PageOutput::PageAdded(page, icon_name) => Message::PageAdded(page, icon_name),
            PageOutput::PageRemoved(page) => Message::ActivePageRemoved(page),
            PageOutput::ToggleContextPage(page_selector) => {
                Message::ToggleContextPage(ContextPage::PageContext(page_selector))
            }
        }
    }
}

impl From<PageMessage> for Message {
    fn from(source: PageMessage) -> Self {
        match source {
            PageMessage::Out(output_message) => output_message.into(),
            source => Message::Page(source),
        }
    }
}

/// Create a COSMIC application from the app model
impl cosmic::Application for AppModel {
    /// The async executor that will be used to run your application's commands.
    type Executor = cosmic::executor::Default;

    /// Data that your application receives to its init method.
    type Flags = ApplicationModule;

    /// Messages which the application and its widgets will emit.
    type Message = Message;

    /// Unique identifier in RDNN (reverse domain name notation) format.
    const APP_ID: &'static str = "com.github.peterpaul.archive-organizer-cosmic";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    /// Initializes the application with any given flags and startup commands.
    fn init(
        core: cosmic::Core,
        application_module: Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {
        // Create a nav bar with three page items.
        let mut nav = nav_bar::Model::default();
        let mut nav_mappings = HashMap::new();

        let (pages, page_action) = Pages::new(&application_module);

        nav.insert()
            .text(pages.display_name(&PageSelector::Sources))
            .data::<PageSelector>(PageSelector::Sources)
            .icon(icon::from_name("resources-symbolic"))
            .with_id(|nav_id| {
                nav_mappings.insert(PageSelector::Sources, nav_id);
            });

        for (index, selector) in pages.all_file_list_selectors().iter().enumerate() {
            nav.insert()
                .text(pages.display_name(selector))
                .data::<PageSelector>(selector.clone())
                .icon(icon::from_name("package-x-generic-symbolic"))
                .apply_if(index == 0, EntityMut::activate)
                .with_id(|nav_id| {
                    nav_mappings.insert(selector.clone(), nav_id);
                });
        }

        // Create the about widget
        let about = About::default()
            .name(fl!("app-title"))
            .icon(widget::icon::from_svg_bytes(APP_ICON))
            .version(env!("CARGO_PKG_VERSION"))
            .links([(fl!("repository"), REPOSITORY)])
            .license(env!("CARGO_PKG_LICENSE"));

        // Construct the app model with the runtime's core.
        let mut app = AppModel {
            core,
            context_page: ContextPage::default(),
            about,
            nav,
            nav_mappings,
            key_binds: HashMap::new(),
            // Optional configuration file for an application.
            config: cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
                .map(|context| match Config::get_entry(&context) {
                    Ok(config) => config,
                    Err((_errors, config)) => {
                        // for why in errors {
                        //     tracing::error!(%why, "error loading app config");
                        // }

                        config
                    }
                })
                .unwrap_or_default(),
            _application_module: application_module,
            pages,
        };

        // Create a startup command that sets the window title.
        let command = app.update_title();

        (
            app,
            cosmic::task::batch(vec![command, page_action.map(ActionExt::map_into)]),
        )
    }

    /// Elements to pack at the start of the header bar.
    fn header_start(&self) -> Vec<Element<'_, Self::Message>> {
        let menu_bar = menu::bar(vec![
            menu::Tree::with_children(
                menu::root(fl!("view")).apply(Element::from),
                menu::items(
                    &self.key_binds,
                    vec![
                        menu::Item::Button(fl!("context"), None, MenuAction::Context),
                        menu::Item::Button(fl!("about"), None, MenuAction::About),
                    ],
                ),
            ),
            menu::Tree::with_children(
                menu::root(fl!("language")).apply(Element::from),
                menu::items(
                    &self.key_binds,
                    vec![
                        menu::Item::Button(
                            fl!("language-english"),
                            None,
                            MenuAction::SwitchTo("en"),
                        ),
                        menu::Item::Button(fl!("language-dutch"), None, MenuAction::SwitchTo("nl")),
                        menu::Item::Button(
                            fl!("language-french"),
                            None,
                            MenuAction::SwitchTo("fr"),
                        ),
                    ],
                ),
            ),
        ]);

        vec![menu_bar.into()]
    }

    /// Enables the COSMIC application to create a nav bar with this model.
    fn nav_model(&self) -> Option<&nav_bar::Model> {
        Some(&self.nav)
    }

    /// Display a context drawer if the context page is requested.
    fn context_drawer(&self) -> Option<context_drawer::ContextDrawer<'_, Self::Message>> {
        if !self.core.window.show_context {
            return None;
        }

        Some(match &self.context_page {
            ContextPage::About => context_drawer::about(
                &self.about,
                |url| Message::LaunchUrl(url.to_string()),
                Message::ToggleContextPage(ContextPage::About),
            ),
            ContextPage::PageContext(page) => {
                let ContextView { title, content } = self.pages.view_context(page).map(Into::into);
                context_drawer::context_drawer(
                    content,
                    Message::ToggleContextPage(ContextPage::PageContext(page.clone())),
                )
                .title(title)
            }
        })
    }

    /// Describes the interface based on the current state of the application model.
    ///
    /// Application events will be processed through the view. Any messages emitted by
    /// events received by widgets will be passed to the update method.
    fn view(&self) -> Element<'_, Self::Message> {
        if let Some(page) = self.nav.data::<PageSelector>(self.nav.active()) {
            self.pages.view(page).map(Into::into)
        } else {
            widget::text::title1(fl!("welcome"))
                .apply(widget::container)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center)
                .into()
        }
    }

    /// Register subscriptions for this application.
    ///
    /// Subscriptions are long-running async tasks running in the background which
    /// emit messages to the application through a channel. They are started at the
    /// beginning of the application, and persist through its lifetime.
    fn subscription(&self) -> Subscription<Self::Message> {
        struct MySubscription;

        Subscription::batch(vec![
            // Create a subscription which emits updates through a channel.
            Subscription::run_with_id(
                std::any::TypeId::of::<MySubscription>(),
                cosmic::iced::stream::channel(4, move |mut channel| async move {
                    _ = channel.send(Message::SubscriptionChannel).await;

                    futures_util::future::pending().await
                }),
            ),
            // Watch for application configuration changes.
            self.core()
                .watch_config::<Config>(Self::APP_ID)
                .map(|update| {
                    // for why in update.errors {
                    //     tracing::error!(?why, "app config error");
                    // }

                    Message::UpdateConfig(update.config)
                }),
        ])
    }

    /// Handles messages emitted by the application and its widgets.
    ///
    /// Tasks may be returned for asynchronous execution of code in the background
    /// on the application's async runtime.
    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        tracing::debug!("received: {message:?}");
        match message {
            Message::SubscriptionChannel => {
                // For example purposes only.
                Task::none()
            }
            Message::ToggleContextPage(context_page) => {
                if self.context_page == context_page {
                    // Close the context drawer if the toggled context page is the same.
                    self.core.window.show_context = !self.core.window.show_context;
                } else {
                    // Open the context drawer to display the requested context page.
                    self.context_page = context_page;
                    self.core.window.show_context = true;
                }
                Task::none()
            }
            Message::ToggleActivePageContext => self
                .nav
                .data::<PageSelector>(self.nav.active())
                .map(|selector| {
                    task::message(Message::ToggleContextPage(ContextPage::PageContext(
                        selector.clone(),
                    )))
                })
                .unwrap_or_else(Task::none),
            Message::UpdateConfig(config) => {
                self.config = config;
                Task::none()
            }
            Message::LaunchUrl(url) => match open::that_detached(&url) {
                Ok(()) => Task::none(),
                Err(err) => {
                    tracing::warn!("failed to open {url:?}: {err}");
                    Task::none()
                }
            },
            Message::Page(page_message) => self.pages.update(page_message).map(ActionExt::map_into),
            Message::PageAdded(selector, icon_name) => {
                let parent = self.nav.active();
                self.nav
                    .insert()
                    .text(self.pages.display_name(&selector))
                    .data::<PageSelector>(selector.clone())
                    .data::<Entity>(parent)
                    .icon(icon::from_name(icon_name))
                    .activate()
                    .with_id(|nav_id| {
                        self.nav_mappings.insert(selector.clone(), nav_id);
                    });
                Task::none()
            }
            Message::ActivePageRemoved(removed_page) => {
                let id = self
                    .nav_mappings
                    .get(&removed_page)
                    .cloned()
                    .unwrap_or_else(|| self.nav.active());

                // Get parent of the active page
                let parent = self.nav.data::<Entity>(id);
                // If the active page has a parent, set that as the new active pagen
                if let Some(parent) = parent {
                    self.nav.activate(*parent);
                }

                // Get selector for active page
                let active_page = self.nav.data::<PageSelector>(id);
                // Verify that the active page is to be removed
                if active_page == Some(&removed_page) {
                    self.nav.remove(id);
                } else {
                    tracing::warn!("cannot (yet) remove page which isn't active");
                    // TODO: when inserting pages, capture the id using `with_id(|entity| store(entity))`
                }
                Task::none()
            }
            Message::SwitchLanguage(language) => {
                // Switch the language
                crate::i18n::localizer().select(&[language]).ok();

                // Update the window title to reflect the new language
                self.update_title()
            }
        }
    }

    /// Called when a nav item is selected.
    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<cosmic::Action<Self::Message>> {
        // Activate the page in the model.
        self.nav.activate(id);

        self.update_title()
    }
}

impl AppModel {
    /// Updates the header and window titles.
    pub fn update_title(&mut self) -> Task<cosmic::Action<Message>> {
        let mut window_title = fl!("app-title");

        if let Some(page) = self.nav.text(self.nav.active()) {
            window_title.push_str(" — ");
            window_title.push_str(page);
        }

        if let Some(id) = self.core.main_window_id() {
            self.set_window_title(window_title, id)
        } else {
            Task::none()
        }
    }
}

/// The context page to display in the context drawer.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum ContextPage {
    #[default]
    About,
    PageContext(PageSelector),
}

pub struct ContextView<'a, M> {
    pub(crate) title: String,
    pub(crate) content: Element<'a, M>,
}

impl<'a, M: 'a> ContextView<'a, M> {
    pub fn map<F, N>(self, mapper: F) -> ContextView<'a, N>
    where
        F: Fn(M) -> N + 'a,
        N: 'a,
    {
        let ContextView { title, content } = self;
        ContextView {
            title,
            content: content.map(mapper),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuAction {
    About,
    Context,
    SwitchTo(&'static str),
}

impl menu::action::MenuAction for MenuAction {
    type Message = Message;

    fn message(&self) -> Self::Message {
        match self {
            MenuAction::About => Message::ToggleContextPage(ContextPage::About),
            MenuAction::Context => Message::ToggleActivePageContext,
            MenuAction::SwitchTo(language) => Message::SwitchLanguage(language.parse().unwrap()),
        }
    }
}
