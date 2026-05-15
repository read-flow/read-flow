// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use cosmic::app::context_drawer;
use cosmic::cosmic_config;
use cosmic::cosmic_config::CosmicConfigEntry;
use cosmic::iced::Subscription;
use cosmic::iced::event;
use cosmic::iced::event::Event;
use cosmic::iced::keyboard::Event as KeyEvent;
use cosmic::iced::mouse;
use cosmic::prelude::*;
use cosmic::task;
use cosmic::widget;
use cosmic::widget::about::About;
use cosmic::widget::icon;
use cosmic::widget::menu;
use futures::StreamExt;
use i18n_embed::unic_langid::LanguageIdentifier;
use provider::r#async::HasSetExpired;
use read_flow_core::scan::DirectorySettings;
use read_flow_widgets::NavItem;
use read_flow_widgets::NavLeaf;
use read_flow_widgets::NavTree;

use crate::ApplicationModule;
use crate::ICON_SIZE;
use crate::aggregator::Document;
use crate::component::check_missing::CheckMissingComponent;
use crate::component::check_missing::CheckMissingMessage;
use crate::component::check_missing::CheckMissingOutput;
use crate::component::scan_progress::ScanComponent;
use crate::component::scan_progress::ScanProgressMessage;
use crate::component::scan_progress::ScanProgressOutput;
use crate::config::Config;
use crate::cosmic_ext::ActionExt;
use crate::fl;
use crate::page::DocumentListMessage;
use crate::page::PageMessage;
use crate::page::PageOutput;
use crate::page::PageSelector;
use crate::page::Pages;
use crate::page::SourcesMessage;
use crate::page::settings_invalidation_subscription;

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
pub(crate) const APP_ICON: &[u8] =
    include_bytes!("../resources/icons/hicolor/scalable/apps/icon.svg");

/// The application model stores app-specific state used to describe its interface and
/// drive its logic.
pub struct ReadFlow {
    /// Application state which is managed by the COSMIC runtime.
    core: cosmic::Core,
    /// Display a context drawer with the designated page if defined.
    context_page: ContextPage,
    /// The about page for this app.
    about: About,
    /// Whether the nav sidebar is currently visible.
    nav_bar_visible: bool,
    /// Current width of the nav sidebar in pixels.
    nav_bar_width: f32,
    /// True while the user is dragging the sidebar resize handle.
    nav_bar_resizing: bool,
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
    /// Check-missing component, present while the dialog is open.
    check_missing_component: Option<CheckMissingComponent>,
}

/// Messages emitted by the application and its widgets.
#[derive(Debug, Clone)]
pub enum Message {
    ToggleContextPage(ContextPage),
    ToggleActivePageContext,
    ToggleNavBar,
    NavBarResizeStart,
    NavBarDrag(f32),
    NavBarResizeEnd,
    UpdateConfig(Config),
    LaunchUrl(String),
    Page(PageMessage),
    PageAdded(PageSelector),
    ActivatePage(PageSelector),
    ActivePageRemoved(PageSelector),
    SwitchLanguage(LanguageIdentifier),
    ExpireDocumentProvider,
    Scan,
    CheckMissing,
    CheckMissingComponent(CheckMissingMessage),
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
            PageOutput::PageAdded(page) => Message::PageAdded(page),
            PageOutput::TogglePage(page_selector) => Message::ActivatePage(page_selector),
            PageOutput::PageRemoved(page) => Message::ActivePageRemoved(page),
            PageOutput::Scan => Message::Scan,
        }
    }
}

impl From<ScanProgressMessage> for Message {
    fn from(msg: ScanProgressMessage) -> Self {
        Message::ScanComponent(msg)
    }
}

impl From<CheckMissingMessage> for Message {
    fn from(msg: CheckMissingMessage) -> Self {
        Message::CheckMissingComponent(msg)
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
        let config = cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
            .map(|context| match Config::get_entry(&context) {
                Ok(config) => config,
                Err((_errors, config)) => config,
            })
            .unwrap_or_default();

        let (mut pages, page_action) = Pages::new(application_module.clone(), config.clone());

        let label = pages.display_name(&PageSelector::Documents);
        pages.register_page(
            PageSelector::Documents,
            "emblem-documents-symbolic",
            label,
            None,
        );
        let label = pages.display_name(&PageSelector::Sources);
        pages.register_page(
            PageSelector::Sources,
            "network-server-symbolic",
            label,
            None,
        );
        let label = pages.display_name(&PageSelector::OnlineLibrary);
        pages.register_page(
            PageSelector::OnlineLibrary,
            "system-search-symbolic",
            label,
            None,
        );
        let label = pages.display_name(&PageSelector::AppSettings);
        pages.register_page(
            PageSelector::AppSettings,
            "preferences-desktop-symbolic",
            label,
            None,
        );
        let label = pages.display_name(&PageSelector::Settings);
        pages.register_page(
            PageSelector::Settings,
            "preferences-system-symbolic",
            label,
            None,
        );

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
            nav_bar_visible: true,
            nav_bar_width: 280.0,
            nav_bar_resizing: false,
            key_binds: HashMap::new(),
            config,
            application_module,
            pages,
            scan_component: None,
            check_missing_component: None,
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
        let navbar_icon = if self.nav_bar_visible {
            "navbar-open-symbolic"
        } else {
            "navbar-closed-symbolic"
        };

        let mut elements = vec![
            widget::button::icon(widget::icon::from_name(navbar_icon).size(ICON_SIZE))
                .on_press(Message::ToggleNavBar)
                .into(),
        ];

        elements.extend(
            self.pages
                .view_header_start(self.pages.active_page())
                .into_iter()
                .map(|e| e.map(Into::into)),
        );

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
                    vec![
                        menu::Item::Button(fl!("scan"), None, MenuAction::Scan),
                        menu::Item::Button(fl!("check-missing"), None, MenuAction::CheckMissing),
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

        elements.push(menu_bar.into());
        elements
    }

    /// Elements to pack at the center of the header bar.
    fn header_center(&self) -> Vec<Element<'_, Self::Message>> {
        self.pages
            .view_header_center(self.pages.active_page())
            .into_iter()
            .map(|e| e.map(Into::into))
            .collect()
    }

    /// Elements to pack at the end of the header bar.
    fn header_end(&self) -> Vec<Element<'_, Self::Message>> {
        let mut elements: Vec<Element<'_, Self::Message>> = self
            .pages
            .view_header_end(self.pages.active_page())
            .into_iter()
            .map(|e| e.map(Into::into))
            .collect();

        elements.push(
            widget::button::icon(widget::icon::from_name("open-menu-symbolic").size(ICON_SIZE))
                .on_press(Message::ToggleActivePageContext)
                .tooltip(fl!("view-options"))
                .into(),
        );

        elements
    }

    fn nav_bar(&self) -> Option<cosmic::Element<'_, cosmic::Action<Self::Message>>> {
        if !self.nav_bar_visible {
            return None;
        }

        let tree = self.build_nav_tree().view();

        if self.core().is_condensed() {
            return Some(
                tree.apply(widget::container)
                    .width(cosmic::iced::Length::Shrink)
                    .height(cosmic::iced::Length::Fill)
                    .into(),
            );
        }

        let sidebar = tree
            .apply(widget::container)
            .width(cosmic::iced::Length::Fixed(self.nav_bar_width))
            .height(cosmic::iced::Length::Fill);

        let handle = widget::mouse_area(
            widget::Space::new()
                .width(cosmic::iced::Length::Fixed(4.0))
                .height(cosmic::iced::Length::Fill),
        )
        .on_press(cosmic::action::app(Message::NavBarResizeStart))
        .on_release(cosmic::action::app(Message::NavBarResizeEnd))
        .interaction(mouse::Interaction::ResizingHorizontally);

        Some(
            widget::Row::new()
                .push(sidebar)
                .push(handle)
                .height(cosmic::iced::Length::Fill)
                .into(),
        )
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
        if let Some(ref component) = self.check_missing_component {
            return component
                .dialog()
                .map(|e| e.map(Message::CheckMissingComponent));
        }
        self.pages
            .dialog(self.pages.active_page())
            .map(|e| e.map(Into::into))
    }

    /// Describes the interface based on the current state of the application model.
    ///
    /// Application events will be processed through the view. Any messages emitted by
    /// events received by widgets will be passed to the update method.
    fn view(&self) -> Element<'_, Self::Message> {
        let active = self.pages.active_page();
        let page = if self.pages.page_list().contains_key(active) {
            active
        } else {
            &PageSelector::Documents
        };
        self.pages.view(page).map(Into::into)
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
        let mut subs = vec![
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
        ];

        // Track cursor and mouse-up globally while the sidebar resize is active.
        if self.nav_bar_resizing {
            subs.push(event::listen_with(|event, _, _| match event {
                Event::Mouse(mouse::Event::CursorMoved { position }) => {
                    Some(Message::NavBarDrag(position.x))
                }
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    Some(Message::NavBarResizeEnd)
                }
                _ => None,
            }));
        }

        Subscription::batch(subs)
    }

    /// Handles messages emitted by the application and its widgets.
    ///
    /// Tasks may be returned for asynchronous execution of code in the background
    /// on the application's async runtime.
    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        tracing::debug!("received: {message:?}");
        match message {
            Message::ToggleNavBar => {
                self.nav_bar_visible = !self.nav_bar_visible;
                Task::none()
            }
            Message::NavBarResizeStart => {
                self.nav_bar_resizing = true;
                Task::none()
            }
            Message::NavBarDrag(x) => {
                if self.nav_bar_resizing {
                    self.nav_bar_width = x.clamp(140.0, 480.0);
                }
                Task::none()
            }
            Message::NavBarResizeEnd => {
                self.nav_bar_resizing = false;
                Task::none()
            }
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
            Message::ToggleActivePageContext => {
                let selector = self.pages.active_page().clone();
                task::message(Message::ToggleContextPage(ContextPage::PageContext(
                    selector,
                )))
            }
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
            // PageOutput messages may arrive directly from UI elements (e.g. nav tree
            // on_activate closures) wrapped in Message::Page rather than through a task.
            // Route them through the same App::update path as task-produced outputs.
            Message::Page(PageMessage::Out(output)) => self.update(output.into()),
            Message::Page(page_message) => self.pages.update(page_message).map(ActionExt::map_into),
            Message::ActivatePage(selector) => {
                self.pages.activate(selector.clone());
                let mut tasks = vec![self.update_title()];
                match &selector {
                    PageSelector::Sources => {
                        tasks.push(task::message(cosmic::Action::App(Message::Page(
                            PageMessage::Sources(SourcesMessage::RefreshStatuses),
                        ))));
                    }
                    PageSelector::Documents => {
                        tasks.push(task::message(cosmic::Action::App(Message::Page(
                            PageMessage::Documents(DocumentListMessage::FocusSearchInput),
                        ))));
                    }
                    _ => {}
                }
                Task::batch(tasks)
            }
            Message::PageAdded(_selector) => self.update_title(),
            Message::ActivePageRemoved(_removed_page) => {
                self.core.window.show_context = false;
                self.update_title()
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
            Message::CheckMissing => {
                let (component, init_task) =
                    CheckMissingComponent::new(self.application_module.clone());
                self.check_missing_component = Some(component);
                init_task.map(ActionExt::map_into)
            }
            Message::CheckMissingComponent(msg) => {
                if let CheckMissingMessage::Out(output) = msg {
                    match output {
                        CheckMissingOutput::Dismissed => {
                            self.check_missing_component = None;
                            Task::none()
                        }
                        CheckMissingOutput::Purged => {
                            self.check_missing_component = None;
                            let document_provider = self.pages.document_provider.clone();
                            task::future(async move {
                                document_provider.set_expired().await;
                                Message::Page(PageMessage::Refresh)
                            })
                        }
                    }
                } else if let Some(ref mut component) = self.check_missing_component {
                    component.update(msg).map(ActionExt::map_into)
                } else {
                    Task::none()
                }
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
                    (application_module, scan_dirs)
                })
                .flat_map(|(application_module, scan_dirs)| {
                    futures::stream::iter(scan_dirs)
                        .then(move |dir| {
                            let application_module = application_module.clone();
                            async move {
                                match application_module.start_scan(&dir).await {
                                    Ok(rx) => futures::stream::unfold(rx, |mut rx| async move {
                                        rx.recv().await.map(|item| (item, rx))
                                    })
                                    .boxed(),
                                    Err(e) => {
                                        tracing::error!("error starting scan of `{dir}`: {e}");
                                        futures::stream::empty().boxed()
                                    }
                                }
                            }
                        })
                        .flatten()
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
                let page = self.pages.active_page().clone();
                self.pages
                    .update(PageMessage::KeyEvent(page, modifiers, key, text))
                    .map(ActionExt::map_into)
            }
            Message::ModifiersChanged(modifiers) => {
                let page = self.pages.active_page().clone();
                self.pages
                    .update(PageMessage::ModifiersChanged(page, modifiers))
                    .map(ActionExt::map_into)
            }
        }
    }
}

impl ReadFlow {
    fn build_nav_tree(&self) -> NavTree<cosmic::Action<Message>> {
        let active = self.pages.active_page();
        let mut tree = NavTree::new();

        for (selector, info) in self.pages.page_list() {
            let is_active = selector == active;

            if let Some(item) = self.pages.nav_tree(selector, is_active) {
                // Page provides its own nav item (e.g. EPUB viewer with TOC).
                let to_action = |msg: PageMessage| cosmic::action::app(Message::Page(msg));
                tree = tree.push(item.map(&to_action));
            } else {
                // Default: simple leaf that navigates via ActivatePage.
                tree = tree.push(NavItem::Leaf(NavLeaf {
                    icon: Some(icon::from_name(info.icon_name).icon()),
                    label: info.label.clone(),
                    active: is_active,
                    on_activate: cosmic::action::app(Message::ActivatePage(selector.clone())),
                }));
            }
        }

        tree
    }

    /// Updates the header and window titles.
    pub fn update_title(&mut self) -> Task<cosmic::Action<Message>> {
        let mut window_title = fl!("app-title");
        window_title.push_str(" — ");
        window_title.push_str(&self.pages.display_name(self.pages.active_page()));

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
    CheckMissing,
    SwitchTo(&'static str),
}

impl menu::action::MenuAction for MenuAction {
    type Message = Message;

    fn message(&self) -> Self::Message {
        match self {
            MenuAction::About => Message::ToggleContextPage(ContextPage::About),
            MenuAction::Context => Message::ToggleActivePageContext,
            MenuAction::Scan => Message::Scan,
            MenuAction::CheckMissing => Message::CheckMissing,
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
            // Forward modifier+key shortcuts even when a widget (e.g. search input) captured
            // the event, so global shortcuts like Ctrl+M work regardless of focus.
            event::Status::Captured if !modifiers.is_empty() => {
                Some(Message::KeyboardEvent(modifiers, key, text))
            }
            event::Status::Captured => None,
        },
        Event::Keyboard(KeyEvent::ModifiersChanged(modifiers)) => {
            Some(Message::ModifiersChanged(modifiers))
        }
        _ => None,
    }
}
