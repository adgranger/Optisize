#Requires -Version 7
<#
.SYNOPSIS
  Benchmark reproductible d'Optisize (et de TreeSize / WizTree s'ils sont présents). Voir docs/bench.md.
.DESCRIPTION
  Lecture seule : lance des exécutables existants et lit les dossiers de référence. N'écrit que dans
  -Out (défaut target\bench) et dans son arbre synthétique (dossier neuf sous %TEMP%, supprimé à la fin).
  Aucune installation, aucun téléchargement, aucune élévation, aucun réglage Windows, aucun vidage de cache.
#>
[CmdletBinding()]
param(
    [string[]]$Optisize = @('target\release\optisize.exe=optisize'),
    [string[]]$Paths,
    [int]$Rounds = 7,
    [int]$Warmup = 1,
    [int[]]$Threads,
    [string]$TreeSize,
    [string]$WizTree,
    [switch]$OptisizeOnly,
    [switch]$Cold,
    [string[]]$Order,
    [switch]$NoSynth,
    [switch]$KeepSynth,
    [switch]$SelfTest,
    [string]$Out = 'target\bench'
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$inv = [Globalization.CultureInfo]::InvariantCulture
$header = 'horodatage,machine,outil,etiquette,version,threads,cache,dossier,tour,secondes,fichiers,dossiers,logique,sur_disque,erreurs,memoire,methode'

function Resolve-RepoPath([string]$p) {
    if ([IO.Path]::IsPathRooted($p)) { return $p }
    $here = Join-Path (Get-Location) $p
    if (Test-Path -LiteralPath $here) { return (Resolve-Path -LiteralPath $here).Path }
    return Join-Path $repo $p
}

if ($SelfTest) {
    $Rounds = 1; $Warmup = 0; $OptisizeOnly = $true; $NoSynth = $false; $KeepSynth = $false
    if (-not $PSBoundParameters.ContainsKey('Paths')) { $Paths = @() }
}
if ($Cold) {
    $Rounds = 1; $Warmup = 0
    Write-Warning "Cache froid : une exécution par outil et par dossier ; l'outil mesuré en premier a l'avantage du disque froid (ordre : -Order)."
}
if (-not $PSBoundParameters.ContainsKey('Paths') -and -not $SelfTest) { $Paths = @('C:\Program Files', 'C:\Windows') }
$cache = if ($Cold) { 'froid' } else { 'chaud' }
$Out = Resolve-RepoPath $Out
New-Item -ItemType Directory -Force -Path $Out | Out-Null

# --- Outils ---------------------------------------------------------------
$runners = [Collections.Generic.List[object]]::new()
foreach ($spec in $Optisize) {
    $exe, $label = $spec -split '=', 2
    $exe = Resolve-RepoPath $exe
    if (-not $label) { $label = [IO.Path]::GetFileNameWithoutExtension($exe) }
    if (-not (Test-Path -LiteralPath $exe)) { throw "Optisize introuvable : $exe (cargo build --release)" }
    $ver = (Get-Item -LiteralPath $exe).VersionInfo.FileVersion
    foreach ($t in ($Threads ? $Threads : @(0))) {
        $runners.Add([pscustomobject]@{ tool = 'optisize'; label = $label; exe = $exe; version = $ver; threads = $t; method = 'bench' })
    }
}

function Find-Tool([string]$explicit, [string[]]$candidates) {
    if ($explicit) { return (Test-Path -LiteralPath $explicit) ? $explicit : $null }
    foreach ($c in $candidates) { if (Test-Path -LiteralPath $c) { return $c } }
    return $null
}
$notes = [Collections.Generic.List[string]]::new()
if (-not $OptisizeOnly) {
    $third = @(
        @{ tool = 'treesize'; name = 'TreeSize'; method = 'cpu-idle'; exe = Find-Tool $TreeSize @(
                "$env:LOCALAPPDATA\Programs\JAM Software\TreeSize Free\TreeSizeFree.exe",
                "$env:ProgramFiles\JAM Software\TreeSize Free\TreeSizeFree.exe") },
        @{ tool = 'wiztree'; name = 'WizTree'; method = 'export'; exe = Find-Tool $WizTree @(
                "$env:ProgramFiles\WizTree\WizTree64.exe",
                "$env:LOCALAPPDATA\Programs\WizTree\WizTree64.exe") }
    )
    $gui = $false
    foreach ($t in $third) {
        if (-not $t.exe) { $notes.Add("$($t.name) : absent (voir docs/bench.md)"); continue }
        $pname = [IO.Path]::GetFileNameWithoutExtension($t.exe)
        if (Get-Process -Name $pname -ErrorAction SilentlyContinue) {
            # Jamais toucher à une instance de l'utilisateur ; une 2e instance pourrait lui transmettre le chemin.
            $notes.Add("$($t.name) : déjà ouvert par l'utilisateur, non mesuré (fermez-le puis relancez)")
            continue
        }
        $ver = (Get-Item -LiteralPath $t.exe).VersionInfo.FileVersion
        $runners.Add([pscustomobject]@{ tool = $t.tool; label = $t.tool; exe = $t.exe; version = $ver; threads = 0; method = $t.method })
        $gui = $gui -or $t.method -eq 'cpu-idle'
    }
    if ($gui) {
        # Même méthode que TreeSize (démarrage de l'interface compris), avec le dernier binaire Optisize.
        $o = $runners | Where-Object tool -EQ 'optisize' | Select-Object -Last 1
        $runners.Add([pscustomobject]@{ tool = 'optisize'; label = 'optisize-gui'; exe = $o.exe; version = $o.version; threads = 0; method = 'cpu-idle' })
    }
}
if ($Cold -and $Order) {
    $runners = [Collections.Generic.List[object]]@($Order | ForEach-Object { $l = $_; $runners | Where-Object label -EQ $l }) + @($runners | Where-Object label -NotIn $Order)
}

# --- Mesures --------------------------------------------------------------
function Invoke-Bench($r, [string]$path) {
    $psi = [Diagnostics.ProcessStartInfo]::new($r.exe)
    foreach ($a in @('--bench', $path, '--json')) { $psi.ArgumentList.Add($a) }
    $psi.RedirectStandardOutput = $true
    $psi.UseShellExecute = $false
    if ($r.threads -gt 0) { $psi.Environment['OPTISIZE_THREADS'] = "$($r.threads)" } else { $psi.Environment.Remove('OPTISIZE_THREADS') | Out-Null }
    $p = [Diagnostics.Process]::Start($psi)
    $text = $p.StandardOutput.ReadToEnd()
    $p.WaitForExit()
    $j = try { $text.Trim() | ConvertFrom-Json } catch { $null }
    if (-not $j -and $text -match '^([\d ]+) fichiers, ([\d ]+) dossiers, .* en ([\d.]+) s => (\d+) fichiers/s .*arbre ([\d,]+) (o|Ko|Mo|Go)') {
        # Binaire d'avant F05 (pas de --json) : ligne humaine ; durée = fichiers / débit (plus précis
        # que les 2 décimales affichées), tailles arrondies donc non reportées.
        $files = [long]($Matches[1] -replace ' ', ''); $rate = [double]$Matches[4]
        $mem = [double]($Matches[5] -replace ',', '.') * @{ o = 1; Ko = 1KB; Mo = 1MB; Go = 1GB }[$Matches[6]]
        return @{ ok = $p.ExitCode -eq 0; why = "code $($p.ExitCode)"; secs = ($rate -gt 0) ? $files / $rate : [double]$Matches[3]
            files = $files; dirs = [long]($Matches[2] -replace ' ', ''); logical = ''; on_disk = ''; errors = ''; mem = [long]$mem; threads = '' }
    }
    if (-not $j) { return @{ ok = $false; why = "sortie illisible (code $($p.ExitCode)) : $($text.Trim())" } }
    @{ ok = $p.ExitCode -eq 0 -and -not $j.root_denied; why = $j.root_denied ? 'racine refusée' : "code $($p.ExitCode)"
        secs = $j.secs; files = $j.files; dirs = $j.dirs; logical = $j.logical; on_disk = $j.on_disk; errors = $j.errors; mem = $j.mem; threads = $j.threads }
}

function Get-TreeCpu([int]$id, [hashtable]$kids, [bool]$refresh) {
    if ($refresh) {
        foreach ($c in Get-CimInstance Win32_Process -Filter "ParentProcessId=$id" -ErrorAction SilentlyContinue) {
            if (-not $kids.ContainsKey($c.ProcessId)) { $kids[$c.ProcessId] = Get-Process -Id $c.ProcessId -ErrorAction SilentlyContinue }
        }
    }
    $t = 0.0
    foreach ($q in @(Get-Process -Id $id -ErrorAction SilentlyContinue) + @($kids.Values)) {
        if ($q) { try { $q.Refresh(); $t += $q.TotalProcessorTime.TotalMilliseconds } catch {} }
    }
    $t
}

function Invoke-CpuIdle($r, [string]$path) {
    # Fin du scan = dernier instant où le temps CPU (processus + enfants) a augmenté, stable depuis 3 s.
    $p = Start-Process -FilePath $r.exe -ArgumentList "`"$path`"" -PassThru
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $kids = @{}; $last = -1.0; $lastUp = 0.0; $i = 0; $timeout = $false
    while ($true) {
        Start-Sleep -Milliseconds 50
        $cpu = Get-TreeCpu $p.Id $kids (($i++ % 20) -eq 0)
        $now = $sw.Elapsed.TotalSeconds
        if ($cpu -gt $last) { $last = $cpu; $lastUp = $now }
        if ($now - $lastUp -ge 3 -and $lastUp -gt 0) { break }
        if ($p.HasExited -or $now -gt 600) { $timeout = -not $p.HasExited; break }
    }
    $exited = $p.HasExited
    if (-not $exited) {
        # Ferme uniquement le processus lancé ici, par son PID.
        $p.CloseMainWindow() | Out-Null
        if (-not $p.WaitForExit(5000)) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
    }
    @{ ok = -not $exited -and -not $timeout; why = $exited ? 'processus terminé de lui-même (boîte de dialogue, instance unique ?)' : 'délai dépassé'
        secs = $lastUp; files = ''; dirs = ''; logical = ''; on_disk = ''; errors = ''; mem = ''; threads = '' }
}

function Invoke-Export($r, [string]$path) {
    # shortcut: options WizTree non vérifiées sur une version installée (absente de la machine de dev) ; à valider avec `/?`.
    $tmp = Join-Path $Out 'tmp'
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    $csv = Join-Path $tmp "wiztree-$PID-$([guid]::NewGuid().ToString('N')).csv"
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process -FilePath $r.exe -ArgumentList "`"$path`"", "/export=`"$csv`"", '/admin=0' -PassThru
    $done = $p.WaitForExit(600000)
    $secs = $sw.Elapsed.TotalSeconds
    if (-not $done) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
    $n = (Test-Path -LiteralPath $csv) ? ((Get-Content -LiteralPath $csv | Measure-Object -Line).Lines - 1) : ''
    if (Test-Path -LiteralPath $csv) { Remove-Item -LiteralPath $csv }
    @{ ok = $done -and $n -ne ''; why = $done ? 'export absent' : 'délai dépassé'
        secs = $secs; files = $n; dirs = ''; logical = ''; on_disk = ''; errors = ''; mem = ''; threads = '' }
}

function Invoke-Run($r, [string]$path) {
    switch ($r.method) { 'bench' { Invoke-Bench $r $path } 'cpu-idle' { Invoke-CpuIdle $r $path } 'export' { Invoke-Export $r $path } }
}

# --- Arbre synthétique ----------------------------------------------------
function New-Synth([int]$n1, [int]$n2, [int]$n3, [int]$files) {
    $root = Join-Path $env:TEMP "optisize-bench-synth-v1-$PID"
    New-Item -ItemType Directory -Path $root -ErrorAction Stop | Out-Null # échoue si le dossier existe
    $script:synthCreated = $root
    $zeros = @{}
    $k = 0
    for ($a = 0; $a -lt $n1; $a++) {
        for ($b = 0; $b -lt $n2; $b++) {
            for ($c = 0; $c -lt $n3; $c++) {
                $leaf = [IO.Directory]::CreateDirectory([IO.Path]::Combine($root, ('a{0:D2}' -f $a), "b$b", "c$c")).FullName
                for ($j = 0; $j -lt $files; $j++) {
                    $len = ($k * 31 + $j * 17) % 4096
                    if (-not $zeros.ContainsKey($len)) { $zeros[$len] = [byte[]]::new($len) }
                    [IO.File]::WriteAllBytes([IO.Path]::Combine($leaf, ('f{0:D2}.dat' -f $j)), $zeros[$len])
                }
                $k++
            }
        }
    }
    $root
}

$synthCreated = $null
$rows = [Collections.Generic.List[object]]::new()
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$csvPath = Join-Path $Out "$stamp-$env:COMPUTERNAME.csv"
$exit = 0
try {
    if (-not $NoSynth) {
        $dims = $SelfTest ? @(2, 2, 2, 3) : @(40, 10, 10, 30)
        Write-Host "Arbre synthétique ($($dims -join ' × ')) …"
        $synth = New-Synth @dims
        $want = @{ files = $dims[0] * $dims[1] * $dims[2] * $dims[3]; dirs = $dims[0] + $dims[0] * $dims[1] + $dims[0] * $dims[1] * $dims[2] }
        $chk = Invoke-Bench ($runners | Where-Object method -EQ 'bench' | Select-Object -First 1) $synth
        if ($chk.files -ne $want.files -or $chk.dirs -ne $want.dirs) {
            throw "Arbre synthétique incorrect : $($chk.files) fichiers / $($chk.dirs) dossiers, attendu $($want.files) / $($want.dirs)"
        }
        $Paths = @($Paths) + $synth
    }

    $cpu = (Get-CimInstance Win32_Processor | Select-Object -First 1).Name.Trim()
    $os = Get-CimInstance Win32_OperatingSystem
    $ram = '{0:N1} Go' -f ($os.TotalVisibleMemorySize / 1MB)
    $power = (powercfg /getactivescheme) -replace '^.*:\s*', ''
    $lines = [Collections.Generic.List[string]]@("# cpu=$cpu ; ram=$ram ; os=$($os.Caption) $($os.BuildNumber) ; alimentation=$power", $header)
    [IO.File]::WriteAllLines($csvPath, $lines, [Text.UTF8Encoding]::new($false))

    foreach ($path in $Paths) {
        $dirLabel = ($path -eq $synth) ? 'synth' : $path
        for ($w = 0; $w -lt $Warmup; $w++) { foreach ($r in $runners) { Invoke-Run $r $path | Out-Null } }
        for ($round = 1; $round -le $Rounds; $round++) {
            $n = $runners.Count
            for ($i = 0; $i -lt $n; $i++) {
                $r = $runners[($i + $round - 1) % $n] # l'ordre tourne d'un cran à chaque tour
                $m = Invoke-Run $r $path
                $row = [pscustomobject]@{
                    horodatage = (Get-Date).ToString('s'); machine = $env:COMPUTERNAME; outil = $r.tool; etiquette = $r.label
                    version = $r.version; threads = ($r.threads -gt 0) ? $r.threads : $m.threads; cache = $cache; dossier = $dirLabel; tour = $round
                    secondes = ([double]$m.secs).ToString('0.0000', $inv); fichiers = $m.files; dossiers = $m.dirs; logique = $m.logical
                    sur_disque = $m.on_disk; erreurs = $m.errors; memoire = $m.mem; methode = $r.method
                    ok = $m.ok; why = $m.why; secs = [double]$m.secs
                }
                $rows.Add($row)
                $row | Select-Object ($header -split ',') | ConvertTo-Csv -UseQuotes AsNeeded | Select-Object -Skip 1 |
                    Add-Content -LiteralPath $csvPath -Encoding utf8NoBOM
                Write-Host ("  {0,-28} {1,-14} tour {2} : {3:0.000} s{4}" -f $r.label, $dirLabel, $round, $row.secs, ($m.ok ? '' : "  INVALIDE ($($m.why))"))
            }
        }
    }

    # --- Résumé -----------------------------------------------------------
    # Mesure invalide : code ≠ 0, racine refusée, ou nombre de fichiers différent de la valeur la plus
    # fréquente du dossier (arbre modifié pendant la mesure). Seulement pour --bench (comptes connus).
    foreach ($g in $rows | Where-Object methode -EQ 'bench' | Group-Object dossier) {
        $mode = ($g.Group | Where-Object ok | Group-Object fichiers | Sort-Object Count -Descending | Select-Object -First 1).Name
        foreach ($x in $g.Group) { if ($x.ok -and "$($x.fichiers)" -ne $mode) { $x.ok = $false; $x.why = "fichiers $($x.fichiers) ≠ $mode" } }
    }
    function Get-Median([double[]]$v) { $s = $v | Sort-Object; $n = $s.Count; if ($n -eq 0) { return [double]::NaN }; ($n % 2) ? $s[($n - 1) / 2] : ($s[$n / 2 - 1] + $s[$n / 2]) / 2 }
    function Get-Quantile([double[]]$v, [double]$q) { $s = @($v | Sort-Object); $pos = ($s.Count - 1) * $q; $lo = [math]::Floor($pos); $hi = [math]::Ceiling($pos); $s[$lo] + ($s[$hi] - $s[$lo]) * ($pos - $lo) }
    $summary = foreach ($g in $rows | Group-Object etiquette, threads, dossier) {
        $f = $g.Group[0]
        $ok = @($g.Group | Where-Object ok)
        $v = [double[]]@($ok | ForEach-Object secs)
        $med = Get-Median $v
        [pscustomobject]@{
            etiquette = $f.etiquette; threads = $f.threads; dossier = $f.dossier; n = $v.Count; invalides = $g.Count - $v.Count
            mediane = $med; min = ($v | Measure-Object -Minimum).Minimum; max = ($v | Measure-Object -Maximum).Maximum
            iqr_pct = ($v.Count -gt 1) ? 100 * ((Get-Quantile $v 0.75) - (Get-Quantile $v 0.25)) / $med : 0
            fichiers = $ok[0]?.fichiers; memoire = (Get-Median ([double[]]@($ok | Where-Object { $_.memoire -ne '' } | ForEach-Object { [double]$_.memoire })))
            rapport = $null
        }
    }
    $baseLabel = $runners[0].label
    foreach ($s in $summary) {
        $b = $summary | Where-Object { $_.etiquette -eq $baseLabel -and ("$($_.threads)" -eq '' -or "$($s.threads)" -eq '' -or $_.threads -eq $s.threads) -and $_.dossier -eq $s.dossier } | Select-Object -First 1
        if ($b -and $s.etiquette -ne $baseLabel -and ($runners | Where-Object { $_.label -eq $s.etiquette -and $_.method -eq 'bench' })) { $s.rapport = $s.mediane / $b.mediane }
    }
    Write-Host "`nRésumé (médianes, $cache, $Rounds tours) — $csvPath"
    $summary | Format-Table etiquette, threads, dossier, n, invalides,
    @{ n = 'médiane s'; e = { '{0:0.000}' -f $_.mediane } }, @{ n = 'min'; e = { '{0:0.000}' -f $_.min } }, @{ n = 'max'; e = { '{0:0.000}' -f $_.max } },
    @{ n = 'IQR %'; e = { '{0:0.0}' -f $_.iqr_pct } }, fichiers, @{ n = 'arbre Mo'; e = { [double]::IsNaN($_.memoire) ? '' : '{0:0.0}' -f ($_.memoire / 1MB) } },
    @{ n = "÷ $baseLabel"; e = { $_.rapport ? '{0:0.000}' -f $_.rapport : '' } } -AutoSize | Out-Host
    foreach ($n in $notes) { Write-Host $n }

    if ($SelfTest) {
        $content = Get-Content -LiteralPath $csvPath -Encoding utf8
        $okHeader = $content[1] -eq $header
        $valid = @($rows | Where-Object ok)
        $perDir = @($valid | Group-Object dossier)
        $okRows = $valid.Count -eq $Paths.Count -and $perDir.Count -eq $Paths.Count -and ($content.Count - 2) -eq $rows.Count
        if (-not ($okHeader -and $okRows)) { Write-Host "SelfTest : ÉCHEC (en-tête $okHeader, lignes $okRows)"; $exit = 1 }
    }
}
finally {
    if ($synthCreated -and -not $KeepSynth) {
        Remove-Item -LiteralPath $synthCreated -Recurse -Force # uniquement le dossier créé par ce script
    }
    $tmp = Join-Path $Out 'tmp'
    if ((Test-Path -LiteralPath $tmp) -and -not (Get-ChildItem -LiteralPath $tmp)) { Remove-Item -LiteralPath $tmp }
}
if ($SelfTest -and $exit -eq 0) {
    if ($synthCreated -and (Test-Path -LiteralPath $synthCreated)) { Write-Host 'SelfTest : ÉCHEC (arbre synthétique non supprimé)'; $exit = 1 }
    else { Write-Host 'SelfTest : OK' }
}
exit $exit
