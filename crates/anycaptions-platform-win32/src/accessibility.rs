//! Windows UI Automation metadata, patterns, and notification factories.

use std::rc::Rc;
use std::sync::RwLock;
use std::thread::ThreadId;
use windows::Win32::Foundation::{E_FAIL, E_INVALIDARG, HWND, LPARAM, RPC_E_WRONG_THREAD, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::*;
use windows_core::{
    BOOL, BSTR, ComObject, Error, HRESULT, IUnknown, Interface, PCWSTR, Result, implement,
};

pub type UiaBounds = UiaRect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum WidgetKind {
    Caption,
    Button,
    Toggle,
    Slider,
    NumberInput,
    TextInput,
    Picker,
    PickerOption,
    Progress,
    Tooltip,
    Status,
    Alert,
    ScrollArea,
    List,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum UiaRole {
    Text = 50020,
    Button = 50000,
    CheckBox = 50002,
    Slider = 50015,
    Spinner = 50016,
    Edit = 50004,
    ComboBox = 50003,
    ListItem = 50007,
    List = 50008,
    ProgressBar = 50012,
    ToolTip = 50022,
    Pane = 50033,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum UiaLiveSetting {
    Off = 0,
    Polite = 1,
    Assertive = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiaPattern {
    Value,
    SelectionItem,
    ExpandCollapse,
    RangeValue,
    Toggle,
    Invoke,
}

const NO_PATTERNS: &[UiaPattern] = &[];
const VALUE_PATTERN: &[UiaPattern] = &[UiaPattern::Value];
const VALUE_AND_RANGE_PATTERNS: &[UiaPattern] = &[UiaPattern::RangeValue, UiaPattern::Value];
const SELECTION_PATTERN: &[UiaPattern] = &[UiaPattern::SelectionItem];
const EXPAND_PATTERN: &[UiaPattern] = &[UiaPattern::ExpandCollapse];
const RANGE_PATTERN: &[UiaPattern] = &[UiaPattern::RangeValue];
const TOGGLE_PATTERN: &[UiaPattern] = &[UiaPattern::Toggle];
const INVOKE_PATTERN: &[UiaPattern] = &[UiaPattern::Invoke];

struct WidgetPresentation {
    role: UiaRole,
    live: UiaLiveSetting,
    patterns: &'static [UiaPattern],
}

// Indexed by WidgetKind's stable representation; one table defines every public mapping.
const PRESENTATIONS: [WidgetPresentation; 14] = [
    WidgetPresentation {
        role: UiaRole::Text,
        live: UiaLiveSetting::Polite,
        patterns: NO_PATTERNS,
    },
    WidgetPresentation {
        role: UiaRole::Button,
        live: UiaLiveSetting::Off,
        patterns: INVOKE_PATTERN,
    },
    WidgetPresentation {
        role: UiaRole::CheckBox,
        live: UiaLiveSetting::Off,
        patterns: TOGGLE_PATTERN,
    },
    WidgetPresentation {
        role: UiaRole::Slider,
        live: UiaLiveSetting::Off,
        patterns: RANGE_PATTERN,
    },
    WidgetPresentation {
        role: UiaRole::Spinner,
        live: UiaLiveSetting::Off,
        patterns: VALUE_AND_RANGE_PATTERNS,
    },
    WidgetPresentation {
        role: UiaRole::Edit,
        live: UiaLiveSetting::Off,
        patterns: VALUE_PATTERN,
    },
    WidgetPresentation {
        role: UiaRole::ComboBox,
        live: UiaLiveSetting::Off,
        patterns: EXPAND_PATTERN,
    },
    WidgetPresentation {
        role: UiaRole::ListItem,
        live: UiaLiveSetting::Off,
        patterns: SELECTION_PATTERN,
    },
    WidgetPresentation {
        role: UiaRole::ProgressBar,
        live: UiaLiveSetting::Off,
        patterns: RANGE_PATTERN,
    },
    WidgetPresentation {
        role: UiaRole::ToolTip,
        live: UiaLiveSetting::Off,
        patterns: NO_PATTERNS,
    },
    WidgetPresentation {
        role: UiaRole::Text,
        live: UiaLiveSetting::Polite,
        patterns: NO_PATTERNS,
    },
    WidgetPresentation {
        role: UiaRole::Text,
        live: UiaLiveSetting::Assertive,
        patterns: NO_PATTERNS,
    },
    WidgetPresentation {
        role: UiaRole::Pane,
        live: UiaLiveSetting::Off,
        patterns: NO_PATTERNS,
    },
    WidgetPresentation {
        role: UiaRole::List,
        live: UiaLiveSetting::Off,
        patterns: NO_PATTERNS,
    },
];

const fn presentation(kind: WidgetKind) -> &'static WidgetPresentation {
    &PRESENTATIONS[kind as usize]
}

pub const fn uia_role(kind: WidgetKind) -> UiaRole {
    presentation(kind).role
}

pub const fn uia_live_setting(kind: WidgetKind) -> UiaLiveSetting {
    presentation(kind).live
}

pub const fn uia_patterns(kind: WidgetKind) -> &'static [UiaPattern] {
    presentation(kind).patterns
}

impl UiaRole {
    pub const fn control_type(self) -> UIA_CONTROLTYPE_ID {
        UIA_CONTROLTYPE_ID(self as i32)
    }
}

impl UiaLiveSetting {
    pub const fn native(self) -> LiveSetting {
        LiveSetting(self as i32)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiaRange {
    pub value: f64,
    pub minimum: f64,
    pub maximum: f64,
    pub small_change: f64,
    pub large_change: f64,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiaMetadata {
    pub kind: WidgetKind,
    pub automation_id: String,
    pub name: String,
    pub description: String,
    pub value: Option<String>,
    pub selected: bool,
    pub expanded: bool,
    pub invalid: bool,
    pub disabled: bool,
    pub read_only: bool,
    pub toggled: bool,
    pub range: Option<UiaRange>,
}

impl UiaMetadata {
    pub fn new(kind: WidgetKind) -> Self {
        Self {
            kind,
            automation_id: String::new(),
            name: String::new(),
            description: String::new(),
            value: None,
            selected: false,
            expanded: false,
            invalid: false,
            disabled: false,
            read_only: false,
            toggled: false,
            range: None,
        }
    }

    fn keyboard_focusable(&self) -> bool {
        !self.disabled
            && matches!(
                self.kind,
                WidgetKind::Button
                    | WidgetKind::Toggle
                    | WidgetKind::Slider
                    | WidgetKind::NumberInput
                    | WidgetKind::TextInput
                    | WidgetKind::Picker
                    | WidgetKind::PickerOption
            )
    }
}

pub fn widget_metadata(
    kind: WidgetKind,
    id: impl Into<String>,
    name: impl Into<String>,
) -> UiaMetadata {
    UiaMetadata {
        automation_id: id.into(),
        name: name.into(),
        ..UiaMetadata::new(kind)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiaAction {
    Value(String),
    RangeValue(f64),
    Selected(bool),
    Expanded(bool),
    Toggled(bool),
    Invoke,
}

/// UI-thread action callback. The provider STA enforces the calling apartment.
pub type UiaActionHandler = Rc<dyn Fn(UiaAction) -> Result<()> + 'static>;

pub fn uia_action_error(message: impl AsRef<str>) -> Error {
    Error::new(E_FAIL, message)
}

pub(crate) fn ensure_thread(owner: ThreadId) -> Result<()> {
    if owner == std::thread::current().id() {
        Ok(())
    } else {
        Err(Error::from_hresult(RPC_E_WRONG_THREAD))
    }
}

struct StaApartment {
    owner: ThreadId,
}

impl StaApartment {
    fn initialize() -> Result<Self> {
        let owner = std::thread::current().id();
        // SAFETY: initializes COM on this thread; every successful call is balanced in Drop.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.ok()?;
        Ok(Self { owner })
    }

    fn ensure_owner(&self) -> Result<()> {
        ensure_thread(self.owner)
    }
}

impl Drop for StaApartment {
    fn drop(&mut self) {
        debug_assert_eq!(self.owner, std::thread::current().id());
        // SAFETY: ProviderOptions_UseComThreading keeps the final release on this
        // owning thread, balancing the successful CoInitializeEx above.
        unsafe { CoUninitialize() }
    }
}

struct UiaState {
    metadata: RwLock<UiaMetadata>,
    hwnd: RwLock<Option<HWND>>,
    selection_container: RwLock<Option<IRawElementProviderSimple>>,
    handler: Option<UiaActionHandler>,
    // Last field: all interfaces drop before the apartment they keep alive.
    apartment: StaApartment,
}

struct PreparedAction {
    action: UiaAction,
    number_value: Option<f64>,
}

impl UiaState {
    fn new(
        metadata: UiaMetadata,
        handler: Option<UiaActionHandler>,
        apartment: StaApartment,
    ) -> Self {
        Self {
            metadata: RwLock::new(metadata),
            hwnd: RwLock::new(None),
            selection_container: RwLock::new(None),
            handler,
            apartment,
        }
    }

    fn read<T>(&self, read: impl FnOnce(&UiaMetadata) -> T) -> Result<T> {
        self.apartment.ensure_owner()?;
        let metadata = self.metadata.read().map_err(|_| state_error())?;
        Ok(read(&metadata))
    }

    fn snapshot(&self) -> Result<UiaMetadata> {
        self.read(|metadata| metadata.clone())
    }

    fn prepare(&self, action: UiaAction) -> Result<(UiaActionHandler, PreparedAction)> {
        self.apartment.ensure_owner()?;
        let metadata = self.metadata.read().map_err(|_| state_error())?;
        if metadata.disabled {
            return Err(uia_error(UIA_E_ELEMENTNOTENABLED));
        }
        if metadata.read_only {
            return Err(uia_error(UIA_E_INVALIDOPERATION));
        }
        let number_value = match &action {
            UiaAction::Value(value) if metadata.kind == WidgetKind::NumberInput => {
                Some(number_input_value(&metadata, value)?)
            }
            _ => None,
        };
        let handler = self
            .handler
            .clone()
            .ok_or_else(|| uia_error(UIA_E_INVALIDOPERATION))?;
        Ok((
            handler,
            PreparedAction {
                action,
                number_value,
            },
        ))
    }

    fn apply(&self, action: UiaAction) -> Result<()> {
        let (handler, prepared) = self.prepare(action)?;
        // GPUI callbacks must never run under the metadata lock.
        handler(prepared.action.clone())?;
        self.commit(prepared)
    }

    fn commit(&self, prepared: PreparedAction) -> Result<()> {
        let mut metadata = self.metadata.write().map_err(|_| state_error())?;
        match prepared.action {
            UiaAction::Value(value) => {
                Self::commit_text_value(&mut metadata, value, prepared.number_value)?;
            }
            UiaAction::RangeValue(value) => Self::commit_range_value(&mut metadata, value)?,
            UiaAction::Selected(selected) => metadata.selected = selected,
            UiaAction::Expanded(expanded) => metadata.expanded = expanded,
            UiaAction::Toggled(toggled) => metadata.toggled = toggled,
            UiaAction::Invoke => {}
        }
        Ok(())
    }

    fn commit_text_value(
        metadata: &mut UiaMetadata,
        value: String,
        number: Option<f64>,
    ) -> Result<()> {
        if let Some(number) = number {
            required_option(metadata.range.as_mut(), UIA_E_NOTSUPPORTED)?.value = number;
        }
        metadata.value = Some(value);
        Ok(())
    }

    fn commit_range_value(metadata: &mut UiaMetadata, value: f64) -> Result<()> {
        required_option(metadata.range.as_mut(), UIA_E_NOTSUPPORTED)?.value = value;
        if metadata.kind == WidgetKind::NumberInput {
            metadata.value = Some(value.to_string());
        }
        Ok(())
    }

    fn read_only(&self, range: bool) -> Result<bool> {
        self.read(|metadata| {
            self.handler.is_none()
                || metadata.read_only
                || (range && metadata.range.is_some_and(|range| range.read_only))
        })
    }
}

#[implement(IValueProvider)]
struct ValuePattern {
    state: Rc<UiaState>,
}

impl IValueProvider_Impl for ValuePattern_Impl {
    fn SetValue(&self, value: &PCWSTR) -> Result<()> {
        // SAFETY: UIA supplies a valid NUL-terminated string for this call.
        let value = unsafe { value.to_string() }
            .map_err(|_| Error::new(E_INVALIDARG, "UIA value is not valid UTF-16"))?;
        self.state.apply(UiaAction::Value(value))
    }

    fn Value(&self) -> Result<BSTR> {
        self.state
            .read(|metadata| BSTR::from(metadata.value.as_deref().unwrap_or_default()))
    }

    fn IsReadOnly(&self) -> Result<BOOL> {
        Ok(BOOL::from(self.state.read_only(true)?))
    }
}

#[implement(IRangeValueProvider)]
struct RangePattern {
    state: Rc<UiaState>,
}

impl RangePattern {
    fn range(&self) -> Result<UiaRange> {
        required_option(
            self.state.read(|metadata| metadata.range)?,
            UIA_E_NOTSUPPORTED,
        )
    }
}

impl IRangeValueProvider_Impl for RangePattern_Impl {
    fn SetValue(&self, value: f64) -> Result<()> {
        if self.state.read_only(true)? {
            return Err(uia_error(UIA_E_INVALIDOPERATION));
        }
        let range = self.range()?;
        validate_range_value(&range, value)?;
        self.state.apply(UiaAction::RangeValue(value))
    }

    fn Value(&self) -> Result<f64> {
        Ok(self.range()?.value)
    }

    fn IsReadOnly(&self) -> Result<BOOL> {
        Ok(BOOL::from(self.state.read_only(true)?))
    }

    fn Maximum(&self) -> Result<f64> {
        Ok(self.range()?.maximum)
    }

    fn Minimum(&self) -> Result<f64> {
        Ok(self.range()?.minimum)
    }

    fn LargeChange(&self) -> Result<f64> {
        Ok(self.range()?.large_change)
    }

    fn SmallChange(&self) -> Result<f64> {
        Ok(self.range()?.small_change)
    }
}

#[implement(ISelectionItemProvider)]
struct SelectionItemPattern {
    state: Rc<UiaState>,
}

impl ISelectionItemProvider_Impl for SelectionItemPattern_Impl {
    fn Select(&self) -> Result<()> {
        self.state.apply(UiaAction::Selected(true))
    }

    fn AddToSelection(&self) -> Result<()> {
        self.state.apply(UiaAction::Selected(true))
    }

    fn RemoveFromSelection(&self) -> Result<()> {
        self.state.apply(UiaAction::Selected(false))
    }

    fn IsSelected(&self) -> Result<BOOL> {
        self.state.read(|metadata| BOOL::from(metadata.selected))
    }

    fn SelectionContainer(&self) -> Result<IRawElementProviderSimple> {
        self.selection_container()
    }
}

impl SelectionItemPattern {
    fn selection_container(&self) -> Result<IRawElementProviderSimple> {
        self.state.apartment.ensure_owner()?;
        required_option(
            self.state
                .selection_container
                .read()
                .map_err(|_| state_error())?
                .clone(),
            UIA_E_ELEMENTNOTAVAILABLE,
        )
    }
}

#[implement(IExpandCollapseProvider)]
struct ExpandCollapsePattern {
    state: Rc<UiaState>,
}

impl IExpandCollapseProvider_Impl for ExpandCollapsePattern_Impl {
    fn Expand(&self) -> Result<()> {
        self.state.apply(UiaAction::Expanded(true))
    }

    fn Collapse(&self) -> Result<()> {
        self.state.apply(UiaAction::Expanded(false))
    }

    fn ExpandCollapseState(&self) -> Result<ExpandCollapseState> {
        self.state.read(|metadata| {
            native_state(
                metadata.expanded,
                ExpandCollapseState_Expanded,
                ExpandCollapseState_Collapsed,
            )
        })
    }
}

#[implement(IToggleProvider)]
struct TogglePattern {
    state: Rc<UiaState>,
}

impl IToggleProvider_Impl for TogglePattern_Impl {
    fn Toggle(&self) -> Result<()> {
        let toggled = !self.state.read(|metadata| metadata.toggled)?;
        self.state.apply(UiaAction::Toggled(toggled))
    }

    fn ToggleState(&self) -> Result<ToggleState> {
        self.state
            .read(|metadata| native_state(metadata.toggled, ToggleState_On, ToggleState_Off))
    }
}

#[implement(IInvokeProvider)]
struct InvokePattern {
    state: Rc<UiaState>,
}

impl IInvokeProvider_Impl for InvokePattern_Impl {
    fn Invoke(&self) -> Result<()> {
        self.state.apply(UiaAction::Invoke)
    }
}

struct UiaPatterns(Vec<(UIA_PATTERN_ID, IUnknown)>);

impl UiaPatterns {
    fn new(kind: WidgetKind, state: &Rc<UiaState>) -> Self {
        let entries = uia_patterns(kind)
            .iter()
            .filter(|pattern| **pattern != UiaPattern::Invoke || state.handler.is_some())
            .map(|pattern| {
                let state = Rc::clone(state);
                match pattern {
                    UiaPattern::Value => {
                        let provider: IValueProvider = ValuePattern { state }.into();
                        (UIA_ValuePatternId, IUnknown::from(provider))
                    }
                    UiaPattern::RangeValue => {
                        let provider: IRangeValueProvider = RangePattern { state }.into();
                        (UIA_RangeValuePatternId, IUnknown::from(provider))
                    }
                    UiaPattern::SelectionItem => {
                        let provider: ISelectionItemProvider =
                            SelectionItemPattern { state }.into();
                        (UIA_SelectionItemPatternId, IUnknown::from(provider))
                    }
                    UiaPattern::ExpandCollapse => {
                        let provider: IExpandCollapseProvider =
                            ExpandCollapsePattern { state }.into();
                        (UIA_ExpandCollapsePatternId, IUnknown::from(provider))
                    }
                    UiaPattern::Toggle => {
                        let provider: IToggleProvider = TogglePattern { state }.into();
                        (UIA_TogglePatternId, IUnknown::from(provider))
                    }
                    UiaPattern::Invoke => {
                        let provider: IInvokeProvider = InvokePattern { state }.into();
                        (UIA_InvokePatternId, IUnknown::from(provider))
                    }
                }
            })
            .collect();
        Self(entries)
    }
}

#[implement(IRawElementProviderSimple)]
struct UiaProvider {
    state: Rc<UiaState>,
    patterns: UiaPatterns,
}

#[allow(non_upper_case_globals)] // Preserve Windows SDK UIA_* identifiers verbatim.
impl UiaProvider {
    fn property(&self, property_id: UIA_PROPERTY_ID) -> Result<VARIANT> {
        self.state.apartment.ensure_owner()?;
        let metadata = self.state.metadata.read().map_err(|_| state_error())?;
        match property_id {
            UIA_ControlTypePropertyId
            | UIA_AutomationIdPropertyId
            | UIA_NamePropertyId
            | UIA_HelpTextPropertyId
            | UIA_IsEnabledPropertyId
            | UIA_IsControlElementPropertyId
            | UIA_IsContentElementPropertyId
            | UIA_IsKeyboardFocusablePropertyId
            | UIA_LiveSettingPropertyId => Ok(Self::basic_property(&metadata, property_id)),
            UIA_ValueValuePropertyId
            | UIA_ValueIsReadOnlyPropertyId
            | UIA_IsDataValidForFormPropertyId => self.value_property(&metadata, property_id),
            UIA_SelectionItemIsSelectedPropertyId
            | UIA_SelectionItemSelectionContainerPropertyId
            | UIA_ExpandCollapseExpandCollapseStatePropertyId
            | UIA_ToggleToggleStatePropertyId => self.widget_state_property(&metadata, property_id),
            UIA_RangeValueValuePropertyId
            | UIA_RangeValueMinimumPropertyId
            | UIA_RangeValueMaximumPropertyId
            | UIA_RangeValueSmallChangePropertyId
            | UIA_RangeValueLargeChangePropertyId
            | UIA_RangeValueIsReadOnlyPropertyId => Ok(self.range_property(&metadata, property_id)),
            UIA_IsValuePatternAvailablePropertyId
            | UIA_IsSelectionItemPatternAvailablePropertyId
            | UIA_IsExpandCollapsePatternAvailablePropertyId
            | UIA_IsRangeValuePatternAvailablePropertyId
            | UIA_IsTogglePatternAvailablePropertyId
            | UIA_IsInvokePatternAvailablePropertyId => Ok(pattern_availability(
                &metadata,
                property_id,
                self.state.handler.is_some(),
            )),
            UIA_NativeWindowHandlePropertyId => self.host_property(),
            _ => empty_property(),
        }
    }

    fn basic_property(metadata: &UiaMetadata, property_id: UIA_PROPERTY_ID) -> VARIANT {
        match property_id {
            UIA_ControlTypePropertyId => uia_role(metadata.kind).control_type().0.into(),
            UIA_AutomationIdPropertyId => metadata.automation_id.as_str().into(),
            UIA_NamePropertyId => metadata.name.as_str().into(),
            UIA_HelpTextPropertyId if !metadata.description.is_empty() => {
                metadata.description.as_str().into()
            }
            UIA_IsEnabledPropertyId => (!metadata.disabled).into(),
            UIA_IsControlElementPropertyId | UIA_IsContentElementPropertyId => true.into(),
            UIA_IsKeyboardFocusablePropertyId => metadata.keyboard_focusable().into(),
            UIA_LiveSettingPropertyId => uia_live_setting(metadata.kind).native().0.into(),
            _ => VARIANT::default(),
        }
    }

    fn value_property(
        &self,
        metadata: &UiaMetadata,
        property_id: UIA_PROPERTY_ID,
    ) -> Result<VARIANT> {
        match property_id {
            UIA_ValueValuePropertyId if supports(metadata, UiaPattern::Value) => {
                Ok(metadata.value.as_deref().unwrap_or_default().into())
            }
            UIA_ValueIsReadOnlyPropertyId if supports(metadata, UiaPattern::Value) => {
                Ok((self.state.handler.is_none()
                    || metadata.read_only
                    || (metadata.kind == WidgetKind::NumberInput
                        && metadata.range.is_some_and(|range| range.read_only)))
                .into())
            }
            UIA_IsDataValidForFormPropertyId
                if matches!(
                    metadata.kind,
                    WidgetKind::TextInput | WidgetKind::NumberInput
                ) =>
            {
                Ok((!metadata.invalid).into())
            }
            _ => empty_property(),
        }
    }

    fn widget_state_property(
        &self,
        metadata: &UiaMetadata,
        property_id: UIA_PROPERTY_ID,
    ) -> Result<VARIANT> {
        match property_id {
            UIA_SelectionItemIsSelectedPropertyId
                if supports(metadata, UiaPattern::SelectionItem) =>
            {
                Ok(metadata.selected.into())
            }
            UIA_SelectionItemSelectionContainerPropertyId
                if supports(metadata, UiaPattern::SelectionItem) =>
            {
                let container = self
                    .state
                    .selection_container
                    .read()
                    .map_err(|_| state_error())?
                    .clone();
                container.map_or_else(empty_property, |provider| {
                    let unknown: IUnknown = provider.cast()?;
                    Ok(unknown.into())
                })
            }
            UIA_ExpandCollapseExpandCollapseStatePropertyId
                if supports(metadata, UiaPattern::ExpandCollapse) =>
            {
                Ok(native_state(
                    metadata.expanded,
                    ExpandCollapseState_Expanded.0,
                    ExpandCollapseState_Collapsed.0,
                )
                .into())
            }
            UIA_ToggleToggleStatePropertyId if supports(metadata, UiaPattern::Toggle) => {
                Ok(native_state(metadata.toggled, ToggleState_On.0, ToggleState_Off.0).into())
            }
            _ => empty_property(),
        }
    }

    fn range_property(&self, metadata: &UiaMetadata, property_id: UIA_PROPERTY_ID) -> VARIANT {
        if !supports(metadata, UiaPattern::RangeValue) {
            return VARIANT::default();
        }
        if property_id == UIA_RangeValueIsReadOnlyPropertyId {
            self.range_read_only_property(metadata).into()
        } else {
            Self::range_value_property(metadata, property_id)
        }
    }

    fn range_value_property(metadata: &UiaMetadata, property_id: UIA_PROPERTY_ID) -> VARIANT {
        match property_id {
            UIA_RangeValueValuePropertyId => metadata
                .range
                .map_or_else(VARIANT::default, |range| range.value.into()),
            UIA_RangeValueMinimumPropertyId => metadata
                .range
                .map_or_else(VARIANT::default, |range| range.minimum.into()),
            UIA_RangeValueMaximumPropertyId => metadata
                .range
                .map_or_else(VARIANT::default, |range| range.maximum.into()),
            UIA_RangeValueSmallChangePropertyId => metadata
                .range
                .map_or_else(VARIANT::default, |range| range.small_change.into()),
            UIA_RangeValueLargeChangePropertyId => metadata
                .range
                .map_or_else(VARIANT::default, |range| range.large_change.into()),
            _ => VARIANT::default(),
        }
    }

    fn range_read_only_property(&self, metadata: &UiaMetadata) -> bool {
        self.state.handler.is_none()
            || metadata.read_only
            || metadata.range.is_some_and(|range| range.read_only)
    }

    fn host_property(&self) -> Result<VARIANT> {
        let hwnd = self.state.hwnd.read().map_err(|_| state_error())?;
        (*hwnd).map_or_else(empty_property, |hwnd| Ok((hwnd.0 as isize as i32).into()))
    }
}

#[allow(non_upper_case_globals)] // Preserve Windows SDK UIA_* identifiers verbatim.
fn pattern_availability(
    metadata: &UiaMetadata,
    property_id: UIA_PROPERTY_ID,
    handler_present: bool,
) -> VARIANT {
    let pattern = match property_id {
        UIA_IsValuePatternAvailablePropertyId => UiaPattern::Value,
        UIA_IsSelectionItemPatternAvailablePropertyId => UiaPattern::SelectionItem,
        UIA_IsExpandCollapsePatternAvailablePropertyId => UiaPattern::ExpandCollapse,
        UIA_IsRangeValuePatternAvailablePropertyId => UiaPattern::RangeValue,
        UIA_IsTogglePatternAvailablePropertyId => UiaPattern::Toggle,
        UIA_IsInvokePatternAvailablePropertyId => UiaPattern::Invoke,
        _ => return VARIANT::default(),
    };
    (supports(metadata, pattern) && (pattern != UiaPattern::Invoke || handler_present)).into()
}

fn supports(metadata: &UiaMetadata, pattern: UiaPattern) -> bool {
    uia_patterns(metadata.kind).contains(&pattern)
}

impl IRawElementProviderSimple_Impl for UiaProvider_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        // UIA routes calls and final release through the factory-owned STA.
        Ok(ProviderOptions_ServerSideProvider | ProviderOptions_UseComThreading)
    }

    fn GetPatternProvider(&self, pattern_id: UIA_PATTERN_ID) -> Result<IUnknown> {
        self.state.apartment.ensure_owner()?;
        match self
            .patterns
            .0
            .iter()
            .find(|(id, _)| *id == pattern_id)
            .map(|(_, provider)| provider.clone())
        {
            Some(provider) => Ok(provider),
            None => {
                // SAFETY: Error::empty maps to S_OK and leaves the caller's
                // zero-initialized NULL out pointer unchanged; no invalid null
                // Rust interface value is constructed.
                Err(Error::empty())
            }
        }
    }

    fn GetPropertyValue(&self, property_id: UIA_PROPERTY_ID) -> Result<VARIANT> {
        self.property(property_id)
    }

    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        self.state.apartment.ensure_owner()?;
        let hwnd = (*self.state.hwnd.read().map_err(|_| state_error())?)
            .ok_or_else(|| uia_error(UIA_E_ELEMENTNOTAVAILABLE))?;
        // SAFETY: the host HWND belongs to the owning GPUI window and outlives this call.
        unsafe { UiaHostProviderFromHwnd(hwnd) }
    }
}

#[derive(Clone)]
pub struct UiaProviderHandle {
    object: ComObject<UiaProvider>,
}

impl UiaProviderHandle {
    pub fn raw_provider(&self) -> IRawElementProviderSimple {
        self.object.to_interface()
    }

    pub fn metadata(&self) -> Result<UiaMetadata> {
        self.object.state.snapshot()
    }

    /// Updates authoritative GPUI state without invoking the UIA action handler.
    pub fn set_metadata(&self, metadata: UiaMetadata) -> Result<()> {
        self.object.state.apartment.ensure_owner()?;
        let mut current = self
            .object
            .state
            .metadata
            .write()
            .map_err(|_| state_error())?;
        if current.kind != metadata.kind {
            return Err(Error::new(E_INVALIDARG, "UIA widget kind cannot change"));
        }
        *current = metadata;
        Ok(())
    }

    pub fn set_hwnd(&self, hwnd: HWND) -> Result<()> {
        self.object.state.apartment.ensure_owner()?;
        *self.object.state.hwnd.write().map_err(|_| state_error())? = Some(hwnd);
        Ok(())
    }

    pub fn set_selection_container(
        &self,
        container: Option<IRawElementProviderSimple>,
    ) -> Result<()> {
        self.object.state.apartment.ensure_owner()?;
        *self
            .object
            .state
            .selection_container
            .write()
            .map_err(|_| state_error())? = container;
        Ok(())
    }

    pub fn return_raw_element_provider(
        &self,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Result<windows::Win32::Foundation::LRESULT> {
        self.object.state.apartment.ensure_owner()?;
        let hwnd = (*self.object.state.hwnd.read().map_err(|_| state_error())?)
            .ok_or_else(|| uia_error(UIA_E_ELEMENTNOTAVAILABLE))?;
        // SAFETY: this is the WM_GETOBJECT response for the owning HWND.
        Ok(unsafe { UiaReturnRawElementProvider(hwnd, wparam, lparam, &self.raw_provider()) })
    }

    pub fn announce(&self) -> Result<()> {
        announce_metadata(&self.raw_provider(), self.object.state.snapshot()?)
    }
}

pub fn announce_metadata(
    provider: &IRawElementProviderSimple,
    metadata: UiaMetadata,
) -> Result<()> {
    match metadata.kind {
        WidgetKind::Caption => {
            // SAFETY: the provider is alive for the duration of the event call.
            unsafe { UiaRaiseAutomationEvent(provider, UIA_LiveRegionChangedEventId) }
        }
        WidgetKind::Status | WidgetKind::Alert => {
            let text = BSTR::from(metadata.value.unwrap_or(metadata.name));
            let activity = BSTR::from(metadata.automation_id);
            let (kind, processing) = if metadata.kind == WidgetKind::Alert {
                (NotificationKind_Other, NotificationProcessing_ImportantAll)
            } else {
                (
                    NotificationKind_ActionCompleted,
                    NotificationProcessing_ImportantMostRecent,
                )
            };
            // SAFETY: both BSTRs and the provider outlive the event call.
            unsafe { UiaRaiseNotificationEvent(provider, kind, processing, &text, &activity) }
        }
        _ => Ok(()),
    }
}

/// Creates providers on the calling thread's STA; call from the owning GPUI UI thread.
pub struct UiaProviderFactory;

impl UiaProviderFactory {
    pub fn create(metadata: UiaMetadata) -> Result<UiaProviderHandle> {
        Self::build(metadata, None)
    }

    pub fn create_with_handler<F>(metadata: UiaMetadata, handler: F) -> Result<UiaProviderHandle>
    where
        F: Fn(UiaAction) -> Result<()> + 'static,
    {
        Self::build(metadata, Some(Rc::new(handler)))
    }

    fn build(
        metadata: UiaMetadata,
        handler: Option<UiaActionHandler>,
    ) -> Result<UiaProviderHandle> {
        let apartment = StaApartment::initialize()?;
        let kind = metadata.kind;
        let state = Rc::new(UiaState::new(metadata, handler, apartment));
        let patterns = UiaPatterns::new(kind, &state);
        Ok(UiaProviderHandle {
            object: ComObject::new(UiaProvider { state, patterns }),
        })
    }
}

fn number_input_value(metadata: &UiaMetadata, text: &str) -> Result<f64> {
    let range = required_option(metadata.range, UIA_E_NOTSUPPORTED)?;
    if range.read_only {
        return Err(uia_error(UIA_E_INVALIDOPERATION));
    }
    let value = text
        .parse::<f64>()
        .map_err(|_| Error::new(E_INVALIDARG, "NumberInput requires a numeric value"))?;
    validate_range_value(&range, value)?;
    Ok(value)
}

fn validate_range_value(range: &UiaRange, value: f64) -> Result<()> {
    if !value.is_finite() || value < range.minimum || value > range.maximum {
        Err(Error::new(E_INVALIDARG, "UIA range value is out of bounds"))
    } else {
        Ok(())
    }
}

fn required_option<T>(value: Option<T>, error: u32) -> Result<T> {
    value.ok_or_else(|| uia_error(error))
}

fn native_state<T>(value: bool, on: T, off: T) -> T {
    if value { on } else { off }
}

fn empty_property() -> Result<VARIANT> {
    Ok(VARIANT::default())
}

fn state_error() -> Error {
    Error::new(E_FAIL, "UIA provider state is unavailable")
}

fn uia_error(code: u32) -> Error {
    Error::from_hresult(HRESULT(code as i32))
}
