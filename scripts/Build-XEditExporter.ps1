[CmdletBinding()]
param(
    [System.IO.DirectoryInfo] $SourceDirectory = (
        Join-Path $PSScriptRoot '..\..\TES5Edit'
    ),

    [System.IO.DirectoryInfo] $WorktreeDirectory = (
        Join-Path $PSScriptRoot '..\target\xedit-source'
    ),

    [System.IO.DirectoryInfo] $BdsDirectory = (
        'C:\Program Files (x86)\Embarcadero\Studio\23.0'
    ),

    [ValidateSet('Debug', 'Release')]
    [string] $Configuration = 'Release',

    [ValidateSet('Win32', 'Win64')]
    [string] $Platform = 'Win32',

    [switch] $UseExistingIdeBuild
)

$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
$lockPath = Join-Path $root 'xedit-source.lock'
$patchDirectory = Join-Path $root 'xedit\patches'
$exporterSourceDirectory = Join-Path $root 'xedit\exporter'
$lock = Get-Content -LiteralPath $lockPath -Raw | ConvertFrom-Json

function Invoke-Git {
    param(
        [Parameter(Mandatory)]
        [string] $Repository,

        [Parameter(Mandatory)]
        [string[]] $Arguments,

        [switch] $AllowFailure
    )

    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $output = & git -c "safe.directory=$Repository" -C $Repository `
            @Arguments 2>&1
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    if (-not $AllowFailure -and $exitCode -ne 0) {
        throw "git $($Arguments -join ' ') failed:`n$($output -join "`n")"
    }
    [pscustomobject] @{
        ExitCode = $exitCode
        Output = $output
    }
}

function Get-TextSha256 {
    param(
        [Parameter(Mandatory)]
        [string] $Text
    )

    $bytes = [System.Text.Encoding]::UTF8.GetBytes($Text)
    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        ([System.BitConverter]::ToString(
            $sha256.ComputeHash($bytes)
        )).Replace('-', '').ToLowerInvariant()
    }
    finally {
        $sha256.Dispose()
    }
}

if (-not (Test-Path -LiteralPath $SourceDirectory.FullName -PathType Container)) {
    throw "TES5Edit source directory not found: $($SourceDirectory.FullName)"
}

if (-not (Test-Path -LiteralPath $WorktreeDirectory.FullName -PathType Container)) {
    New-Item -ItemType Directory -Force -Path (
        Split-Path -Parent $WorktreeDirectory.FullName
    ) | Out-Null
    Invoke-Git -Repository $SourceDirectory.FullName -Arguments @(
        'worktree',
        'add',
        '--detach',
        $WorktreeDirectory.FullName,
        [string] $lock.commit
    ) | Out-Null
}

$commitResult = Invoke-Git -Repository $WorktreeDirectory.FullName -Arguments @(
    'rev-parse',
    'HEAD'
)
$actualCommit = ([string](
    $commitResult.Output | Select-Object -Last 1
)).Trim()
if ($actualCommit -ne [string] $lock.commit) {
    throw "xEdit worktree is at $actualCommit, expected $($lock.commit)"
}

Invoke-Git -Repository $WorktreeDirectory.FullName -Arguments @(
    'submodule',
    'update',
    '--init',
    '--recursive'
) | Out-Null

$patches = @(
    Get-ChildItem -LiteralPath $patchDirectory -Filter '*.patch' |
        Sort-Object Name
)
if ($patches.Count -eq 0) {
    throw "No xEdit patches found in $patchDirectory"
}

$exporterSources = @(
    Get-ChildItem -LiteralPath $exporterSourceDirectory -Filter '*.pas' |
        Sort-Object Name
)
if ($exporterSources.Count -eq 0) {
    throw "No xEdit exporter sources found in $exporterSourceDirectory"
}

$patchInputs = @($patches) + @($exporterSources)
$patchDescriptor = (
    $patchInputs | ForEach-Object {
        $hash = (
            Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256
        ).Hash.ToLowerInvariant()
        "$($_.FullName.Substring($root.Path.Length + 1))=$hash"
    }
) -join "`n"
$patchSha256 = Get-TextSha256 -Text ($patchDescriptor + "`n")
$patchMarkerPath = Join-Path $WorktreeDirectory.FullName '.bethkit-patchset'
$appliedPatchSha256 = if (Test-Path -LiteralPath $patchMarkerPath) {
    (Get-Content -LiteralPath $patchMarkerPath -Raw).Trim()
}
else {
    ''
}

if ($appliedPatchSha256 -ne $patchSha256) {
    if ($appliedPatchSha256 -ne '') {
        throw (
            'The xEdit patch set changed after it was applied. Remove and ' +
            'recreate the isolated target\xedit-source worktree.'
        )
    }

    foreach ($patch in $patches) {
        $check = Invoke-Git -Repository $WorktreeDirectory.FullName `
            -AllowFailure -Arguments @('apply', '--check', $patch.FullName)
        if ($check.ExitCode -eq 0) {
            Invoke-Git -Repository $WorktreeDirectory.FullName -Arguments @(
                'apply',
                $patch.FullName
            ) | Out-Null
            continue
        }

        $reverseCheck = Invoke-Git -Repository $WorktreeDirectory.FullName `
            -AllowFailure -Arguments @(
                'apply',
                '--reverse',
                '--check',
                $patch.FullName
            )
        if ($reverseCheck.ExitCode -ne 0) {
            throw "Patch is neither applicable nor already applied: $($patch.Name)"
        }
    }

    [System.IO.File]::WriteAllText(
        $patchMarkerPath,
        "$patchSha256`n",
        [System.Text.UTF8Encoding]::new($false)
    )
}

foreach ($exporterSource in $exporterSources) {
    Copy-Item -LiteralPath $exporterSource.FullName -Destination (
        Join-Path $WorktreeDirectory.FullName "xDump\$($exporterSource.Name)"
    ) -Force
}

$dccName = if ($Platform -eq 'Win64') { 'dcc64.exe' } else { 'dcc32.exe' }
$dccPath = Join-Path $BdsDirectory.FullName "bin\$dccName"
$rsvarsPath = Join-Path $BdsDirectory.FullName 'bin\rsvars.bat'
if (-not (Test-Path -LiteralPath $dccPath -PathType Leaf)) {
    throw "Delphi compiler not found: $dccPath"
}
if (-not (Test-Path -LiteralPath $rsvarsPath -PathType Leaf)) {
    throw "Delphi environment script not found: $rsvarsPath"
}

$dccHash = (
    Get-FileHash -LiteralPath $dccPath -Algorithm SHA256
).Hash.ToLowerInvariant()
$dccVersion = (Get-Item -LiteralPath $dccPath).VersionInfo.FileVersion
$buildDescriptor = [ordered] @{
    source_commit = [string] $lock.commit
    patch_sha256 = $patchSha256
    delphi_compiler = $dccName
    delphi_file_version = $dccVersion
    delphi_sha256 = $dccHash
    configuration = $Configuration
    platform = $Platform
} | ConvertTo-Json -Compress
$buildSha256 = Get-TextSha256 -Text $buildDescriptor

$buildInfoPath = Join-Path (
    $WorktreeDirectory.FullName
) 'xDump\BethkitBuildInfo.inc'
$buildInfo = @"
  BETHKIT_XEDIT_SOURCE_TAG = '$($lock.tag)';
  BETHKIT_XEDIT_SOURCE_COMMIT = '$($lock.commit)';
  BETHKIT_XEDIT_SOURCE_ARCHIVE_SHA256 =
    '$($lock.archive_sha256)';
  BETHKIT_EXPORTER_VERSION = '1';
  BETHKIT_EXPORTER_PATCH_SHA256 =
    '$patchSha256';
  BETHKIT_EXPORTER_BUILD_SHA256 =
    '$buildSha256';
"@
[System.IO.File]::WriteAllText(
    $buildInfoPath,
    $buildInfo.Replace("`r`n", "`n"),
    [System.Text.UTF8Encoding]::new($false)
)

$projectPath = Join-Path $WorktreeDirectory.FullName 'xDump.dproj'
$expectedExecutable = Join-Path $WorktreeDirectory.FullName 'Build\xDump.exe'
$previousWriteTime = if (Test-Path -LiteralPath $expectedExecutable) {
    (Get-Item -LiteralPath $expectedExecutable).LastWriteTimeUtc
}
else {
    [datetime]::MinValue
}

if (-not $UseExistingIdeBuild) {
    $command = (
        'call "{0}" && msbuild "{1}" /t:Build ' +
        '/p:Config={2} /p:Platform={3} /v:minimal'
    ) -f $rsvarsPath, $projectPath, $Configuration, $Platform
    $buildOutput = & cmd.exe /d /c $command 2>&1
    $buildExitCode = $LASTEXITCODE
    $buildText = $buildOutput -join "`n"
    if ($buildText -match 'does not support command line compiling') {
        throw (
            'The installed Delphi license does not permit command-line ' +
            'builds. Open xDump.dproj in the Delphi IDE, select Release and ' +
            'Win32, build the stamped worktree, then rerun this script with ' +
            "-UseExistingIdeBuild.`n$buildText"
        )
    }
    if ($buildExitCode -ne 0) {
        throw "Delphi build failed with exit code ${buildExitCode}:`n$buildText"
    }
    if (-not (Test-Path -LiteralPath $expectedExecutable -PathType Leaf)) {
        throw "Delphi reported success but did not create $expectedExecutable"
    }
    if (
        (Get-Item -LiteralPath $expectedExecutable).LastWriteTimeUtc -le
        $previousWriteTime
    ) {
        throw "Delphi reported success but did not update $expectedExecutable"
    }
}
elseif (-not (Test-Path -LiteralPath $expectedExecutable -PathType Leaf)) {
    throw (
        "-UseExistingIdeBuild requires an IDE-built executable at " +
        $expectedExecutable
    )
}

$artifactDirectory = Join-Path $root 'target\xedit-exporter'
New-Item -ItemType Directory -Force -Path $artifactDirectory | Out-Null
$artifactPath = Join-Path $artifactDirectory 'bethkit-xedit-exporter.exe'
Copy-Item -LiteralPath $expectedExecutable -Destination $artifactPath -Force

$binarySha256 = (
    Get-FileHash -LiteralPath $artifactPath -Algorithm SHA256
).Hash.ToLowerInvariant()
$provenance = & $artifactPath --bethkit-provenance | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) {
    throw 'The built exporter failed its provenance self-test'
}
if ($provenance.exporter_binary_sha256 -ne $binarySha256) {
    throw 'The exporter reported a different binary SHA-256'
}
if ($provenance.exporter_patch_sha256 -ne $patchSha256) {
    throw 'The exporter reported a different patch SHA-256'
}
if ($provenance.exporter_build_sha256 -ne $buildSha256) {
    throw 'The exporter reported a different build SHA-256'
}

[pscustomobject] @{
    exporter = $artifactPath
    exporter_binary_sha256 = $binarySha256
    exporter_patch_sha256 = $patchSha256
    exporter_build_sha256 = $buildSha256
    source_commit = $actualCommit
}
