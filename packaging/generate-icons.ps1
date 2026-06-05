param(
    [string]$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")),
    [string]$SourceSvg = "",
    [string]$OutputDir = ""
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($SourceSvg)) {
    $SourceSvg = Join-Path $Root "packaging/icons/courier.svg"
}
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
    $OutputDir = Join-Path $Root "packaging/icons"
}

if (-not (Test-Path -LiteralPath $SourceSvg)) {
    throw "Source SVG is missing: $SourceSvg"
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$png = Join-Path $OutputDir "courier.png"
$ico = Join-Path $OutputDir "courier.ico"
$icns = Join-Path $OutputDir "courier.icns"

function Require-AnyCommand($names) {
    foreach ($name in $names) {
        $command = Get-Command $name -ErrorAction SilentlyContinue
        if ($command) {
            return $command.Source
        }
    }
    throw "None of these icon tools are installed: $($names -join ', ')"
}

function Convert-SvgToPng($source, $destination, $size) {
    $magick = Get-Command "magick" -ErrorAction SilentlyContinue
    if ($magick) {
        & $magick.Source -background none -density 384 $source -resize "${size}x${size}" $destination
        if ($LASTEXITCODE -ne 0) {
            throw "magick failed while generating $destination"
        }
        return
    }

    $rsvg = Get-Command "rsvg-convert" -ErrorAction SilentlyContinue
    if ($rsvg) {
        & $rsvg.Source -w $size -h $size -o $destination $source
        if ($LASTEXITCODE -ne 0) {
            throw "rsvg-convert failed while generating $destination"
        }
        return
    }

    throw "SVG to PNG conversion requires ImageMagick magick or rsvg-convert"
}

function Generate-Png {
    Convert-SvgToPng $SourceSvg $png 256
    Write-Host "Generated Linux PNG icon: $png"
}

function Generate-Ico {
    $magick = Require-AnyCommand @("magick", "convert")
    $sizes = @(16, 24, 32, 48, 64, 128, 256)
    $work = Join-Path $OutputDir "ico-work"
    New-Item -ItemType Directory -Force -Path $work | Out-Null
    $inputs = @()
    foreach ($size in $sizes) {
        $out = Join-Path $work "courier-$size.png"
        Convert-SvgToPng $SourceSvg $out $size
        $inputs += $out
    }
    & $magick @inputs $ico
    if ($LASTEXITCODE -ne 0) {
        throw "ImageMagick failed while generating $ico"
    }
    Write-Host "Generated Windows ICO icon: $ico"
}

function Generate-Icns {
    if (-not $IsMacOS) {
        Write-Warning "Skipping ICNS generation: iconutil is only available on macOS"
        return
    }

    $iconutil = Require-AnyCommand @("iconutil")
    $iconset = Join-Path $OutputDir "courier.iconset"
    New-Item -ItemType Directory -Force -Path $iconset | Out-Null

    foreach ($entry in @(
        @{Name = "icon_16x16.png"; Size = 16},
        @{Name = "icon_16x16@2x.png"; Size = 32},
        @{Name = "icon_32x32.png"; Size = 32},
        @{Name = "icon_32x32@2x.png"; Size = 64},
        @{Name = "icon_128x128.png"; Size = 128},
        @{Name = "icon_128x128@2x.png"; Size = 256},
        @{Name = "icon_256x256.png"; Size = 256},
        @{Name = "icon_256x256@2x.png"; Size = 512},
        @{Name = "icon_512x512.png"; Size = 512},
        @{Name = "icon_512x512@2x.png"; Size = 1024}
    )) {
        Convert-SvgToPng $SourceSvg (Join-Path $iconset $entry.Name) $entry.Size
    }

    & $iconutil -c icns $iconset -o $icns
    if ($LASTEXITCODE -ne 0) {
        throw "iconutil failed while generating $icns"
    }
    Write-Host "Generated macOS ICNS icon: $icns"
}

Generate-Png
Generate-Ico
Generate-Icns
