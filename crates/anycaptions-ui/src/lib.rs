//! GPUI views, widgets, text measurement, and embedded assets for AnyCaptions.
//!
//! This crate contains no native global-service side effects.

pub mod assets;
pub mod caption;
pub mod dictation;
pub mod overlay;
pub mod settings;
pub mod text_measurement;
pub mod theme;
pub mod widgets;

use std::{cell::RefCell, rc::Rc};

use anycaptions_platform_win32::accessibility::{
    UiaAction, UiaMetadata, UiaProviderFactory, uia_action_error,
};
use anycaptions_platform_win32::accessibility_tree::UiaFragment;
use gpui::{AnyWindowHandle, AppContext as _, AsyncApp, Context, WeakEntity, Window};

pub(crate) fn dispatch_uia_action<V: 'static>(
    app: &Rc<RefCell<AsyncApp>>,
    window: AnyWindowHandle,
    weak: &WeakEntity<V>,
    apply: impl FnOnce(&mut V, &mut Window, &mut Context<V>) + 'static,
) -> gpui::Result<()> {
    let weak = weak.clone();
    let result = app
        .borrow_mut()
        .update_window(window, |_, window, cx| {
            weak.update(cx, |view, cx| apply(view, window, cx))
        })
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    result.map_err(|error| std::io::Error::other(error.to_string()))?;
    Ok(())
}

pub(crate) fn create_uia_action_node(
    root: &UiaFragment,
    metadata: UiaMetadata,
    app: AsyncApp,
    handler: impl Fn(UiaAction, &Rc<RefCell<AsyncApp>>) -> gpui::Result<()> + 'static,
) -> gpui::Result<UiaFragment> {
    let app = Rc::new(RefCell::new(app));
    let provider = UiaProviderFactory::create_with_handler(metadata, move |action| {
        handler(action, &app).map_err(|error| uia_action_error(error.to_string()))
    })?;
    Ok(root.add_child(provider)?)
}
