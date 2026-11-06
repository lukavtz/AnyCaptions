#requires -Version 5.1
<#
Queries the GPUI overlay and settings through Windows UI Automation, then runs
keyboard-only settings, picker, validation, and focus-return scenarios.

The production overlay uses WS_EX_NOACTIVATE. Keyboard scenarios report SKIP
and UIA patterns drive provider state instead. Pattern failures remain FAIL;
they never get reclassified as platform limitations.

  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-accessibility.ps1
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-accessibility.ps1 -Exe path\to\anycaptions.exe
#>
[CmdletBinding()]
param(
    [string]$Exe,
    [Alias('Args')]
    [string]$Arguments = '',
    [int]$TimeoutSeconds = 15
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms

$root = Split-Path -Parent $PSScriptRoot
if (-not $Exe) { $Exe = Join-Path $root 'target\debug\anycaptions.exe' }
$Exe = [IO.Path]::GetFullPath($Exe)
if (-not (Test-Path -LiteralPath $Exe)) {
    Write-Output "FAIL exe missing $Exe"
    Write-Output 'RESULT: FAIL'
    exit 1
}
if ($TimeoutSeconds -lt 1) { throw 'TimeoutSeconds must be positive' }

if (-not ('Win32.Accessibility' -as [type])) {
Add-Type -Namespace Win32 -Name Accessibility -MemberDefinition @'
public const int GWL_EXSTYLE = -20;
public const int WS_EX_NOACTIVATE = 0x08000000;

[DllImport("user32.dll")]
public static extern bool SetForegroundWindow(IntPtr hWnd);

[DllImport("user32.dll")]
public static extern IntPtr GetWindowLongPtr(IntPtr hWnd, int nIndex);

[DllImport("user32.dll", CharSet = CharSet.Unicode)]
public static extern bool PostMessageW(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);
'@
}

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public enum UiaLiveSetting
{
    Off = 0,
    Polite = 1,
    Assertive = 2
}

public enum UiaTreeScope
{
    Descendants = 4
}

[StructLayout(LayoutKind.Sequential)]
public struct UiaPoint
{
    public int X;
    public int Y;
}

[ComImport, Guid("352ffba8-0973-437c-a61f-f64cafd81df9"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IUIAutomationCondition
{
}

// SDK-ordered interface prefixes. COM owns every pointer; no manual vtable access.
[ComImport, Guid("d22108aa-8ac5-49a5-837b-37bbb3d7591e"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IUIAutomationElement
{
    [PreserveSig] int SetFocus();
    [PreserveSig] int GetRuntimeId(out IntPtr runtimeId);
    [PreserveSig] int FindFirst(UiaTreeScope scope, IUIAutomationCondition condition, out IUIAutomationElement found);
    [PreserveSig] int FindAll(UiaTreeScope scope, IUIAutomationCondition condition, out IntPtr found);
    [PreserveSig] int FindFirstBuildCache(UiaTreeScope scope, IUIAutomationCondition condition, IntPtr cacheRequest, out IUIAutomationElement found);
    [PreserveSig] int FindAllBuildCache(UiaTreeScope scope, IUIAutomationCondition condition, IntPtr cacheRequest, out IntPtr found);
    [PreserveSig] int BuildUpdatedCache(IntPtr cacheRequest, out IUIAutomationElement updated);
    [PreserveSig] int GetCurrentPropertyValue(int propertyId, out object value);
    [PreserveSig] int GetCurrentPropertyValueEx(int propertyId, [MarshalAs(UnmanagedType.Bool)] bool ignoreDefaultValue, out object value);
}

[ComImport, Guid("30cbe57d-d9d0-452a-ab13-7ac5ac4825ee"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IUIAutomation
{
    [PreserveSig] int CompareElements(IUIAutomationElement first, IUIAutomationElement second, [MarshalAs(UnmanagedType.Bool)] out bool areSame);
    [PreserveSig] int CompareRuntimeIds(IntPtr first, IntPtr second, [MarshalAs(UnmanagedType.Bool)] out bool areSame);
    [PreserveSig] int GetRootElement(out IUIAutomationElement root);
    [PreserveSig] int ElementFromHandle(IntPtr hwnd, out IUIAutomationElement element);
    [PreserveSig] int ElementFromPoint(UiaPoint point, out IUIAutomationElement element);
    [PreserveSig] int GetFocusedElement(out IUIAutomationElement element);
    [PreserveSig] int GetRootElementBuildCache(IntPtr cacheRequest, out IUIAutomationElement root);
    [PreserveSig] int ElementFromHandleBuildCache(IntPtr hwnd, IntPtr cacheRequest, out IUIAutomationElement element);
    [PreserveSig] int ElementFromPointBuildCache(UiaPoint point, IntPtr cacheRequest, out IUIAutomationElement element);
    [PreserveSig] int GetFocusedElementBuildCache(IntPtr cacheRequest, out IUIAutomationElement element);
    [PreserveSig] int CreateTreeWalker(IUIAutomationCondition condition, out IntPtr walker);
    [PreserveSig] int get_ControlViewWalker(out IntPtr walker);
    [PreserveSig] int get_ContentViewWalker(out IntPtr walker);
    [PreserveSig] int get_RawViewWalker(out IntPtr walker);
    [PreserveSig] int get_RawViewCondition(out IUIAutomationCondition condition);
    [PreserveSig] int get_ControlViewCondition(out IUIAutomationCondition condition);
    [PreserveSig] int get_ContentViewCondition(out IUIAutomationCondition condition);
    [PreserveSig] int CreateCacheRequest(out IntPtr cacheRequest);
    [PreserveSig] int CreateTrueCondition(out IUIAutomationCondition condition);
    [PreserveSig] int CreateFalseCondition(out IUIAutomationCondition condition);
    [PreserveSig] int CreatePropertyCondition(int propertyId, object value, out IUIAutomationCondition condition);
    [PreserveSig] int CreatePropertyConditionEx(int propertyId, object value, int flags, out IUIAutomationCondition condition);
    [PreserveSig] int CreateAndCondition(IUIAutomationCondition first, IUIAutomationCondition second, out IUIAutomationCondition condition);
}

[ComImport, Guid("ff48dba4-60ef-4201-aa87-54103eef594e")]
public class CUIAutomation
{
}

public static class NativeUia
{
    public const int LiveSettingPropertyId = 30135;
    public const int IsDataValidForFormPropertyId = 30103;
    private const int ProcessIdPropertyId = 30002;
    private const int AutomationIdPropertyId = 30011;

    public static object GetCurrentPropertyValue(int processId, string automationId, int propertyId)
    {
        IUIAutomation automation = null;
        IUIAutomationElement root = null;
        IUIAutomationElement element = null;
        IUIAutomationCondition processCondition = null;
        IUIAutomationCondition idCondition = null;
        IUIAutomationCondition condition = null;
        try
        {
            automation = (IUIAutomation)new CUIAutomation();
            Check(automation.GetRootElement(out root));
            Check(automation.CreatePropertyCondition(ProcessIdPropertyId, processId, out processCondition));
            Check(automation.CreatePropertyCondition(AutomationIdPropertyId, automationId, out idCondition));
            Check(automation.CreateAndCondition(processCondition, idCondition, out condition));
            Check(root.FindFirst(UiaTreeScope.Descendants, condition, out element));
            if (element == null)
            {
                throw new InvalidOperationException("UIA element '" + automationId + "' was not found");
            }

            object value;
            Check(element.GetCurrentPropertyValueEx(propertyId, true, out value));
            if (value == null || value is ErrorWrapper || Marshal.IsComObject(value))
            {
                throw new NotSupportedException("UIA property " + propertyId + " is not supported by '" + automationId + "'");
            }
            return value;
        }
        finally
        {
            Release(condition);
            Release(idCondition);
            Release(processCondition);
            Release(element);
            Release(root);
            Release(automation);
        }
    }

    private static void Check(int hr)
    {
        if (hr < 0) { Marshal.ThrowExceptionForHR(hr); }
    }

    private static void Release(object comObject)
    {
        if (comObject != null && Marshal.IsComObject(comObject))
        {
            Marshal.FinalReleaseComObject(comObject);
        }
    }
}
'@

$automation = [System.Windows.Automation.AutomationElement]
$tree = [System.Windows.Automation.TreeScope]
$control = [System.Windows.Automation.ControlType]
$script:failed = $false
$script:skipped = $false
$script:process = $null
$script:processId = 0

function Assert-Accessibility {
    param([string]$Name, [bool]$Ok, [string]$Actual, [string]$Expected)
    if ($Ok) {
        Write-Output "PASS $Name"
    } else {
        $script:failed = $true
        Write-Output "FAIL $Name $Actual vs expected $Expected"
    }
}

function Get-UiaProperty {
    param([System.Windows.Automation.AutomationElement]$Element, [int]$Id)
    if ($Id -eq [NativeUia]::LiveSettingPropertyId -or
        $Id -eq [NativeUia]::IsDataValidForFormPropertyId -or
        $Id -eq [System.Windows.Automation.AutomationElement]::IsEnabledProperty.Id) {
        return [NativeUia]::GetCurrentPropertyValue($script:processId, $Element.Current.AutomationId, $Id)
    }

    $property = [System.Windows.Automation.AutomationProperty]::LookupById($Id)
    if ($Id -eq [System.Windows.Automation.ExpandCollapsePattern]::ExpandCollapseStateProperty.Id) {
        $property = [System.Windows.Automation.ExpandCollapsePattern]::ExpandCollapseStateProperty
    } elseif ($Id -eq [System.Windows.Automation.SelectionItemPattern]::IsSelectedProperty.Id) {
        $property = [System.Windows.Automation.SelectionItemPattern]::IsSelectedProperty
    }
    if ($null -eq $property) { throw "Unsupported managed UIA property $Id" }
    return $Element.GetCurrentPropertyValue($property, $true)
}

function Find-AppElement {
    param([string]$AutomationId)
    $processCondition = [System.Windows.Automation.PropertyCondition]::new(
        $automation::ProcessIdProperty,
        $script:processId
    )
    $idCondition = [System.Windows.Automation.PropertyCondition]::new(
        $automation::AutomationIdProperty,
        $AutomationId
    )
    $condition = [System.Windows.Automation.AndCondition]::new($processCondition, $idCondition)
    return $automation::RootElement.FindFirst($tree::Descendants, $condition)
}

function Wait-AppElement {
    param([string]$AutomationId, [switch]$Gone)
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $element = Find-AppElement $AutomationId
        if (($Gone -and $null -eq $element) -or (-not $Gone -and $null -ne $element)) {
            return $element
        }
        if ($script:process.HasExited) { return $null }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    return $null
}

function Require-AppElement {
    param([string]$AutomationId)
    $element = Wait-AppElement $AutomationId
    if ($null -eq $element) { throw "UIA element '$AutomationId' was not found" }
    return $element
}

function Set-UiaFocus {
    param([System.Windows.Automation.AutomationElement]$Element)
    $Element.SetFocus()
    Start-Sleep -Milliseconds 150
}

function Send-UiaKeys {
    param([string]$Keys)
    [System.Windows.Forms.SendKeys]::SendWait($Keys)
    Start-Sleep -Milliseconds 200
}

try {
    $start = @{ FilePath = $Exe; PassThru = $true }
    if (-not [string]::IsNullOrWhiteSpace($Arguments)) { $start.ArgumentList = $Arguments }
    $script:process = Start-Process @start
    $script:processId = $script:process.Id

    $caption = Require-AppElement 'caption-live'
    Assert-Accessibility 'caption control type' `
        ($caption.Current.ControlType.Id -eq $control::Text.Id) `
        $caption.Current.ControlType.ProgrammaticName $control::Text.ProgrammaticName
    $liveSetting = [UiaLiveSetting](Get-UiaProperty $caption ([NativeUia]::LiveSettingPropertyId))
    Assert-Accessibility 'caption polite live region' `
        ($liveSetting -eq [UiaLiveSetting]::Polite) "$liveSetting" 'Polite'

    $toggle = Require-AppElement 'settings-toggle'
    Assert-Accessibility 'settings button name' `
        (-not [string]::IsNullOrWhiteSpace($toggle.Current.Name)) $toggle.Current.Name 'non-empty localized name'
    Assert-Accessibility 'settings button control type' `
        ($toggle.Current.ControlType.Id -eq $control::Button.Id) `
        $toggle.Current.ControlType.ProgrammaticName $control::Button.ProgrammaticName

    $windowHandle = [IntPtr]$toggle.Current.NativeWindowHandle
    if ($windowHandle -eq [IntPtr]::Zero) {
        $processCondition = [System.Windows.Automation.PropertyCondition]::new(
            $automation::ProcessIdProperty,
            $script:processId
        )
        $window = $automation::RootElement.FindFirst($tree::Children, $processCondition)
        if ($null -ne $window) { $windowHandle = [IntPtr]$window.Current.NativeWindowHandle }
    }
    if ($windowHandle -eq [IntPtr]::Zero) {
        $script:process.Refresh()
        $windowHandle = $script:process.MainWindowHandle
    }
    $extendedStyle = 0
    if ($windowHandle -ne [IntPtr]::Zero) {
        $extendedStyle = ([Win32.Accessibility]::GetWindowLongPtr($windowHandle, [Win32.Accessibility]::GWL_EXSTYLE)).ToInt64()
    }
    $noActivate = ($extendedStyle -band [Win32.Accessibility]::WS_EX_NOACTIVATE) -ne 0
    if ($noActivate) {
        $script:skipped = $true
        Write-Output 'SKIP keyboard scenarios: WS_EX_NOACTIVATE prevents foreground keyboard delivery'
        $invokePattern = $toggle.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
        $invokePattern.Invoke()
        $settings = Require-AppElement 'settings'
        Assert-Accessibility 'settings opens through UIA invoke' ($null -ne $settings) 'present' 'present'

        $picker = Require-AppElement 'source-language'
        Assert-Accessibility 'language picker control type' `
            ($picker.Current.ControlType.Id -eq $control::ComboBox.Id) `
            $picker.Current.ControlType.ProgrammaticName $control::ComboBox.ProgrammaticName
        $readyDeadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
        do {
            $picker = Require-AppElement 'source-language'
            if ([bool](Get-UiaProperty $picker ([System.Windows.Automation.AutomationElement]::IsEnabledProperty.Id))) { break }
            Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $readyDeadline)
        Assert-Accessibility 'language picker becomes enabled' ([bool](Get-UiaProperty $picker ([System.Windows.Automation.AutomationElement]::IsEnabledProperty.Id))) "$($picker.Current.IsEnabled)" 'True'
        $expandCollapse = $picker.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern)
        $expandCollapse.Expand()
        $menu = Require-AppElement 'source-language-menu'
        $expandedProperty = [System.Windows.Automation.ExpandCollapsePattern]::ExpandCollapseStateProperty
        $expanded = [System.Windows.Automation.ExpandCollapseState](Get-UiaProperty $picker $expandedProperty.Id)
        Assert-Accessibility 'picker expanded state' `
            ($expanded -eq [System.Windows.Automation.ExpandCollapseState]::Expanded) "$expanded" 'Expanded'
        Assert-Accessibility 'picker menu control type' `
            ($menu.Current.ControlType.Id -eq $control::List.Id) `
            $menu.Current.ControlType.ProgrammaticName $control::List.ProgrammaticName

        $items = $menu.FindAll($tree::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
        $selected = 0
        $selectedProperty = [System.Windows.Automation.SelectionItemPattern]::IsSelectedProperty
        foreach ($item in $items) {
            if ($item.Current.ControlType.Id -eq $control::ListItem.Id -and [bool](Get-UiaProperty $item $selectedProperty.Id)) {
                $selected++
            }
        }
        Assert-Accessibility 'picker exposes selected option' ($selected -eq 1) "$selected selected" 'exactly one selected option'

        $expandCollapse.Collapse()
        [void](Wait-AppElement 'source-language-menu' -Gone)
        $menuAfterCollapse = Find-AppElement 'source-language-menu'
        $settingsAfterCollapse = Find-AppElement 'settings'
        Assert-Accessibility 'picker collapses through UIA pattern' ($null -eq $menuAfterCollapse) 'menu present' 'menu closed'
        Assert-Accessibility 'picker collapse preserves settings' ($null -ne $settingsAfterCollapse) 'settings present' 'settings present'

        $color = Require-AppElement 'caption-color'
        Assert-Accessibility 'color input control type' `
            ($color.Current.ControlType.Id -eq $control::Edit.Id) `
            $color.Current.ControlType.ProgrammaticName $control::Edit.ProgrammaticName
        $valuePattern = $color.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
        $valuePattern.SetValue('red')
        $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
        do {
            $valid = [bool](Get-UiaProperty $color ([NativeUia]::IsDataValidForFormPropertyId))
            if (-not $valid) { break }
            Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $deadline)
        Assert-Accessibility 'invalid color exposed through UIA' (-not $valid) "$valid" 'False'
    } else {
        if ($windowHandle -ne [IntPtr]::Zero -and
            -not [Win32.Accessibility]::SetForegroundWindow($windowHandle)) {
            throw 'Could not activate the application window for keyboard input'
        }
        Set-UiaFocus $toggle
        Send-UiaKeys ' '
        $settings = Require-AppElement 'settings'
        Assert-Accessibility 'settings opens from keyboard' ($null -ne $settings) 'present' 'present'

        $focusedBefore = $automation::FocusedElement.Current.AutomationId
        Send-UiaKeys '{TAB}'
        $focusedAfter = $automation::FocusedElement.Current.AutomationId
        Assert-Accessibility 'Tab moves settings focus' `
            ($focusedAfter -ne $focusedBefore -and -not [string]::IsNullOrWhiteSpace($focusedAfter)) `
            "$focusedBefore -> $focusedAfter" 'a different focusable control'

        $picker = Require-AppElement 'source-language'
        Assert-Accessibility 'language picker control type' `
            ($picker.Current.ControlType.Id -eq $control::ComboBox.Id) `
            $picker.Current.ControlType.ProgrammaticName $control::ComboBox.ProgrammaticName
        Write-Host "PICKER_ENABLED=$($picker.Current.IsEnabled)"
        Set-UiaFocus $picker
        Send-UiaKeys ' '
        $menu = Require-AppElement 'source-language-menu'
        $expandedProperty = [System.Windows.Automation.ExpandCollapsePattern]::ExpandCollapseStateProperty
        $expanded = [System.Windows.Automation.ExpandCollapseState](Get-UiaProperty $picker $expandedProperty.Id)
        Assert-Accessibility 'picker expanded state' `
            ($expanded -eq [System.Windows.Automation.ExpandCollapseState]::Expanded) "$expanded" 'Expanded'
        Assert-Accessibility 'picker menu control type' `
            ($menu.Current.ControlType.Id -eq $control::List.Id) `
            $menu.Current.ControlType.ProgrammaticName $control::List.ProgrammaticName

        Send-UiaKeys '{DOWN}'
        $items = $menu.FindAll($tree::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
        $selected = 0
        $selectedProperty = [System.Windows.Automation.SelectionItemPattern]::IsSelectedProperty
        foreach ($item in $items) {
            if ($item.Current.ControlType.Id -eq $control::ListItem.Id -and [bool](Get-UiaProperty $item $selectedProperty.Id)) {
                $selected++
            }
        }
        Assert-Accessibility 'picker exposes selected option' ($selected -eq 1) "$selected selected" 'exactly one selected option'

        Send-UiaKeys '{ESC}'
        [void](Wait-AppElement 'source-language-menu' -Gone)
        $menuAfterEscape = Find-AppElement 'source-language-menu'
        $settingsAfterPickerEscape = Find-AppElement 'settings'
        Assert-Accessibility 'Escape closes picker first' ($null -eq $menuAfterEscape) 'menu present' 'menu closed'
        Assert-Accessibility 'picker Escape preserves settings' ($null -ne $settingsAfterPickerEscape) 'settings present' 'settings present'

        $color = Require-AppElement 'caption-color'
        Assert-Accessibility 'color input control type' `
            ($color.Current.ControlType.Id -eq $control::Edit.Id) `
            $color.Current.ControlType.ProgrammaticName $control::Edit.ProgrammaticName
        Set-UiaFocus $color
        Send-UiaKeys '^a'
        Send-UiaKeys 'red'
        $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
        do {
            $valid = [bool](Get-UiaProperty $color ([NativeUia]::IsDataValidForFormPropertyId))
            if (-not $valid) { break }
            Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $deadline)
        Assert-Accessibility 'invalid color exposed through UIA' (-not $valid) "$valid" 'False'

        Send-UiaKeys '{ESC}'
        [void](Wait-AppElement 'settings' -Gone)
        $settingsAfterEscape = Find-AppElement 'settings'
        Assert-Accessibility 'Escape closes settings' ($null -eq $settingsAfterEscape) 'settings present' 'settings closed'
        $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
        do {
            $focused = $automation::FocusedElement.Current.AutomationId
            if ($focused -eq 'settings-toggle') { break }
            Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $deadline)
        Assert-Accessibility 'settings focus returns to trigger' ($focused -eq 'settings-toggle') $focused 'settings-toggle'
    }
}
catch {
    $script:failed = $true
    Write-Output "FAIL $($_.Exception.Message)"
}
finally {
    if ($null -ne $script:process -and -not $script:process.HasExited) {
        $script:process.Refresh()
        $handle = $script:process.MainWindowHandle
        if ($handle -ne [IntPtr]::Zero) {
            [void][Win32.Accessibility]::PostMessageW($handle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
        }
        [void]$script:process.WaitForExit(2000)
        if (-not $script:process.HasExited) {
            Stop-Process -Id $script:process.Id -Force -ErrorAction SilentlyContinue
        }
    }
}

if ($script:failed) {
    Write-Output 'RESULT: FAIL'
    exit 1
}
if ($script:skipped) {
    Write-Output 'RESULT: SKIP'
    exit 0
}
Write-Output 'RESULT: PASS'
exit 0
