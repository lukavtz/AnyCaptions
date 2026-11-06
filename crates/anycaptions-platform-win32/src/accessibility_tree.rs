//! UIA fragment-tree composition and `WM_GETOBJECT` integration for a GPUI HWND.

use std::{
    cell::{Cell, RefCell},
    ptr::null_mut,
    rc::{Rc, Weak},
    thread::ThreadId,
};

use windows::{
    Win32::{
        Foundation::{E_FAIL, E_OUTOFMEMORY, HWND, LPARAM, LRESULT, RECT, WPARAM},
        System::{
            Com::SAFEARRAY,
            Ole::{SafeArrayCreateVector, SafeArrayDestroy, SafeArrayPutElement},
            Variant::{VARIANT, VT_I4},
        },
        UI::{
            Accessibility::*,
            Input::KeyboardAndMouse::SetFocus,
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::{GetWindowRect, OBJID_CLIENT, WM_GETOBJECT, WM_NCDESTROY},
        },
    },
    core::{ComObject, Error, HRESULT, IUnknown, Interface, Result, implement},
};

use crate::{
    WindowEffects,
    accessibility::{
        UiaMetadata, UiaProviderFactory, UiaProviderHandle, WidgetKind, announce_metadata,
        ensure_thread, widget_metadata as metadata,
    },
};

struct Node {
    provider: UiaProviderHandle,
    object: ComObject<Fragment>,
    parent: Option<i32>,
    children: Vec<i32>,
    bounds: UiaRect,
    focus_handler: Option<Rc<dyn Fn()>>,
}

struct SubclassState {
    tree: Weak<RefCell<TreeState>>,
    alive: Rc<Cell<bool>>,
}

struct Subclass {
    hwnd: HWND,
    id: usize,
    state: *mut SubclassState,
    alive: Rc<Cell<bool>>,
}

impl Drop for Subclass {
    fn drop(&mut self) {
        if !self.alive.replace(false) {
            return;
        }
        // SAFETY: this attachment owns the subclass reference data until it is removed.
        unsafe {
            let _ = RemoveWindowSubclass(self.hwnd, Some(window_proc), self.id);
            drop(Box::from_raw(self.state));
        }
    }
}

struct TreeState {
    owner: ThreadId,
    hwnd: HWND,
    nodes: Vec<Option<Node>>,
    focused: Option<i32>,
    subclass: Option<Subclass>,
}

impl TreeState {
    fn ensure_owner(&self) -> Result<()> {
        ensure_thread(self.owner)
    }

    fn node(&self, id: i32) -> Result<&Node> {
        self.nodes
            .get(id as usize)
            .and_then(Option::as_ref)
            .ok_or_else(element_unavailable)
    }

    fn fragment(&self, id: i32) -> Result<IRawElementProviderFragment> {
        Ok(self.node(id)?.object.to_interface())
    }

    fn simple(&self, id: i32) -> Result<IRawElementProviderSimple> {
        Ok(self.node(id)?.object.to_interface())
    }

    fn root_fragment(&self) -> Result<IRawElementProviderFragmentRoot> {
        Ok(self.node(0)?.object.to_interface())
    }
}

/// Owns the native subclass and every fragment attached to one GPUI window.
/// Create and use it on the window's UI thread.
pub struct UiaTree {
    state: Rc<RefCell<TreeState>>,
}

impl UiaTree {
    pub fn attach(effects: &WindowEffects, root: UiaProviderHandle) -> Result<Self> {
        Self::attach_hwnd(effects.hwnd(), root)
    }

    pub fn attach_hwnd(hwnd: HWND, root: UiaProviderHandle) -> Result<Self> {
        root.set_hwnd(hwnd)?;
        let state = Rc::new(RefCell::new(TreeState {
            owner: std::thread::current().id(),
            hwnd,
            nodes: Vec::new(),
            focused: None,
            subclass: None,
        }));
        let root_handle = UiaFragment {
            state: Rc::clone(&state),
            id: 0,
        };
        let object = ComObject::new(Fragment {
            tree: Rc::downgrade(&state),
            id: 0,
        });
        state.borrow_mut().nodes.push(Some(Node {
            provider: root,
            object,
            parent: None,
            children: Vec::new(),
            bounds: UiaRect::default(),
            focus_handler: None,
        }));

        let alive = Rc::new(Cell::new(true));
        let subclass_state = Box::new(SubclassState {
            tree: Rc::downgrade(&state),
            alive: Rc::clone(&alive),
        });
        let subclass_state = Box::into_raw(subclass_state);
        let id = subclass_state as usize;
        // SAFETY: the reference data remains allocated until this subclass is removed
        // or WM_NCDESTROY releases it. The id is its stable allocation address.
        if !unsafe { SetWindowSubclass(hwnd, Some(window_proc), id, subclass_state as usize) }
            .as_bool()
        {
            // SAFETY: installation failed, so the callback never took ownership.
            unsafe { drop(Box::from_raw(subclass_state)) };
            return Err(Error::from_thread());
        }
        state.borrow_mut().subclass = Some(Subclass {
            hwnd,
            id,
            state: subclass_state,
            alive,
        });
        debug_assert_eq!(root_handle.id, 0);
        Ok(Self { state })
    }

    pub fn root(&self) -> UiaFragment {
        UiaFragment {
            state: Rc::clone(&self.state),
            id: 0,
        }
    }
}

/// A stable handle to one node. Removing it from its parent invalidates the handle
/// for UIA while existing COM clients receive `UIA_E_ELEMENTNOTAVAILABLE`.
#[derive(Clone)]
pub struct UiaFragment {
    state: Rc<RefCell<TreeState>>,
    id: i32,
}

impl UiaFragment {
    pub fn metadata(&self) -> Result<UiaMetadata> {
        let state = self.state.try_borrow().map_err(|_| state_error())?;
        state.ensure_owner()?;
        state.node(self.id)?.provider.metadata()
    }

    fn raw_interface<T>(&self, get: impl FnOnce(&TreeState, i32) -> Result<T>) -> Result<T> {
        let state = self.state.try_borrow().map_err(|_| state_error())?;
        state.ensure_owner()?;
        get(&state, self.id)
    }

    pub fn raw_provider(&self) -> Result<IRawElementProviderSimple> {
        self.raw_interface(TreeState::simple)
    }

    pub fn raw_fragment(&self) -> Result<IRawElementProviderFragment> {
        self.raw_interface(TreeState::fragment)
    }

    pub fn set_metadata(&self, metadata: UiaMetadata) -> Result<()> {
        let previous_disabled = self.metadata()?.disabled;
        let disabled = metadata.disabled;
        {
            let state = self.state.try_borrow().map_err(|_| state_error())?;
            state.ensure_owner()?;
            state.node(self.id)?.provider.set_metadata(metadata)?;
        }
        if previous_disabled != disabled {
            let provider = self.raw_provider()?;
            let previous_value = VARIANT::from(previous_disabled);
            let new_value = VARIANT::from(disabled);
            // SAFETY: the fragment provider and variants outlive the synchronous event call.
            unsafe {
                UiaRaiseAutomationPropertyChangedEvent(
                    &provider,
                    UIA_IsEnabledPropertyId,
                    &previous_value,
                    &new_value,
                )
            }?;
        }
        Ok(())
    }

    pub fn set_bounds(&self, bounds: UiaRect) -> Result<()> {
        let mut state = self.state.try_borrow_mut().map_err(|_| state_error())?;
        state.ensure_owner()?;
        let node = state
            .nodes
            .get_mut(self.id as usize)
            .and_then(Option::as_mut)
            .ok_or_else(element_unavailable)?;
        node.bounds = bounds;
        Ok(())
    }

    pub fn set_focus_handler(&self, handler: impl Fn() + 'static) -> Result<()> {
        let mut state = self.state.try_borrow_mut().map_err(|_| state_error())?;
        state.ensure_owner()?;
        state
            .nodes
            .get_mut(self.id as usize)
            .and_then(Option::as_mut)
            .ok_or_else(element_unavailable)?
            .focus_handler = Some(Rc::new(handler));
        Ok(())
    }

    pub fn set_focused(&self, focused: bool) -> Result<()> {
        let changed = self.focus_changed(focused)?;
        let provider = self.focus_provider(focused, changed)?;
        self.store_focus(focused)?;
        if let Some(provider) = provider {
            Self::raise_focus_changed(&provider)?;
        }
        Ok(())
    }

    fn focus_changed(&self, focused: bool) -> Result<bool> {
        let state = self.state.try_borrow().map_err(|_| state_error())?;
        state.ensure_owner()?;
        state.node(self.id)?;
        Ok(if focused {
            state.focused != Some(self.id)
        } else {
            state.focused == Some(self.id)
        })
    }

    fn focus_provider(
        &self,
        focused: bool,
        changed: bool,
    ) -> Result<Option<IRawElementProviderSimple>> {
        (focused && changed)
            .then(|| self.raw_provider())
            .transpose()
    }

    fn store_focus(&self, focused: bool) -> Result<()> {
        let mut state = self.state.try_borrow_mut().map_err(|_| state_error())?;
        state.ensure_owner()?;
        if focused {
            state.focused = Some(self.id);
        } else if state.focused == Some(self.id) {
            state.focused = None;
        }
        Ok(())
    }

    fn raise_focus_changed(provider: &IRawElementProviderSimple) -> Result<()> {
        // SAFETY: the provider remains alive for the duration of the event call.
        unsafe { UiaRaiseAutomationEvent(provider, UIA_AutomationFocusChangedEventId) }
    }

    pub fn add_child(&self, provider: UiaProviderHandle) -> Result<UiaFragment> {
        let child = self.insert_child(provider)?;
        self.configure_child_container(&child)?;
        raise_structure_changed(self, StructureChangeType_ChildAdded, child.id)?;
        Ok(child)
    }

    fn insert_child(&self, provider: UiaProviderHandle) -> Result<UiaFragment> {
        let mut state = self.state.try_borrow_mut().map_err(|_| state_error())?;
        state.ensure_owner()?;
        state.node(self.id)?;
        let id = i32::try_from(state.nodes.len()).map_err(|_| state_error())?;
        provider.set_hwnd(state.hwnd)?;
        let child = UiaFragment {
            state: Rc::clone(&self.state),
            id,
        };
        let object = ComObject::new(Fragment {
            tree: Rc::downgrade(&self.state),
            id,
        });
        state.nodes.push(Some(Node {
            provider,
            object,
            parent: Some(self.id),
            children: Vec::new(),
            bounds: UiaRect::default(),
            focus_handler: None,
        }));
        state
            .nodes
            .get_mut(self.id as usize)
            .and_then(Option::as_mut)
            .ok_or_else(element_unavailable)?
            .children
            .push(id);
        Ok(child)
    }

    fn configure_child_container(&self, child: &UiaFragment) -> Result<()> {
        if self.metadata()?.kind == WidgetKind::List {
            let container = self.raw_provider()?;
            self.with_child_provider(child.id, |provider| {
                provider.set_selection_container(Some(container))
            })?;
        }
        Ok(())
    }

    pub fn detach(&self) -> Result<()> {
        let event = {
            let mut state = self.state.try_borrow_mut().map_err(|_| state_error())?;
            state.ensure_owner()?;
            detach_node(&mut state, self.id)?
        };
        if let Some((parent, id)) = event {
            let parent = UiaFragment {
                state: Rc::clone(&self.state),
                id: parent,
            };
            raise_structure_changed(&parent, StructureChangeType_ChildRemoved, id)?;
        }
        Ok(())
    }

    /// Raises the event through the fragment wrapper returned by WM_GETOBJECT,
    /// so clients observe the same COM identity they discovered in the tree.
    pub fn announce(&self) -> Result<()> {
        announce_metadata(&self.raw_provider()?, self.metadata()?)
    }

    fn with_child_provider<T>(
        &self,
        id: i32,
        read: impl FnOnce(&UiaProviderHandle) -> Result<T>,
    ) -> Result<T> {
        let state = self.state.try_borrow().map_err(|_| state_error())?;
        read(&state.node(id)?.provider)
    }
}

pub fn update_live_node(
    fragment: &UiaFragment,
    current: &mut String,
    kind: WidgetKind,
    id: &str,
    text: String,
) -> Result<()> {
    if *current == text {
        return Ok(());
    }
    fragment.set_metadata(metadata(kind, id, text.clone()))?;
    fragment.announce()?;
    *current = text;
    Ok(())
}

pub fn create_live_node(
    parent: &UiaFragment,
    kind: WidgetKind,
    id: &str,
    text: &str,
) -> Result<UiaFragment> {
    let fragment = parent.add_child(UiaProviderFactory::create(metadata(
        kind,
        id,
        text.to_owned(),
    ))?)?;
    fragment.announce()?;
    Ok(fragment)
}

pub struct LiveNode {
    pub fragment: UiaFragment,
    pub text: String,
}

pub fn clear_live_node(slot: &mut Option<LiveNode>) -> Result<()> {
    if let Some(node) = slot.take() {
        node.fragment.detach()?;
    }
    Ok(())
}

pub fn sync_live_node(
    parent: &UiaFragment,
    slot: &mut Option<LiveNode>,
    kind: WidgetKind,
    id: &str,
    text: String,
) -> Result<()> {
    if text.is_empty() {
        return clear_live_node(slot);
    }
    if let Some(node) = slot.as_mut() {
        update_live_node(&node.fragment, &mut node.text, kind, id, text)?;
    } else {
        let fragment = create_live_node(parent, kind, id, &text)?;
        *slot = Some(LiveNode { fragment, text });
    }
    Ok(())
}

fn detach_node(state: &mut TreeState, id: i32) -> Result<Option<(i32, i32)>> {
    let (parent, children) = {
        let Some(node) = state.nodes.get_mut(id as usize).and_then(Option::take) else {
            return Ok(None);
        };
        (node.parent, node.children)
    };
    if state.focused == Some(id) {
        state.focused = None;
    }
    for child in children {
        let _ = detach_node(state, child)?;
    }
    if let Some(parent) = parent {
        if let Some(siblings) = state
            .nodes
            .get_mut(parent as usize)
            .and_then(Option::as_mut)
        {
            siblings.children.retain(|child| *child != id);
        }
        Ok(Some((parent, id)))
    } else {
        // The native root lives until the subclass is destroyed.
        state.nodes[id as usize] = None;
        Ok(None)
    }
}

fn raise_structure_changed(parent: &UiaFragment, kind: StructureChangeType, id: i32) -> Result<()> {
    let provider = parent.raw_provider()?;
    let mut runtime_id = [UiaAppendRuntimeId as i32, id];
    // SAFETY: the runtime id pointer and length remain valid for this synchronous call.
    unsafe { UiaRaiseStructureChangedEvent(&provider, kind, runtime_id.as_mut_ptr(), 2) }
}

#[implement(
    IRawElementProviderSimple,
    IRawElementProviderFragment,
    IRawElementProviderFragmentRoot
)]
struct Fragment {
    tree: Weak<RefCell<TreeState>>,
    id: i32,
}

impl Fragment {
    fn state(&self) -> Result<Rc<RefCell<TreeState>>> {
        self.tree.upgrade().ok_or_else(element_unavailable)
    }

    fn navigate(&self, direction: NavigateDirection) -> Result<Option<i32>> {
        let state = self.state()?;
        let state = state.try_borrow().map_err(|_| state_error())?;
        state.ensure_owner()?;
        navigation_target(&state, self.id, direction)
    }

    fn hit_test(state: &TreeState, id: i32, x: f64, y: f64) -> Result<Option<i32>> {
        let node = state.node(id)?;
        for child in node.children.iter().rev() {
            if let Some(hit) = Self::hit_test(state, *child, x, y)? {
                return Ok(Some(hit));
            }
        }
        let bounds = node.bounds;
        let contains = bounds.width > 0.0
            && bounds.height > 0.0
            && x >= bounds.left
            && y >= bounds.top
            && x < bounds.left + bounds.width
            && y < bounds.top + bounds.height;
        Ok(contains.then_some(id))
    }
}

#[allow(non_upper_case_globals)] // Preserve Windows SDK navigation identifiers verbatim.
fn navigation_target(
    state: &TreeState,
    id: i32,
    direction: NavigateDirection,
) -> Result<Option<i32>> {
    let node = state.node(id)?;
    Ok(match direction {
        NavigateDirection_Parent => node.parent,
        NavigateDirection_FirstChild => node.children.first().copied(),
        NavigateDirection_LastChild => node.children.last().copied(),
        NavigateDirection_NextSibling => sibling(id, node, state, true),
        NavigateDirection_PreviousSibling => sibling(id, node, state, false),
        _ => None,
    })
}

fn sibling(id: i32, node: &Node, state: &TreeState, next: bool) -> Option<i32> {
    let siblings = &state.node(node.parent?).ok()?.children;
    let index = siblings.iter().position(|child| *child == id)?;
    if next {
        siblings.get(index + 1).copied()
    } else {
        index
            .checked_sub(1)
            .and_then(|index| siblings.get(index))
            .copied()
    }
}

impl IRawElementProviderSimple_Impl for Fragment_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider | ProviderOptions_UseComThreading)
    }

    fn GetPatternProvider(&self, pattern_id: UIA_PATTERN_ID) -> Result<IUnknown> {
        let state = self.state()?;
        let provider = {
            let state = state.try_borrow().map_err(|_| state_error())?;
            state.ensure_owner()?;
            state.node(self.id)?.provider.raw_provider()
        };
        // SAFETY: this delegates the native UIA pattern request to the real provider.
        unsafe { provider.GetPatternProvider(pattern_id) }
    }

    fn GetPropertyValue(&self, property_id: UIA_PROPERTY_ID) -> Result<VARIANT> {
        let state = self.state()?;
        let (provider, focused) = {
            let state = state.try_borrow().map_err(|_| state_error())?;
            state.ensure_owner()?;
            (
                state.node(self.id)?.provider.raw_provider(),
                state.focused == Some(self.id),
            )
        };
        if property_id == UIA_HasKeyboardFocusPropertyId {
            return Ok(focused.into());
        }
        // SAFETY: this delegates all metadata and pattern properties to the real provider.
        unsafe { provider.GetPropertyValue(property_id) }
    }

    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        if self.id != 0 {
            // Non-root fragments are not directly hosted by the HWND.
            return Err(Error::empty());
        }
        let state = self.state()?;
        let (provider, hwnd) = {
            let state = state.try_borrow().map_err(|_| state_error())?;
            state.ensure_owner()?;
            (state.node(self.id)?.provider.raw_provider(), state.hwnd)
        };
        // SAFETY: the host provider is obtained for the owning HWND.
        unsafe { provider.HostRawElementProvider() }
            .or_else(|_| unsafe { UiaHostProviderFromHwnd(hwnd) })
    }
}

impl IRawElementProviderFragment_Impl for Fragment_Impl {
    fn Navigate(&self, direction: NavigateDirection) -> Result<IRawElementProviderFragment> {
        let target = self.navigate(direction)?;
        let Some(target) = target else {
            // SAFETY: the generated wrapper writes this null COM pointer to the UIA out parameter
            // and does not drop it; null is the documented empty navigation result.
            return Ok(unsafe { IRawElementProviderFragment::from_raw(null_mut()) });
        };
        let state = self.state()?;
        let state = state.try_borrow().map_err(|_| state_error())?;
        state.fragment(target)
    }

    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        runtime_id(self.id)
    }

    fn BoundingRectangle(&self) -> Result<UiaRect> {
        let state = self.state()?;
        let state = state.try_borrow().map_err(|_| state_error())?;
        state.ensure_owner()?;
        if self.id != 0 {
            return Ok(state.node(self.id)?.bounds);
        }
        let mut rect = RECT::default();
        // SAFETY: the HWND is owned by GPUI and the rectangle pointer is valid.
        unsafe { GetWindowRect(state.hwnd, &mut rect) }?;
        Ok(UiaRect {
            left: rect.left as f64,
            top: rect.top as f64,
            width: (rect.right - rect.left) as f64,
            height: (rect.bottom - rect.top) as f64,
        })
    }

    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(null_mut())
    }

    fn SetFocus(&self) -> Result<()> {
        let fragment = UiaFragment {
            state: self.state()?,
            id: self.id,
        };
        fragment.set_focused(true)?;
        let (hwnd, handler) = {
            let state = fragment.state.try_borrow().map_err(|_| state_error())?;
            (state.hwnd, state.node(self.id)?.focus_handler.clone())
        };
        // A null previous-focus return is successful; the windows crate models it
        // as an error, so only the GPUI callback's result is relevant here.
        let _ = unsafe { SetFocus(Some(hwnd)) };
        if let Some(handler) = handler {
            handler();
        }
        Ok(())
    }

    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        let state = self.state()?;
        let state = state.try_borrow().map_err(|_| state_error())?;
        state.root_fragment()
    }
}

impl IRawElementProviderFragmentRoot_Impl for Fragment_Impl {
    fn ElementProviderFromPoint(&self, x: f64, y: f64) -> Result<IRawElementProviderFragment> {
        let state = self.state()?;
        let state = state.try_borrow().map_err(|_| state_error())?;
        state.ensure_owner()?;
        let target = Fragment::hit_test(&state, 0, x, y)?.unwrap_or(0);
        state.fragment(target)
    }

    fn GetFocus(&self) -> Result<IRawElementProviderFragment> {
        let state = self.state()?;
        let state = state.try_borrow().map_err(|_| state_error())?;
        state.ensure_owner()?;
        match state.focused {
            Some(id) => state.fragment(id),
            None => Err(Error::empty()),
        }
    }
}

fn runtime_id(id: i32) -> Result<*mut SAFEARRAY> {
    // SAFETY: the SAFEARRAY is created, initialized, and destroyed on failure.
    unsafe {
        let array = SafeArrayCreateVector(VT_I4, 0, 2);
        if array.is_null() {
            return Err(Error::from_hresult(E_OUTOFMEMORY));
        }
        for (index, value) in [UiaAppendRuntimeId as i32, id].into_iter().enumerate() {
            if let Err(error) =
                SafeArrayPutElement(array, &(index as i32), &value as *const i32 as _)
            {
                let _ = SafeArrayDestroy(array);
                return Err(error);
            }
        }
        Ok(array)
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    reference_data: usize,
) -> LRESULT {
    let state = reference_data as *const SubclassState;
    if message == WM_GETOBJECT
        && (lparam.0 as i32 == OBJID_CLIENT.0 || lparam.0 as i32 == UiaRootObjectId)
    {
        // SAFETY: reference_data is owned by this installed subclass.
        if let Some(tree) = unsafe { (*state).tree.upgrade() } {
            let provider = tree.try_borrow().ok().and_then(|tree| tree.simple(0).ok());
            if let Some(provider) = provider {
                // SAFETY: this is the UIA response for the provider's owning HWND.
                return unsafe { UiaReturnRawElementProvider(hwnd, wparam, lparam, &provider) };
            }
        }
    }
    // SAFETY: forwards every unhandled message through the original GPUI procedure.
    let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
    if message == WM_NCDESTROY {
        // SAFETY: reference_data is owned by this subclass and released exactly once.
        unsafe {
            let alive = (*state).alive.clone();
            if alive.replace(false) {
                let _ = RemoveWindowSubclass(hwnd, Some(window_proc), state as usize);
                drop(Box::from_raw(state as *mut SubclassState));
            }
        }
    }
    result
}

fn state_error() -> Error {
    Error::new(E_FAIL, "UIA fragment tree is unavailable")
}

fn element_unavailable() -> Error {
    Error::from_hresult(HRESULT(
        windows::Win32::UI::Accessibility::UIA_E_ELEMENTNOTAVAILABLE as i32,
    ))
}

#[cfg(all(test, windows))]
mod tests {
    use windows::{
        Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_OVERLAPPED,
            },
        },
        core::{Interface, PCWSTR, w},
    };

    use std::sync::{Arc, Mutex};

    use crate::accessibility::{UiaAction, UiaProviderFactory};

    use super::*;

    struct WindowGuard(HWND);

    impl Drop for WindowGuard {
        fn drop(&mut self) {
            // SAFETY: this test created the HWND and destroys it once.
            unsafe { DestroyWindow(self.0).unwrap() };
        }
    }

    fn native_window() -> Result<(WindowGuard, WindowEffects)> {
        // SAFETY: creates a private STATIC window; no class registration is needed.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                PCWSTR::null(),
                WS_OVERLAPPED,
                0,
                0,
                80,
                40,
                None,
                None,
                None,
                None,
            )?
        };
        let effects = WindowEffects::new(hwnd)?;
        Ok((WindowGuard(hwnd), effects))
    }

    fn metadata(kind: WidgetKind, id: &str) -> UiaMetadata {
        UiaMetadata {
            automation_id: id.into(),
            name: id.into(),
            ..UiaMetadata::new(kind)
        }
    }

    fn automation_id(fragment: &IRawElementProviderFragment) -> Result<String> {
        let simple = fragment.cast::<IRawElementProviderSimple>()?;
        // SAFETY: the fragment implements IRawElementProviderSimple.
        let value = unsafe { simple.GetPropertyValue(UIA_AutomationIdPropertyId) }?;
        // SAFETY: AutomationId is returned as VT_BSTR by UiaProvider.
        Ok(unsafe { value.Anonymous.Anonymous.Anonymous.bstrVal.clone() }.to_string())
    }

    struct NativeTreeFixture {
        _window: WindowGuard,
        _tree: UiaTree,
        caption: UiaFragment,
        settings: UiaFragment,
        list: UiaFragment,
        option: UiaFragment,
    }

    fn add_test_child(parent: &UiaFragment, kind: WidgetKind, id: &str) -> Result<UiaFragment> {
        parent.add_child(UiaProviderFactory::create(metadata(kind, id))?)
    }

    fn native_tree_with_root() -> Result<(WindowGuard, UiaTree)> {
        let (_window, effects) = native_window()?;
        let root = UiaProviderFactory::create(metadata(WidgetKind::ScrollArea, "overlay"))?;
        let tree = UiaTree::attach(&effects, root)?;
        Ok((_window, tree))
    }

    fn native_tree_fixture() -> Result<NativeTreeFixture> {
        let (_window, tree) = native_tree_with_root()?;
        let caption = add_test_child(&tree.root(), WidgetKind::Caption, "caption-live")?;
        let settings = add_test_child(&tree.root(), WidgetKind::ScrollArea, "settings")?;
        let list = add_test_child(&settings, WidgetKind::List, "source-language-menu")?;
        let option = add_test_child(
            &list,
            WidgetKind::PickerOption,
            "source-language-option-pt-BR",
        )?;
        Ok(NativeTreeFixture {
            _window,
            _tree: tree,
            caption,
            settings,
            list,
            option,
        })
    }

    fn assert_navigation(
        fragment: &IRawElementProviderFragment,
        direction: NavigateDirection,
        expected: &str,
    ) -> Result<()> {
        // SAFETY: production fragments perform native navigation.
        let target = unsafe { fragment.Navigate(direction)? };
        assert_eq!(automation_id(&target)?, expected);
        Ok(())
    }

    fn assert_navigation_cases(
        cases: &[(&IRawElementProviderFragment, NavigateDirection, &str)],
    ) -> Result<()> {
        for (fragment, direction, expected) in cases {
            assert_navigation(fragment, *direction, expected)?;
        }
        Ok(())
    }

    #[test]
    fn native_tree_navigates_siblings_parents_and_options() -> Result<()> {
        let fixture = native_tree_fixture()?;
        let caption = fixture.caption.raw_fragment()?;
        let settings = fixture.settings.raw_fragment()?;
        let list = fixture.list.raw_fragment()?;
        let option = fixture.option.raw_fragment()?;
        assert_navigation_cases(&[
            (&caption, NavigateDirection_NextSibling, "settings"),
            (&settings, NavigateDirection_PreviousSibling, "caption-live"),
            (&settings, NavigateDirection_Parent, "overlay"),
            (
                &settings,
                NavigateDirection_FirstChild,
                "source-language-menu",
            ),
            (
                &list,
                NavigateDirection_FirstChild,
                "source-language-option-pt-BR",
            ),
            (&option, NavigateDirection_Parent, "source-language-menu"),
        ])
    }

    #[test]
    fn native_tree_fragment_root_stays_stable() -> Result<()> {
        let fixture = native_tree_fixture()?;
        // SAFETY: the option returns the production root interface.
        let root = unsafe { fixture.option.raw_fragment()?.FragmentRoot() }?;
        let root_fragment = root.cast::<IRawElementProviderFragment>()?;
        assert_eq!(automation_id(&root_fragment)?, "overlay");
        Ok(())
    }

    struct NativeLockFixture {
        _window: WindowGuard,
        tree: UiaTree,
        lock: UiaFragment,
        actions: Arc<Mutex<Vec<UiaAction>>>,
    }

    fn native_lock_fixture() -> Result<NativeLockFixture> {
        let (_window, effects) = native_window()?;
        let root = UiaProviderFactory::create(metadata(WidgetKind::ScrollArea, "overlay"))?;
        let tree = UiaTree::attach(&effects, root)?;
        let actions = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&actions);
        let lock = tree
            .root()
            .add_child(UiaProviderFactory::create_with_handler(
                metadata(WidgetKind::Button, "lock"),
                move |action| {
                    recorded.lock().unwrap().push(action);
                    Ok(())
                },
            )?)?;
        lock.set_bounds(UiaRect {
            left: 100.0,
            top: 40.0,
            width: 32.0,
            height: 32.0,
        })?;
        Ok(NativeLockFixture {
            _window,
            tree,
            lock,
            actions,
        })
    }

    #[test]
    fn lock_fragment_has_clickable_bounds_and_metadata() -> Result<()> {
        let fixture = native_lock_fixture()?;
        let fragment = unsafe {
            fixture
                .tree
                .root()
                .raw_fragment()?
                .Navigate(NavigateDirection_FirstChild)?
        };
        assert_eq!(automation_id(&fragment)?, "lock");
        assert_eq!(unsafe { fragment.BoundingRectangle() }?.width, 32.0);
        let simple = fragment.cast::<IRawElementProviderSimple>()?;
        let role = unsafe { simple.GetPropertyValue(UIA_ControlTypePropertyId) }?;
        assert_eq!(i32::try_from(&role)?, UIA_ButtonControlTypeId.0);
        Ok(())
    }

    #[test]
    fn lock_fragment_invokes_through_fragment_wrapper() -> Result<()> {
        let fixture = native_lock_fixture()?;
        let simple = fixture
            .lock
            .raw_fragment()?
            .cast::<IRawElementProviderSimple>()?;
        let invoke =
            unsafe { simple.GetPatternProvider(UIA_InvokePatternId) }?.cast::<IInvokeProvider>()?;
        unsafe { invoke.Invoke() }?;
        assert_eq!(
            fixture.actions.lock().unwrap().as_slice(),
            &[UiaAction::Invoke]
        );
        Ok(())
    }

    struct NativeDetachedFixture {
        _window: WindowGuard,
        tree: UiaTree,
        settings: UiaFragment,
        color: UiaFragment,
    }

    fn native_detached_fixture() -> Result<NativeDetachedFixture> {
        let (_window, tree) = native_tree_with_root()?;
        let settings = add_test_child(&tree.root(), WidgetKind::ScrollArea, "settings")?;
        let color = add_test_child(&settings, WidgetKind::TextInput, "caption-color")?;
        color.set_bounds(UiaRect {
            left: 10.0,
            top: 20.0,
            width: 30.0,
            height: 40.0,
        })?;
        Ok(NativeDetachedFixture {
            _window,
            tree,
            settings,
            color,
        })
    }

    #[test]
    fn detached_subtrees_leave_navigation() -> Result<()> {
        let fixture = native_detached_fixture()?;
        fixture.settings.detach()?;
        assert!(fixture.color.raw_fragment().is_err());
        // SAFETY: the root now has no child; UIA's null result uses the S_OK path.
        let result = unsafe {
            fixture
                .tree
                .root()
                .raw_fragment()?
                .Navigate(NavigateDirection_FirstChild)
        };
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn hit_testing_uses_fragment_bounds() -> Result<()> {
        let fixture = native_detached_fixture()?;
        // SAFETY: root hit-testing returns the native child fragment.
        let root_interface = fixture
            .tree
            .root()
            .raw_fragment()?
            .cast::<IRawElementProviderFragmentRoot>()?;
        let hit = unsafe { root_interface.ElementProviderFromPoint(15.0, 25.0) }?;
        assert_eq!(automation_id(&hit)?, "caption-color");
        Ok(())
    }
}
