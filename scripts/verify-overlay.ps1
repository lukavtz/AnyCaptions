#requires -Version 5.1
<#
Verifies the production GPUI Normal overlay, not the spike.

  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-overlay.ps1
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-overlay.ps1 -Exe target\debug\anycaptions.exe -ScreenshotPath artifacts\overlay.png

The default interactive pass moves and resizes the real window, locks through
its toolbar, recovers with normal Ctrl+Alt+L, and clicks a probe behind it.
Use -SkipInteractive only for a headless style/geometry smoke pass.
#>
[CmdletBinding()]
param(
    [string]$Exe,
    [Alias('Args')]
    [string]$Arguments = '',
    [string]$Title = 'AnyCaptions',
    [int]$TimeoutSeconds = 15,
    [string]$ScreenshotPath,
    [switch]$SkipInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
if (-not $Exe) { $Exe = Join-Path $root 'target\debug\anycaptions.exe' }
$Exe = [IO.Path]::GetFullPath($Exe)
if (-not (Test-Path -LiteralPath $Exe -PathType Leaf)) {
    throw "Production executable not found: $Exe"
}
if (-not $ScreenshotPath) {
    $ScreenshotPath = Join-Path $root 'artifacts\overlay-normal.png'
}
$ScreenshotPath = [IO.Path]::GetFullPath($ScreenshotPath)
$screenshotDirectory = Split-Path -Parent $ScreenshotPath
if (-not (Test-Path -LiteralPath $screenshotDirectory)) {
    [void][IO.Directory]::CreateDirectory($screenshotDirectory)
}

$native = @'
using System;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Text;
using System.Windows.Forms;

public static class ProductionOverlayNative
{
    public const int GWL_EXSTYLE = -20;
    public const int GCLP_HICON = -14;
    public const uint WS_EX_TRANSPARENT = 0x20;
    public const uint WS_EX_TOOLWINDOW = 0x80;
    public const uint WS_EX_LAYERED = 0x80000;
    public const uint WS_EX_TOPMOST = 0x8;
    public const uint WS_EX_NOACTIVATE = 0x8000000;
    public const uint WS_EX_APPWINDOW = 0x40000;
    public const uint WM_CLOSE = 0x10;
    public const uint WM_GETICON = 0x7f;
    public const uint MONITOR_DEFAULTTONEAREST = 2;
    public const uint SWP_NOSIZE = 0x1;
    public const uint SWP_NOMOVE = 0x2;
    public const uint SWP_NOZORDER = 0x4;
    public const uint SWP_NOACTIVATE = 0x10;
    public const uint MOUSEEVENTF_LEFTDOWN = 0x2;
    public const uint MOUSEEVENTF_LEFTUP = 0x4;
    public const uint KEYEVENTF_KEYUP = 0x2;
    public const byte VK_CONTROL = 0x11;
    public const byte VK_MENU = 0x12;
    public const byte VK_L = 0x4c;

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }

    [StructLayout(LayoutKind.Sequential)]
    public struct MONITORINFO
    {
        public int Size;
        public RECT Monitor;
        public RECT Work;
        public uint Flags;
    }

    [UnmanagedFunctionPointer(CallingConvention.Winapi)]
    public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hwnd);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowTextLength(IntPtr hwnd);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int count);

    [DllImport("user32.dll")]
    public static extern IntPtr GetWindowLongPtr(IntPtr hwnd, int index);

    [DllImport("user32.dll")]
    public static extern IntPtr GetClassLongPtr(IntPtr hwnd, int index);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);

    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    public static extern uint GetDpiForWindow(IntPtr hwnd);

    [DllImport("user32.dll")]
    public static extern IntPtr MonitorFromWindow(IntPtr hwnd, uint flags);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern bool GetMonitorInfo(IntPtr monitor, ref MONITORINFO info);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int width, int height, uint flags);

    [DllImport("user32.dll")]
    public static extern bool SetCursorPos(int x, int y);

    [DllImport("user32.dll")]
    public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extraInfo);

    [DllImport("user32.dll")]
    public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extraInfo);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint message, UIntPtr wParam, IntPtr lParam, uint flags, uint timeout, out UIntPtr result);

    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    public static extern uint ExtractIconEx(string file, int index, out IntPtr large, out IntPtr small, uint count);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr FindWindow(string className, string title);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetClassName(IntPtr hwnd, StringBuilder className, int count);
}

public sealed class OverlayClickProbe : NativeWindow
{
    public int Clicks;
    private int width;
    private int height;

    public void Create(int x, int y, int width, int height)
    {
        this.width = width;
        this.height = height;
        CreateParams parameters = new CreateParams
        {
            Caption = "AnyCaptions native click probe",
            X = x,
            Y = y,
            Width = width,
            Height = height,
            Style = unchecked((int)(0x80000000 | 0x10000000))
        };
        CreateHandle(parameters);
    }

    protected override void WndProc(ref Message message)
    {
        if (message.Msg == 0x201) { Clicks++; }
        if (message.Msg == 0x14)
        {
            using (Graphics graphics = Graphics.FromHdc(message.WParam))
            using (SolidBrush brush = new SolidBrush(Color.FromArgb(255, 255, 0, 255)))
            {
                graphics.FillRectangle(brush, 0, 0, width, height);
            }
            message.Result = new IntPtr(1);
            return;
        }
        base.WndProc(ref message);
    }

    public void CloseProbe()
    {
        if (Handle != IntPtr.Zero) { DestroyHandle(); }
    }
}
'@

if (-not ('ProductionOverlayNative' -as [type])) {
    Add-Type -TypeDefinition $native -ReferencedAssemblies @(
        'System',
        'System.Drawing',
        'System.Windows.Forms'
    )
}
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

$script:failed = $false
function Assert-Overlay {
    param([string]$Name, [bool]$Ok, [string]$Actual = '', [string]$Expected = '')
    if ($Ok) {
        Write-Output "PASS: $Name"
    } else {
        $script:failed = $true
        Write-Output "FAIL: $Name; actual=$Actual expected=$Expected"
    }
}

function Wait-Overlay {
    param([scriptblock]$Condition, [string]$Description, [int]$Milliseconds = 5000)
    $deadline = [DateTime]::UtcNow.AddMilliseconds($Milliseconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        [System.Windows.Forms.Application]::DoEvents()
        if (& $Condition) { return $true }
        Start-Sleep -Milliseconds 50
    }
    Write-Host "TIMEOUT: $Description"
    return $false
}

function Get-ExStyle([IntPtr]$Hwnd) {
    $value = [ProductionOverlayNative]::GetWindowLongPtr($Hwnd, [ProductionOverlayNative]::GWL_EXSTYLE)
    return [uint32]$value.ToInt64()
}

function Get-Rect([IntPtr]$Hwnd) {
    $rect = New-Object ProductionOverlayNative+RECT
    if (-not [ProductionOverlayNative]::GetWindowRect($Hwnd, [ref]$rect)) {
        throw 'GetWindowRect failed'
    }
    return $rect
}

function Get-WorkArea([IntPtr]$Hwnd) {
    $monitor = [ProductionOverlayNative]::MonitorFromWindow($Hwnd, [ProductionOverlayNative]::MONITOR_DEFAULTTONEAREST)
    $info = New-Object ProductionOverlayNative+MONITORINFO
    $info.Size = [Runtime.InteropServices.Marshal]::SizeOf($info)
    if ($monitor -eq [IntPtr]::Zero -or -not [ProductionOverlayNative]::GetMonitorInfo($monitor, [ref]$info)) {
        throw 'GetMonitorInfo failed'
    }
    return $info.Work
}

function Find-ProductionOverlay([uint32]$ProcessId) {
    $candidates = New-Object 'System.Collections.Generic.List[System.IntPtr]'
    $callback = [ProductionOverlayNative+EnumWindowsProc]{
        param([IntPtr]$hwnd, [IntPtr]$lParam)
        $owner = [uint32]0
        [void][ProductionOverlayNative]::GetWindowThreadProcessId($hwnd, [ref]$owner)
        if ($owner -eq $ProcessId -and [ProductionOverlayNative]::IsWindowVisible($hwnd)) {
            $style = Get-ExStyle $hwnd
            if (($style -band [ProductionOverlayNative]::WS_EX_APPWINDOW) -ne 0 -and
                ($style -band [ProductionOverlayNative]::WS_EX_TOOLWINDOW) -eq 0) {
                $candidates.Add($hwnd)
            }
        }
        return $true
    }
    [void][ProductionOverlayNative]::EnumWindows($callback, [IntPtr]::Zero)
    if ($candidates.Count -eq 0) { return [IntPtr]::Zero }
    foreach ($candidate in $candidates) {
        $length = [ProductionOverlayNative]::GetWindowTextLength($candidate)
        $text = [Text.StringBuilder]::new($length + 1)
        [void][ProductionOverlayNative]::GetWindowText($candidate, $text, $text.Capacity)
        if ($text.ToString() -eq $Title) { return $candidate }
    }
    return $candidates[0]
}

function Test-TaskbarButton {
    $conditions = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Button
    )
    $taskbars = @(
        [ProductionOverlayNative]::FindWindow('Shell_TrayWnd', $null),
        [ProductionOverlayNative]::FindWindow('Shell_SecondaryTrayWnd', $null)
    )
    foreach ($taskbar in $taskbars) {
        if ($taskbar -eq [IntPtr]::Zero) { continue }
        $element = [System.Windows.Automation.AutomationElement]::FromHandle($taskbar)
        $buttons = $element.FindAll([System.Windows.Automation.TreeScope]::Descendants, $conditions)
        foreach ($button in $buttons) {
            if ($button.Current.Name.IndexOf($Title, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
                return $true
            }
        }
    }
    return $false
}

function Send-LockShortcut {
    [ProductionOverlayNative]::keybd_event([ProductionOverlayNative]::VK_CONTROL, 0, 0, [UIntPtr]::Zero)
    [ProductionOverlayNative]::keybd_event([ProductionOverlayNative]::VK_MENU, 0, 0, [UIntPtr]::Zero)
    [ProductionOverlayNative]::keybd_event([ProductionOverlayNative]::VK_L, 0, 0, [UIntPtr]::Zero)
    [ProductionOverlayNative]::keybd_event([ProductionOverlayNative]::VK_L, 0, [ProductionOverlayNative]::KEYEVENTF_KEYUP, [UIntPtr]::Zero)
    [ProductionOverlayNative]::keybd_event([ProductionOverlayNative]::VK_MENU, 0, [ProductionOverlayNative]::KEYEVENTF_KEYUP, [UIntPtr]::Zero)
    [ProductionOverlayNative]::keybd_event([ProductionOverlayNative]::VK_CONTROL, 0, [ProductionOverlayNative]::KEYEVENTF_KEYUP, [UIntPtr]::Zero)
}

function Send-Click([int]$X, [int]$Y) {
    [void][ProductionOverlayNative]::SetCursorPos($X, $Y)
    Start-Sleep -Milliseconds 100
    [ProductionOverlayNative]::mouse_event([ProductionOverlayNative]::MOUSEEVENTF_LEFTDOWN, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 60
    [ProductionOverlayNative]::mouse_event([ProductionOverlayNative]::MOUSEEVENTF_LEFTUP, 0, 0, 0, [UIntPtr]::Zero)
    [System.Windows.Forms.Application]::DoEvents()
    Start-Sleep -Milliseconds 150
    [System.Windows.Forms.Application]::DoEvents()
}

function Click-LockToolbarButton([IntPtr]$Hwnd) {
    $root = [System.Windows.Automation.AutomationElement]::FromHandle($Hwnd)
    $lockId = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
        'lock'
    )
    $lockElement = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $lockId)
    if ($lockElement -ne $null) {
        $bounds = $lockElement.Current.BoundingRectangle
        if (-not $bounds.IsEmpty) {
            $invoke = $lockElement.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
            $invoke.Invoke()
            return $true
        }
    }
    $walker = [System.Windows.Automation.TreeWalker]::RawViewWalker
    $child = $walker.GetFirstChild($root)
    for ($index = 0; $index -lt 64 -and $child -ne $null; $index++) {
        if ($child.Current.AutomationId -eq 'lock') {
            $bounds = $child.Current.BoundingRectangle
            if (-not $bounds.IsEmpty) {
                $invoke = $child.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
                $invoke.Invoke()
                return $true
            }
        }
        $child = $walker.GetNextSibling($child)
    }
    $condition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Button
    )
    $buttons = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $condition)
    $lockNames = @('Bloquear janela', 'Lock window', 'Bloquear ventana')
    foreach ($button in $buttons) {
        if ($lockNames -notcontains $button.Current.Name) { continue }
        $bounds = $button.Current.BoundingRectangle
        if ($bounds.IsEmpty) { return $false }
        Send-Click ([int]($bounds.Left + $bounds.Width / 2)) ([int]($bounds.Top + $bounds.Height / 2))
        return $true
    }
    # The remaining locales use the same toolbar order; Close is the final
    # button and Lock immediately precedes it.
    if ($buttons.Count -ge 2) {
        $bounds = $buttons.Item($buttons.Count - 2).Current.BoundingRectangle
        if (-not $bounds.IsEmpty) {
            Send-Click ([int]($bounds.Left + $bounds.Width / 2)) ([int]($bounds.Top + $bounds.Height / 2))
            return $true
        }
    }
    return $false
}

function Send-Drag([int]$X, [int]$Y, [int]$DeltaX, [int]$DeltaY) {
    [void][ProductionOverlayNative]::SetCursorPos($X, $Y)
    Start-Sleep -Milliseconds 100
    [ProductionOverlayNative]::mouse_event([ProductionOverlayNative]::MOUSEEVENTF_LEFTDOWN, 0, 0, 0, [UIntPtr]::Zero)
    for ($step = 1; $step -le 8; $step++) {
        $nextX = $X + [int]($DeltaX * $step / 8)
        $nextY = $Y + [int]($DeltaY * $step / 8)
        [void][ProductionOverlayNative]::SetCursorPos($nextX, $nextY)
        Start-Sleep -Milliseconds 45
    }
    [ProductionOverlayNative]::mouse_event([ProductionOverlayNative]::MOUSEEVENTF_LEFTUP, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 250
}

function Save-OverlayScreenshot([IntPtr]$Hwnd, [string]$Path, [double]$Scale) {
    $rect = Get-Rect $Hwnd
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    $bitmap = New-Object Drawing.Bitmap $width, $height
    try {
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
        } finally {
            $graphics.Dispose()
        }
        # Above the toolbar's 8-logical-pixel top padding, the known overlay
        # background is 0f0f0f at alpha a6 over the probe's opaque magenta.
        $sampleX = [int]($width / 2)
        $sampleY = [Math]::Min($height - 1, [Math]::Max(1, [int](2 * $Scale)))
        $pixel = $bitmap.GetPixel($sampleX, $sampleY)
        $bitmap.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
        return $pixel
    } finally {
        $bitmap.Dispose()
    }
}

function Test-OverlayBackgroundPixel([Drawing.Color]$Pixel) {
    # (15 * 166 + 255 * 89) / 255 = 99 for red/blue; green = 10.
    return [Math]::Abs($Pixel.R - 99) -le 4 -and
        [Math]::Abs($Pixel.G - 10) -le 4 -and
        [Math]::Abs($Pixel.B - 99) -le 4
}

$proc = $null
$hwnd = [IntPtr]::Zero
$probe = $null
try {
    if ([string]::IsNullOrWhiteSpace($Arguments)) {
        $proc = Start-Process -FilePath $Exe -PassThru
    } else {
        $proc = Start-Process -FilePath $Exe -ArgumentList $Arguments -PassThru
    }
    $found = Wait-Overlay -Description 'production Normal overlay window' -Milliseconds ($TimeoutSeconds * 1000) -Condition {
        if ($proc.HasExited) { return $false }
        $script:hwnd = Find-ProductionOverlay ([uint32]$proc.Id)
        return $script:hwnd -ne [IntPtr]::Zero
    }
    if (-not $found) { throw 'Production overlay did not create a Normal taskbar window' }
    $hwnd = $script:hwnd
    $style = Get-ExStyle $hwnd
    Assert-Overlay 'Normal overlay uses WS_EX_APPWINDOW' (($style -band [ProductionOverlayNative]::WS_EX_APPWINDOW) -ne 0) ('0x{0:x8}' -f $style)
    Assert-Overlay 'Normal overlay excludes WS_EX_TOOLWINDOW' (($style -band [ProductionOverlayNative]::WS_EX_TOOLWINDOW) -eq 0) ('0x{0:x8}' -f $style)
    Assert-Overlay 'Overlay is topmost' (($style -band [ProductionOverlayNative]::WS_EX_TOPMOST) -ne 0)
    Assert-Overlay 'Overlay is noactivate' (($style -band [ProductionOverlayNative]::WS_EX_NOACTIVATE) -ne 0)
    Assert-Overlay 'Overlay is layered' (($style -band [ProductionOverlayNative]::WS_EX_LAYERED) -ne 0)
    Assert-Overlay 'Edit mode is interactive' (($style -band [ProductionOverlayNative]::WS_EX_TRANSPARENT) -eq 0)
    Assert-Overlay 'Production process stays alive' (-not $proc.HasExited)

    $work = Get-WorkArea $hwnd
    $rect = Get-Rect $hwnd
    $inside = $rect.Left -ge $work.Left -and $rect.Top -ge $work.Top -and
        $rect.Right -le $work.Right -and $rect.Bottom -le $work.Bottom
    Assert-Overlay 'Restored geometry stays inside monitor work area' $inside (
        '{0},{1} {2}x{3}' -f $rect.Left, $rect.Top, ($rect.Right - $rect.Left), ($rect.Bottom - $rect.Top)
    ) ('work={0},{1}-{2},{3}' -f $work.Left, $work.Top, $work.Right, $work.Bottom)
    $dpi = [ProductionOverlayNative]::GetDpiForWindow($hwnd)
    Assert-Overlay 'Window reports a valid DPI' ($dpi -ge 96) ([string]$dpi) '>= 96'
    $expectedMinimumWidth = [int][Math]::Ceiling(320.0 * $dpi / 96.0)
    $expectedMinimumHeight = [int][Math]::Ceiling(160.0 * $dpi / 96.0)
    Assert-Overlay 'Logical minimum width reaches native geometry' (($rect.Right - $rect.Left) -ge $expectedMinimumWidth) ([string]($rect.Right - $rect.Left)) ([string]$expectedMinimumWidth)
    Assert-Overlay 'Logical minimum height reaches native geometry' (($rect.Bottom - $rect.Top) -ge $expectedMinimumHeight) ([string]($rect.Bottom - $rect.Top)) ([string]$expectedMinimumHeight)

    $taskbarButton = Test-TaskbarButton
    Assert-Overlay 'Shell taskbar contains the Normal overlay button' $taskbarButton
    $iconResult = [UIntPtr]::Zero
    [void][ProductionOverlayNative]::SendMessageTimeout(
        $hwnd,
        [ProductionOverlayNative]::WM_GETICON,
        [UIntPtr]::Zero,
        [IntPtr]::Zero,
        0,
        1000,
        [ref]$iconResult
    )
    $classIcon = [ProductionOverlayNative]::GetClassLongPtr($hwnd, [ProductionOverlayNative]::GCLP_HICON)
    $largeIcon = [IntPtr]::Zero
    $smallIcon = [IntPtr]::Zero
    $iconCount = [ProductionOverlayNative]::ExtractIconEx($Exe, 0, [ref]$largeIcon, [ref]$smallIcon, 1)
    Assert-Overlay 'Window or executable supplies the taskbar icon' (
        $iconResult -ne [UIntPtr]::Zero -or $classIcon -ne [IntPtr]::Zero -or $iconCount -gt 0
    ) ('window={0} class={1} resources={2}' -f $iconResult, $classIcon, $iconCount)

    $scale = $dpi / 96.0
    $probe = New-Object OverlayClickProbe
    $probe.Create($work.Left, $work.Top, $work.Right - $work.Left, $work.Bottom - $work.Top)
    Start-Sleep -Milliseconds 200
    [System.Windows.Forms.Application]::DoEvents()
    $pixel = Save-OverlayScreenshot $hwnd $ScreenshotPath $scale
    Assert-Overlay 'Screenshot contains identifiable overlay background over controlled native pixels' (
        Test-OverlayBackgroundPixel $pixel
    ) ([string]$pixel) 'RGB(99+/-4, 10+/-4, 99+/-4) over opaque RGB(255, 0, 255)'
    Write-Output "SCREENSHOT: $ScreenshotPath"

    if (-not $SkipInteractive) {
        # Start from a known safe work-area location. This sends real native
        # move/resize notifications through the production GPUI observers.
        $safeX = $work.Left + [int](40 * $scale)
        $safeY = $work.Top + [int](40 * $scale)
        $availableWidth = [Math]::Floor($work.Right - $safeX - (84 * $scale))
        $availableHeight = [Math]::Floor($work.Bottom - $safeY - (44 * $scale))
        if ($availableWidth -lt $expectedMinimumWidth -or $availableHeight -lt $expectedMinimumHeight) {
            throw 'Monitor work area is too small for interactive overlay verification'
        }
        $safeWidth = [Math]::Min($rect.Right - $rect.Left, [Math]::Min([int](800 * $scale), [int]$availableWidth))
        $safeHeight = [Math]::Min($rect.Bottom - $rect.Top, [Math]::Min([int](160 * $scale), [int]$availableHeight))
        [void][ProductionOverlayNative]::SetWindowPos(
            $hwnd,
            [IntPtr]::Zero,
            $safeX,
            $safeY,
            $safeWidth,
            $safeHeight,
            [ProductionOverlayNative]::SWP_NOZORDER -bor [ProductionOverlayNative]::SWP_NOACTIVATE
        )
        Start-Sleep -Milliseconds 300

        $before = Get-Rect $hwnd
        $dragX = $before.Left + [int](55 * $scale)
        $dragY = $before.Top + [int](16 * $scale)
        Send-Drag $dragX $dragY ([int](48 * $scale)) ([int](24 * $scale))
        $after = Get-Rect $hwnd
        Assert-Overlay 'Toolbar drag moves the real HWND' (
            $after.Left -gt $before.Left -and $after.Top -gt $before.Top
        ) ('{0},{1} -> {2},{3}' -f $before.Left, $before.Top, $after.Left, $after.Top)

        $before = Get-Rect $hwnd
        $contentX = $before.Left + [int](($before.Right - $before.Left) / 2)
        $contentY = $before.Top + [int](88 * $scale)
        Send-Drag $contentX $contentY ([int](20 * $scale)) ([int](12 * $scale))
        $after = Get-Rect $hwnd
        Assert-Overlay 'Caption content is excluded from native drag regions' (
            $after.Left -eq $before.Left -and $after.Top -eq $before.Top -and
            $after.Right -eq $before.Right -and $after.Bottom -eq $before.Bottom
        ) ('{0},{1},{2},{3}' -f $after.Left, $after.Top, $after.Right, $after.Bottom)

        $before = Get-Rect $hwnd
        Send-Drag ($before.Right - 2) ($before.Top + [int](80 * $scale)) ([int](36 * $scale)) 0
        $after = Get-Rect $hwnd
        Assert-Overlay 'East primary drag resizes width with fixed West edge' (
            $after.Right -gt $before.Right -and $after.Left -eq $before.Left
        ) ('{0},{1} -> {2},{3}' -f $before.Left, $before.Right, $after.Left, $after.Right)

        $before = Get-Rect $hwnd
        Send-Drag ($before.Left + 2) ($before.Top + [int](80 * $scale)) ([int](-24 * $scale)) 0
        $after = Get-Rect $hwnd
        Assert-Overlay 'West primary drag resizes width with fixed East edge' (
            $after.Left -lt $before.Left -and $after.Right -eq $before.Right
        ) ('{0},{1} -> {2},{3}' -f $before.Left, $before.Right, $after.Left, $after.Right)

        $before = Get-Rect $hwnd
        Send-Drag ($before.Left + [int](($before.Right - $before.Left) / 2)) ($before.Bottom - 2) 0 ([int](20 * $scale))
        $after = Get-Rect $hwnd
        Assert-Overlay 'South primary drag resizes height with fixed North edge' (
            $after.Bottom -gt $before.Bottom -and $after.Top -eq $before.Top
        ) ('{0},{1} -> {2},{3}' -f $before.Top, $before.Bottom, $after.Top, $after.Bottom)

        $workAfter = Get-WorkArea $hwnd
        Assert-Overlay 'Interactive geometry remains on a valid work area' (
            $after.Left -ge $workAfter.Left -and $after.Top -ge $workAfter.Top -and
            $after.Right -le $workAfter.Right -and $after.Bottom -le $workAfter.Bottom
        )

        # Enter Locked through the real toolbar action. Ctrl+Alt+L is reserved
        # for recovery because HostEvent::Activate always selects Edit mode.
        if (-not (Click-LockToolbarButton $hwnd)) {
            throw 'Production toolbar lock button was not found or had no clickable bounds'
        }
        $locked = Wait-Overlay -Description 'WS_EX_TRANSPARENT after production toolbar lock' -Condition {
            ((Get-ExStyle $hwnd) -band [ProductionOverlayNative]::WS_EX_TRANSPARENT) -ne 0
        }
        Assert-Overlay 'Lock enables native click-through style' $locked
        $lockedStyle = Get-ExStyle $hwnd
        Assert-Overlay 'Locked overlay preserves taskbar/topmost/noactivate' (
            ($lockedStyle -band [ProductionOverlayNative]::WS_EX_APPWINDOW) -ne 0 -and
            ($lockedStyle -band [ProductionOverlayNative]::WS_EX_TOOLWINDOW) -eq 0 -and
            ($lockedStyle -band [ProductionOverlayNative]::WS_EX_TOPMOST) -ne 0 -and
            ($lockedStyle -band [ProductionOverlayNative]::WS_EX_NOACTIVATE) -ne 0
        )

        $after = Get-Rect $hwnd
        $clickX = $after.Left + [int](($after.Right - $after.Left) / 2)
        $clickY = $after.Top + [int](($after.Bottom - $after.Top) / 2)
        $beforeClicks = $probe.Clicks
        Send-Click $clickX $clickY
        Assert-Overlay 'Locked overlay delivers a real mouse click to the window beneath' ($probe.Clicks -gt $beforeClicks) ([string]$probe.Clicks) ([string]$beforeClicks)

        Send-LockShortcut
        $unlocked = Wait-Overlay -Description 'interactive style after recovery shortcut' -Condition {
            ((Get-ExStyle $hwnd) -band [ProductionOverlayNative]::WS_EX_TRANSPARENT) -eq 0
        }
        Assert-Overlay 'Recovery restores interactive style' $unlocked
        $beforeClicks = $probe.Clicks
        Send-Click $clickX $clickY
        Assert-Overlay 'Unlocked overlay intercepts the real mouse click' ($probe.Clicks -eq $beforeClicks) ([string]$probe.Clicks) ([string]$beforeClicks)
        Assert-Overlay 'Clicking never activates the overlay HWND' (
            [ProductionOverlayNative]::GetForegroundWindow() -ne $hwnd
        )

        $pixel = Save-OverlayScreenshot $hwnd $ScreenshotPath $scale
        Assert-Overlay 'Post-interaction screenshot retains identifiable overlay pixels' (
            Test-OverlayBackgroundPixel $pixel
        ) ([string]$pixel) 'RGB(99+/-4, 10+/-4, 99+/-4) over opaque RGB(255, 0, 255)'
        Write-Output "INTERACTIVE_SCREENSHOT: $ScreenshotPath"
    }

    if ($script:failed) { throw 'One or more production overlay checks failed' }
    Write-Output 'RESULT: PASS'
} catch {
    $script:failed = $true
    Write-Output ('ERROR: ' + $_.Exception.Message)
    Write-Output 'RESULT: FAIL'
} finally {
    if ($probe -ne $null) { $probe.CloseProbe() }
    if ($proc -ne $null) {
        if ($hwnd -ne [IntPtr]::Zero) {
            $result = [UIntPtr]::Zero
            [void][ProductionOverlayNative]::SendMessageTimeout(
                $hwnd,
                [ProductionOverlayNative]::WM_CLOSE,
                [UIntPtr]::Zero,
                [IntPtr]::Zero,
                0,
                1000,
                [ref]$result
            )
        }
        if (-not $proc.HasExited) {
            Start-Sleep -Milliseconds 500
            $proc.Refresh()
        }
        if (-not $proc.HasExited) { $proc.Kill() }
        $proc.Dispose()
    }
}

if ($script:failed) { exit 1 }
exit 0
