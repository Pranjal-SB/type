# Tests for install.ps1. Plain asserts, not Pester: the Pester that ships with
# Windows is 3.4, its syntax is not the one anybody writes today, and requiring
# a module install to test an installer is testing the wrong thing.
#
#     powershell -NoProfile -File tests\install_test.ps1
#
# Run it in `powershell.exe`, not `pwsh`. `irm ... | iex` executes in whatever
# ships with Windows, which is 5.1, so 5.1 is what these tests have to pass in.
#
# The network half is not covered here, same as the sh tests: what matters is
# everything after the bytes arrive.

Set-StrictMode -Version 2
$ErrorActionPreference = 'Stop'

$here = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$script = Join-Path $here 'install.ps1'
$pass = 0
$fail = 0

function ok($name) {
    $script:pass++
    Write-Host "ok   $name"
}

function no($name, $why) {
    $script:fail++
    Write-Host "FAIL $name"
    Write-Host "   $why"
}

# A throwaway release laid out exactly as release.yml builds one: Compress-Archive
# on a directory keeps that directory as the archive root, so the binary lives one
# level down.
function New-Fixture($root) {
    $name = 'typ-v9.9.9-x86_64-pc-windows-msvc'
    $dir = Join-Path $root $name
    New-Item -ItemType Directory $dir | Out-Null
    # Not a real exe. Nothing here executes it; the sh tests cover "the installed
    # thing runs", and a fake PE would only test Compress-Archive.
    Set-Content -Path (Join-Path $dir 'typ.exe') -Value 'not really a binary'
    Set-Content -Path (Join-Path $dir 'THIRD-PARTY-LICENSES.md') -Value 'notices'
    $zip = Join-Path $root "$name.zip"
    Compress-Archive -Path $dir -DestinationPath $zip
    $hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
    Set-Content -Path "$zip.sha256" -Value "$hash  $name.zip" -Encoding Ascii
    Remove-Item -Recurse -Force $dir
    return $zip
}

function New-TempDir {
    $p = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory $p | Out-Null
    return $p
}

# Dot-sourcing gives the tests the functions without running Main. The guard is
# the last line of install.ps1.
$env:TYP_INSTALL_LIB = '1'
. $script

# --- a good archive installs the binary ---------------------------------------
$t = New-TempDir
$zip = New-Fixture $t
$bin = Join-Path $t 'bin'
try {
    Install-TypArchive -Archive $zip -SumFile "$zip.sha256" -BinDir $bin | Out-Null
    if (Test-Path (Join-Path $bin 'typ.exe')) {
        ok 'a good archive installs typ.exe'
    } else {
        no 'a good archive installs typ.exe' 'nothing at the destination'
    }
} catch {
    no 'a good archive installs typ.exe' $_.Exception.Message
}
Remove-Item -Recurse -Force $t

# --- a bad checksum installs nothing ------------------------------------------
# The case that matters. Everything else is convenience; this one decides whether
# a tampered or truncated download reaches the filesystem.
$t = New-TempDir
$zip = New-Fixture $t
$bad = "$zip.bad.sha256"
Set-Content -Path $bad -Value ('0' * 64 + '  ' + (Split-Path -Leaf $zip)) -Encoding Ascii
$bin = Join-Path $t 'bin'
$threw = $false
try {
    Install-TypArchive -Archive $zip -SumFile $bad -BinDir $bin | Out-Null
} catch {
    $threw = $true
}
if (-not $threw) {
    no 'a mismatched checksum aborts' 'it reported success'
} elseif (Test-Path $bin) {
    no 'a mismatched checksum aborts' "it failed but still created $bin"
} else {
    ok 'a mismatched checksum aborts and installs nothing'
}
Remove-Item -Recurse -Force $t

# --- an unknown architecture is named, not guessed ----------------------------
try {
    $got = Get-TypTarget -Arch 'IA64'
    no 'an unknown architecture fails' "returned '$got' instead of throwing"
} catch {
    if ($_.Exception.Message -match 'IA64') {
        ok 'an unknown architecture fails, and says which one'
    } else {
        no 'an unknown architecture fails' "message does not name the arch: $($_.Exception.Message)"
    }
}

# --- the two architectures Windows actually reports ---------------------------
if ((Get-TypTarget -Arch 'AMD64') -eq 'x86_64-pc-windows-msvc') {
    ok 'AMD64 resolves to the x86_64 MSVC target'
} else {
    no 'AMD64 resolves to the x86_64 MSVC target' (Get-TypTarget -Arch 'AMD64')
}

# No aarch64-pc-windows-msvc row in release.yml, and arm64 Windows runs x64
# binaries under emulation, so this is a real install rather than a fallback that
# hides a missing build. When that row is added, this test changes with it.
if ((Get-TypTarget -Arch 'ARM64') -eq 'x86_64-pc-windows-msvc') {
    ok 'ARM64 resolves to the x86_64 build, which it can emulate'
} else {
    no 'ARM64 resolves to the x86_64 build' (Get-TypTarget -Arch 'ARM64')
}

# --- PATH is appended once, not once per run ----------------------------------
# The failure this exists to catch is silent: a persistent user PATH that grows a
# duplicate every time someone re-runs the installer.
$p = 'C:\a;C:\b'
$added = Join-TypPath -Existing $p -Dir 'C:\typ'
if ($added -eq 'C:\a;C:\b;C:\typ') {
    ok 'a missing directory is appended'
} else {
    no 'a missing directory is appended' "got '$added'"
}

if ($null -eq (Join-TypPath -Existing $added -Dir 'C:\typ')) {
    ok 'a directory already on PATH is not appended again'
} else {
    no 'a directory already on PATH is not appended again' 'it would have grown a duplicate'
}

# Trailing separators and case are both normal in a real user PATH.
if ($null -eq (Join-TypPath -Existing 'C:\a;C:\TYP;' -Dir 'C:\typ')) {
    ok 'PATH matching ignores case and trailing separators'
} else {
    no 'PATH matching ignores case and trailing separators' 'it would have grown a duplicate'
}

# --- a truncated script does nothing ------------------------------------------
# PowerShell parses a whole script before running any of it, so a half-arrived
# script cannot half-execute the way `sh` can. This asserts that rather than
# assuming it, because the property is what makes `irm | iex` safe here.
$t = New-TempDir
$partial = Join-Path $t 'partial.ps1'
$raw = Get-Content $script -Raw
Set-Content -Path $partial -Value $raw.Substring(0, 900) -Encoding Ascii
$dest = Join-Path $t 'bin'
# The parse error goes to stderr, and a native command writing stderr under
# ErrorActionPreference=Stop throws NativeCommandError in *this* script. That is
# the harness reacting, not the case failing, so the preference is relaxed for
# the one line that expects the child to fail.
$prev = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
& powershell.exe -NoProfile -ExecutionPolicy Bypass -File $partial -BinDir $dest 2>&1 | Out-Null
$ErrorActionPreference = $prev
if (Test-Path $dest) {
    no 'a truncated script is a no-op' "it created $dest"
} else {
    ok 'a truncated script installs nothing and creates nothing'
}
Remove-Item -Recurse -Force $t

Write-Host ""
Write-Host "$pass passed, $fail failed"

# Claim the exit status rather than falling off the end into whatever the last
# native call left behind. The truncated-script case above runs a child
# powershell.exe that exits non-zero, $LASTEXITCODE outlives it, and CI's
# generated step ends with `exit $LASTEXITCODE`, so the suite reported
# "9 passed, 0 failed" and then failed the job anyway.
if ($fail -ne 0) { exit 1 }
exit 0
