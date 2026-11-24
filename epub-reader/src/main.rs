use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use cosmic::Action;
use cosmic::Application;
use cosmic::ApplicationExt;
use cosmic::Element;
use cosmic::Task;
use cosmic::app::Core;
use cosmic::cosmic_config::Config;
use cosmic::cosmic_config::ConfigGet;
use cosmic::cosmic_config::ConfigSet;
use cosmic::iced::ContentFit;
use cosmic::iced::Event;
use cosmic::iced::Length;
use cosmic::iced::Subscription;
use cosmic::iced::event;
use cosmic::iced::keyboard::Event as KeyEvent;
use cosmic::iced::keyboard::Key;
use cosmic::iced::keyboard::key::Named;
use cosmic::widget;
use cosmic_epub::page_view::DualPageMode;
use cosmic_epub::page_view::PageViewMessage;
use cosmic_epub::pagination::BlockCut;
use cosmic_epub::reader::LoadedBook;
use cosmic_epub::reader::Position;
use cosmic_epub::reader::Reader;
use cosmic_epub::render::BookImage;
use cosmic_epub::render::RenderMessage;
use sha2::Digest;
use sha2::Sha256;

#[derive(Clone, Debug)]
enum Message {
    OpenFile,
    FileSelected(Option<PathBuf>),
    Loaded(PathBuf, Result<LoadedBook, String>),
    Reader(PageViewMessage),
    TurnPage(bool),
    TextSize(f32),
    Spread(DualPageMode),
    ToggleContents,
    SelectContents(usize),
    CloseImage,
}

struct App {
    core: Core,
    reader: Option<Reader>,
    path: Option<PathBuf>,
    opening: bool,
    picking_file: bool,
    show_contents: bool,
    image: Option<BookImage>,
    error: Option<String>,
}

impl App {
    fn open(&mut self, path: PathBuf) -> Task<Action<Message>> {
        self.opening = true;
        self.error = None;
        let load_path = path.clone();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    let path = load_path.canonicalize().unwrap_or(load_path);
                    let result = LoadedBook::open(&path).map_err(|error| error.to_string());
                    (path, result)
                })
                .await
            },
            move |result| {
                let (path, result) =
                    result.unwrap_or_else(|error| (path.clone(), Err(error.to_string())));
                Action::App(Message::Loaded(path, result))
            },
        )
    }

    fn save_position(&self) {
        let Some((path, position)) = self
            .path
            .as_deref()
            .zip(self.reader.as_ref().and_then(Reader::position))
        else {
            return;
        };
        if let Err(error) = save_position(path, position) {
            eprintln!("Could not save reading position: {error}");
        }
    }

    fn report_error(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
    }
}

/// @feature: reading.standalone_epub
impl Application for App {
    type Executor = cosmic::executor::Default;
    type Flags = Option<PathBuf>;
    type Message = Message;

    const APP_ID: &'static str = "io.github.read_flow.CosmicEpubReader";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, path: Self::Flags) -> (Self, Task<Action<Message>>) {
        let mut app = Self {
            core,
            reader: None,
            path: None,
            opening: false,
            picking_file: false,
            show_contents: false,
            image: None,
            error: None,
        };
        let task = path.map_or_else(Task::none, |path| app.open(path));
        (app, task)
    }

    fn header_start(&self) -> Vec<Element<'_, Message>> {
        let mut controls = vec![
            widget::button::standard("Open File…")
                .on_press_maybe((!self.opening && !self.picking_file).then_some(Message::OpenFile))
                .into(),
        ];
        if self.reader.is_some() {
            controls.push(
                widget::button::standard("Contents")
                    .on_press(Message::ToggleContents)
                    .into(),
            );
        }
        controls
    }

    fn header_center(&self) -> Vec<Element<'_, Message>> {
        let title = self.reader.as_ref().map_or("EPUB Reader", Reader::title);
        vec![
            widget::text::heading(title)
                .wrapping(cosmic::iced::widget::text::Wrapping::None)
                .into(),
        ]
    }

    fn header_end(&self) -> Vec<Element<'_, Message>> {
        let Some(reader) = self.reader.as_ref() else {
            return Vec::new();
        };
        let size = reader.font_size();
        let spread = reader.spread();
        vec![
            widget::button::standard("−")
                .tooltip("Decrease text size")
                .on_press_maybe((size > 12.0).then_some(Message::TextSize(size - 1.0)))
                .into(),
            widget::button::standard("+")
                .tooltip("Increase text size")
                .on_press_maybe((size < 24.0).then_some(Message::TextSize(size + 1.0)))
                .into(),
            widget::dropdown(
                &["Single", "Two pages", "Automatic"],
                Some(match spread {
                    DualPageMode::Off => 0,
                    DualPageMode::On => 1,
                    DualPageMode::Auto => 2,
                }),
                |index| {
                    Message::Spread(
                        [DualPageMode::Off, DualPageMode::On, DualPageMode::Auto][index],
                    )
                },
            )
            .into(),
        ]
    }

    fn subscription(&self) -> Subscription<Message> {
        event::listen_with(|event, status, _| {
            if status == event::Status::Captured {
                return None;
            }
            let Event::Keyboard(KeyEvent::KeyPressed { key, modifiers, .. }) = event else {
                return None;
            };
            if modifiers.control()
                && matches!(&key, Key::Character(key) if key.eq_ignore_ascii_case("o"))
            {
                return Some(Message::OpenFile);
            }
            match key {
                Key::Named(Named::ArrowLeft | Named::PageUp) => Some(Message::TurnPage(false)),
                Key::Named(Named::ArrowRight | Named::PageDown) => Some(Message::TurnPage(true)),
                Key::Named(Named::Escape) => Some(Message::CloseImage),
                _ => None,
            }
        })
    }

    fn view(&self) -> Element<'_, Message> {
        if let Some(image) = &self.image {
            let preview: Element<'_, Message> = match image {
                BookImage::Raster { handle, .. } => widget::image(handle.clone())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .content_fit(ContentFit::ScaleDown)
                    .into(),
                BookImage::Svg(handle) => widget::svg(handle.clone())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .content_fit(ContentFit::ScaleDown)
                    .into(),
            };
            return widget::column::with_children(vec![
                widget::button::standard("Back to book")
                    .on_press(Message::CloseImage)
                    .into(),
                preview,
            ])
            .spacing(8)
            .padding(8)
            .into();
        }

        let mut column = widget::column::with_capacity(2)
            .width(Length::Fill)
            .height(Length::Fill);
        if let Some(error) = &self.error {
            column = column.push(widget::container(widget::text::body(error)).padding(12));
        }
        if let Some(reader) = &self.reader {
            let content = reader.view().map(Message::Reader);
            if !self.show_contents {
                return column.push(content).into();
            }
            let entries = reader
                .toc()
                .iter()
                .enumerate()
                .fold(
                    widget::column::with_capacity(reader.toc().len()),
                    |column, (index, entry)| {
                        column.push(
                            widget::button::standard(entry.label.as_str())
                                .on_press(Message::SelectContents(index)),
                        )
                    },
                )
                .spacing(4);
            return column
                .push(widget::row::with_children(vec![
                    widget::scrollable(entries)
                        .width(Length::Fixed(220.0))
                        .height(Length::Fill)
                        .into(),
                    content,
                ]))
                .into();
        }
        if self.opening {
            column = column.push(widget::text::body("Opening EPUB…"));
        } else {
            column =
                column.push(widget::button::suggested("Open EPUB…").on_press(Message::OpenFile));
        }
        widget::container(column.padding(16)).into()
    }

    fn update(&mut self, message: Message) -> Task<Action<Message>> {
        match message {
            Message::OpenFile if !self.opening && !self.picking_file => {
                self.picking_file = true;
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .add_filter("EPUB", &["epub"])
                            .pick_file()
                            .await
                            .map(|file| file.path().into())
                    },
                    |path| Action::App(Message::FileSelected(path)),
                );
            }
            Message::FileSelected(Some(path)) if self.reader.is_some() => {
                self.picking_file = false;
                if let Err(error) = spawn_reader(&path) {
                    self.report_error(format!("Could not open a new window: {error}"));
                }
            }
            Message::FileSelected(Some(path)) => {
                self.picking_file = false;
                return self.open(path);
            }
            Message::Loaded(path, result) => {
                self.opening = false;
                match result {
                    Ok(book) => {
                        let reader = Reader::from_loaded(&path, book, load_position(&path));
                        let title = reader.title().to_owned();
                        self.reader = Some(reader);
                        self.path = Some(path);
                        if let Some(id) = self.core.main_window_id() {
                            return self.set_window_title(title, id);
                        }
                    }
                    Err(error) => self.report_error(format!("Could not open EPUB: {error}")),
                }
            }
            Message::Reader(PageViewMessage::NextPage) | Message::TurnPage(true) => {
                self.turn_page(true)
            }
            Message::Reader(PageViewMessage::PreviousPage) | Message::TurnPage(false) => {
                self.turn_page(false)
            }
            Message::TextSize(size) => {
                if let Some(reader) = &mut self.reader {
                    reader.set_font_size(size);
                }
            }
            Message::Spread(spread) => {
                if let Some(reader) = &mut self.reader {
                    reader.set_spread(spread);
                }
            }
            Message::ToggleContents => self.show_contents = !self.show_contents,
            Message::SelectContents(index) => {
                if let Some(reader) = &mut self.reader {
                    reader.select_nav(index);
                }
                self.save_position();
            }
            Message::Reader(PageViewMessage::Block(RenderMessage::FollowLink(href))) => {
                if let Some(reader) = &mut self.reader {
                    reader.follow_link(&href);
                }
                self.save_position();
            }
            Message::Reader(PageViewMessage::Block(RenderMessage::CopyCodeBlock(text))) => {
                return cosmic::iced::clipboard::write(text);
            }
            Message::Reader(PageViewMessage::Block(RenderMessage::OpenImageViewer(image))) => {
                self.image = Some(image);
            }
            Message::CloseImage => self.image = None,
            Message::FileSelected(None) => self.picking_file = false,
            Message::OpenFile => {}
        }
        Task::none()
    }
}

impl App {
    fn turn_page(&mut self, forward: bool) {
        if self.image.is_none()
            && let Some(reader) = &mut self.reader
        {
            reader.turn_page(forward);
            self.save_position();
        }
    }
}

fn position_key(path: &Path) -> String {
    use std::fmt::Write as _;
    Sha256::digest(path.as_os_str().as_encoded_bytes())
        .iter()
        .fold(String::from("book_"), |mut key, byte| {
            write!(key, "{byte:02x}").expect("write to string");
            key
        })
}

fn load_position(path: &Path) -> Option<Position> {
    read_position(&Config::new_state(App::APP_ID, 1).ok()?, path)
}

fn read_position(config: &Config, path: &Path) -> Option<Position> {
    let (chapter_href, cfi, block, child, char_offset): (String, String, usize, usize, usize) =
        config.get(&position_key(path)).ok()?;
    Some(Position {
        chapter_href,
        cfi,
        block,
        cut: BlockCut { child, char_offset },
    })
}

fn save_position(path: &Path, position: Position) -> Result<(), cosmic::cosmic_config::Error> {
    write_position(&Config::new_state(App::APP_ID, 1)?, path, position)
}

fn write_position(
    config: &Config,
    path: &Path,
    position: Position,
) -> Result<(), cosmic::cosmic_config::Error> {
    config.set(
        &position_key(path),
        (
            position.chapter_href,
            position.cfi,
            position.block,
            position.cut.child,
            position.cut.char_offset,
        ),
    )
}

fn spawn_reader(path: &Path) -> std::io::Result<()> {
    let mut child = Command::new(std::env::current_exe()?).arg(path).spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut paths = std::env::args_os().skip(1).map(PathBuf::from);
    let path = paths.next();
    for additional in paths {
        spawn_reader(&additional)?;
    }
    cosmic::app::run::<App>(cosmic::app::Settings::default(), path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn book_positions_survive_reopen_and_are_stored_separately() {
        let state = tempfile::tempdir().expect("temporary reading state");
        let config = || {
            Config::with_custom_path(App::APP_ID, 1, state.path().into())
                .expect("isolated test configuration")
        };
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../features/fixtures/sample.epub");
        let other = Path::new("other.epub");
        let initial = Reader::open(&path, None)
            .expect("open fixture")
            .position()
            .unwrap();
        let saved = Position {
            cfi: "epubcfi(/6/2!/4/4:0)".into(),
            block: 1,
            cut: BlockCut {
                child: 0,
                char_offset: 12,
            },
            ..initial.clone()
        };
        write_position(&config(), other, initial).expect("save other book");
        write_position(&config(), &path, saved.clone()).expect("save fixture book");

        let restored = Reader::open(&path, read_position(&config(), &path))
            .expect("reopen fixture")
            .position();
        assert_eq!(restored, Some(saved.clone()));
        assert_ne!(read_position(&config(), other), Some(saved));
    }
}
