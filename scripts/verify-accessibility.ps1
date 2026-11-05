#requires -Version 5.1
<#
Queries the GPUI overlay and settings through Windows UI Automation, then runs
keyboard-only settings, picker, validation, and focus-return scenarios.

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
[DllImport("user32.dll")]
public static extern bool SetForegroundWindow(IntPtr hWnd);

[DllImport("user32.dll", CharSet = CharSet.Unicode)]
public static extern bool PostMessageW(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);
'@
}

$automation = [System.Windows.Automation.AutomationElement]
$tree = [System.Windows.Automation.TreeScope]
$control = [System.Windows.Automation.ControlType]
$script:failed = $false
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
    $property = [System.Windows.Automation.AutomationProperty]::LookupById($Id)
    return $Element.GetCurrentPropertyValue($property)
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
    $liveSetting = [int](Get-UiaProperty $caption 30135)
    Assert-Accessibility 'caption polite live region' ($liveSetting -eq 1) "$liveSetting" '1 (Polite)'

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
    if ($windowHandle -ne [IntPtr]::Zero) { [void][Win32.Accessibility]::SetForegroundWindow($windowHandle) }

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
    Set-UiaFocus $picker
    Send-UiaKeys ' '
    $menu = Require-AppElement 'source-language-menu'
    $expanded = [int](Get-UiaProperty $picker 30070)
    Assert-Accessibility 'picker expanded state' ($expanded -eq 1) "$expanded" '1 (Expanded)'
    Assert-Accessibility 'picker menu control type' `
        ($menu.Current.ControlType.Id -eq $control::List.Id) `
        $menu.Current.ControlType.ProgrammaticName $control::List.ProgrammaticName

    Send-UiaKeys '{DOWN}'
    $items = $menu.FindAll($tree::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
    $selected = 0
    foreach ($item in $items) {
        if ($item.Current.ControlType.Id -eq $control::ListItem.Id -and [bool](Get-UiaProperty $item 30079)) {
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
        $valid = [bool](Get-UiaProperty $color 30103)
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
Write-Output 'RESULT: PASS'
exit 0
