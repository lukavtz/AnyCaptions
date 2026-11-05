[CmdletBinding()]
param(
    [string]$Root,
    [string]$InputRoot,
    [string]$LockPath,
    [string]$ReleaseDirectory,
    [string]$Executable,
    [string]$Version,
    [string]$MinisignPath = 'minisign',
    [string]$PublicKey,
    [switch]$ProvisionFromUpstream,
    [switch]$LockOnly,
    [switch]$InputsOnly,
    [switch]$SkipInstaller,
    [switch]$SkipSignature
)

$invocationPath = [string]$MyInvocation.MyCommand.Path
if ([string]::IsNullOrWhiteSpace($invocationPath)) {
    throw 'Cannot resolve the release verifier script path.'
}
$scriptDirectory = Split-Path -Parent ([IO.Path]::GetFullPath($invocationPath))
if ([string]::IsNullOrWhiteSpace($Root)) {
    $Root = Split-Path -Parent $scriptDirectory
}
if ([string]::IsNullOrWhiteSpace($InputRoot)) {
    $InputRoot = $Root
}
if ([string]::IsNullOrWhiteSpace($LockPath)) {
    $LockPath = Join-Path $Root 'packaging/artifacts.lock.json'
}

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$requestedExitCode = 1
$maximumPackageBytes = 1024L * 1024L * 1024L

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

function Test-SafeName {
    param([string]$Name)
    return -not [string]::IsNullOrWhiteSpace($Name) `
        -and -not $Name.StartsWith('.') `
        -and $Name -match '^[A-Za-z0-9][A-Za-z0-9._-]*$' `
        -and $Name -notmatch '[/\\]'
}

function Test-RelativeDirectory {
    param([string]$Path)
    if ([string]::IsNullOrWhiteSpace($Path) -or [IO.Path]::IsPathRooted($Path)) {
        return $false
    }
    $segments = $Path -split '/'
    return $Path -notmatch '\\' -and (@($segments | Where-Object { $_ -in @('', '.', '..') })).Count -eq 0
}

function Assert-EntryShape {
    param([object]$Entry, [string]$Kind)
    if ($null -eq $Entry.PSObject.Properties['name'] -or
        $null -eq $Entry.PSObject.Properties['size'] -or
        $null -eq $Entry.PSObject.Properties['sha256']) {
        Stop-WithCode 4 "$Kind lock entries require name, size, and sha256 properties."
    }
    if (-not (Test-SafeName $Entry.name)) {
        Stop-WithCode 4 "Unsafe $Kind file name '$($Entry.name)'."
    }
    if ($Kind -eq 'DLL' -and -not $Entry.name.EndsWith('.dll', [StringComparison]::OrdinalIgnoreCase)) {
        Stop-WithCode 4 "Runtime lock entry is not a DLL: $($Entry.name)"
    }
    if ($null -ne $Entry.size -and ($Entry.size -isnot [byte] -and $Entry.size -isnot [int16] -and
        $Entry.size -isnot [int32] -and $Entry.size -isnot [int64] -or [long]$Entry.size -lt 0)) {
        Stop-WithCode 4 "Invalid size for $($Entry.name)."
    }
    if ($null -ne $Entry.sha256 -and ([string]$Entry.sha256 -notmatch '^[0-9a-f]{64}$')) {
        Stop-WithCode 4 "SHA-256 for $($Entry.name) must be lowercase hex or null."
    }
}

function Read-Lock {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        Stop-WithCode 4 "Artifact lock not found: $Path"
    }
    try {
        $lock = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
    }
    catch {
        Stop-WithCode 4 "Cannot parse artifact lock ${Path}: $($_.Exception.Message)"
    }
    if ($lock.schemaVersion -ne 1 -or [string]::IsNullOrWhiteSpace($lock.minisignPublicKey)) {
        Stop-WithCode 4 'Unsupported lock schema or missing minisign public key.'
    }
    if ($null -eq $lock.runtime -or $lock.runtime.expectedDllCount -ne 17 -or
        -not (Test-RelativeDirectory $lock.runtime.directory)) {
        Stop-WithCode 4 'Runtime lock must use a safe relative directory and exactly 17 DLLs.'
    }
    $dllNames = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($entry in @($lock.runtime.files)) {
        Assert-EntryShape $entry 'DLL'
        if (-not $dllNames.Add([string]$entry.name)) {
            Stop-WithCode 4 "Duplicate DLL lock entry: $($entry.name)"
        }
    }
    if ($dllNames.Count -ne 17 -or @($lock.runtime.files).Count -ne 17) {
        Stop-WithCode 4 'Runtime lock must contain exactly 17 unique DLLs.'
    }

    $requiredModels = @{
        'backend/models/asr/nemotron-3.5-asr-streaming-0.6b' = @('nemotron-3.5-asr-streaming-0.6b.q8_0.gguf')
        'backend/models/translation/opus-mt-en-pt' = @('config.json', 'model.bin', 'shared_vocabulary.json', 'source.spm', 'target.spm')
        'backend/models/translation/opus-mt-pt-en' = @('config.json', 'model.bin', 'shared_vocabulary.json', 'source.spm', 'target.spm')
    }
    if (@($lock.models).Count -ne $requiredModels.Count) {
        Stop-WithCode 4 'Lock must contain the ASR directory and both OPUS directories only.'
    }
    $modelDirectories = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($model in @($lock.models)) {
        if (-not $modelDirectories.Add([string]$model.directory)) {
            Stop-WithCode 4 "Duplicate model directory: $($model.directory)"
        }
        if (-not (Test-RelativeDirectory $model.directory) -or -not $requiredModels.ContainsKey([string]$model.directory)) {
            Stop-WithCode 4 "Unexpected model directory: $($model.directory)"
        }
        $actualNames = @($model.files | ForEach-Object { $_.name })
        $expectedNames = @($requiredModels[$model.directory])
        [Array]::Sort($actualNames, [StringComparer]::Ordinal)
        [Array]::Sort($expectedNames, [StringComparer]::Ordinal)
        if (($actualNames -join "`n") -cne ($expectedNames -join "`n")) {
            Stop-WithCode 4 "Model file set does not match the required layout: $($model.directory)"
        }
        $names = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        foreach ($entry in @($model.files)) {
            Assert-EntryShape $entry 'model'
            if (-not $names.Add([string]$entry.name)) {
                Stop-WithCode 4 "Duplicate model lock entry: $($model.directory)/$($entry.name)"
            }
        }
    }
    return $lock
}

function Get-RuntimePath {
    param([object]$Lock, [string]$SourceRoot, [object]$Entry)
    return (Join-Path (Join-Path $SourceRoot ($Lock.runtime.directory -replace '/', '\')) $Entry.name)
}

function Get-ModelPath {
    param([object]$Model, [string]$SourceRoot, [object]$Entry)
    return (Join-Path (Join-Path $SourceRoot ($Model.directory -replace '/', '\')) $Entry.name)
}

function Assert-CompletePin {
    param([object]$Entry, [string]$DisplayPath)
    if ($null -eq $Entry.size -or [string]::IsNullOrWhiteSpace([string]$Entry.sha256)) {
        Stop-WithCode 4 "Missing reviewed production size/SHA-256 pin for $DisplayPath. Refusing to package an unpinned input."
    }
}

function Assert-PinnedFile {
    param([string]$Path, [object]$Entry, [string]$DisplayPath)
    $item = Get-Item -LiteralPath $Path
    if ($item.Length -ne [long]$Entry.size) {
        Stop-WithCode 3 "Size mismatch for $DisplayPath. Expected $($Entry.size), got $($item.Length)."
    }
    $actualHash = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -cne $Entry.sha256) {
        Stop-WithCode 3 "SHA-256 mismatch for $DisplayPath. Expected $($Entry.sha256), got $actualHash."
    }
}

function Assert-ExactFileSet {
    param([string]$Directory, [string[]]$ExpectedNames, [switch]$DllOnly)
    $actualNames = @([IO.Directory]::EnumerateFiles($Directory) | ForEach-Object {
        $name = [IO.Path]::GetFileName($_)
        if (-not $DllOnly -or $name.EndsWith('.dll', [StringComparison]::OrdinalIgnoreCase)) {
            $name
        }
    })
    [Array]::Sort($actualNames, [StringComparer]::Ordinal)
    [Array]::Sort($ExpectedNames, [StringComparer]::Ordinal)
    if (($actualNames -join "`n") -cne ($ExpectedNames -join "`n")) {
        Stop-WithCode 4 "Unlocked or missing files in $Directory. Actual: $($actualNames -join ', ')"
    }
}

function Assert-ExternalInputs {
    param([object]$Lock, [string]$SourceRoot)

    # Missing files take precedence over incomplete pins and always produce 2.
    foreach ($entry in @($Lock.runtime.files)) {
        $path = Get-RuntimePath $Lock $SourceRoot $entry
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            Stop-WithCode 2 "Missing locked DLL: $path"
        }
    }
    foreach ($model in @($Lock.models)) {
        foreach ($entry in @($model.files)) {
            $path = Get-ModelPath $model $SourceRoot $entry
            if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
                Stop-WithCode 2 "Missing locked model input: $path"
            }
        }
    }

    foreach ($entry in @($Lock.runtime.files)) {
        Assert-CompletePin $entry "$($Lock.runtime.directory)/$($entry.name)"
    }
    foreach ($model in @($Lock.models)) {
        foreach ($entry in @($model.files)) {
            Assert-CompletePin $entry "$($model.directory)/$($entry.name)"
        }
    }

    foreach ($entry in @($Lock.runtime.files)) {
        Assert-PinnedFile (Get-RuntimePath $Lock $SourceRoot $entry) $entry "$($Lock.runtime.directory)/$($entry.name)"
    }
    $runtimeDirectory = Join-Path $SourceRoot ($Lock.runtime.directory -replace '/', '\')
    Assert-ExactFileSet $runtimeDirectory @($Lock.runtime.files | ForEach-Object { $_.name }) -DllOnly

    foreach ($model in @($Lock.models)) {
        foreach ($entry in @($model.files)) {
            Assert-PinnedFile (Get-ModelPath $model $SourceRoot $entry) $entry "$($model.directory)/$($entry.name)"
        }
        $modelDirectory = Join-Path $SourceRoot ($model.directory -replace '/', '\')
        if (@([IO.Directory]::EnumerateDirectories($modelDirectory)).Count -ne 0) {
            Stop-WithCode 4 "Unexpected subdirectory in locked model directory: $modelDirectory"
        }
        Assert-ExactFileSet $modelDirectory @($model.files | ForEach-Object { $_.name })
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

function Get-UpstreamAsset {
    param([string]$BaseUrl, [string]$Name, [object]$Metadata, [string]$Destination)
    if ($null -eq $Metadata -or $null -eq $Metadata.size -or
        [string]::IsNullOrWhiteSpace([string]$Metadata.sha256)) {
        Stop-WithCode 4 "Upstream asset size/SHA-256 is missing for $Name."
    }
    try {
        Invoke-WebRequest -Uri "$BaseUrl/$Name" -OutFile $Destination -UseBasicParsing | Out-Null
    }
    catch {
        Stop-WithCode 5 "Could not download verified upstream asset ${Name}: $($_.Exception.Message)"
    }
    $item = Get-Item -LiteralPath $Destination
    if ($item.Length -ne [long]$Metadata.size) {
        Stop-WithCode 3 "Upstream asset size mismatch for $Name. Expected $($Metadata.size), got $($item.Length)."
    }
    $actualHash = (Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -cne $Metadata.sha256) {
        Stop-WithCode 3 "Upstream asset SHA-256 mismatch for $Name. Expected $($Metadata.sha256), got $actualHash."
    }
}

function Install-UpstreamInputs {
    param([object]$Lock, [string]$SourceRoot)
    $version = [string]$Lock.upstreamRelease.version
    $baseUrl = [string]$Lock.upstreamRelease.baseUrl
    if ([string]::IsNullOrWhiteSpace($version) -or [string]::IsNullOrWhiteSpace($baseUrl)) {
        Stop-WithCode 4 'Upstream release version/baseUrl is required for provisioning.'
    }
    $updateName = "anycaptions-update-$version.zip"
    $installerName = "AnyCaptions_${version}_x64-setup.exe"
    $updateProperty = $Lock.upstreamRelease.assets.PSObject.Properties[$updateName]
    $installerProperty = $Lock.upstreamRelease.assets.PSObject.Properties[$installerName]
    if ($null -eq $updateProperty -or $null -eq $installerProperty) {
        Stop-WithCode 4 'Upstream update zip and installer metadata are required for provisioning.'
    }

    $work = Join-Path ([IO.Path]::GetTempPath()) "anycaptions-upstream-$([Guid]::NewGuid().ToString('N'))"
    [IO.Directory]::CreateDirectory($work) | Out-Null
    try {
        $updatePath = Join-Path $work $updateName
        $installerPath = Join-Path $work $installerName
        Get-UpstreamAsset $baseUrl $updateName $updateProperty.Value $updatePath
        Get-UpstreamAsset $baseUrl $installerName $installerProperty.Value $installerPath

        # The official update archive supplies only the locked DLL set. Its own
        # upstream asset hash is checked before any archive entry is consumed.
        $runtimeDirectory = Join-Path $SourceRoot ($Lock.runtime.directory -replace '/', '\')
        [IO.Directory]::CreateDirectory($runtimeDirectory) | Out-Null
        $archive = [IO.Compression.ZipFile]::OpenRead($updatePath)
        try {
            [string[]]$actualNames = @($archive.Entries | ForEach-Object { $_.FullName })
            [string[]]$expectedNames = @('anycaptions.exe') + @($Lock.runtime.files | ForEach-Object { $_.name })
            [Array]::Sort($actualNames, [StringComparer]::Ordinal)
            [Array]::Sort($expectedNames, [StringComparer]::Ordinal)
            if ($actualNames.Count -ne 18 -or ($actualNames -join "`n") -cne ($expectedNames -join "`n")) {
                Stop-WithCode 3 'Official update zip does not match the expected executable/DLL layout.'
            }
            foreach ($entry in @($Lock.runtime.files)) {
                $zipEntry = $archive.GetEntry($entry.name)
                $source = $zipEntry.Open()
                $destination = [IO.File]::Open(
                    (Join-Path $runtimeDirectory $entry.name),
                    [IO.FileMode]::Create,
                    [IO.FileAccess]::Write,
                    [IO.FileShare]::None
                )
                try {
                    $source.CopyTo($destination)
                }
                finally {
                    $destination.Dispose()
                    $source.Dispose()
                }
            }
        }
        finally {
            $archive.Dispose()
        }

        # The hash-verified official NSIS installer is the source of the three
        # shipped model directories. /D must be last; NSIS treats the remainder
        # of the command line as the extraction/install path.
        $installRoot = Join-Path $work 'installed'
        try {
            $process = Start-Process -FilePath $installerPath -ArgumentList @('/S', "/D=$installRoot") -WorkingDirectory $work -Wait -PassThru
        }
        catch {
            Stop-WithCode 5 "Could not execute verified upstream installer: $($_.Exception.Message)"
        }
        if ($process.ExitCode -ne 0) {
            Stop-WithCode 5 "Verified upstream installer failed with exit code $($process.ExitCode)."
        }
        foreach ($model in @($Lock.models)) {
            $installRelative = ([string]$model.directory).Substring('backend/'.Length)
            $sourceDirectory = Join-Path $installRoot ($installRelative -replace '/', '\')
            $destinationDirectory = Join-Path $SourceRoot ($model.directory -replace '/', '\')
            [IO.Directory]::CreateDirectory($destinationDirectory) | Out-Null
            foreach ($entry in @($model.files)) {
                $sourcePath = Join-Path $sourceDirectory $entry.name
                if (-not (Test-Path -LiteralPath $sourcePath -PathType Leaf)) {
                    Stop-WithCode 2 "Verified upstream installer did not produce model input: $sourcePath"
                }
                Copy-Item -LiteralPath $sourcePath -Destination (Join-Path $destinationDirectory $entry.name) -Force
            }
        }
        Write-Host "Provisioned and hash-checked official upstream v$version DLL/model inputs."
    }
    finally {
        Remove-Item -LiteralPath $work -Recurse -Force
    }
}

function Get-StreamHash {
    param([System.IO.Stream]$Stream)
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($sha.ComputeHash($Stream))).Replace('-', '').ToLowerInvariant()
    }
    finally {
        $sha.Dispose()
    }
}

function Assert-ReleaseOutputs {
    param(
        [object]$Lock,
        [string]$SourceRoot,
        [string]$OutputDirectory,
        [string]$ExecutablePath,
        [string]$ReleaseVersion,
        [string]$MinisignCommand,
        [string]$VerificationKey
    )

    $asset = "anycaptions-update-$ReleaseVersion.zip"
    $zipPath = Join-Path $OutputDirectory $asset
    $manifestPath = Join-Path $OutputDirectory 'update.json'
    $signaturePath = Join-Path $OutputDirectory 'update.json.minisig'
    $installerPath = Join-Path $OutputDirectory "AnyCaptions_${ReleaseVersion}_x64-setup.exe"
    foreach ($path in @($zipPath, $manifestPath)) {
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            Stop-WithCode 2 "Missing release output: $path"
        }
    }
    if (-not $SkipInstaller -and -not (Test-Path -LiteralPath $installerPath -PathType Leaf)) {
        Stop-WithCode 2 "Missing release output: $installerPath"
    }
    if (-not $SkipSignature -and -not (Test-Path -LiteralPath $signaturePath -PathType Leaf)) {
        Stop-WithCode 2 "Missing release output: $signaturePath"
    }

    if (-not $SkipSignature) {
        $minisign = Resolve-Tool $MinisignCommand
        & $minisign -V -m $manifestPath -x $signaturePath -P $VerificationKey
        if ($LASTEXITCODE -ne 0) {
            Stop-WithCode 5 "Minisign verification failed for $manifestPath."
        }
    }

    try {
        $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    }
    catch {
        Stop-WithCode 3 "Cannot parse update manifest: $($_.Exception.Message)"
    }
    if ($manifest.version -cne $ReleaseVersion -or $manifest.asset -cne $asset -or
        ($manifest.size -isnot [int64] -and $manifest.size -isnot [int32]) -or
        [long]$manifest.size -le 0 -or [long]$manifest.size -gt $maximumPackageBytes -or
        ([string]$manifest.sha256) -cnotmatch '^[0-9a-f]{64}$') {
        Stop-WithCode 3 'Update manifest fields do not match the release contract.'
    }
    $zipItem = Get-Item -LiteralPath $zipPath
    $zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ([long]$manifest.size -ne $zipItem.Length -or $manifest.sha256 -cne $zipHash) {
        Stop-WithCode 3 'Update manifest size or SHA-256 does not match the update zip.'
    }

    $sourceByName = @{ 'anycaptions.exe' = $ExecutablePath }
    $hashByName = @{}
    foreach ($entry in @($Lock.runtime.files)) {
        $sourceByName[$entry.name] = Get-RuntimePath $Lock $SourceRoot $entry
        $hashByName[$entry.name] = [string]$entry.sha256
    }
    $hashByName['anycaptions.exe'] = (Get-FileHash -LiteralPath $ExecutablePath -Algorithm SHA256).Hash.ToLowerInvariant()
    $expectedNames = @($sourceByName.Keys)
    [Array]::Sort($expectedNames, [StringComparer]::Ordinal)

    try {
        $archive = [IO.Compression.ZipFile]::OpenRead($zipPath)
    }
    catch {
        Stop-WithCode 3 "Cannot open update zip: $($_.Exception.Message)"
    }
    try {
        $actualNames = @($archive.Entries | ForEach-Object { $_.FullName })
        if ($actualNames.Count -ne 18 -or ($actualNames -join "`n") -cne ($expectedNames -join "`n")) {
            Stop-WithCode 3 'Update zip must contain exactly anycaptions.exe and the 17 locked DLLs, flat and sorted.'
        }
        foreach ($entry in $archive.Entries) {
            if ($entry.FullName.Contains('/') -or $entry.FullName.Contains('\') -or
                $entry.LastWriteTime.DateTime -ne [DateTime]::new(1980, 1, 1, 0, 0, 0)) {
                Stop-WithCode 3 "Unsafe path or non-deterministic timestamp in update entry: $($entry.FullName)"
            }
            $source = Get-Item -LiteralPath $sourceByName[$entry.FullName]
            if ($entry.Length -ne $source.Length) {
                Stop-WithCode 3 "Update entry size differs from locked input: $($entry.FullName)"
            }
            $stream = $entry.Open()
            try {
                $entryHash = Get-StreamHash $stream
            }
            finally {
                $stream.Dispose()
            }
            if ($entryHash -cne $hashByName[$entry.FullName]) {
                Stop-WithCode 3 "Update entry content differs from its input: $($entry.FullName)"
            }
        }
    }
    finally {
        $archive.Dispose()
    }

    if (-not $SkipInstaller -and (Get-Item -LiteralPath $installerPath).Length -le 0) {
        Stop-WithCode 3 "Installer is empty: $installerPath"
    }
}

Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

$scriptExitCode = 0
try {
    $lock = Read-Lock $LockPath
    if ([string]::IsNullOrWhiteSpace($Executable)) {
        $Executable = Join-Path $Root 'target/release/anycaptions.exe'
    }
    if ([string]::IsNullOrWhiteSpace($PublicKey)) {
        $PublicKey = [string]$lock.minisignPublicKey
    }

    if ($LockOnly) {
        Write-Host 'Artifact lock schema is valid. File existence and incomplete production pins were not checked.'
    }
    else {
        if ($ProvisionFromUpstream) {
            Install-UpstreamInputs $lock $InputRoot
        }
        Assert-ExternalInputs $lock $InputRoot
        if ($InputsOnly) {
            Write-Host 'All external DLL/model inputs exist and match their reviewed size/SHA-256 pins.'
        }
        else {
            Assert-Version $Version
            if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
                Stop-WithCode 2 "Missing executable: $Executable"
            }
            if ([string]::IsNullOrWhiteSpace($ReleaseDirectory)) {
                $ReleaseDirectory = Join-Path $Root "release/$Version"
            }
            Assert-ReleaseOutputs $lock $InputRoot $ReleaseDirectory $Executable $Version $MinisignPath $PublicKey
            $skips = @()
            if ($SkipInstaller) { $skips += 'installer' }
            if ($SkipSignature) { $skips += 'minisign' }
            $suffix = if ($skips.Count -eq 0) { '' } else { " (fixture-only checks skipped: $($skips -join ', '))" }
            Write-Host "Release verification passed for $Version$suffix."
        }
    }
}
catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    $scriptExitCode = $requestedExitCode
}

exit $scriptExitCode
