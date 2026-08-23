<#
.SYNOPSIS
    TYPE installer for Windows.

.DESCRIPTION
    irm https://raw.githubusercontent.com/Pranjal-SB/type/main/install.ps1 | iex

    Downloads the release archive for this machine, checks it against the SHA-256
    published beside it, and puts typ.exe in %LOCALAPPDATA%\Programs\typ. No
    administrator prompt: a per-user install that never asks beats a machine-wide
    one that does, and an editor has no business wanting Administrator.

    Written for Windows PowerShell 5.1, not 7. `irm ... | iex` runs in whatever
    ships with Windows, and that is 5.1 - so no ternary, no ??, no -Parallel, and
    the tests run under powershell.exe rather than pwsh.

    Piped into `iex` there is no way to pass arguments, so the two knobs are also
    environment variables: TYP_VERSION and TYP_BIN_DIR.

    Tested by tests\install_test.ps1.
#>
param(
    # A specific release tag, e.g. v0.2.6. Default: the latest release.
    [string]$Version = $env:TYP_VERSION,
    # Where typ.exe goes.
    [string]$BinDir = $env:TYP_BIN_DIR,
    [switch]$Help
)

$ErrorActionPreference = 'Stop'

# 5.1 on an unpatched Windows still negotiates TLS 1.0 by default, and GitHub has
# not accepted that since 2018 - the symptom is an unhelpful "request was
# aborted". This is the counterpart to curl's --tlsv1.2 in install.sh.
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

# Invoke-WebRequest in 5.1 renders a progress bar by repainting the console on
# every chunk, which costs more wall clock than the download does. Silencing it
# turns a minute-long download into a few seconds.
$ProgressPreference = 'SilentlyContinue'

$Repo = 'Pranjal-SB/type'

# Which release archive this machine wants.
#
# ARM64 gets the x86_64 build on purpose. There is no aarch64-pc-windows-msvc row
# in release.yml, and arm64 Windows runs x64 binaries under emulation well enough
# for a terminal editor. When that row is added this function changes with it -
# it is a deliberate mapping, not a silent fallback hiding a missing build.
function Get-TypTarget {
    param([string]$Arch)
    switch ($Arch) {
        'AMD64' { return 'x86_64-pc-windows-msvc' }
        'ARM64' { return 'x86_64-pc-windows-msvc' }
        default { throw "unsupported architecture: $Arch (Windows builds are published for AMD64 and ARM64)" }
    }
}

# The only function that touches the network.
function Invoke-TypDownload {
    param([string]$Url, [string]$Dest)
    Invoke-WebRequest -Uri $Url -OutFile $Dest -UseBasicParsing
}

# Returns the PATH string with $Dir appended, or $null if it is already there.
#
# Pure, and separate from the registry write, because the failure it guards
# against is silent: a persistent user PATH that grows another copy of the same
# directory every time someone re-runs the installer. Comparison ignores case and
# trailing slashes because a real user PATH has both.
function Join-TypPath {
    param([string]$Existing, [string]$Dir)
    $want = $Dir.TrimEnd('\')
    foreach ($entry in $Existing.Split(';')) {
        if ($entry.TrimEnd('\') -ieq $want) { return $null }
    }
    if ([string]::IsNullOrEmpty($Existing)) { return $want }
    return $Existing.TrimEnd(';') + ';' + $want
}

# Setting the persistent value does not change the shell that is running, so both
# have to be written or the user is told to run a command that is not yet found.
function Add-TypPath {
    param([string]$Dir)
    $user = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($null -eq $user) { $user = '' }
    $updated = Join-TypPath -Existing $user -Dir $Dir
    if ($null -eq $updated) { return }
    [Environment]::SetEnvironmentVariable('Path', $updated, 'User')
    $session = Join-TypPath -Existing $env:Path -Dir $Dir
    if ($null -ne $session) { $env:Path = $session }
    Write-Host "added $Dir to your PATH (new terminals will pick it up)"
}

# Everything after the bytes arrive. Split out from the download so the tests can
# drive it against a local fixture.
#
# Nothing is written to the destination until the checksum has passed. A tampered
# or truncated archive must not leave a half-installed binary behind, and the
# cheapest way to guarantee that is to do the risky part somewhere disposable.
function Install-TypArchive {
    param([string]$Archive, [string]$SumFile, [string]$BinDir)

    $want = (Get-Content $SumFile -Raw).Trim().Split(' ')[0].ToLower()
    $got = (Get-FileHash $Archive -Algorithm SHA256).Hash.ToLower()
    if ($want -ne $got) {
        throw "checksum mismatch for $(Split-Path -Leaf $Archive) - refusing to install. The download was corrupted, interrupted, or tampered with. (expected $want, got $got)"
    }

    $unpack = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory $unpack | Out-Null
    try {
        Expand-Archive -Path $Archive -DestinationPath $unpack -Force
        # release.yml compresses the directory, not its contents, so the binary is
        # one level down. -Recurse covers the flat case too, so a change to the
        # packaging step does not silently break installs.
        $found = Get-ChildItem -Path $unpack -Filter 'typ.exe' -Recurse |
            Select-Object -First 1
        if ($null -eq $found) { throw "no typ.exe inside $(Split-Path -Leaf $Archive)" }

        if (-not (Test-Path $BinDir)) { New-Item -ItemType Directory $BinDir | Out-Null }
        Copy-Item $found.FullName (Join-Path $BinDir 'typ.exe') -Force
        Write-Host "installed $(Join-Path $BinDir 'typ.exe')"
    } finally {
        Remove-Item -Recurse -Force $unpack -ErrorAction SilentlyContinue
    }
}

function Show-TypUsage {
    Write-Host @'
Install TYPE, the terminal IDE.

USAGE:
    install.ps1 [-Version <TAG>] [-BinDir <DIR>]

    -Version <TAG>   Install a specific release, e.g. v0.2.6. Default: latest.
                     Also read from $env:TYP_VERSION.
    -BinDir <DIR>    Where to put typ.exe. Default: %LOCALAPPDATA%\Programs\typ.
                     Also read from $env:TYP_BIN_DIR.

Piped through iex there is no way to pass arguments; use the environment
variables instead:

    $env:TYP_VERSION = "v0.2.6"; irm <url> | iex
'@
}

# Takes what it needs rather than reaching into the script scope. Assigning to a
# script-scope variable inside a function silently creates a function-local copy,
# which works right up until someone adds a second function that expects to see
# the change.
function Main {
    param([string]$Version, [string]$BinDir, [switch]$Help)

    if ($Help) { Show-TypUsage; return }

    if ([string]::IsNullOrEmpty($BinDir)) {
        $BinDir = Join-Path $env:LOCALAPPDATA 'Programs\typ'
    }

    $target = Get-TypTarget -Arch $env:PROCESSOR_ARCHITECTURE

    if ([string]::IsNullOrEmpty($Version)) {
        # The archive name carries the tag, which "latest" does not tell us. The
        # API is 60 requests an hour per IP unauthenticated, which is plenty for
        # an installer; -Version is the escape hatch if it is ever not.
        $rel = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -UseBasicParsing
        $Version = $rel.tag_name
        if ([string]::IsNullOrEmpty($Version)) {
            throw 'could not work out the latest release; pass -Version'
        }
    }

    $name = "typ-$Version-$target"
    $work = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory $work | Out-Null
    try {
        $base = "https://github.com/$Repo/releases/download/$Version"
        $zip = Join-Path $work "$name.zip"
        Write-Host "downloading $name"
        Invoke-TypDownload -Url "$base/$name.zip" -Dest $zip
        Invoke-TypDownload -Url "$base/$name.zip.sha256" -Dest "$zip.sha256"

        Install-TypArchive -Archive $zip -SumFile "$zip.sha256" -BinDir $BinDir
        Add-TypPath -Dir $BinDir
    } finally {
        Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
    }
}

# Dot-sourced by the tests to get the functions without running anything. Stays
# last, matching install.sh - though on Windows the parser is what makes a
# half-arrived script safe, since PowerShell parses a whole script before running
# any of it.
if ($env:TYP_INSTALL_LIB -ne '1') { Main -Version $Version -BinDir $BinDir -Help:$Help }
