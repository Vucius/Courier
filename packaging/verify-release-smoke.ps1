param(
    [string]$Root = (Resolve-Path (Join-Path $PSScriptRoot ".."))
)

$ErrorActionPreference = "Stop"

$requiredFiles = @(
    "Cargo.toml",
    "README.md",
    "LICENSE",
    "packaging/courier.app.toml",
    "packaging/release-manifest.json",
    "packaging/migrations.md",
    "packaging/installers/courier.installers.toml",
    "packaging/icons/courier.svg",
    "migrations/001_init.sql",
    "migrations/002_search.sql"
)

foreach ($relative in $requiredFiles) {
    $path = Join-Path $Root $relative
    if (-not (Test-Path -LiteralPath $path)) {
        throw "Missing release smoke input: $relative"
    }
}

$cargoToml = Get-Content -Raw -LiteralPath (Join-Path $Root "Cargo.toml")
$manifest = Get-Content -Raw -LiteralPath (Join-Path $Root "packaging/release-manifest.json") |
    ConvertFrom-Json
$appToml = Get-Content -Raw -LiteralPath (Join-Path $Root "packaging/courier.app.toml")
$installerToml = Get-Content -Raw -LiteralPath (Join-Path $Root "packaging/installers/courier.installers.toml")
$migrationNotes = Get-Content -Raw -LiteralPath (Join-Path $Root "packaging/migrations.md")

function Read-IntegerValue($content, $name) {
    $match = [regex]::Match($content, "(?m)^\s*$name\s*=\s*(\d+)\s*$")
    if (-not $match.Success) {
        throw "Unable to read $name"
    }
    [int]$match.Groups[1].Value
}

$cargoDatabaseSchema = Read-IntegerValue $cargoToml "database_schema"
$cargoConfigSchema = Read-IntegerValue $cargoToml "config_schema"
$appDatabaseSchema = Read-IntegerValue $appToml "minimum_database_schema"
$appConfigSchema = Read-IntegerValue $appToml "minimum_config_schema"

if ([int]$manifest.database_schema -ne $cargoDatabaseSchema) {
    throw "release-manifest database_schema does not match workspace metadata"
}
if ([int]$manifest.config_schema -ne $cargoConfigSchema) {
    throw "release-manifest config_schema does not match workspace metadata"
}
if ($appDatabaseSchema -ne $cargoDatabaseSchema) {
    throw "courier.app.toml minimum_database_schema does not match workspace metadata"
}
if ($appConfigSchema -ne $cargoConfigSchema) {
    throw "courier.app.toml minimum_config_schema does not match workspace metadata"
}

if ($manifest.installer_metadata -ne "packaging/installers/courier.installers.toml") {
    throw "release-manifest installer_metadata does not point at the installer metadata file"
}
if (-not $manifest.data_preservation.preserve_on_uninstall) {
    throw "release-manifest must preserve user data on uninstall"
}

$manifestText = Get-Content -Raw -LiteralPath (Join-Path $Root "packaging/release-manifest.json")
if ($manifestText.Contains("pending-")) {
    throw "release-manifest still contains pending installer placeholders"
}

foreach ($needle in @(
    "preserve_user_data_on_uninstall = true",
    'format = "msi"',
    'format = "dmg"',
    "linux.appimage",
    "linux.deb",
    "linux.rpm"
)) {
    if (-not $installerToml.Contains($needle)) {
        throw "packaging/installers/courier.installers.toml is missing $needle"
    }
}

foreach ($needle in @("Storage::initialize_with_report", "001_init.sql", "002_search.sql")) {
    if (-not $migrationNotes.Contains($needle)) {
        throw "packaging/migrations.md is missing $needle"
    }
}

$artifactFiles = @(
    "courier.app.toml",
    "release-manifest.json",
    "migrations.md",
    "installers/courier.installers.toml",
    "icons/courier.svg"
)
foreach ($relative in $artifactFiles) {
    $path = Join-Path $Root "packaging/$relative"
    if ((Get-Item -LiteralPath $path).Length -le 0) {
        throw "Release metadata file is empty: $relative"
    }
}

Write-Host "Release smoke metadata verified: database schema $cargoDatabaseSchema, config schema $cargoConfigSchema"
