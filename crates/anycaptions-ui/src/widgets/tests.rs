use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gpui::{
    AppContext, ClipboardItem, Entity, EntityInputHandler, Focusable, KeyUpEvent, Keystroke,
    Modifiers, MouseButton, TestAppContext, point, px,
};

use super::{
    Button, FocusScope, NumberInput, Picker, PickerFooter, PickerOption, Progress, Slider,
    TextInput, Toggle, bind_keys,
};

fn test_options() -> Vec<PickerOption> {
    vec![
        PickerOption::new("alpha", "Alpha"),
        PickerOption::new("beta", "Beta").disabled(true),
        PickerOption::new("hidden", "Hidden").hidden(true),
        PickerOption::new("delta", "Delta"),
    ]
}

fn read<V: 'static, R>(
    view: &Entity<V>,
    cx: &gpui::VisualTestContext,
    read: impl FnOnce(&V) -> R,
) -> R {
    cx.read_entity(view, |view, _| read(view))
}

fn focus<V: Focusable>(view: &Entity<V>, cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        let handle = view.read(cx).focus_handle(cx);
        window.focus(&handle);
    });
}

fn focused_view<V: gpui::Render + Focusable>(
    cx: &mut TestAppContext,
    build: impl FnOnce(&mut gpui::Window, &mut gpui::Context<V>) -> V,
) -> (Entity<V>, &mut gpui::VisualTestContext) {
    cx.update(bind_keys);
    let (view, cx) = cx.add_window_view(build);
    focus(&view, cx);
    (view, cx)
}

fn input_with_max_length<'a>(
    cx: &'a mut TestAppContext,
    content: &'static str,
    max_length: usize,
) -> (Entity<TextInput>, &'a mut gpui::VisualTestContext) {
    focused_view(cx, move |_, cx| {
        TextInput::new("Caption shortcut", cx)
            .with_content(content)
            .with_max_length(max_length)
    })
}

fn assert_input_content(input: &Entity<TextInput>, cx: &gpui::VisualTestContext, expected: &str) {
    assert_eq!(
        read(input, cx, |input| input.content().to_owned()),
        expected
    );
}

fn click(cx: &mut gpui::VisualTestContext, position: gpui::Point<gpui::Pixels>) {
    cx.simulate_mouse_down(position, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::default());
}

fn press_key(cx: &mut gpui::VisualTestContext, key: &str) {
    cx.simulate_keystrokes(key);
    cx.simulate_event(KeyUpEvent {
        keystroke: Keystroke::parse(key).unwrap(),
    });
}

fn assert_focused<V: Focusable>(view: &Entity<V>, cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        assert!(view.read(cx).focus_handle(cx).is_focused(window));
    });
}

#[gpui::test]
fn button_confirm_activates_and_disabled_button_does_not(cx: &mut TestAppContext) {
    let presses = Rc::new(Cell::new(0));
    let presses_in_view = presses.clone();
    let (button, cx) = focused_view(cx, |_, cx| {
        Button::new("Save", cx).on_press(move |_, _| {
            presses_in_view.set(presses_in_view.get() + 1);
        })
    });

    press_key(cx, "enter");
    assert_eq!(presses.get(), 1);

    button.update(cx, |button, cx| button.set_disabled(true, cx));
    press_key(cx, "space");
    assert_eq!(presses.get(), 1);
}

#[gpui::test]
fn toggle_confirm_changes_checked_state(cx: &mut TestAppContext) {
    let changes = Rc::new(RefCell::new(Vec::<bool>::new()));
    let changes_in_view = changes.clone();
    let (toggle, cx) = focused_view(cx, |_, cx| {
        Toggle::new("Background", false, cx).on_change(move |checked, _, _| {
            changes_in_view.borrow_mut().push(checked);
        })
    });

    press_key(cx, "space");
    press_key(cx, "enter");
    assert!(!read(&toggle, cx, Toggle::checked));
    assert_eq!(changes.take(), vec![true, false]);
}

#[gpui::test]
fn text_input_ime_replaces_selection_and_commits(cx: &mut TestAppContext) {
    let (input, cx) = focused_view(cx, |_, cx| {
        TextInput::new("Caption shortcut", cx).with_content("ab")
    });

    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(Some(0..1), "に", Some(0..0), window, cx);
        });
    });
    assert_eq!(read(&input, cx, |input| input.content().to_owned()), "にb");
    let (marked, selected) = cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            (
                input.marked_text_range(window, cx),
                input
                    .selected_text_range(false, window, cx)
                    .map(|selection| selection.range),
            )
        })
    });
    assert_eq!(marked, Some(0..1));
    assert_eq!(selected, Some(0..0));

    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(None, "日", None, window, cx);
        });
    });
    assert_eq!(read(&input, cx, |input| input.content().to_owned()), "日b");

    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_text_in_range(None, "日", window, cx);
        });
    });
    assert_eq!(read(&input, cx, |input| input.content().to_owned()), "日b");
    let marked =
        cx.update(|window, cx| input.update(cx, |input, cx| input.marked_text_range(window, cx)));
    assert_eq!(marked, None);
}

#[gpui::test]
fn text_input_selection_home_and_end_use_grapheme_boundaries(cx: &mut TestAppContext) {
    let (input, cx) = focused_view(cx, |_, cx| {
        TextInput::new("Caption shortcut", cx).with_content("a🇧🇷b")
    });

    cx.simulate_keystrokes("end shift-left shift-left");
    assert_eq!(read(&input, cx, TextInput::selected_range), 1..10);

    cx.simulate_keystrokes("home");
    assert_eq!(read(&input, cx, TextInput::selected_range), 0..0);
    cx.simulate_keystrokes("shift-right");
    assert_eq!(read(&input, cx, TextInput::selected_range), 0..1);
    cx.simulate_keystrokes("end");
    assert_eq!(read(&input, cx, TextInput::selected_range), 10..10);
}

#[gpui::test]
fn text_input_clipboard_copy_cut_and_paste(cx: &mut TestAppContext) {
    let (input, cx) = focused_view(cx, |_, cx| TextInput::new("Caption shortcut", cx));
    cx.write_to_clipboard(ClipboardItem::new_string("a\nb".into()));

    cx.simulate_keystrokes("ctrl-v");
    assert_eq!(read(&input, cx, |input| input.content().to_owned()), "a b");
    cx.simulate_keystrokes("ctrl-a ctrl-c");
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("a b".into())
    );
    cx.simulate_keystrokes("ctrl-x");
    assert_eq!(read(&input, cx, |input| input.content().to_owned()), "");
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("a b".into())
    );
}

#[gpui::test]
fn text_input_max_length_counts_utf16_without_splitting_characters(cx: &mut TestAppContext) {
    let (input, cx) = input_with_max_length(cx, "a", 3);

    cx.simulate_keystrokes("end");
    cx.simulate_input("😀");
    assert_input_content(&input, cx, "a😀");
    cx.simulate_input("b");
    assert_input_content(&input, cx, "a😀");

    cx.write_to_clipboard(ClipboardItem::new_string("c".into()));
    cx.simulate_keystrokes("ctrl-v");
    assert_input_content(&input, cx, "a😀");
    cx.write_to_clipboard(ClipboardItem::new_string("abcd".into()));
    cx.simulate_keystrokes("ctrl-a ctrl-v");
    assert_input_content(&input, cx, "abc");
}

#[gpui::test]
fn text_input_space_is_inserted_on_full_key_cycle(cx: &mut TestAppContext) {
    let (input, cx) = focused_view(cx, |_, cx| TextInput::new("Caption shortcut", cx));

    press_key(cx, "space");

    assert_input_content(&input, cx, " ");
}

#[gpui::test]
fn text_input_builders_enforce_max_length_in_either_order(cx: &mut TestAppContext) {
    let (content_first, cx) = input_with_max_length(cx, "a😀b", 3);
    assert_input_content(&content_first, cx, "a😀");

    let (length_first, cx) = focused_view(cx, |_, cx| {
        TextInput::new("Caption shortcut", cx)
            .with_max_length(3usize)
            .with_content("a😀b")
    });
    assert_input_content(&length_first, cx, "a😀");
}

#[gpui::test]
fn picker_skips_disabled_and_hidden_options(cx: &mut TestAppContext) {
    let (picker, cx) = focused_view(cx, |window, cx| Picker::new(test_options(), window, cx));

    press_key(cx, "enter");
    assert!(read(&picker, cx, Picker::is_open));
    cx.simulate_keystrokes("down");
    press_key(cx, "enter");
    assert_eq!(
        read(&picker, cx, |picker| picker.value().to_owned()),
        "delta"
    );
    assert!(!read(&picker, cx, Picker::is_open));

    cx.simulate_keystrokes("down home end");
    press_key(cx, "enter");
    assert_eq!(
        read(&picker, cx, |picker| picker.value().to_owned()),
        "delta"
    );
    cx.simulate_keystrokes("down");
    assert!(read(&picker, cx, Picker::is_open));
    press_key(cx, "enter");
    assert_eq!(
        read(&picker, cx, |picker| picker.value().to_owned()),
        "alpha"
    );
}

#[gpui::test]
fn picker_mouse_selection_and_outside_close(cx: &mut TestAppContext) {
    let (picker, cx) = focused_view(cx, |window, cx| {
        Picker::new(test_options(), window, cx).with_selected("delta")
    });

    cx.simulate_keystrokes("down");
    let first_row = read(&picker, cx, |picker| {
        picker.scroll_handle.bounds_for_item(0)
    })
    .expect("first picker option painted");
    let first_option = point(first_row.left() + px(2.0), first_row.top() + px(2.0));
    click(cx, first_option);
    assert_eq!(
        read(&picker, cx, |picker| picker.value().to_owned()),
        "alpha"
    );
    assert!(!read(&picker, cx, Picker::is_open));

    cx.simulate_keystrokes("down");
    let outside = point(px(500.0), px(500.0));
    click(cx, outside);
    assert!(!read(&picker, cx, Picker::is_open));
    assert_eq!(
        read(&picker, cx, |picker| picker.value().to_owned()),
        "alpha"
    );
    assert_focused(&picker, cx);

    cx.simulate_keystrokes("down");
    assert!(read(&picker, cx, Picker::is_open));
    cx.simulate_keystrokes("escape");
    assert!(!read(&picker, cx, Picker::is_open));
    assert_focused(&picker, cx);
}

#[gpui::test]
fn picker_footer_keyboard_and_mouse_fire_once(cx: &mut TestAppContext) {
    let calls = Rc::new(Cell::new(0));
    let calls_in_footer = calls.clone();
    let (picker, cx) = focused_view(cx, |window, cx| {
        Picker::new(test_options(), window, cx)
            .footer(PickerFooter::new("Download more languages", move |_, _| {
                calls_in_footer.set(calls_in_footer.get() + 1)
            }))
    });

    press_key(cx, "enter");
    cx.update(|window, _| window.focus_next());
    press_key(cx, "space");
    assert_eq!(calls.get(), 1);
    assert!(!read(&picker, cx, Picker::is_open));
    assert_focused(&picker, cx);

    press_key(cx, "enter");
    let footer_row = read(&picker, cx, |picker| {
        picker.scroll_handle.bounds_for_item(3)
    })
    .expect("picker footer painted");
    click(
        cx,
        point(footer_row.left() + px(2.0), footer_row.top() + px(2.0)),
    );
    assert_eq!(calls.get(), 2);
    assert!(!read(&picker, cx, Picker::is_open));
}

#[gpui::test]
fn picker_escape_closes_without_closing_settings(cx: &mut TestAppContext) {
    cx.update(bind_keys);
    let escapes = Rc::new(Cell::new(0));
    let escapes_in_scope = escapes.clone();
    let mut picker_slot = None;
    let (_scope, cx) = cx.add_window_view(|window, cx| {
        let picker = cx.new(|cx| Picker::new(test_options(), window, cx));
        picker_slot = Some(picker.clone());
        FocusScope::new(picker, cx).on_escape(move |_, _| {
            escapes_in_scope.set(escapes_in_scope.get() + 1);
        })
    });
    let picker = picker_slot.unwrap();

    cx.simulate_keystrokes("down");
    cx.simulate_keystrokes("escape");
    assert!(!read(&picker, cx, Picker::is_open));
    assert_focused(&picker, cx);
    assert_eq!(escapes.get(), 0);

    cx.simulate_keystrokes("escape");
    assert_eq!(escapes.get(), 1);
}

#[gpui::test]
fn slider_snaps_and_clamps_keyboard_and_drag_values(cx: &mut TestAppContext) {
    let (slider, cx) = focused_view(cx, |_, cx| Slider::new(0.0, 10.0, 2.0, 5.0, cx));
    assert_eq!(read(&slider, cx, Slider::value), 6.0);

    cx.simulate_keystrokes("home right");
    assert_eq!(read(&slider, cx, Slider::value), 2.0);
    cx.simulate_keystrokes("end left");
    assert_eq!(read(&slider, cx, Slider::value), 8.0);

    let bounds = read(&slider, cx, |slider| slider.track_bounds).expect("slider track painted");
    cx.simulate_mouse_down(
        point(bounds.left(), bounds.top()),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        point(bounds.right(), bounds.top()),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        point(bounds.right(), bounds.top()),
        MouseButton::Left,
        Modifiers::default(),
    );
    assert_eq!(read(&slider, cx, Slider::value), 10.0);

    slider.update(cx, |slider, cx| {
        assert_eq!(slider.value_for_ratio(-1.0), 0.0);
        assert_eq!(slider.value_for_ratio(2.0), 10.0);
        slider.set_value(-100.0, cx);
    });
    assert_eq!(read(&slider, cx, Slider::value), 0.0);
    slider.update(cx, |slider, cx| slider.set_value(100.0, cx));
    assert_eq!(read(&slider, cx, Slider::value), 10.0);
}

#[gpui::test]
fn number_input_accepts_text_and_clamps_arrow_steps(cx: &mut TestAppContext) {
    let (number, cx) = focused_view(cx, |_, cx| NumberInput::new(0.0, 10.0, 1.0, 5.0, cx));

    cx.simulate_keystrokes("up");
    assert_eq!(read(&number, cx, NumberInput::value), 6.0);
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("12");
    assert_eq!(read(&number, cx, NumberInput::value), 10.0);
    cx.simulate_keystrokes("down");
    assert_eq!(read(&number, cx, NumberInput::value), 9.0);
    assert_eq!(read(&number, cx, |number| number.text().to_owned()), "9");
}

#[gpui::test]
fn focus_scope_tab_and_shift_tab_traverse_in_render_order(cx: &mut TestAppContext) {
    cx.update(bind_keys);
    let mut first_slot = None;
    let mut second_slot = None;
    let (_scope, cx) = cx.add_window_view(|_, cx| {
        let first = cx.new(|cx| Button::new("First", cx));
        let second = cx.new(|cx| Button::new("Second", cx));
        first_slot = Some(first.clone());
        second_slot = Some(second.clone());
        FocusScope::with_children(vec![first.into(), second.into()], cx)
    });
    let first = first_slot.unwrap();
    let second = second_slot.unwrap();
    focus(&first, cx);

    cx.simulate_keystrokes("tab");
    assert_focused(&second, cx);
    cx.simulate_keystrokes("shift-tab");
    assert_focused(&first, cx);
}

#[gpui::test]
fn progress_clamps_fraction(cx: &mut TestAppContext) {
    let (progress, cx) = cx.add_window_view(|_, _| Progress::new(1.5));
    assert_eq!(read(&progress, cx, Progress::fraction), 1.0);
    progress.update(cx, |progress, cx| progress.set_fraction(-0.5, cx));
    assert_eq!(read(&progress, cx, Progress::fraction), 0.0);
}
