use crate::accessibility::*;
use std::sync::{Arc, Mutex};
use windows::Win32::System::Variant::{VT_BSTR, VT_EMPTY};
use windows::Win32::UI::Accessibility::*;
use windows_core::{HRESULT, Interface, w};

#[test]
fn caption_is_a_polite_text_live_region() {
    assert_eq!(uia_role(WidgetKind::Caption), UiaRole::Text);
    assert_eq!(
        uia_live_setting(WidgetKind::Caption),
        UiaLiveSetting::Polite
    );
    assert_eq!(uia_live_setting(WidgetKind::Status), UiaLiveSetting::Polite);
    assert_eq!(
        uia_live_setting(WidgetKind::Alert),
        UiaLiveSetting::Assertive
    );
    assert_eq!(uia_live_setting(WidgetKind::Button), UiaLiveSetting::Off);
}

#[test]
fn widget_roles_and_patterns_cover_the_settings_surface() {
    let cases = [
        (WidgetKind::Button, UiaRole::Button),
        (WidgetKind::Toggle, UiaRole::CheckBox),
        (WidgetKind::Slider, UiaRole::Slider),
        (WidgetKind::NumberInput, UiaRole::Spinner),
        (WidgetKind::TextInput, UiaRole::Edit),
        (WidgetKind::Picker, UiaRole::ComboBox),
        (WidgetKind::PickerOption, UiaRole::ListItem),
        (WidgetKind::Progress, UiaRole::ProgressBar),
        (WidgetKind::Tooltip, UiaRole::ToolTip),
        (WidgetKind::ScrollArea, UiaRole::Pane),
        (WidgetKind::List, UiaRole::List),
    ];
    for (kind, role) in cases {
        assert_eq!(uia_role(kind), role, "wrong role for {kind:?}");
    }
    assert!(uia_patterns(WidgetKind::Toggle).contains(&UiaPattern::Toggle));
    assert!(uia_patterns(WidgetKind::TextInput).contains(&UiaPattern::Value));
    assert!(uia_patterns(WidgetKind::Picker).contains(&UiaPattern::ExpandCollapse));
    assert!(uia_patterns(WidgetKind::PickerOption).contains(&UiaPattern::SelectionItem));
    assert!(uia_patterns(WidgetKind::Progress).contains(&UiaPattern::RangeValue));
    assert!(uia_patterns(WidgetKind::Caption).is_empty());
    assert!(uia_patterns(WidgetKind::List).is_empty());
    assert_eq!(
        uia_role(WidgetKind::List).control_type(),
        UIA_ListControlTypeId
    );
}

#[test]
fn provider_exposes_native_role_name_value_and_live_properties() {
    let provider = UiaProviderFactory::create(UiaMetadata {
        automation_id: "caption".into(),
        name: "Hello world".into(),
        ..UiaMetadata::new(WidgetKind::Caption)
    })
    .unwrap();
    let raw = provider.raw_provider();
    unsafe {
        let role = raw.GetPropertyValue(UIA_ControlTypePropertyId).unwrap();
        assert_eq!(i32::try_from(&role).unwrap(), UIA_TextControlTypeId.0);

        let name = raw.GetPropertyValue(UIA_NamePropertyId).unwrap();
        assert_eq!(name.vt(), VT_BSTR);
        let name = &name.Anonymous.Anonymous.Anonymous.bstrVal;
        assert_eq!(name.to_string(), "Hello world");

        let automation_id = raw.GetPropertyValue(UIA_AutomationIdPropertyId).unwrap();
        let automation_id = &automation_id.Anonymous.Anonymous.Anonymous.bstrVal;
        assert_eq!(automation_id.to_string(), "caption");

        let live = raw.GetPropertyValue(UIA_LiveSettingPropertyId).unwrap();
        assert_eq!(i32::try_from(&live).unwrap(), Polite.0);

        let unsupported = raw.GetPropertyValue(UIA_ValueValuePropertyId).unwrap();
        assert_eq!(unsupported.vt(), VT_EMPTY);
    }
}

#[test]
fn provider_exposes_selection_expansion_invalid_progress_and_toggle_state() {
    let option = UiaProviderFactory::create(UiaMetadata {
        selected: true,
        ..UiaMetadata::new(WidgetKind::PickerOption)
    })
    .unwrap();
    let picker = UiaProviderFactory::create(UiaMetadata {
        expanded: true,
        ..UiaMetadata::new(WidgetKind::Picker)
    })
    .unwrap();
    let input = UiaProviderFactory::create(UiaMetadata {
        invalid: true,
        value: Some("red".into()),
        ..UiaMetadata::new(WidgetKind::TextInput)
    })
    .unwrap();
    let progress = UiaProviderFactory::create(UiaMetadata {
        range: Some(UiaRange {
            value: 42.0,
            minimum: 0.0,
            maximum: 100.0,
            small_change: 1.0,
            large_change: 10.0,
            read_only: true,
        }),
        ..UiaMetadata::new(WidgetKind::Progress)
    })
    .unwrap();
    let toggle = UiaProviderFactory::create(UiaMetadata {
        toggled: true,
        ..UiaMetadata::new(WidgetKind::Toggle)
    })
    .unwrap();

    unsafe {
        let selected = option
            .raw_provider()
            .GetPropertyValue(UIA_SelectionItemIsSelectedPropertyId)
            .unwrap();
        assert!(bool::try_from(&selected).unwrap());
        let expanded = picker
            .raw_provider()
            .GetPropertyValue(UIA_ExpandCollapseExpandCollapseStatePropertyId)
            .unwrap();
        assert_eq!(
            i32::try_from(&expanded).unwrap(),
            ExpandCollapseState_Expanded.0
        );
        let valid = input
            .raw_provider()
            .GetPropertyValue(UIA_IsDataValidForFormPropertyId)
            .unwrap();
        assert!(!bool::try_from(&valid).unwrap());
        let value = input
            .raw_provider()
            .GetPropertyValue(UIA_ValueValuePropertyId)
            .unwrap();
        assert_eq!(
            value.Anonymous.Anonymous.Anonymous.bstrVal.to_string(),
            "red"
        );
        let value = progress
            .raw_provider()
            .GetPropertyValue(UIA_RangeValueValuePropertyId)
            .unwrap();
        assert_eq!(f64::try_from(&value).unwrap(), 42.0);
        let toggled = toggle
            .raw_provider()
            .GetPropertyValue(UIA_ToggleToggleStatePropertyId)
            .unwrap();
        assert_eq!(i32::try_from(&toggled).unwrap(), ToggleState_On.0);
    }
}

#[test]
fn provider_patterns_have_real_native_interfaces() {
    let picker = UiaProviderFactory::create(UiaMetadata::new(WidgetKind::Picker)).unwrap();
    let option = UiaProviderFactory::create(UiaMetadata::new(WidgetKind::PickerOption)).unwrap();
    let slider = UiaProviderFactory::create(UiaMetadata {
        range: Some(UiaRange {
            value: 5.0,
            minimum: 0.0,
            maximum: 10.0,
            small_change: 1.0,
            large_change: 2.0,
            read_only: false,
        }),
        ..UiaMetadata::new(WidgetKind::Slider)
    })
    .unwrap();
    unsafe {
        let expand = picker
            .raw_provider()
            .GetPatternProvider(UIA_ExpandCollapsePatternId)
            .unwrap();
        assert!(!expand.as_raw().is_null());
        let selection = option
            .raw_provider()
            .GetPatternProvider(UIA_SelectionItemPatternId)
            .unwrap();
        assert!(!selection.as_raw().is_null());
        let range = slider
            .raw_provider()
            .GetPatternProvider(UIA_RangeValuePatternId)
            .unwrap();
        assert!(!range.as_raw().is_null());
    }
}

#[test]
fn native_pattern_actions_reach_the_handler_and_update_metadata() {
    let actions = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&actions);
    let provider = UiaProviderFactory::create_with_handler(
        UiaMetadata::new(WidgetKind::Picker),
        move |action| {
            recorded.lock().unwrap().push(action);
            Ok(())
        },
    )
    .unwrap();
    unsafe {
        let unknown = provider
            .raw_provider()
            .GetPatternProvider(UIA_ExpandCollapsePatternId)
            .unwrap();
        let pattern = unknown.cast::<IExpandCollapseProvider>().unwrap();
        pattern.Expand().unwrap();
    }
    assert!(provider.metadata().unwrap().expanded);
    assert_eq!(
        actions.lock().unwrap().as_slice(),
        &[UiaAction::Expanded(true)]
    );
}

fn test_provider(metadata: UiaMetadata) -> UiaProviderHandle {
    UiaProviderFactory::create_with_handler(metadata, |_| Ok(())).unwrap()
}

fn test_pattern<T: Interface>(provider: &UiaProviderHandle, id: UIA_PATTERN_ID) -> T {
    unsafe {
        provider
            .raw_provider()
            .GetPatternProvider(id)
            .unwrap()
            .cast::<T>()
            .unwrap()
    }
}

fn test_range(value: f64) -> UiaRange {
    UiaRange {
        value,
        minimum: 0.0,
        maximum: 10.0,
        small_change: 0.5,
        large_change: 1.0,
        read_only: false,
    }
}

#[test]
fn provider_owns_an_sta_and_requests_com_threading() {
    let provider = UiaProviderFactory::create(UiaMetadata::new(WidgetKind::Caption)).unwrap();
    let raw = provider.raw_provider();
    let options = unsafe { raw.ProviderOptions() }.unwrap();
    assert!(options.contains(ProviderOptions_ServerSideProvider));
    assert!(options.contains(ProviderOptions_UseComThreading));
}

#[test]
fn unsupported_pattern_returns_success_with_a_null_provider() {
    let provider = UiaProviderFactory::create(UiaMetadata::new(WidgetKind::Caption)).unwrap();
    let result = unsafe {
        provider
            .raw_provider()
            .GetPatternProvider(UIA_InvokePatternId)
    };
    let error = result.expect_err("a NULL provider is returned through the S_OK path");
    assert_eq!(error.code(), HRESULT(0));
}

#[test]
fn native_value_action_roundtrips() {
    let provider = test_provider(UiaMetadata::new(WidgetKind::TextInput));
    let value = test_pattern::<IValueProvider>(&provider, UIA_ValuePatternId);
    unsafe { value.SetValue(w!("hello")) }.unwrap();
    assert_eq!(provider.metadata().unwrap().value.as_deref(), Some("hello"));
}

#[test]
fn number_input_value_and_range_actions_stay_canonical() {
    let provider = test_provider(UiaMetadata {
        value: Some("5".into()),
        range: Some(test_range(5.0)),
        ..UiaMetadata::new(WidgetKind::NumberInput)
    });
    let value = test_pattern::<IValueProvider>(&provider, UIA_ValuePatternId);
    let range = test_pattern::<IRangeValueProvider>(&provider, UIA_RangeValuePatternId);

    unsafe { value.SetValue(w!("7.5")) }.unwrap();
    let metadata = provider.metadata().unwrap();
    assert_eq!(metadata.value.as_deref(), Some("7.5"));
    assert_eq!(metadata.range.unwrap().value, 7.5);

    unsafe { range.SetValue(8.0) }.unwrap();
    let metadata = provider.metadata().unwrap();
    assert_eq!(metadata.value.as_deref(), Some("8"));
    assert_eq!(metadata.range.unwrap().value, 8.0);
}

#[test]
fn native_selection_and_toggle_actions_roundtrip() {
    let option = test_provider(UiaMetadata::new(WidgetKind::PickerOption));
    let selection = test_pattern::<ISelectionItemProvider>(&option, UIA_SelectionItemPatternId);
    unsafe { selection.Select() }.unwrap();
    assert!(option.metadata().unwrap().selected);
    unsafe { selection.RemoveFromSelection() }.unwrap();
    assert!(!option.metadata().unwrap().selected);

    let toggle = test_provider(UiaMetadata::new(WidgetKind::Toggle));
    let pattern = test_pattern::<IToggleProvider>(&toggle, UIA_TogglePatternId);
    unsafe { pattern.Toggle() }.unwrap();
    assert!(toggle.metadata().unwrap().toggled);
    assert_eq!(unsafe { pattern.ToggleState() }.unwrap(), ToggleState_On);
}

#[test]
fn caption_status_and_alert_announce_through_native_uia() {
    for kind in [WidgetKind::Caption, WidgetKind::Status, WidgetKind::Alert] {
        let provider = UiaProviderFactory::create(UiaMetadata {
            automation_id: "announcement".into(),
            name: format!("{kind:?} text"),
            ..UiaMetadata::new(kind)
        })
        .unwrap();
        provider.announce().unwrap();
    }
}
