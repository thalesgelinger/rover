$ErrorActionPreference = "Stop"

$Repo = "thalesgelinger/rover"
$BaseUrl = "https://github.com/$Repo/releases/download"
$RoverHome = if ($env:ROVER_HOME) { $env:ROVER_HOME } else { Join-Path $HOME ".rover" }
$BinDir = Join-Path $RoverHome "bin"
$NoModifyPath = $env:ROVER_NO_MODIFY_PATH -eq "1"

function Fail($Message) {
  Write-Error $Message
  exit 1
}

function Get-Target {
  switch ($env:PROCESSOR_ARCHITECTURE) {
    "AMD64" { return "x86_64-pc-windows-msvc" }
    default { Fail "unsupported Windows arch: $env:PROCESSOR_ARCHITECTURE" }
  }
}

function Get-Version {
  if ($env:ROVER_VERSION) {
    return $env:ROVER_VERSION
  }

  $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases" | Select-Object -First 1
  return $release.tag_name
}

function Add-RoverPath {
  $CurrentPath = [Environment]::GetEnvironmentVariable("Path", "User")
  $Parts = @()
  if ($CurrentPath) {
    $Parts = $CurrentPath -split ";" | Where-Object { $_ }
  }

  if ($Parts -contains $BinDir) {
    return
  }

  if ($NoModifyPath) {
    Write-Host "Add Rover to PATH: $BinDir"
    return
  }

  [Environment]::SetEnvironmentVariable("ROVER_HOME", $RoverHome, "User")
  $NewPath = if ($CurrentPath) { "$BinDir;$CurrentPath" } else { $BinDir }
  [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
  $env:Path = "$BinDir;$env:Path"
  Write-Host "Updated user PATH. Reopen your terminal if rover is not found."
}

function Test-Checksum($Archive, $SumsFile) {
  $ArchiveName = Split-Path $Archive -Leaf
  $Line = Get-Content $SumsFile | Where-Object { $_ -match [regex]::Escape($ArchiveName) } | Select-Object -First 1
  if (-not $Line) {
    Fail "checksum missing for $ArchiveName"
  }

  $Expected = ($Line -split "\s+")[0].ToLowerInvariant()
  $Actual = (Get-FileHash -Algorithm SHA256 $Archive).Hash.ToLowerInvariant()
  if ($Expected -ne $Actual) {
    Fail "checksum mismatch"
  }
}

$Target = Get-Target
$Version = Get-Version
if (-not $Version) {
  Fail "could not resolve latest release"
}

$Asset = "rover-$Version-$Target"
$Archive = "$Asset.zip"
$Temp = Join-Path ([System.IO.Path]::GetTempPath()) "rover-install-$([System.Guid]::NewGuid())"
New-Item -ItemType Directory -Path $Temp | Out-Null

try {
  Write-Host "Installing Rover $Version for $Target"
  $ArchivePath = Join-Path $Temp $Archive
  $SumsPath = Join-Path $Temp "SHA256SUMS"

  Invoke-WebRequest -Uri "$BaseUrl/$Version/$Archive" -OutFile $ArchivePath
  Invoke-WebRequest -Uri "$BaseUrl/$Version/SHA256SUMS" -OutFile $SumsPath
  Test-Checksum $ArchivePath $SumsPath

  Expand-Archive -Path $ArchivePath -DestinationPath $Temp -Force
  New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
  Copy-Item -Path (Join-Path $Temp "$Asset\rover.exe") -Destination (Join-Path $BinDir "rover.exe") -Force

  Add-RoverPath
  Write-Host "Rover installed: $(Join-Path $BinDir "rover.exe")"
  Write-Host "Run: rover --help"
}
finally {
  Remove-Item -Recurse -Force $Temp -ErrorAction SilentlyContinue
}
