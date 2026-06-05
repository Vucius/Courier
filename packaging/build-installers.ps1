param(
    [string]$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")),
    [string]$Version = "0.1.0",
    [ValidateSet("all", "windows", "macos", "linux")]
    [string]$Platform = "all",
    [string]$TargetDir = "",
    [string]$OutputDir = ""
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($TargetDir)) {
    $TargetDir = Join-Path $Root "target/release"
}
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
    $OutputDir = Join-Path $Root "target/release/installers"
}

$manifest = Get-Content -Raw -LiteralPath (Join-Path $Root "packaging/release-manifest.json") |
    ConvertFrom-Json
$installerToml = Get-Content -Raw -LiteralPath (Join-Path $Root "packaging/installers/courier.installers.toml")
$binaryName = $manifest.binary
$windowsBinary = Join-Path $TargetDir "$binaryName.exe"
$unixBinary = Join-Path $TargetDir $binaryName

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

function Read-TomlString($content, $name) {
    $match = [regex]::Match($content, "(?m)^\s*$name\s*=\s*`"([^`"]+)`"\s*$")
    if (-not $match.Success) {
        throw "Unable to read TOML string: $name"
    }
    $match.Groups[1].Value
}

function Expand-ArtifactName($name) {
    $name.Replace('${version}', $Version)
}

function Require-Command($name) {
    $command = Get-Command $name -ErrorAction SilentlyContinue
    if (-not $command) {
        throw "Required installer tool is missing: $name"
    }
    $command.Source
}

function Copy-Required($source, $destination) {
    if (-not (Test-Path -LiteralPath $source)) {
        throw "Required packaging input is missing: $source"
    }
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
    Copy-Item -LiteralPath $source -Destination $destination -Force
}

function Build-WindowsInstaller {
    if (-not (Test-Path -LiteralPath $windowsBinary)) {
        throw "Windows release binary is missing: $windowsBinary"
    }

    $wix = Require-Command "wix"
    $artifact = Join-Path $OutputDir (Expand-ArtifactName $manifest.artifacts.windows.installer)
    $work = Join-Path $OutputDir "windows-wix"
    New-Item -ItemType Directory -Force -Path $work | Out-Null

    $license = Join-Path $Root "LICENSE"
    $icon = Join-Path $Root (Read-TomlString $installerToml "icon")
    $wxs = Join-Path $work "Courier.wxs"
    $upgradeCode = Read-TomlString $installerToml "upgrade_code"
    $bundleId = $manifest.bundle_identifier
    $publisher = $manifest.publisher

    @"
<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">
  <Package Name="$($manifest.name)" Manufacturer="$publisher" Version="$Version" UpgradeCode="$upgradeCode" Scope="perUser">
    <MajorUpgrade DowngradeErrorMessage="A newer version of Courier is already installed." />
    <MediaTemplate EmbedCab="yes" />
    <Icon Id="CourierIcon" SourceFile="$icon" />
    <Property Id="ARPPRODUCTICON" Value="CourierIcon" />
    <Feature Id="MainFeature" Title="$($manifest.name)" Level="1">
      <ComponentGroupRef Id="CourierComponents" />
    </Feature>
  </Package>
  <Fragment>
    <StandardDirectory Id="LocalAppDataFolder">
      <Directory Id="CourierInstallDir" Name="$bundleId" />
    </StandardDirectory>
  </Fragment>
  <Fragment>
    <ComponentGroup Id="CourierComponents" Directory="CourierInstallDir">
      <Component Id="CourierBinary">
        <File Source="$windowsBinary" KeyPath="yes" />
      </Component>
      <Component Id="CourierLicense">
        <File Source="$license" KeyPath="yes" />
      </Component>
    </ComponentGroup>
  </Fragment>
</Wix>
"@ | Set-Content -LiteralPath $wxs -Encoding UTF8

    & $wix build $wxs -out $artifact
    if ($LASTEXITCODE -ne 0) {
        throw "WiX failed with exit code $LASTEXITCODE"
    }
    Write-Host "Built Windows installer: $artifact"
}

function Build-MacosInstaller {
    if (-not (Test-Path -LiteralPath $unixBinary)) {
        throw "macOS release binary is missing: $unixBinary"
    }

    $hdiutil = Require-Command "hdiutil"
    $bundleName = $manifest.artifacts.macos.bundle
    $artifact = Join-Path $OutputDir (Expand-ArtifactName $manifest.artifacts.macos.dmg)
    $appRoot = Join-Path $OutputDir $bundleName
    $contents = Join-Path $appRoot "Contents"
    $macos = Join-Path $contents "MacOS"
    $resources = Join-Path $contents "Resources"
    New-Item -ItemType Directory -Force -Path $macos, $resources | Out-Null
    Copy-Required $unixBinary (Join-Path $macos $binaryName)
    Copy-Required (Join-Path $Root "packaging/icons/courier.icns") (Join-Path $resources "courier.icns")

    @"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>$binaryName</string>
  <key>CFBundleIdentifier</key><string>$($manifest.bundle_identifier)</string>
  <key>CFBundleName</key><string>$($manifest.name)</string>
  <key>CFBundleDisplayName</key><string>$($manifest.name)</string>
  <key>CFBundleIconFile</key><string>courier.icns</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$Version</string>
  <key>CFBundleVersion</key><string>$Version</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
</dict>
</plist>
"@ | Set-Content -LiteralPath (Join-Path $contents "Info.plist") -Encoding UTF8

    if (Test-Path -LiteralPath $artifact) {
        Remove-Item -LiteralPath $artifact -Force
    }
    & $hdiutil create -volname $manifest.name -srcfolder $appRoot -ov -format UDZO $artifact
    if ($LASTEXITCODE -ne 0) {
        throw "hdiutil failed with exit code $LASTEXITCODE"
    }
    Write-Host "Built macOS installer: $artifact"
}

function Build-LinuxInstallers {
    if (-not (Test-Path -LiteralPath $unixBinary)) {
        throw "Linux release binary is missing: $unixBinary"
    }

    $desktopFile = Read-TomlString $installerToml "desktop_file"
    $icon = Join-Path $Root "packaging/icons/courier.png"
    $appDir = Join-Path $OutputDir "Courier.AppDir"
    $usrBin = Join-Path $appDir "usr/bin"
    $usrApps = Join-Path $appDir "usr/share/applications"
    $usrIcons = Join-Path $appDir "usr/share/icons/hicolor/256x256/apps"
    New-Item -ItemType Directory -Force -Path $usrBin, $usrApps, $usrIcons | Out-Null
    Copy-Required $unixBinary (Join-Path $usrBin $binaryName)
    Copy-Required $icon (Join-Path $usrIcons "dev.hephaestus.courier.png")

    @"
[Desktop Entry]
Type=Application
Name=Courier
Exec=$binaryName
Icon=dev.hephaestus.courier
Categories=Network;Email;
Terminal=false
"@ | Set-Content -LiteralPath (Join-Path $usrApps $desktopFile) -Encoding UTF8
    Copy-Item -LiteralPath (Join-Path $usrApps $desktopFile) -Destination (Join-Path $appDir $desktopFile) -Force

    $appRun = Join-Path $appDir "AppRun"
    @"
#!/usr/bin/env sh
HERE="`$(dirname "`$(readlink -f "`$0")")"
exec "`$HERE/usr/bin/$binaryName" "`$@"
"@ | Set-Content -LiteralPath $appRun -Encoding UTF8

    if ($IsLinux -or $IsMacOS) {
        chmod +x $appRun
        chmod +x (Join-Path $usrBin $binaryName)
    }

    Build-AppImage $appDir
    Build-Deb $appDir
    Build-Rpm $appDir
}

function Build-AppImage($appDir) {
    $appImageTool = Get-Command "appimagetool" -ErrorAction SilentlyContinue
    if (-not $appImageTool) {
        Write-Warning "Skipping AppImage: appimagetool is not installed"
        return
    }
    $artifact = Join-Path $OutputDir (Expand-ArtifactName $manifest.artifacts.linux.appimage)
    & $appImageTool.Source $appDir $artifact
    if ($LASTEXITCODE -ne 0) {
        throw "appimagetool failed with exit code $LASTEXITCODE"
    }
    Write-Host "Built AppImage: $artifact"
}

function Build-Deb($appDir) {
    $dpkg = Get-Command "dpkg-deb" -ErrorAction SilentlyContinue
    if (-not $dpkg) {
        Write-Warning "Skipping deb: dpkg-deb is not installed"
        return
    }

    $debRoot = Join-Path $OutputDir "deb-root"
    $controlDir = Join-Path $debRoot "DEBIAN"
    New-Item -ItemType Directory -Force -Path $controlDir | Out-Null
    Copy-Item -LiteralPath (Join-Path $appDir "usr") -Destination $debRoot -Recurse -Force
    @"
Package: courier
Version: $Version
Section: mail
Priority: optional
Architecture: amd64
Maintainer: Hephaestus
Description: Local-first desktop email client
"@ | Set-Content -LiteralPath (Join-Path $controlDir "control") -Encoding UTF8

    $artifact = Join-Path $OutputDir (Expand-ArtifactName $manifest.artifacts.linux.deb)
    & $dpkg.Source --build $debRoot $artifact
    if ($LASTEXITCODE -ne 0) {
        throw "dpkg-deb failed with exit code $LASTEXITCODE"
    }
    Write-Host "Built deb: $artifact"
}

function Build-Rpm($appDir) {
    $rpmbuild = Get-Command "rpmbuild" -ErrorAction SilentlyContinue
    if (-not $rpmbuild) {
        Write-Warning "Skipping rpm: rpmbuild is not installed"
        return
    }

    $rpmRoot = Join-Path $OutputDir "rpm-root"
    $buildRoot = Join-Path $rpmRoot "BUILDROOT/courier-$Version-1.x86_64"
    $specDir = Join-Path $rpmRoot "SPECS"
    $rpmOut = Join-Path $OutputDir (Expand-ArtifactName $manifest.artifacts.linux.rpm)
    New-Item -ItemType Directory -Force -Path $buildRoot, $specDir | Out-Null
    Copy-Item -LiteralPath (Join-Path $appDir "usr") -Destination $buildRoot -Recurse -Force

    $spec = Join-Path $specDir "courier.spec"
    @"
Name: courier
Version: $Version
Release: 1
Summary: Local-first desktop email client
License: MIT
BuildArch: x86_64

%description
Courier is a local-first desktop email client.

%files
/usr/bin/$binaryName
/usr/share/applications/dev.hephaestus.courier.desktop
/usr/share/icons/hicolor/256x256/apps/dev.hephaestus.courier.png
"@ | Set-Content -LiteralPath $spec -Encoding UTF8

    & $rpmbuild.Source -bb $spec --buildroot $buildRoot --define "_topdir $rpmRoot"
    if ($LASTEXITCODE -ne 0) {
        throw "rpmbuild failed with exit code $LASTEXITCODE"
    }
    $built = Get-ChildItem -Path (Join-Path $rpmRoot "RPMS") -Recurse -Filter "*.rpm" | Select-Object -First 1
    if (-not $built) {
        throw "rpmbuild completed without producing an rpm"
    }
    Copy-Item -LiteralPath $built.FullName -Destination $rpmOut -Force
    Write-Host "Built rpm: $rpmOut"
}

switch ($Platform) {
    "windows" { Build-WindowsInstaller }
    "macos" { Build-MacosInstaller }
    "linux" { Build-LinuxInstallers }
    "all" {
        if ($IsWindows) {
            Build-WindowsInstaller
        } elseif ($IsMacOS) {
            Build-MacosInstaller
        } elseif ($IsLinux) {
            Build-LinuxInstallers
        } else {
            throw "Unsupported platform for installer build"
        }
    }
}
