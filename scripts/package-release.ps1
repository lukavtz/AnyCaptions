[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Version,
    [string]$Root,
    [string]$InputRoot,
    [string]$LockPath,
    [string]$OutputDirectory,
    [string]$Executable,
    [string]$NsisPath = 'makensis',
    [string]$MinisignPath = 'minisign',
    [string]$SecretKeyPath = $env:ANYCAPTIONS_UPDATE_KEY,
    [switch]$SecretKeyHasNoPassword,
    [switch]$SkipInstaller,
    [switch]$SkipSignature
)

$invocationPath = [string]$MyInvocation.MyCommand.Path
if ([string]::IsNullOrWhiteSpace($invocationPath)) {
    throw 'Cannot resolve the release packager script path.'
}
$scriptDirectory = Split-Path -Parent ([IO.Path]::GetFullPath($invocationPath))
$toolRoot = Split-Path -Parent $scriptDirectory
$verifyScript = Join-Path $scriptDirectory 'verify-release.ps1'
if ([string]::IsNullOrWhiteSpace($Root)) {
    $Root = $toolRoot
}
if ([string]::IsNullOrWhiteSpace($InputRoot)) {
    $InputRoot = $Root
}
if ([string]::IsNullOrWhiteSpace($LockPath)) {
    $LockPath = Join-Path $Root 'packaging/artifacts.lock.json'
}
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = Join-Path $Root "release/$Version"
}
if ([string]::IsNullOrWhiteSpace($Executable)) {
    $Executable = Join-Path $Root 'target/release/anycaptions.exe'
}

$ErrorActionPreference = 'Stop'
$requestedExitCode = 1
$powerShell = (Get-Process -Id $PID).Path
$fixedZipTimestamp = [DateTimeOffset]::new(1980, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
$stage = $null
$stageOwned = $false

Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

function Stop-WithCode {
    param([int]$Code, [string]$Message)
    $script:requestedExitCode = $Code
    throw $Message
}

function Assert-Version {
    param([string]$Value)
    if ($Value -notmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') {
        Stop-WithCode 4 "Version must be numeric x.y.z; got '$Value'."
    }
}

function Resolve-Tool {
    param([string]$NameOrPath)
    $command = Get-Command $NameOrPath -ErrorAction SilentlyContinue
    if ($null -eq $command) {
        Stop-WithCode 5 "Required tool not found: $NameOrPath"
    }
    return $command.Source
}

function Invoke-Verifier {
    param([switch]$InputsOnly)
    $arguments = @(
        '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $verifyScript,
        '-Root', $Root,
        '-InputRoot', $InputRoot,
        '-LockPath', $LockPath,
        '-Executable', $Executable,
        '-MinisignPath', $MinisignPath
    )
    if ($InputsOnly) {
        $arguments += '-InputsOnly'
    }
    else {
        $arguments += @('-ReleaseDirectory', $OutputDirectory, '-Version', $Version)
        if ($SkipInstaller) { $arguments += '-SkipInstaller' }
        if ($SkipSignature) { $arguments += '-SkipSignature' }
    }
    $previousErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        & $powerShell @arguments
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    if ($exitCode -ne 0) {
        Stop-WithCode $exitCode "Release verification failed with exit code $exitCode."
    }
}

function Copy-LockedInputs {
    param([object]$Lock, [string]$PayloadDirectory)
    Copy-Item -LiteralPath $Executable -Destination (Join-Path $PayloadDirectory 'anycaptions.exe')
    $runtimeDirectory = Join-Path $InputRoot ($Lock.runtime.directory -replace '/', '\')
    foreach ($entry in @($Lock.runtime.files)) {
        Copy-Item -LiteralPath (Join-Path $runtimeDirectory $entry.name) -Destination (Join-Path $PayloadDirectory $entry.name)
    }
    foreach ($model in @($Lock.models)) {
        $installRelative = ([string]$model.directory).Substring('backend/'.Length)
        $sourceDirectory = Join-Path $InputRoot ($model.directory -replace '/', '\')
        $destinationDirectory = Join-Path $PayloadDirectory ($installRelative -replace '/', '\')
        [IO.Directory]::CreateDirectory($destinationDirectory) | Out-Null
        foreach ($entry in @($model.files)) {
            Copy-Item -LiteralPath (Join-Path $sourceDirectory $entry.name) -Destination (Join-Path $destinationDirectory $entry.name)
        }
    }
}

function New-UpdatePackage {
    param([object]$Lock, [string]$PayloadDirectory, [string]$ZipPath)
    $updateFiles = [IO.FileInfo[]]@(
        Get-Item -LiteralPath (Join-Path $PayloadDirectory 'anycaptions.exe')
        foreach ($entry in @($Lock.runtime.files)) {
            Get-Item -LiteralPath (Join-Path $PayloadDirectory $entry.name)
        }
    )
    $fileByName = @{}
    foreach ($file in $updateFiles) {
        $fileByName[$file.Name] = $file
    }
    $updateNames = [string[]]@($fileByName.Keys)
    [Array]::Sort($updateNames, [StringComparer]::Ordinal)

    $fileStream = [IO.File]::Open($ZipPath, [IO.FileMode]::Create, [IO.FileAccess]::Write, [IO.FileShare]::None)
    $archive = $null
    try {
        $archive = [IO.Compression.ZipArchive]::new($fileStream, [IO.Compression.ZipArchiveMode]::Create, $false)
        foreach ($name in $updateNames) {
            $file = $fileByName[$name]
            $entry = $archive.CreateEntry($file.Name, [IO.Compression.CompressionLevel]::Optimal)
            $entry.LastWriteTime = $fixedZipTimestamp
            $input = [IO.File]::Open($file.FullName, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
            $output = $entry.Open()
            try {
                $input.CopyTo($output)
            }
            finally {
                $output.Dispose()
                $input.Dispose()
            }
        }
    }
    finally {
        if ($null -ne $archive) { $archive.Dispose() }
        $fileStream.Dispose()
    }
}

$scriptExitCode = 0
try {
    Assert-Version $Version
    $Root = [IO.Path]::GetFullPath($Root)
    $InputRoot = [IO.Path]::GetFullPath($InputRoot)
    $LockPath = [IO.Path]::GetFullPath($LockPath)
    $OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
    $Executable = [IO.Path]::GetFullPath($Executable)

    # Validation is shared with the standalone verifier. Missing input = 2;
    # digest/size mismatch = 3; incomplete or unsafe lock data = 4.
    Invoke-Verifier -InputsOnly
    $lock = Get-Content -LiteralPath $LockPath -Raw | ConvertFrom-Json
    if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
        Stop-WithCode 2 "Missing executable: $Executable"
    }

    [IO.Directory]::CreateDirectory($OutputDirectory) | Out-Null
    $stage = Join-Path $OutputDirectory ".payload-$PID"
    if (Test-Path -LiteralPath $stage) {
        Stop-WithCode 1 "Staging path already exists; refusing to remove unrelated data: $stage"
    }
    [IO.Directory]::CreateDirectory($stage) | Out-Null
    $stageOwned = $true
    Copy-LockedInputs $lock $stage

    $installerPath = Join-Path $OutputDirectory "AnyCaptions_${Version}_x64-setup.exe"
    if (-not $SkipInstaller) {
        $makensis = Resolve-Tool $NsisPath
        $nsi = Join-Path $toolRoot 'packaging/installer.nsi'
        $icon = Join-Path $toolRoot 'backend/icons/icon.ico'
        $payloadBytes = (Get-ChildItem -LiteralPath $stage -Recurse -File | Measure-Object -Property Length -Sum).Sum
        $installSizeKb = [long][Math]::Ceiling(([double]$payloadBytes + 262144.0) / 1024.0)
        $nsisArguments = @(
            '/V4', '/INPUTCHARSET', 'UTF8',
            "/DVERSION=$Version",
            "/DPAYLOAD_DIR=$stage",
            "/DOUTPUT_FILE=$installerPath",
            "/DICON_FILE=$icon",
            "/DINSTALL_SIZE_KB=$installSizeKb",
            $nsi
        )
        & $makensis @nsisArguments
        if ($LASTEXITCODE -ne 0) {
            Stop-WithCode 5 "NSIS failed with exit code $LASTEXITCODE."
        }
    }

    $asset = "anycaptions-update-$Version.zip"
    $zipPath = Join-Path $OutputDirectory $asset
    New-UpdatePackage $lock $stage $zipPath
    $zipItem = Get-Item -LiteralPath $zipPath
    $zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $manifestPath = Join-Path $OutputDirectory 'update.json'
    $manifest = [ordered]@{
        version = $Version
        asset = $asset
        size = [long]$zipItem.Length
        sha256 = $zipHash
    }
    [IO.File]::WriteAllText(
        $manifestPath,
        (($manifest | ConvertTo-Json -Compress) + "`n"),
        [Text.UTF8Encoding]::new($false)
    )

    if (-not $SkipSignature) {
        $minisign = Resolve-Tool $MinisignPath
        if ([string]::IsNullOrWhiteSpace($SecretKeyPath)) {
            $SecretKeyPath = Join-Path ([Environment]::GetFolderPath('UserProfile')) '.anycaptions/updater.key'
        }
        if (-not (Test-Path -LiteralPath $SecretKeyPath -PathType Leaf)) {
            Stop-WithCode 5 "Minisign secret key not found: $SecretKeyPath"
        }
        $signaturePath = Join-Path $OutputDirectory 'update.json.minisig'
        $signArguments = @('-S', '-m', $manifestPath, '-x', $signaturePath, '-s', $SecretKeyPath)
        if ($SecretKeyHasNoPassword) {
            $signArguments += '-W'
        }
        & $minisign @signArguments
        if ($LASTEXITCODE -ne 0) {
            Stop-WithCode 5 "Minisign signing failed with exit code $LASTEXITCODE."
        }
    }

    Invoke-Verifier
    Write-Host "Packaged $Version in $OutputDirectory"
}
catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    $scriptExitCode = $requestedExitCode
}
finally {
    if ($stageOwned -and $null -ne $stage -and (Test-Path -LiteralPath $stage)) {
        Remove-Item -LiteralPath $stage -Recurse -Force
    }
}

exit $scriptExitCode
