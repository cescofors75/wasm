param(
    [switch]$Serve,
    [int]$Port = 8080
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $scriptDir

function Resolve-CommandPath {
    param(
        [Parameter(Mandatory = $true)]
        [string]$PrimaryPath,

        [Parameter(Mandatory = $true)]
        [string]$CommandName
    )

    if (Test-Path $PrimaryPath) {
        return $PrimaryPath
    }

    $command = Get-Command $CommandName -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }

    return $null
}

$rustupPath = Resolve-CommandPath -PrimaryPath (Join-Path $env:USERPROFILE '.cargo/bin/rustup.exe') -CommandName 'rustup.exe'
$rustcPath = Resolve-CommandPath -PrimaryPath (Join-Path $env:USERPROFILE '.cargo/bin/rustc.exe') -CommandName 'rustc.exe'
$null = New-Item -ItemType Directory -Force '.build'
$coreTemp = '.build/libraydrone_core.tmp.rlib'
$coreSource = Join-Path $scriptDir 'core\src\lib.rs'
$wasmTemp = '.build/raydrone.tmp.wasm'

Remove-Item $coreTemp, $wasmTemp -Force -ErrorAction SilentlyContinue

if (-not $rustcPath) {
    throw 'rustc no encontrado. Instala Rustup primero.'
}

if (-not (Test-Path $coreSource)) {
    throw "No se encontro $coreSource. Restaura el crate core."
}

if ($rustupPath) {
    $wasmTarget = 'wasm32-unknown-unknown'
    $installedTargets = @(& $rustupPath target list --installed 2>$null)
    if ($LASTEXITCODE -ne 0) {
        throw "rustup target list fallo con codigo $LASTEXITCODE"
    }

    if ($installedTargets -notcontains $wasmTarget) {
        $previousErrorActionPreference = $ErrorActionPreference
        try {
            $ErrorActionPreference = 'Continue'
            & $rustupPath target add $wasmTarget 2>$null
            $targetAddExitCode = $LASTEXITCODE
        } finally {
            $ErrorActionPreference = $previousErrorActionPreference
        }

        if ($targetAddExitCode -ne 0) {
            throw "rustup target add $wasmTarget fallo con codigo $targetAddExitCode"
        }
    }
}

try {
    & $rustcPath --edition 2021 `
        --target wasm32-unknown-unknown `
        -O -C panic=abort -C lto=fat `
        --crate-name raydrone_core --crate-type=lib `
        $coreSource -o $coreTemp
    if ($LASTEXITCODE -ne 0) { throw "rustc core fallo con codigo $LASTEXITCODE" }

    & $rustcPath --edition 2021 `
        --target wasm32-unknown-unknown `
        -O -C panic=abort -C lto=fat `
        --extern raydrone_core=$coreTemp `
        --crate-type=cdylib `
        raydrone.rs -o $wasmTemp
    if ($LASTEXITCODE -ne 0) { throw "rustc wasm fallo con codigo $LASTEXITCODE" }

    Move-Item -Force $coreTemp .build/libraydrone_core.rlib
    Move-Item -Force $wasmTemp raydrone.wasm
} finally {
    Remove-Item $coreTemp, $wasmTemp -Force -ErrorAction SilentlyContinue
}

$size = (Get-Item raydrone.wasm).Length
Write-Output "OK raydrone.wasm generado ($size bytes)"

if ($Serve) {
    $python = Get-Command python.exe -ErrorAction SilentlyContinue
    if (-not $python) {
        $python = Get-Command py.exe -ErrorAction SilentlyContinue
    }
    if (-not $python) {
        throw 'Python no encontrado. Instala Python o habilita el launcher py.exe.'
    }

    Write-Output "Sirviendo http://localhost:$Port/"
    & $python.Source -m http.server $Port
}
