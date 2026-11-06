[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$invocationPath = [string]$MyInvocation.MyCommand.Path
if ([string]::IsNullOrWhiteSpace($invocationPath)) {
    throw 'Cannot resolve the packaging test script path.'
}
$scriptDirectory = Split-Path -Parent ([IO.Path]::GetFullPath($invocationPath))
$repoRoot = Split-Path -Parent $scriptDirectory
$packageScript = Join-Path $scriptDirectory 'package-release.ps1'
$verifyScript = Join-Path $scriptDirectory 'verify-release.ps1'
$powerShell = (Get-Process -Id $PID).Path
$fixtureRoot = Join-Path ([IO.Path]::GetTempPath()) "anycaptions-packaging-$([Guid]::NewGuid().ToString('N'))"

Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) {
        throw "Assertion failed: $Message"
    }
}

function Write-FixtureFile {
    param([string]$Path, [byte[]]$Bytes)
    $directory = Split-Path -Parent $Path
    [IO.Directory]::CreateDirectory($directory) | Out-Null
    [IO.File]::WriteAllBytes($Path, $Bytes)
}

function Get-Sha256 {
    param([string]$Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Invoke-PackageFixture {
    param(
        [string]$Executable,
        [string]$LockPath,
        [string]$OutputDirectory,
        [string]$Version = '9.9.9'
    )

    $previousErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $output = & $powerShell -NoProfile -ExecutionPolicy Bypass -File $packageScript `
            -Root $fixtureRoot `
            -InputRoot $fixtureRoot `
            -Executable $Executable `
            -LockPath $LockPath `
            -OutputDirectory $OutputDirectory `
            -Version $Version `
            -SkipInstaller `
            -SkipSignature 2>&1
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    return [pscustomobject]@{
        ExitCode = $exitCode
        Output = ($output -join [Environment]::NewLine)
    }
}

function Invoke-VerifyFixture {
    param(
        [string]$Executable,
        [string]$LockPath,
        [string]$ReleaseDirectory,
        [string]$Version = '9.9.9'
    )

    $previousErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $output = & $powerShell -NoProfile -ExecutionPolicy Bypass -File $verifyScript `
            -Root $fixtureRoot `
            -InputRoot $fixtureRoot `
            -Executable $Executable `
            -LockPath $LockPath `
            -ReleaseDirectory $ReleaseDirectory `
            -Version $Version `
            -SkipInstaller `
            -SkipSignature 2>&1
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    return [pscustomobject]@{
        ExitCode = $exitCode
        Output = ($output -join [Environment]::NewLine)
    }
}

$failed = $false
try {
    [IO.Directory]::CreateDirectory($fixtureRoot) | Out-Null

    # The production lock supplies the layout. Fixture bytes get their own real
    # pins; no production digest is invented or replaced on disk.
    $productionLockPath = Join-Path $repoRoot 'packaging/artifacts.lock.json'
    $productionLock = Get-Content -LiteralPath $productionLockPath -Raw | ConvertFrom-Json
    $lockJson = $productionLock | ConvertTo-Json -Depth 12
    $fixtureLock = $lockJson | ConvertFrom-Json

    $runtimeDirectory = Join-Path $fixtureRoot ($fixtureLock.runtime.directory -replace '/', '\')
    foreach ($entry in @($fixtureLock.runtime.files)) {
        $path = Join-Path $runtimeDirectory $entry.name
        $bytes = [Text.Encoding]::UTF8.GetBytes("fixture DLL: $($entry.name)")
        Write-FixtureFile -Path $path -Bytes $bytes
        $entry.size = $bytes.LongLength
        $entry.sha256 = Get-Sha256 -Path $path
    }

    foreach ($model in @($fixtureLock.models)) {
        $modelDirectory = Join-Path $fixtureRoot ($model.directory -replace '/', '\')
        foreach ($entry in @($model.files)) {
            $path = Join-Path $modelDirectory $entry.name
            $bytes = [Text.Encoding]::UTF8.GetBytes("fixture model: $($model.directory)/$($entry.name)")
            Write-FixtureFile -Path $path -Bytes $bytes
            $entry.size = $bytes.LongLength
            $entry.sha256 = Get-Sha256 -Path $path
        }
    }

    $executable = Join-Path $fixtureRoot 'target/release/anycaptions.exe'
    Write-FixtureFile -Path $executable -Bytes ([Text.Encoding]::UTF8.GetBytes('fixture executable'))

    $fixtureLockPath = Join-Path $fixtureRoot 'artifacts.fixture.lock.json'
    [IO.File]::WriteAllText(
        $fixtureLockPath,
        (($fixtureLock | ConvertTo-Json -Depth 12) + "`n"),
        [Text.UTF8Encoding]::new($false)
    )

    # A schema-only CI check must not require the large production inputs.
    $previousErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $lockOnlyOutput = & $powerShell -NoProfile -ExecutionPolicy Bypass -File $verifyScript `
            -LockPath $productionLockPath -LockOnly 2>&1
        $lockOnlyExitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    Assert-True ($lockOnlyExitCode -eq 0) "production lock schema validation: $($lockOnlyOutput -join ' ')"

    $releaseDirectory = Join-Path $fixtureRoot 'release/9.9.9'

    # Missing locked DLL: required contract exit code 2.
    $missingEntry = @($fixtureLock.runtime.files)[0]
    $missingPath = Join-Path $runtimeDirectory $missingEntry.name
    $missingBytes = [IO.File]::ReadAllBytes($missingPath)
    Remove-Item -LiteralPath $missingPath -Force
    $missingResult = Invoke-PackageFixture -Executable $executable -LockPath $fixtureLockPath -OutputDirectory $releaseDirectory
    Assert-True ($missingResult.ExitCode -eq 2) "missing DLL must exit 2, got $($missingResult.ExitCode): $($missingResult.Output)"
    Write-FixtureFile -Path $missingPath -Bytes $missingBytes

    # Mismatched OPUS companion: required contract exit code 3.
    $opusModel = @($fixtureLock.models | Where-Object { $_.directory -like '*opus-mt-en-pt' })[0]
    $companion = @($opusModel.files | Where-Object { $_.name -eq 'config.json' })[0]
    $companionPath = Join-Path (Join-Path $fixtureRoot ($opusModel.directory -replace '/', '\')) $companion.name
    $companionBytes = [IO.File]::ReadAllBytes($companionPath)
    Write-FixtureFile -Path $companionPath -Bytes ([Text.Encoding]::UTF8.GetBytes('tampered companion'))
    $mismatchResult = Invoke-PackageFixture -Executable $executable -LockPath $fixtureLockPath -OutputDirectory $releaseDirectory
    Assert-True ($mismatchResult.ExitCode -eq 3) "model mismatch must exit 3, got $($mismatchResult.ExitCode): $($mismatchResult.Output)"
    Write-FixtureFile -Path $companionPath -Bytes $companionBytes

    # Complete fixtures package and verify without requiring production NSIS,
    # minisign, or model inputs. Those external tools remain mandatory by default.
    $validResult = Invoke-PackageFixture -Executable $executable -LockPath $fixtureLockPath -OutputDirectory $releaseDirectory
    Assert-True ($validResult.ExitCode -eq 0) "complete fixture must package: $($validResult.Output)"

    $verifyResult = Invoke-VerifyFixture -Executable $executable -LockPath $fixtureLockPath -ReleaseDirectory $releaseDirectory
    Assert-True ($verifyResult.ExitCode -eq 0) "complete fixture must verify: $($verifyResult.Output)"

    $assetName = 'anycaptions-update-9.9.9.zip'
    $zipPath = Join-Path $releaseDirectory $assetName
    $manifestPath = Join-Path $releaseDirectory 'update.json'
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    Assert-True ($manifest.version -eq '9.9.9') 'manifest version'
    Assert-True ($manifest.asset -eq $assetName) 'manifest asset name'
    Assert-True ($manifest.size -eq (Get-Item -LiteralPath $zipPath).Length) 'manifest size matches update zip'
    Assert-True ($manifest.sha256 -ceq (Get-Sha256 -Path $zipPath)) 'manifest SHA-256 matches update zip'

    $expectedNames = @('anycaptions.exe') + @($fixtureLock.runtime.files | ForEach-Object { $_.name })
    [Array]::Sort($expectedNames, [StringComparer]::Ordinal)
    $archive = [IO.Compression.ZipFile]::OpenRead($zipPath)
    try {
        $actualNames = @($archive.Entries | ForEach-Object { $_.FullName })
        Assert-True ($actualNames.Count -eq 18) 'update zip has one executable and 17 DLLs'
        Assert-True (($actualNames -join "`n") -ceq ($expectedNames -join "`n")) 'update zip is flat, exact, and deterministically ordered'
        foreach ($entry in $archive.Entries) {
            Assert-True (-not $entry.FullName.Contains('/') -and -not $entry.FullName.Contains('\')) "flat entry: $($entry.FullName)"
            Assert-True ($entry.FullName -notlike '*model*') "no model payload: $($entry.FullName)"
            Assert-True ($entry.LastWriteTime.DateTime -eq [DateTime]::new(1980, 1, 1, 0, 0, 0)) "fixed wall timestamp: $($entry.FullName)"
        }
    }
    finally {
        $archive.Dispose()
    }
}
catch {
    $message = $_ | Out-String
    [Console]::Error.WriteLine($message)
    $failed = $true
}
finally {
    if (Test-Path -LiteralPath $fixtureRoot) {
        Remove-Item -LiteralPath $fixtureRoot -Recurse -Force
    }
}

if ($failed) {
    exit 1
}

Write-Host 'Packaging fixtures passed: missing DLL=2, model mismatch=3, valid manifest and flat deterministic zip.'
exit 0
