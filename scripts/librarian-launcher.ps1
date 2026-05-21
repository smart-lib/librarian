param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$LibrarianArgs
)

$ErrorActionPreference = "Stop"

$root = Join-Path $PSScriptRoot ".librarian"
$exe = Join-Path $PSScriptRoot "librarian.exe"

if (-not (Test-Path $exe)) {
    throw "librarian.exe was not found next to this launcher: $exe"
}

$env:LIBRARIAN_HOME = $root
& $exe @LibrarianArgs
exit $LASTEXITCODE
