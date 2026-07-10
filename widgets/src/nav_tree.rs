// SPDX-License-Identifier: AGPL-3.0-or-later

use cosmic::Apply;
use cosmic::Element;
use cosmic::iced::Alignment;
use cosmic::iced::Background;
use cosmic::iced::Length;
use cosmic::widget;
use cosmic::widget::button;
use cosmic::widget::menu;
use cosmic::widget::space;

const INDENT_WIDTH: f32 = 16.0;
const ICON_SIZE: f32 = 16.0;
const CHEVRON_ICON_SIZE: u16 = 10;

pub struct NavLeaf<Message> {
    pub icon: Option<cosmic::widget::icon::Icon>,
    pub label: String,
    pub active: bool,
    pub on_activate: Message,
}

pub struct NavNode<Message> {
    pub icon: Option<cosmic::widget::icon::Icon>,
    pub label: String,
    pub active: bool,
    pub collapsed: bool,
    pub on_activate: Message,
    pub on_toggle: Message,
    pub children: Vec<NavItem<Message>>,
    /// If `Some`, a "Expand All" entry appears in the right-click context menu.
    pub on_expand_all: Option<Message>,
    /// If `Some`, a "Collapse All" entry appears in the right-click context menu.
    pub on_collapse_all: Option<Message>,
}

pub enum NavItem<Message> {
    Leaf(NavLeaf<Message>),
    Node(NavNode<Message>),
}

impl<Message: Clone> NavItem<Message> {
    pub fn map<N: Clone, F: Fn(Message) -> N>(self, f: &F) -> NavItem<N> {
        match self {
            NavItem::Leaf(leaf) => NavItem::Leaf(NavLeaf {
                icon: leaf.icon,
                label: leaf.label,
                active: leaf.active,
                on_activate: f(leaf.on_activate),
            }),
            NavItem::Node(node) => NavItem::Node(NavNode {
                icon: node.icon,
                label: node.label,
                active: node.active,
                collapsed: node.collapsed,
                on_activate: f(node.on_activate),
                on_toggle: f(node.on_toggle),
                children: node.children.into_iter().map(|c| c.map(f)).collect(),
                on_expand_all: node.on_expand_all.map(f),
                on_collapse_all: node.on_collapse_all.map(f),
            }),
        }
    }
}

#[derive(Default)]
pub struct NavTree<Message> {
    items: Vec<NavItem<Message>>,
}

impl<Message: Clone + 'static> NavTree<Message> {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn push(mut self, item: NavItem<Message>) -> Self {
        self.items.push(item);
        self
    }

    pub fn map<N: Clone + 'static, F: Fn(Message) -> N>(self, f: F) -> NavTree<N> {
        NavTree {
            items: self.items.into_iter().map(|item| item.map(&f)).collect(),
        }
    }

    /// Consume the tree and produce a nav-bar sidebar element.
    ///
    /// Returns `Element<'static, Message>` because all content is cloned/owned
    /// internally, so the element carries no borrows from the tree.  The
    /// `'static` bound is coercible to any shorter lifetime at the call site.
    pub fn view(self) -> Element<'static, Message> {
        let theme = cosmic::theme::active();
        let cosmic = theme.cosmic();
        let space_xxs = cosmic.space_xxs();
        let space_s = cosmic.space_s();

        let items: Vec<Element<'static, Message>> = self
            .items
            .into_iter()
            .flat_map(|item| render_item(item, 0, space_s, space_xxs))
            .collect();

        widget::Column::with_children(items)
            .spacing(space_xxs)
            .apply(widget::container)
            .padding(space_xxs)
            .apply(widget::scrollable)
            .class(cosmic::style::iced::Scrollable::Minimal)
            .height(Length::Fill)
            .apply(widget::container)
            .height(Length::Fill)
            .class(cosmic::theme::Container::custom(
                cosmic::widget::nav_bar::nav_bar_style,
            ))
            .into()
    }
}

fn render_item<Message: Clone + 'static>(
    item: NavItem<Message>,
    depth: u16,
    space_s: u16,
    space_xxs: u16,
) -> Vec<Element<'static, Message>> {
    match item {
        NavItem::Leaf(leaf) => vec![render_leaf(leaf, depth, space_s)],
        NavItem::Node(node) => render_node(node, depth, space_s, space_xxs),
    }
}

fn render_leaf<Message: Clone + 'static>(
    leaf: NavLeaf<Message>,
    depth: u16,
    space_s: u16,
) -> Element<'static, Message> {
    let mut row = widget::Row::new()
        .spacing(space_s)
        .align_y(Alignment::Center);

    if depth > 0 {
        row = row.push(widget::Space::new().width(Length::Fixed(f32::from(depth) * INDENT_WIDTH)));
    }

    match leaf.icon {
        Some(icon) => row = row.push(icon),
        None => {
            row = row.push(widget::Space::new().width(Length::Fixed(ICON_SIZE)));
        }
    }

    row = row.push(widget::text(leaf.label));

    button::custom(row)
        .class(nav_button_class(leaf.active))
        .width(Length::Fill)
        .on_press(leaf.on_activate)
        .into()
}

fn render_node<Message: Clone + 'static>(
    node: NavNode<Message>,
    depth: u16,
    space_s: u16,
    space_xxs: u16,
) -> Vec<Element<'static, Message>> {
    let chevron_name = if node.collapsed {
        "go-next-symbolic"
    } else {
        "go-down-symbolic"
    };

    let mut body_row = widget::Row::new()
        .spacing(space_s)
        .align_y(Alignment::Center);

    if depth > 0 {
        body_row = body_row
            .push(widget::Space::new().width(Length::Fixed(f32::from(depth) * INDENT_WIDTH)));
    }

    match node.icon {
        Some(icon) => body_row = body_row.push(icon),
        None => {
            body_row = body_row.push(widget::Space::new().width(Length::Fixed(ICON_SIZE)));
        }
    }

    body_row = body_row.push(widget::text(node.label));

    let body_btn = button::custom(body_row)
        .class(nav_button_class(node.active))
        .width(Length::Fill)
        .on_press(node.on_activate);

    let chevron_btn = button::icon(widget::icon::from_name(chevron_name).size(CHEVRON_ICON_SIZE))
        .on_press(node.on_toggle);

    let row = widget::Row::new()
        .push(body_btn)
        .push(chevron_btn)
        .align_y(Alignment::Center);

    // Build a right-click context menu when expand/collapse-all messages are provided.
    let context_items = build_context_items(&node.on_expand_all, &node.on_collapse_all);
    let header: Element<'static, Message> = if context_items.is_some() {
        cosmic::widget::context_menu(row, context_items).into()
    } else {
        row.into()
    };

    let mut result = vec![header];

    if !node.collapsed {
        for child in node.children {
            result.extend(render_item(child, depth + 1, space_s, space_xxs));
        }
    }

    result
}

fn build_context_items<Message: Clone + 'static>(
    on_expand_all: &Option<Message>,
    on_collapse_all: &Option<Message>,
) -> Option<Vec<menu::Tree<Message>>> {
    if on_expand_all.is_none() && on_collapse_all.is_none() {
        return None;
    }

    let mut items: Vec<menu::Tree<Message>> = Vec::new();

    if let Some(msg) = on_expand_all {
        let btn: Element<'static, Message> = menu::menu_button(vec![
            widget::text("Expand All").into(),
            space::horizontal().into(),
        ])
        .on_press(msg.clone())
        .into();
        items.push(menu::Tree::from(btn));
    }

    if let Some(msg) = on_collapse_all {
        let btn: Element<'static, Message> = menu::menu_button(vec![
            widget::text("Collapse All").into(),
            space::horizontal().into(),
        ])
        .on_press(msg.clone())
        .into();
        items.push(menu::Tree::from(btn));
    }

    Some(items)
}

fn nav_button_class(selected: bool) -> cosmic::theme::Button {
    cosmic::theme::Button::Custom {
        active: Box::new(move |_focused, theme| nav_item_style(selected, false, theme)),
        hovered: Box::new(move |_focused, theme| nav_item_style(selected, true, theme)),
        pressed: Box::new(move |_focused, theme| nav_item_style(selected, true, theme)),
        disabled: Box::new(move |theme| nav_item_style(false, false, theme)),
    }
}

fn nav_item_style(
    selected: bool,
    hovered: bool,
    theme: &cosmic::Theme,
) -> cosmic::widget::button::Style {
    let cosmic = theme.cosmic();
    let component = &theme.current_container().component;

    let mut style = cosmic::widget::button::Style::new();
    style.border_radius = cosmic.corner_radii.radius_s.into();

    if selected || hovered {
        style.background = Some(Background::Color(component.hover.into()));
    }

    if selected {
        style.text_color = Some(cosmic.accent_text_color().into());
        style.icon_color = Some(cosmic.accent_text_color().into());
    } else {
        style.text_color = Some(component.on.into());
        style.icon_color = Some(component.on.into());
    }

    style
}
