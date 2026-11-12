# Суммарная память Desktop Island: основной процесс + все дочерние процессы WebView2.
# Использование: powershell -File tools/measure-ram.ps1 [-Label "сценарий"]
param([string]$Label = "")
$procs = Get-CimInstance Win32_Process
$root = $procs | Where-Object { $_.Name -eq "desktop-island.exe" } | Select-Object -First 1
if (-not $root) { Write-Output "desktop-island.exe не запущен"; exit 1 }
$ids = New-Object System.Collections.Generic.HashSet[uint32]
[void]$ids.Add($root.ProcessId)
do {
  $added = $false
  foreach ($p in $procs) {
    if (-not $ids.Contains($p.ProcessId) -and $ids.Contains($p.ParentProcessId)) { [void]$ids.Add($p.ProcessId); $added = $true }
  }
} while ($added)
$rows = foreach ($id in $ids) {
  $gp = Get-Process -Id $id -ErrorAction SilentlyContinue
  if ($gp) { [pscustomobject]@{ Name = $gp.ProcessName; Pid = $id; WorkingSetMB = [math]::Round($gp.WorkingSet64 / 1MB, 1); PrivateMB = [math]::Round($gp.PrivateMemorySize64 / 1MB, 1) } }
}
$ws = ($rows | Measure-Object WorkingSetMB -Sum).Sum
$pv = ($rows | Measure-Object PrivateMB -Sum).Sum
Write-Output ("{0,-34} процессов: {1,2}  Working Set: {2,7:N1} МБ  Private: {3,7:N1} МБ" -f $Label, $rows.Count, $ws, $pv)
