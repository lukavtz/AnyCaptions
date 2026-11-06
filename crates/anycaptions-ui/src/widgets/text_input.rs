use std::{ops::Range, rc::Rc};

use unicode_segmentation::UnicodeSegmentation;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, FocusHandle, GlobalElementId, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, Render, ShapedLine,
    SharedString, Style, TextRun, UTF16Selection, UnderlineStyle, Window, div, fill, hsla, point,
    prelude::*, px, relative, rgb, rgba, size,
};

use super::{
    Backspace, Copy, Cut, Delete, End, Home, KEY_CONTEXT, Left, Paste, Right, SelectAll,
    SelectLeft, SelectRight,
};

type ChangeHandler = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;

macro_rules! input_replacement {
    (plain $name:ident) => {
        fn $name(
            &mut self,
            range_utf16: Option<Range<usize>>,
            new_text: &str,
            window: &mut Window,
            cx: &mut Context<Self>,
        ) {
            self.replace_range(range_utf16, new_text, None, false, window, cx);
        }
    };
    (marked $name:ident) => {
        fn $name(
            &mut self,
            range_utf16: Option<Range<usize>>,
            new_text: &str,
            new_selected_range_utf16: Option<Range<usize>>,
            window: &mut Window,
            cx: &mut Context<Self>,
        ) {
            self.replace_range(
                range_utf16,
                new_text,
                new_selected_range_utf16,
                true,
                window,
                cx,
            );
        }
    };
}

pub struct TextInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    max_length: Option<usize>,
    invalid: bool,
    disabled: bool,
    on_change: Option<ChangeHandler>,
}

impl TextInput {
    pub fn new(placeholder: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            content: "".into(),
            placeholder: placeholder.into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
            max_length: None,
            invalid: false,
            disabled: false,
            on_change: None,
        }
    }

    pub fn with_content(mut self, content: impl Into<SharedString>) -> Self {
        let content = content.into();
        self.content = match self.max_length {
            Some(max_length) => Self::truncate_utf16(&content, max_length).into(),
            None => content,
        };
        self.selected_range = self.content.len()..self.content.len();
        self
    }

    pub fn with_max_length(mut self, max_length: usize) -> Self {
        self.max_length = Some(max_length);
        self.content = Self::truncate_utf16(&self.content, max_length).into();
        let len = self.content.len();
        self.selected_range = self.selected_range.start.min(len)..self.selected_range.end.min(len);
        if let Some(range) = &mut self.marked_range {
            range.start = range.start.min(len);
            range.end = range.end.min(len);
        }
        self
    }

    widget_builders!(flag invalid);
    widget_builders!(disabled);
    widget_builders!(change on_change, SharedString);

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn selected_range(&self) -> Range<usize> {
        self.selected_range.clone()
    }

    pub fn set_content(&mut self, content: &str, cx: &mut Context<Self>) {
        let content = self.truncate_for_range(0..self.content.len(), content);
        self.content = content.into();
        self.selected_range = self.content.len()..self.content.len();
        self.selection_reversed = false;
        self.marked_range = None;
        cx.notify();
    }

    fn horizontal_boundary(&self, forward: bool) -> usize {
        if forward {
            self.next_boundary(self.cursor_offset())
        } else {
            self.previous_boundary(self.cursor_offset())
        }
    }

    fn move_horizontal(&mut self, forward: bool, cx: &mut Context<Self>) {
        let offset = if self.selected_range.is_empty() {
            self.horizontal_boundary(forward)
        } else if forward {
            self.selected_range.end
        } else {
            self.selected_range.start
        };
        self.move_to(offset, cx);
    }

    fn select_horizontal(&mut self, forward: bool, cx: &mut Context<Self>) {
        self.select_to(self.horizontal_boundary(forward), cx);
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        self.move_horizontal(false, cx);
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        self.move_horizontal(true, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_horizontal(false, cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_horizontal(true, cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.selection_reversed = false;
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn erase(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.horizontal_boundary(forward), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        self.erase(false, window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        self.erase(true, window, cx);
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let text = text.replace("\r\n", " ").replace(['\r', '\n'], " ");
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn copy_selection_to_clipboard(&self, cx: &App) -> bool {
        if self.selected_range.is_empty() {
            return false;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(
            self.content[self.selected_range.clone()].to_string(),
        ));
        true
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        self.copy_selection_to_clipboard(cx);
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if self.copy_selection_to_clipboard(cx) {
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.is_selecting = true;
        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        line.closest_index_for_x(position.x - bounds.left())
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify();
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(self.content.len())
    }

    fn truncate_utf16(text: &str, max_length: usize) -> String {
        let mut used = 0;
        let mut truncated = String::new();
        for character in text.chars() {
            if used + character.len_utf16() > max_length {
                break;
            }
            used += character.len_utf16();
            truncated.push(character);
        }
        truncated
    }

    fn truncate_for_range(&self, range: Range<usize>, new_text: &str) -> String {
        let Some(max_length) = self.max_length else {
            return new_text.to_owned();
        };
        let retained = self.content[..range.start].encode_utf16().count()
            + self.content[range.end..].encode_utf16().count();
        Self::truncate_utf16(new_text, max_length.saturating_sub(retained))
    }

    fn replace_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        mark: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range| Self::range_from_utf16(&self.content, range))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        let new_text = self.truncate_for_range(range.clone(), new_text);
        let cursor = range.start + new_text.len();

        let mut content = String::with_capacity(
            range.start + new_text.len() + self.content.len().saturating_sub(range.end),
        );
        content.push_str(&self.content[..range.start]);
        content.push_str(&new_text);
        content.push_str(&self.content[range.end..]);
        self.content = content.into();

        self.marked_range = (mark && !new_text.is_empty()).then_some(range.start..cursor);
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|selected| {
                let start = Self::offset_from_utf16(&new_text, selected.start);
                let end = Self::offset_from_utf16(&new_text, selected.end);
                (range.start + start)..(range.start + end)
            })
            .unwrap_or(cursor..cursor);
        self.selection_reversed = false;
        cx.notify();
        if let Some(handler) = self.on_change.clone() {
            handler(self.content.clone(), window, cx);
        }
    }

    fn offset_from_utf16(text: &str, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for character in text.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += character.len_utf16();
            utf8_offset += character.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(text: &str, offset: usize) -> usize {
        text[..offset].encode_utf16().count()
    }

    fn range_to_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
        Self::offset_to_utf16(text, range.start)..Self::offset_to_utf16(text, range.end)
    }

    fn range_from_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
        Self::offset_from_utf16(text, range.start)..Self::offset_from_utf16(text, range.end)
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = Self::range_from_utf16(&self.content, &range_utf16);
        if range.start > range.end || range.end > self.content.len() {
            return None;
        }
        actual_range.replace(Self::range_to_utf16(&self.content, &range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        if self.disabled && !ignore_disabled_input {
            return None;
        }
        Some(UTF16Selection {
            range: Self::range_to_utf16(&self.content, &self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| Self::range_to_utf16(&self.content, range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked_range = None;
    }

    input_replacement!(plain replace_text_in_range);
    input_replacement!(marked replace_and_mark_text_in_range);

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.last_layout.as_ref()?;
        let range = Self::range_from_utf16(&self.content, &range_utf16);
        Some(Bounds::from_corners(
            point(
                bounds.left() + last_layout.x_for_index(range.start),
                bounds.top(),
            ),
            point(
                bounds.left() + last_layout.x_for_index(range.end),
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        if self.content.is_empty() {
            return Some(0);
        }
        let line_point = self.last_bounds?.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;
        let utf8_index = last_layout.index_for_x(point.x - line_point.x)?;
        Some(Self::offset_to_utf16(&self.content, utf8_index))
    }
}

struct TextElement {
    input: Entity<TextInput>,
}

struct PrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let style = window.text_style();
        let (display_text, text_color) = if content.is_empty() {
            (input.placeholder.clone(), hsla(0., 0., 0.7, 0.4))
        } else {
            (content, style.color)
        };
        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if let Some(marked_range) = input.marked_range.as_ref() {
            vec![
                TextRun {
                    len: marked_range.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked_range.end - marked_range.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - marked_range.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![run]
        };
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);
        let cursor_pos = line.x_for_index(cursor);
        let (selection, cursor) = if selected_range.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_pos, bounds.top()),
                        size(px(2.), bounds.bottom() - bounds.top()),
                    ),
                    rgb(0x60a5fa),
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() + line.x_for_index(selected_range.start),
                            bounds.top(),
                        ),
                        point(
                            bounds.left() + line.x_for_index(selected_range.end),
                            bounds.bottom(),
                        ),
                    ),
                    rgba(0x2563eb66),
                )),
                None,
            )
        };
        PrepaintState {
            line: Some(line),
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        let line = prepaint.line.take().unwrap();
        line.paint(bounds.origin, window.line_height(), window, cx)
            .unwrap();
        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }
        self.input.update(cx, |input, _| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

impl_focus_handle!(TextInput);

impl Render for TextInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let border = if self.invalid {
            rgb(0xf87171)
        } else {
            rgb(0x4b5563)
        };
        let focus_border = if self.invalid {
            rgb(0xf87171)
        } else {
            rgb(0x93c5fd)
        };
        let mut element = div()
            .id("text-input")
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .tab_index(0)
            .tab_stop(!self.disabled)
            .cursor(CursorStyle::IBeam)
            .w_full()
            .p_1()
            .rounded_md()
            .border_1()
            .border_color(border)
            .bg(rgb(0x111827))
            .text_color(rgb(0xf9fafb))
            .in_focus(move |style| style.border_color(focus_border))
            .child(TextElement { input: cx.entity() });

        if !self.disabled {
            element = element
                .on_action(cx.listener(Self::backspace))
                .on_action(cx.listener(Self::delete))
                .on_action(cx.listener(Self::left))
                .on_action(cx.listener(Self::right))
                .on_action(cx.listener(Self::select_left))
                .on_action(cx.listener(Self::select_right))
                .on_action(cx.listener(Self::select_all))
                .on_action(cx.listener(Self::home))
                .on_action(cx.listener(Self::end))
                .on_action(cx.listener(Self::paste))
                .on_action(cx.listener(Self::cut))
                .on_action(cx.listener(Self::copy))
                .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
                .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
                .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
                .on_mouse_move(cx.listener(Self::on_mouse_move));
        }
        element
    }
}
