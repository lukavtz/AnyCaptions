#requires -Version 5.1
<#
Asserts the Win32 contract of the GPUI spike overlay window.

  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-overlay.ps1
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-overlay.ps1 -Args '--mode interactive'
#>
[CmdletBinding()]
param(
    [string]$Exe,
    # $Args is a PowerShell automatic variable, so the public switch is an alias.
    [Alias('Args')]
    [string]$Arguments = '--mode click-through',
    [string]$Title = 'AnyCaptions Spike',
    # The spike probes whether GPUI tolerates the layered bit; the orchestrator
    # picks the verdict after the visual test.
    [switch]$SkipLayered
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$GWL_EXSTYLE = -20
$WM_CLOSE = 0x0010
$WS_EX_TRANSPARENT = [long]0x20

$root = Split-Path -Parent $PSScriptRoot
if (-not $Exe) { $Exe = Join-Path $root 'target\debug\spike-overlay.exe' }
$Exe = [IO.Path]::GetFullPath($Exe)
if (-not (Test-Path -LiteralPath $Exe)) {
    Write-Output "FAIL exe missing $Exe"
    Write-Output 'RESULT: FAIL'
    exit 1
}

$mode = $null
if ($Arguments -match '--mode\s+(click-through|interactive)') { $mode = $Matches[1] }
if (-not $mode) {
    Write-Output 'FAIL mode unknown --mode value vs expected click-through or interactive'
    Write-Output 'RESULT: FAIL'
    exit 1
}

if (-not ('Win32.Overlay' -as [type])) {
Add-Type -Namespace Win32 -Name Overlay -MemberDefinition @'
[DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
public static extern IntPtr FindWindowW(string lpClassName, string lpWindowName);

[DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW", SetLastError = true)]
public static extern IntPtr GetWindowLongPtrW(IntPtr hWnd, int nIndex);

[DllImport("user32.dll", EntryPoint = "GetWindowLongW", SetLastError = true)]
public static extern int GetWindowLongW(IntPtr hWnd, int nIndex);

[StructLayout(LayoutKind.Sequential)]
public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }

[DllImport("user32.dll", SetLastError = true)]
public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

[DllImport("user32.dll", SetLastError = true)]
public static extern bool PostMessageW(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);

[DllImport("user32.dll")]
public static extern uint GetDpiForWindow(IntPtr hWnd);
'@
}

$script:failed = $false
function Assert-Overlay {
    param([string]$Name, [bool]$Ok, [string]$Actual, [string]$Expected)
    if ($Ok) {
        Write-Output "PASS $Name"
    } else {
        $script:failed = $true
        Write-Output "FAIL $Name $Actual vs expected $Expected"
    }
}

$proc = Start-Process -FilePath $Exe -ArgumentList $Arguments -PassThru
$hwnd = [IntPtr]::Zero

try {
    # Poll for the window; a sleeping process may take a while to create it.
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while ([DateTime]::UtcNow -lt $deadline) {
        # $null marshals to "" for a string parameter; [NullString]::Value is the real NULL.
        $hwnd = [Win32.Overlay]::FindWindowW([NullString]::Value, $Title)
        if ($hwnd -ne [IntPtr]::Zero -or $proc.HasExited) { break }
        Start-Sleep -Milliseconds 100
    }

    $found = $hwnd -ne [IntPtr]::Zero
    Assert-Overlay 'window found' $found $(if ($found) { "hwnd 0x$($hwnd.ToInt64().ToString('X'))" } else { 'hwnd 0x0' }) "window titled '$Title'"

    $exStyle = [long]0
    if ($found) {
        if ([IntPtr]::Size -eq 8) {
            $exStyle = ([Win32.Overlay]::GetWindowLongPtrW($hwnd, $GWL_EXSTYLE)).ToInt64()
        } else {
            $exStyle = [long]([Win32.Overlay]::GetWindowLongW($hwnd, $GWL_EXSTYLE))
        }
    }

    $required = @(
        @{ Name = 'WS_EX_TOPMOST';    Value = [long]0x8 },
        @{ Name = 'WS_EX_TOOLWINDOW'; Value = [long]0x80 },
        @{ Name = 'WS_EX_LAYERED';    Value = [long]0x80000 },
        @{ Name = 'WS_EX_NOACTIVATE'; Value = [long]0x8000000 }
    )
    foreach ($flag in $required) {
        if ($flag.Name -eq 'WS_EX_LAYERED' -and $SkipLayered) {
            Write-Output 'SKIP layered'
            continue
        }
        $set = ($exStyle -band $flag.Value) -eq $flag.Value
        Assert-Overlay "exstyle $($flag.Name)" $set ('0x{0:X}' -f $exStyle) ('0x{0:X}' -f $flag.Value)
    }

    $transparent = ($exStyle -band $WS_EX_TRANSPARENT) -eq $WS_EX_TRANSPARENT
    if ($mode -eq 'click-through') {
        Assert-Overlay 'exstyle WS_EX_TRANSPARENT present' $transparent ('0x{0:X}' -f $exStyle) '0x20'
    } else {
        Assert-Overlay 'exstyle WS_EX_TRANSPARENT absent' (-not $transparent) ('0x{0:X}' -f $exStyle) 'without 0x20'
    }

    $width = 0
    $height = 0
    $rectErr = $null
    $dpi = 0
    if ($found) {
        $dpi = [int][Win32.Overlay]::GetDpiForWindow($hwnd)
        $rect = New-Object 'Win32.Overlay+RECT'
        if ([Win32.Overlay]::GetWindowRect($hwnd, [ref]$rect)) {
            $width = $rect.Right - $rect.Left
            $height = $rect.Bottom - $rect.Top
        } else {
            $rectErr = "GetWindowRect error $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
        }
    } else {
        $rectErr = 'no window'
    }
    if ($dpi -le 0) { $dpi = 96 }
    $expectedW = [int][Math]::Round(800 * $dpi / 96)
    $expectedH = [int][Math]::Round(160 * $dpi / 96)
    Assert-Overlay 'window size 800x160 logical' ($width -eq $expectedW -and $height -eq $expectedH) `
        $(if ($null -eq $rectErr) { "${width}x${height} at ${dpi}dpi" } else { $rectErr }) "${expectedW}x${expectedH} at ${dpi}dpi"

    $proc.Refresh()
    $alive = -not $proc.HasExited
    Assert-Overlay 'process alive' $alive $(if ($alive) { "pid $($proc.Id)" } else { "exited with $($proc.ExitCode)" }) 'running process'
}
finally {
    if (-not $proc.HasExited) {
        if ($hwnd -ne [IntPtr]::Zero) {
            [void][Win32.Overlay]::PostMessageW($hwnd, [uint32]$WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero)
        }
        [void]$proc.WaitForExit(2000)
        if (-not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
    }
}

if ($script:failed) {
    Write-Output 'RESULT: FAIL'
    exit 1
}
Write-Output 'RESULT: PASS'
exit 0