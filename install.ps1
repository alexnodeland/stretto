# Install stretto on Windows (x64) from a GitHub release: stretto.exe,
# stretto-proxy.exe, stretto-procedure.exe, stretto-mcp-demo.exe and
# stretto-console.exe.
#
#   irm https://github.com/alexnodeland/stretto/releases/latest/download/install.ps1 | iex
#   & ([scriptblock]::Create((irm https://github.com/alexnodeland/stretto/releases/latest/download/install.ps1))) -Version v0.2.0
#
# It downloads the release's archive and its SHA256SUMS, checks the
# archive's checksum, and copies the binaries into PREFIX\bin. It
# changes nothing else: if that directory is not on PATH, it says how to
# add it. docs/install.md has the other ways to install.
#
# -Version  the release to install, such as v0.2.0 (default: the latest)
# -Prefix   install into PREFIX\bin (default: %LOCALAPPDATA%\Programs\stretto)
# -BaseUrl  fetch the release's files from this URL instead of GitHub: a
#           mirror, or a directory served over HTTP for testing
param(
    [string]$Version = "latest",
    [string]$Prefix = "",
    [string]$BaseUrl = ""
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
# Windows PowerShell 5.1 may not offer TLS 1.2, which GitHub requires.
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$repo = "alexnodeland/stretto"
$binaries = @("stretto", "stretto-proxy", "stretto-procedure", "stretto-mcp-demo")
# Installed when the archive has it: a release from before the console has
# none, and installs without it.
$optional = @("stretto-console")
# The x64 build; Windows on Arm runs it under emulation.
$target = "x86_64-pc-windows-msvc"
$archive = "stretto-$target.zip"

if (-not $Prefix) {
    $Prefix = Join-Path $env:LOCALAPPDATA "Programs\stretto"
}
if ($BaseUrl) {
    $url = $BaseUrl.TrimEnd("/")
} elseif ($Version -eq "latest") {
    $url = "https://github.com/$repo/releases/latest/download"
} else {
    $tag = if ($Version.StartsWith("v")) { $Version } else { "v$Version" }
    $url = "https://github.com/$repo/releases/download/$tag"
}

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("stretto-install-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    Write-Host "install.ps1: downloading $url/$archive"
    Invoke-WebRequest -UseBasicParsing -Uri "$url/$archive" -OutFile (Join-Path $tmp $archive)
    Invoke-WebRequest -UseBasicParsing -Uri "$url/SHA256SUMS" -OutFile (Join-Path $tmp "SHA256SUMS")
    $expected = $null
    foreach ($line in Get-Content (Join-Path $tmp "SHA256SUMS")) {
        $fields = $line.Trim() -split "\s+"
        if ($fields.Count -ge 2 -and ($fields[1] -eq $archive -or $fields[1] -eq "*$archive")) {
            $expected = $fields[0].ToLower()
            break
        }
    }
    if (-not $expected) {
        throw "SHA256SUMS has no checksum for $archive"
    }
    $actual = (Get-FileHash -Algorithm SHA256 -Path (Join-Path $tmp $archive)).Hash.ToLower()
    if ($actual -ne $expected) {
        throw "$archive does not match its checksum (expected $expected, got $actual): not installed"
    }

    Expand-Archive -Path (Join-Path $tmp $archive) -DestinationPath $tmp -Force
    $unpacked = Join-Path $tmp "stretto-$target"
    foreach ($bin in $binaries) {
        if (-not (Test-Path (Join-Path $unpacked "$bin.exe"))) {
            throw "$archive has no $bin.exe"
        }
    }
    foreach ($bin in $optional) {
        if (Test-Path (Join-Path $unpacked "$bin.exe")) {
            $binaries += $bin
        }
    }
    $bindir = Join-Path $Prefix "bin"
    New-Item -ItemType Directory -Force -Path $bindir | Out-Null
    foreach ($bin in $binaries) {
        Copy-Item -Force -Path (Join-Path $unpacked "$bin.exe") -Destination (Join-Path $bindir "$bin.exe")
    }
    $installed = & (Join-Path $bindir "stretto.exe") --version
    if ($LASTEXITCODE -ne 0) {
        throw "$bindir\stretto.exe does not run on this system"
    }
} finally {
    Remove-Item -Recurse -Force -Path $tmp -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host "$installed is installed in ${bindir}: $($binaries -join ', ')."
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
$onPath = ($env:Path -split ";") + ($userPath -split ";") | Where-Object { $_ -and ($_.TrimEnd("\") -eq $bindir.TrimEnd("\")) }
if (-not $onPath) {
    Write-Host ""
    Write-Host "$bindir is not on your PATH. Add it for new shells with:"
    Write-Host ""
    Write-Host "    [Environment]::SetEnvironmentVariable('Path', '$bindir;' + [Environment]::GetEnvironmentVariable('Path', 'User'), 'User')"
}
Write-Host @"

Next:
    stretto doctor
        checks the installation
    stretto init --host claude-code --domain NAME -- <server command>
        runs an MCP server behind stretto-proxy, recording its sessions
        (also claude-desktop, cursor, vscode), and says what comes next
    stretto completions powershell | Out-String | Invoke-Expression
        completes stretto's commands in this session (docs/install.md)

The quickstart, examples/quickstart, needs a POSIX shell: run it in WSL or
in the container image.
"@
