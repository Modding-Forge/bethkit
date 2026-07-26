[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [System.IO.FileInfo] $Exporter,

    [Parameter(Mandatory)]
    [ValidatePattern('^[0-9a-fA-F]{64}$')]
    [string] $ExpectedExporterSha256,

    [Parameter(Mandatory)]
    [ValidatePattern('^[0-9a-fA-F]{64}$')]
    [string] $ExpectedPatchSha256,

    [Parameter(Mandatory)]
    [ValidatePattern('^[0-9a-fA-F]{64}$')]
    [string] $ExpectedBuildSha256,

    [System.IO.DirectoryInfo] $OutputDirectory = (
        Join-Path $PSScriptRoot '..\target\xedit-definitions'
    )
)

$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
$cargo = if (Get-Command cargo -ErrorAction SilentlyContinue) {
    (Get-Command cargo).Source
}
else {
    Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
}
if (-not (Test-Path -LiteralPath $cargo -PathType Leaf)) {
    throw 'Cargo was not found'
}

& (Join-Path $PSScriptRoot 'Test-XEditExporter.ps1') `
    -Exporter $Exporter `
    -ExpectedExporterSha256 $ExpectedExporterSha256 `
    -ExpectedPatchSha256 $ExpectedPatchSha256 `
    -ExpectedBuildSha256 $ExpectedBuildSha256 | Out-Null

New-Item -ItemType Directory -Force -Path $OutputDirectory.FullName | Out-Null
$games = @(
    'skyrim_le',
    'skyrim_se',
    'skyrim_vr',
    'fallout_3',
    'fallout_nv',
    'fallout_4',
    'fallout_4_vr',
    'fallout_76',
    'oblivion',
    'morrowind',
    'starfield'
)

$exports = @()
foreach ($game in $games) {
    $primary = Join-Path $OutputDirectory.FullName "$game.json"
    $verification = Join-Path $OutputDirectory.FullName "$game.verify.json"
    & $Exporter.FullName --bethkit-export --game $game --output $primary
    if ($LASTEXITCODE -ne 0) {
        throw "Exporter failed for $game with exit code $LASTEXITCODE"
    }
    & $Exporter.FullName --bethkit-export --game $game --output $verification
    if ($LASTEXITCODE -ne 0) {
        throw "Exporter verification run failed for $game"
    }
    $primaryHash = (
        Get-FileHash -LiteralPath $primary -Algorithm SHA256
    ).Hash
    $verificationHash = (
        Get-FileHash -LiteralPath $verification -Algorithm SHA256
    ).Hash
    if ($primaryHash -ne $verificationHash) {
        throw "xEdit export is not deterministic for $game"
    }
    $exports += $primary
}

$inventory = Join-Path $OutputDirectory.FullName 'callback-inventory.json'
& $cargo run --locked -p bethkit-schema --bin bethkit-xedit-converter -- `
    inventory $inventory @exports
if ($LASTEXITCODE -ne 0) {
    throw 'Callback inventory validation failed'
}

$rules = Join-Path $root 'xedit\conversion-rules.json'
$audit = Join-Path $OutputDirectory.FullName 'callback-audit.json'
& $cargo run --locked -p bethkit-schema --bin bethkit-xedit-converter -- `
    audit $inventory $rules $audit
if ($LASTEXITCODE -ne 0) {
    throw 'Callback classification audit failed'
}

[pscustomobject] @{
    output_directory = $OutputDirectory.FullName
    callback_inventory = $inventory
    callback_audit = $audit
    games = $games.Count
}
