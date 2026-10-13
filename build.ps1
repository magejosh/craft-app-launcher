[CmdletBinding()]
param([switch]$Test, [switch]$StageOnly)
$ErrorActionPreference = 'Stop'
if (-not $Test -and -not $StageOnly) { throw "Agent-control qualification is staging only; use -StageOnly. Delivered dist is protected." }
if ($Test -and $StageOnly) { throw 'Use -Test for tests or -StageOnly for an isolated release, not both.' }
$project = $PSScriptRoot
if ([IO.Path]::GetPathRoot($project) -ne 'E:\') { throw 'This build script requires the project on E:. Review its storage policy before building elsewhere.' }
$build = Join-Path $project '.build'
# All variables below apply only to this PowerShell process and its children.
$env:TEMP = Join-Path $build 'temp'
$env:TMP = $env:TEMP
$env:CARGO_HOME = 'E:\DevTools\Rust\cargo-home'
$env:RUSTUP_HOME = 'E:\DevTools\Rust\rustup-home'
$env:CARGO_TARGET_DIR = Join-Path $build 'target'
$env:CARGO_BUILD_JOBS = '1'
$env:RUSTUP_AUTO_INSTALL = '0'
$target = 'x86_64-pc-windows-msvc'
$toolchainBin = Join-Path $env:RUSTUP_HOME "toolchains\stable-$target\bin"
$cargo = Join-Path $toolchainBin 'cargo.exe'
$rustc = Join-Path $toolchainBin 'rustc.exe'
foreach ($folder in @($env:TEMP, $env:CARGO_HOME, $env:RUSTUP_HOME, $env:CARGO_TARGET_DIR)) { New-Item -ItemType Directory -Force -Path $folder | Out-Null }
if (-not (Test-Path -LiteralPath $cargo) -or -not (Test-Path -LiteralPath $rustc)) { throw 'The minimal stable Rust toolchain is missing from E:\DevTools\Rust. No download was attempted.' }
$vcvars = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvars64.bat'
if (-not (Test-Path -LiteralPath $vcvars)) { throw 'The verified Visual Studio 18 BuildTools vcvars64.bat is missing. Review the compiler setup before building.' }
# Read the existing Microsoft compiler environment into this process; no registry/PATH writes.
$compilerEnvironment = & $env:ComSpec /d /s /c "call `"$vcvars`" >nul && set"
if ($LASTEXITCODE -ne 0) { throw 'Microsoft x64 compiler environment setup failed.' }
foreach ($line in $compilerEnvironment) {
    if ($line -match '^([^=]+)=(.*)$') { [Environment]::SetEnvironmentVariable($Matches[1], $Matches[2], 'Process') }
}
# Pin the compiler and put its DLL directory first for this process only.
$env:RUSTC = $rustc
$env:PATH = $toolchainBin + ';' + $env:PATH
& $rustc --version
if ($LASTEXITCODE -ne 0) { throw 'The E: Rust compiler could not run.' }
Push-Location -LiteralPath $project
try {
    if ($Test) {
        & $cargo test --offline --locked --jobs 1 --target $target -- --nocapture
        if ($LASTEXITCODE -ne 0) { throw 'Tests failed.' }
    } else {
        & $cargo build --release --offline --locked --jobs 1 --target $target
        if ($LASTEXITCODE -ne 0) { throw 'Build failed.' }
        if ($StageOnly) {
            $destination = Join-Path $build 'candidates\agent-control-mcp'
            New-Item -ItemType Directory -Force -Path $destination | Out-Null
        } else {
            $destination = Join-Path $project 'dist'
            New-Item -ItemType Directory -Force -Path $destination | Out-Null
            if (Get-Process -Name craft-app-launcher -ErrorAction SilentlyContinue) { throw "A launcher is running. Release remains staged under $env:CARGO_TARGET_DIR; dist was not replaced." }
        }
        Copy-Item -LiteralPath (Join-Path $env:CARGO_TARGET_DIR "$target\release\craft-app-launcher.exe") -Destination (Join-Path $destination 'craft-app-launcher.exe')
        Copy-Item -LiteralPath (Join-Path $env:CARGO_TARGET_DIR "$target\release\craft-launcher-agent.exe") -Destination (Join-Path $destination 'craft-launcher-agent.exe')
        $pair = @{ protocol = 1; gui_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $destination 'craft-app-launcher.exe')).Hash.ToLowerInvariant(); agent_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $destination 'craft-launcher-agent.exe')).Hash.ToLowerInvariant() }
        [IO.File]::WriteAllText((Join-Path $destination 'craft-launcher-pair.json'), ($pair | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
        Write-Output "Staged GUI, console agent and protocol/hash manifest in $destination"
    }
} finally { Pop-Location }
