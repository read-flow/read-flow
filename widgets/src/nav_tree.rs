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
    pub on_close: Option<Message>,
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
    pub on_close: Option<Message>,
}

pub enum NavItem<Message> {
    Leaf(NavLeaf<Message>),
    Node(NavNode<Message>),
    SectionLabel(String),
}

impl<Message: Clone> NavItem<Message> {
    pub fn map<N: Clone, F: Fn(Message) -> N>(self, f: &F) -> NavItem<N> {
        match self {
            NavItem::Leaf(leaf) => NavItem::Leaf(NavLeaf {
                icon: leaf.icon,
                label: leaf.label,
                active: leaf.active,
                on_activate: f(leaf.on_activate),
                on_close: leaf.on_close.map(f),
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
                on_close: node.on_close.map(f),
            }),
            NavItem::SectionLabel(text) => NavItem::SectionLabel(text),
        }
    }
}

#[derive(Default)]
pub struct NavTree<Message> {
    items: Vec<NavItem<Message>>,
    omit_icon_placeholders: bool,
}

impl<Message: Clone + 'static> NavTree<Message> {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            omit_icon_placeholders: false,
        }
    }

    pub fn push(mut self, item: NavItem<Message>) -> Self {
        self.items.push(item);
        self
    }

    /// Let entries without an icon start at their nesting indentation.
    pub fn without_icon_placeholders(mut self) -> Self {
        self.omit_icon_placeholders = true;
        self
    }

    pub fn map<N: Clone + 'static, F: Fn(Message) -> N>(self, f: F) -> NavTree<N> {
        NavTree {
            items: self.items.into_iter().map(|item| item.map(&f)).collect(),
            omit_icon_placeholders: self.omit_icon_placeholders,
        }
    }

    /// Consume the tree and produce a nav-bar sidebar element.
    ///
    /// Returns `Element<'static, Message>` because all content is cloned/owned
    /// internally, so the element carries no borrows from the tree.  The
    /// `'static` bound is coercible to any shorter lifetime at the call site.
    pub fn view(self) -> Element<'static, Message> {
        self.view_with_spacing(cosmic::theme::spacing().space_xxs, INDENT_WIDTH)
    }

    /// Render with custom row spacing and per-level indentation in logical pixels.
    pub fn view_with_spacing(
        self,
        row_spacing: u16,
        indent_width: f32,
    ) -> Element<'static, Message> {
        let theme = cosmic::theme::active();
        let cosmic = theme.cosmic();
        let space_xxs = cosmic.space_xxs();
        let space_s = cosmic.space_s();

        let items: Vec<Element<'static, Message>> = self
            .items
            .into_iter()
            .flat_map(|item| {
                render_item(item, 0, space_s, indent_width, self.omit_icon_placeholders)
            })
            .collect();

        widget::Column::with_children(items)
            .spacing(row_spacing)
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

/// Build navigation items from preorder (depth, label) pairs.
/// A depth jump nests under the preceding entry without inserting empty levels.
pub fn nav_items_from_depths<Message>(
    entries: &[(usize, &str)],
    collapsed: &std::collections::HashSet<usize>,
    active: Option<usize>,
    activate: fn(usize) -> Message,
    toggle: fn(usize) -> Message,
) -> Vec<NavItem<Message>> {
    fn build<Message>(
        entries: &[(usize, &str)],
        offset: usize,
        collapsed: &std::collections::HashSet<usize>,
        active: Option<usize>,
        activate: fn(usize) -> Message,
        toggle: fn(usize) -> Message,
    ) -> Vec<NavItem<Message>> {
        let mut result = Vec::new();
        let mut i = 0;
        while let Some(&(entry_depth, label)) = entries.get(i) {
            let next = i + 1;
            let end = next
                + entries[next..]
                    .iter()
                    .take_while(|(child_depth, _)| *child_depth > entry_depth)
                    .count();
            let index = offset + i;
            let children = build(
                &entries[next..end],
                offset + next,
                collapsed,
                active,
                activate,
                toggle,
            );
            let label = label.to_owned();
            let active = active == Some(index);
            let on_activate = activate(index);
            result.push(if children.is_empty() {
                NavItem::Leaf(NavLeaf {
                    icon: None,
                    label,
                    active,
                    on_activate,
                    on_close: None,
                })
            } else {
                NavItem::Node(NavNode {
                    icon: None,
                    label,
                    active,
                    collapsed: collapsed.contains(&index),
                    on_activate,
                    on_toggle: toggle(index),
                    children,
                    on_expand_all: None,
                    on_collapse_all: None,
                    on_close: None,
                })
            });
            i = end;
        }
        result
    }
    build(entries, 0, collapsed, active, activate, toggle)
}

fn render_item<Message: Clone + 'static>(
    item: NavItem<Message>,
    depth: u16,
    space_s: u16,
    indent_width: f32,
    omit_icon_placeholders: bool,
) -> Vec<Element<'static, Message>> {
    match item {
        NavItem::Leaf(leaf) => vec![render_leaf(
            leaf,
            depth,
            space_s,
            indent_width,
            omit_icon_placeholders,
        )],
        NavItem::Node(node) => {
            render_node(node, depth, space_s, indent_width, omit_icon_placeholders)
        }
        NavItem::SectionLabel(text) => vec![render_section_label(text)],
    }
}

fn render_leaf<Message: Clone + 'static>(
    leaf: NavLeaf<Message>,
    depth: u16,
    space_s: u16,
    indent_width: f32,
    omit_icon_placeholders: bool,
) -> Element<'static, Message> {
    let mut row = widget::Row::new()
        .spacing(space_s)
        .align_y(Alignment::Center);

    if depth > 0 {
        row = row.push(widget::Space::new().width(Length::Fixed(f32::from(depth) * indent_width)));
    }

    match leaf.icon {
        Some(icon) => row = row.push(icon),
        None if !omit_icon_placeholders => {
            row = row.push(widget::Space::new().width(Length::Fixed(ICON_SIZE)));
        }
        None => {}
    }

    row = row.push(widget::text(leaf.label));

    let body_btn = button::custom(row)
        .class(nav_button_class(leaf.active))
        .width(Length::Fill)
        .on_press(leaf.on_activate);

    match leaf.on_close {
        Some(on_close) => {
            let close_btn = button::icon(
                widget::icon::from_name("window-close-symbolic").size(CHEVRON_ICON_SIZE),
            )
            .on_press(on_close);
            widget::Row::new()
                .push(body_btn)
                .push(close_btn)
                .align_y(Alignment::Center)
                .into()
        }
        None => body_btn.into(),
    }
}

fn render_section_label<Message: 'static>(text: String) -> Element<'static, Message> {
    widget::text::caption(text)
        .apply(widget::container)
        .padding([12, 8, 4, 8])
        .into()
}

fn render_node<Message: Clone + 'static>(
    node: NavNode<Message>,
    depth: u16,
    space_s: u16,
    indent_width: f32,
    omit_icon_placeholders: bool,
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
            .push(widget::Space::new().width(Length::Fixed(f32::from(depth) * indent_width)));
    }

    match node.icon {
        Some(icon) => body_row = body_row.push(icon),
        None if !omit_icon_placeholders => {
            body_row = body_row.push(widget::Space::new().width(Length::Fixed(ICON_SIZE)));
        }
        None => {}
    }

    body_row = body_row.push(widget::text(node.label));

    let body_btn = button::custom(body_row)
        .class(nav_button_class(node.active))
        .width(Length::Fill)
        .on_press(node.on_activate);

    let chevron_btn = button::icon(widget::icon::from_name(chevron_name).size(CHEVRON_ICON_SIZE))
        .on_press(node.on_toggle);

    let mut row = widget::Row::new()
        .push(body_btn)
        .push(chevron_btn)
        .align_y(Alignment::Center);

    if let Some(on_close) = node.on_close {
        let close_btn =
            button::icon(widget::icon::from_name("window-close-symbolic").size(CHEVRON_ICON_SIZE))
                .on_press(on_close);
        row = row.push(close_btn);
    }

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
            result.extend(render_item(
                child,
                depth + 1,
                space_s,
                indent_width,
                omit_icon_placeholders,
            ));
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

#[cfg(test)]
mod tests {
    use cosmic_golden::golden_test;

    use super::*;

    #[golden_test(260, 90)]
    fn section_label_renders_as_caption() -> Element<'static, String> {
        NavTree::new()
            .push(NavItem::Leaf(NavLeaf {
                icon: None,
                label: "Dashboard".into(),
                active: true,
                on_activate: "activate-dashboard".to_string(),
                on_close: None,
            }))
            .push(NavItem::SectionLabel("Open".into()))
            .push(NavItem::Leaf(NavLeaf {
                icon: None,
                label: "Sample.pdf".into(),
                active: false,
                on_activate: "activate-pdf".to_string(),
                on_close: None,
            }))
            .view()
    }

    #[golden_test(260, 60)]
    fn leaf_with_close_shows_sibling_close_button() -> Element<'static, String> {
        NavTree::new()
            .push(NavItem::Leaf(NavLeaf {
                icon: None,
                label: "Sample.pdf".into(),
                active: false,
                on_activate: "activate".to_string(),
                on_close: Some("close".to_string()),
            }))
            .view()
    }

    #[golden_test(260, 60)]
    fn node_with_close_collapsed() -> Element<'static, String> {
        NavTree::new()
            .push(NavItem::Node(NavNode {
                icon: None,
                label: "Meditations".into(),
                active: true,
                collapsed: true,
                on_activate: "activate".to_string(),
                on_toggle: "toggle".to_string(),
                children: vec![],
                on_expand_all: None,
                on_collapse_all: None,
                on_close: Some("close".to_string()),
            }))
            .view()
    }

    #[golden_test(260, 130)]
    fn node_with_close_expanded_shows_children_and_keeps_close() -> Element<'static, String> {
        NavTree::new()
            .push(NavItem::Node(NavNode {
                icon: None,
                label: "Meditations".into(),
                active: true,
                collapsed: false,
                on_activate: "activate".to_string(),
                on_toggle: "toggle".to_string(),
                children: vec![NavItem::Leaf(NavLeaf {
                    icon: None,
                    label: "Book I".into(),
                    active: false,
                    on_activate: "activate-chapter".to_string(),
                    on_close: None,
                })],
                on_expand_all: None,
                on_collapse_all: None,
                on_close: Some("close".to_string()),
            }))
            .view()
    }
}

#[cfg(test)]
mod depth_tests {
    use super::*;

    #[test]
    fn nested_entries_keep_their_selection_and_toggle_indices() {
        let entries = [(0, "Part"), (1, "Chapter"), (2, "Section"), (0, "Appendix")];
        let items = nav_items_from_depths(
            &entries,
            &[0].into(),
            Some(2),
            |i| (true, i),
            |i| (false, i),
        );
        let [NavItem::Node(part), NavItem::Leaf(appendix)] = items.as_slice() else {
            panic!("expected a part and appendix");
        };
        assert!(part.collapsed);
        assert_eq!(part.on_toggle, (false, 0));
        let [NavItem::Node(chapter)] = part.children.as_slice() else {
            panic!("expected a nested chapter");
        };
        let [NavItem::Leaf(section)] = chapter.children.as_slice() else {
            panic!("expected a nested section");
        };
        assert!(section.active);
        assert_eq!(section.on_activate, (true, 2));
        assert_eq!(appendix.on_activate, (true, 3));
    }
}

#[cfg(test)]
#[test]
fn skipped_depth_levels_keep_entries_in_the_tree() {
    use NavItem::Leaf;
    use NavItem::Node;

    let entries = [(0, "Part"), (2, "Section"), (1, "Chapter"), (0, "Appendix")];
    let items = nav_items_from_depths(&entries, &Default::default(), Some(1), |i| i, |i| i);
    let [Node(part), Leaf(appendix)] = items.as_slice() else {
        panic!("expected a part and appendix");
    };
    let [Leaf(section), Leaf(chapter)] = part.children.as_slice() else {
        panic!("expected both entries under the part");
    };
    assert!(section.active);
    assert_eq!(
        (
            section.on_activate,
            chapter.on_activate,
            appendix.on_activate
        ),
        (1, 2, 3)
    );
}
