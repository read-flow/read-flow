// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use cosmic::app::context_drawer;
use cosmic::cosmic_config;
use cosmic::cosmic_config::CosmicConfigEntry;
use cosmic::iced::Subscription;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::event;
use cosmic::iced::event::Event;
use cosmic::iced::keyboard::Event as KeyEvent;
use cosmic::prelude::*;
use cosmic::task;
use cosmic::widget;
use cosmic::widget::about::About;
use cosmic::widget::icon;
use cosmic::widget::menu;
use cosmic::widget::nav_bar;
use cosmic::widget::segmented_button::Entity;
use futures::StreamExt;
use i18n_embed::unic_langid::LanguageIdentifier;
use provider::r#async::HasSetExpired;
use read_flow_core::scan::DirectorySettings;

use crate::ApplicationModule;
use crate::aggregator::Document;
use crate::component::scan_progress::ScanComponent;
use crate::component::scan_progress::ScanProgressMessage;
use crate::component::scan_progress::ScanProgressOutput;
use crate::config::Config;
use crate::cosmic_ext::ActionExt;
use crate::fl;
use crate::layout::full_page;
use crate::page::PageMessage;
use crate::page::PageOutput;
use crate::page::PageSelector;
use crate::page::Pages;
use crate::page::SourcesMessage;
use crate::page::settings_invalidation_subscription;

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const APP_ICON: &[u8] = include_bytes!("../resources/icons/hicolor/scalable/apps/icon.svg");

/// The application model stores app-specific state used to describe its interface and
/// drive its logic.
pub struct ReadFlow {
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
    application_module: Arc<ApplicationModule>,
    /// Pages
    pages: Pages,
    /// Scan progress component, present while scanning or showing the last result.
    scan_component: Option<ScanComponent>,
}

/// Messages emitted by the application and its widgets.
#[derive(Debug, Clone)]
pub enum Message {
    ToggleContextPage(ContextPage),
    ToggleActivePageContext,
    UpdateConfig(Config),
    LaunchUrl(String),
    Page(PageMessage),
    PageAdded(PageSelector, &'static str),
    ActivatePage(PageSelector),
    ActivePageRemoved(PageSelector),
    SwitchLanguage(LanguageIdentifier),
    ExpireDocumentProvider,
    Scan,
    ScanComponent(ScanProgressMessage),
    KeyboardEvent(
        cosmic::iced::keyboard::Modifiers,
        cosmic::iced::keyboard::Key,
        Option<cosmic::iced::core::SmolStr>,
    ),
    ModifiersChanged(cosmic::iced::keyboard::Modifiers),
}

impl From<PageOutput> for Message {
    fn from(source: PageOutput) -> Self {
        match source {
            PageOutput::PageAdded(page, icon_name) => Message::PageAdded(page, icon_name),
            PageOutput::TogglePage(page_selector) => Message::ActivatePage(page_selector),
            PageOutput::PageRemoved(page) => Message::ActivePageRemoved(page),
        }
    }
}

impl From<ScanProgressMessage> for Message {
    fn from(msg: ScanProgressMessage) -> Self {
        Message::ScanComponent(msg)
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
impl cosmic::Application for ReadFlow {
    /// The async executor that will be used to run your application's commands.
    type Executor = cosmic::executor::Default;

    /// Data that your application receives to its init method.
    type Flags = (Arc<ApplicationModule>, Vec<PathBuf>);

    /// Messages which the application and its widgets will emit.
    type Message = Message;

    /// Unique identifier in RDNN (reverse domain name notation) format.
    const APP_ID: &'static str = "com.github.peterpaul.read-flow";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    /// Initializes the application with any given flags and startup commands.
    fn init(
        core: cosmic::Core,
        (application_module, initial_files): Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {
        // Create a nav bar with three page items.
        let mut nav = nav_bar::Model::default();
        let mut nav_mappings = HashMap::new();

        let config = cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
            .map(|context| match Config::get_entry(&context) {
                Ok(config) => config,
                Err((_errors, config)) => config,
            })
            .unwrap_or_default();

        let (pages, page_action) = Pages::new(application_module.clone(), config.clone());

        nav.insert()
            .text(pages.display_name(&PageSelector::Documents))
            .data::<PageSelector>(PageSelector::Documents)
            .icon(icon::from_name("emblem-documents-symbolic"))
            .with_id(|nav_id| {
                nav_mappings.insert(PageSelector::Documents, nav_id);
            })
            .activate();

        nav.insert()
            .text(pages.display_name(&PageSelector::Sources))
            .data::<PageSelector>(PageSelector::Sources)
            .icon(icon::from_name("network-server-symbolic"))
            .with_id(|nav_id| {
                nav_mappings.insert(PageSelector::Sources, nav_id);
            });

        nav.insert()
            .text(pages.display_name(&PageSelector::AppSettings))
            .data::<PageSelector>(PageSelector::AppSettings)
            .icon(icon::from_name("preferences-desktop-symbolic"))
            .with_id(|nav_id| {
                nav_mappings.insert(PageSelector::AppSettings, nav_id);
            });

        nav.insert()
            .text(pages.display_name(&PageSelector::Settings))
            .data::<PageSelector>(PageSelector::Settings)
            .icon(icon::from_name("preferences-system-symbolic"))
            .with_id(|nav_id| {
                nav_mappings.insert(PageSelector::Settings, nav_id);
            });

        // Create the about widget
        let about = About::default()
            .name(fl!("app-title"))
            .icon(widget::icon::from_svg_bytes(APP_ICON))
            .version(env!("CARGO_PKG_VERSION"))
            .links([(fl!("repository"), REPOSITORY)])
            .license(env!("CARGO_PKG_LICENSE"));

        // Construct the app model with the runtime's core.
        let mut app = ReadFlow {
            core,
            context_page: ContextPage::default(),
            about,
            nav,
            nav_mappings,
            key_binds: HashMap::new(),
            config,
            application_module,
            pages,
            scan_component: None,
        };

        // Create a startup command that sets the window title.
        let command = app.update_title();

        // Emit OpenDocument for each file passed on the command line.
        let open_tasks: Vec<_> = initial_files
            .iter()
            .filter_map(|path| Document::from_local_path(path))
            .map(|doc| {
                cosmic::task::message(cosmic::action::app(Message::Page(
                    PageMessage::OpenDocument(doc),
                )))
            })
            .collect();

        (
            app,
            cosmic::task::batch(
                [command, page_action.map(ActionExt::map_into)]
                    .into_iter()
                    .chain(open_tasks)
                    .collect::<Vec<_>>(),
            ),
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
                menu::root(fl!("actions")).apply(Element::from),
                menu::items(
                    &self.key_binds,
                    vec![menu::Item::Button(fl!("scan"), None, MenuAction::Scan)],
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

    /// Elements to pack at the center of the header bar.
    fn header_center(&self) -> Vec<Element<'_, Self::Message>> {
        if let Some(page) = self.nav.data::<PageSelector>(self.nav.active()) {
            self.pages
                .view_header_center(page)
                .into_iter()
                .map(|e| e.map(Into::into))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Elements to pack at the end of the header bar.
    fn header_end(&self) -> Vec<Element<'_, Self::Message>> {
        let mut elements = if let Some(page) = self.nav.data::<PageSelector>(self.nav.active()) {
            self.pages
                .view_header_end(page)
                .into_iter()
                .map(|e| e.map(Into::into))
                .collect()
        } else {
            Vec::new()
        };

        elements.push(
            widget::button::icon(widget::icon::from_name("open-menu-symbolic").size(16))
                .on_press(Message::ToggleActivePageContext)
                .tooltip(fl!("context"))
                .into(),
        );

        elements
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

    fn dialog(&self) -> Option<Element<'_, Self::Message>> {
        let page = self.nav.data::<PageSelector>(self.nav.active())?;
        self.pages.dialog(page).map(|e| e.map(Into::into))
    }

    /// Describes the interface based on the current state of the application model.
    ///
    /// Application events will be processed through the view. Any messages emitted by
    /// events received by widgets will be passed to the update method.
    fn view(&self) -> Element<'_, Self::Message> {
        if let Some(page) = self.nav.data::<PageSelector>(self.nav.active()) {
            self.pages.view(page).map(Into::into)
        } else {
            widget::Column::new()
                .push(widget::icon::from_svg_bytes(APP_ICON).icon().size(256))
                .push(widget::text::title1(fl!("welcome")))
                .align_x(Horizontal::Center)
                .apply(full_page)
        }
    }

    fn footer(&self) -> Option<Element<'_, Self::Message>> {
        Some(
            self.scan_component
                .as_ref()?
                .view()
                .map(Message::ScanComponent),
        )
    }

    /// Register subscriptions for this application.
    ///
    /// Subscriptions are long-running async tasks running in the background which
    /// emit messages to the application through a channel. They are started at the
    /// beginning of the application, and persist through its lifetime.
    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::batch(vec![
            // Subscribe to document provider cache invalidation events
            self.pages
                .document_provider
                .invalidation_subscription(|| Message::Page(PageMessage::Refresh)),
            settings_invalidation_subscription(self.application_module.clone(), || {
                Message::ExpireDocumentProvider
            }),
            // Watch for application configuration changes.
            self.core()
                .watch_config::<Config>(Self::APP_ID)
                .map(|update| {
                    for why in update.errors {
                        tracing::error!(?why, "app config error");
                    }

                    Message::UpdateConfig(update.config)
                }),
            // Forward keyboard events to the active page
            event::listen_with(filter_keyboard_events),
        ])
    }

    /// Handles messages emitted by the application and its widgets.
    ///
    /// Tasks may be returned for asynchronous execution of code in the background
    /// on the application's async runtime.
    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        tracing::debug!("received: {message:?}");
        match message {
            Message::ToggleContextPage(context_page) => {
                if self.context_page == context_page {
                    // Toggle context drawer if the toggled context page is the same.
                    self.core.window.show_context = !self.core.window.show_context;
                } else {
                    // Set the context drawer to display the requested context page.
                    self.context_page = context_page.clone();
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
                self.pages.update_app_config(&config);
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
            Message::ActivatePage(selector) => {
                if let Some(id) = self.nav_mappings.get(&selector).cloned() {
                    self.nav.activate(id);
                }
                Task::none()
            }
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
            Message::ExpireDocumentProvider => {
                let document_provider = self.pages.document_provider.clone();
                task::future(async move {
                    document_provider.set_expired().await;
                    Message::Page(PageMessage::Refresh)
                })
            }
            Message::Scan => {
                self.scan_component = Some(ScanComponent::new());

                let application_module = self.application_module.clone();
                let stream = futures::stream::once(async move {
                    let settings = application_module.settings().await;
                    let scan_dirs: Vec<_> = settings
                        .scan
                        .directories
                        .iter()
                        .filter_map(|(path, settings)| match settings {
                            DirectorySettings::Scan { .. } => Some(path.clone()),
                            DirectorySettings::Ignore { .. } => None,
                        })
                        .collect();
                    let mut receivers = vec![];
                    for dir in scan_dirs {
                        match application_module.start_scan(&dir).await {
                            Ok(rx) => receivers.push(rx),
                            Err(e) => {
                                tracing::error!("error starting scan of `{dir}`: {e}");
                            }
                        }
                    }
                    receivers
                })
                .flat_map(|receivers| {
                    receivers
                        .into_iter()
                        .fold(futures::stream::empty().boxed(), |acc, rx| {
                            acc.chain(futures::stream::unfold(rx, |mut rx| async move {
                                rx.recv().await.map(|item| (item, rx))
                            }))
                            .boxed()
                        })
                });

                task::stream(
                    stream.map(|e| Message::ScanComponent(ScanProgressMessage::Progress(e))),
                )
                .chain(task::message(Message::ScanComponent(
                    ScanProgressMessage::Completed,
                )))
            }
            Message::ScanComponent(msg) => {
                if let ScanProgressMessage::Out(output) = msg {
                    match output {
                        ScanProgressOutput::Dismissed => {
                            self.scan_component = None;
                            Task::none()
                        }
                        ScanProgressOutput::Completed => {
                            let document_provider = self.pages.document_provider.clone();
                            task::future(async move {
                                document_provider.set_expired().await;
                                Message::Page(PageMessage::Refresh)
                            })
                        }
                    }
                } else if let Some(ref mut component) = self.scan_component {
                    component.update(msg).map(ActionExt::map_into)
                } else {
                    Task::none()
                }
            }
            Message::KeyboardEvent(modifiers, key, text) => {
                if let Some(page) = self.nav.data::<PageSelector>(self.nav.active()) {
                    self.pages
                        .update(PageMessage::KeyEvent(page.clone(), modifiers, key, text))
                        .map(ActionExt::map_into)
                } else {
                    Task::none()
                }
            }
            Message::ModifiersChanged(modifiers) => {
                if let Some(page) = self.nav.data::<PageSelector>(self.nav.active()) {
                    self.pages
                        .update(PageMessage::ModifiersChanged(page.clone(), modifiers))
                        .map(ActionExt::map_into)
                } else {
                    Task::none()
                }
            }
        }
    }

    /// Called when a nav item is selected.
    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<cosmic::Action<Self::Message>> {
        // Activate the page in the model.
        self.nav.activate(id);

        let mut tasks = vec![self.update_title()];

        if self.nav.data::<PageSelector>(id) == Some(&PageSelector::Sources) {
            tasks.push(task::message(cosmic::Action::App(Message::Page(
                PageMessage::Sources(SourcesMessage::RefreshStatuses),
            ))));
        }

        Task::batch(tasks)
    }
}

impl ReadFlow {
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
    Scan,
    SwitchTo(&'static str),
}

impl menu::action::MenuAction for MenuAction {
    type Message = Message;

    fn message(&self) -> Self::Message {
        match self {
            MenuAction::About => Message::ToggleContextPage(ContextPage::About),
            MenuAction::Context => Message::ToggleActivePageContext,
            MenuAction::Scan => Message::Scan,
            MenuAction::SwitchTo(language) => Message::SwitchLanguage(language.parse().unwrap()),
        }
    }
}

fn filter_keyboard_events(
    event: cosmic::iced::Event,
    status: event::Status,
    _window_id: cosmic::iced::window::Id,
) -> Option<Message> {
    match event {
        Event::Keyboard(KeyEvent::KeyPressed {
            key,
            modifiers,
            text,
            ..
        }) => match status {
            event::Status::Ignored => Some(Message::KeyboardEvent(modifiers, key, text)),
            event::Status::Captured => None,
        },
        Event::Keyboard(KeyEvent::ModifiersChanged(modifiers)) => {
            Some(Message::ModifiersChanged(modifiers))
        }
        _ => None,
    }
}
