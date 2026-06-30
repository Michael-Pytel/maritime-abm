<#
.SYNOPSIS
    Build report.tex into build_latex/report.pdf (3x pdflatex + bibtex).
.DESCRIPTION
    Run from anywhere; the script locates its own folder. Clears the stale
    root-level *.aux/*.bbl/... artifacts that otherwise shadow build_latex/
    and make new labels show as "undefined", then runs the full pass sequence.
.EXAMPLE
    ./build.ps1
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$out  = Join-Path $root 'build_latex'
$jobname = 'report'

Push-Location $root
try {
    if (-not (Test-Path $out)) { New-Item -ItemType Directory -Path $out | Out-Null }

    # Remove stale root-level build artifacts that shadow build_latex/report.aux.
    Get-ChildItem -Path $root -File -Filter "$jobname.*" |
        Where-Object { $_.Extension -in '.aux','.bbl','.blg','.log','.out','.toc' } |
        Remove-Item -Force -ErrorAction SilentlyContinue

    $pdflatex = @('-interaction=nonstopmode', '-halt-on-error', "-output-directory=$out", "$jobname.tex")

    Write-Host '[1/4] pdflatex (pass 1)...' -ForegroundColor Cyan
    & pdflatex @pdflatex | Out-Null

    Write-Host '[2/4] bibtex...' -ForegroundColor Cyan
    & bibtex (Join-Path $out $jobname) | Out-Null

    Write-Host '[3/4] pdflatex (pass 2)...' -ForegroundColor Cyan
    & pdflatex @pdflatex | Out-Null

    Write-Host '[4/4] pdflatex (pass 3)...' -ForegroundColor Cyan
    & pdflatex @pdflatex | Out-Null

    $log = Join-Path $out "$jobname.log"

    # Surface hard errors and unresolved references.
    $errors = Select-String -Path $log -Pattern '^! ' -SimpleMatch -ErrorAction SilentlyContinue
    $undef  = Select-String -Path $log -Pattern 'Warning: (Reference|Citation).*undefined' -ErrorAction SilentlyContinue

    if ($errors) {
        Write-Host "`nBuild reported LaTeX errors:" -ForegroundColor Red
        $errors | ForEach-Object { Write-Host "  $($_.Line)" -ForegroundColor Red }
        exit 1
    }
    if ($undef) {
        Write-Host "`nUndefined references/citations:" -ForegroundColor Yellow
        $undef | ForEach-Object { Write-Host "  $($_.Line)" -ForegroundColor Yellow }
    }

    $pages = (Select-String -Path $log -Pattern '\((\d+) pages?' -AllMatches |
              Select-Object -Last 1).Matches.Groups[1].Value
    Write-Host "`nDone -> $out\$jobname.pdf ($pages pages)" -ForegroundColor Green
}
finally {
    Pop-Location
}
