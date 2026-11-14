# Dev tool: what Microsoft Defender did to HotSwap's files, from Defender's own event log.
#
#   powershell -File tools\defender-watch.ps1              # the last 7 days
#   powershell -File tools\defender-watch.ps1 -Hours 2     # the last 2 hours
#   powershell -File tools\defender-watch.ps1 -Follow      # keep watching, print new events live
#
# Shows detections (1116), what Defender did about them (1117: quarantine, remove, allow),
# behavior blocks (1015), failed actions (1118/1119), and real-time protection being turned off
# or on (5001/5000) and settings changes (5007), so a test run says whether protection was on.
# Only events that mention HotSwap (or every event with -All). Read-only: changes nothing.
# Not part of the app on purpose: an app reading its antivirus's log looks like malware checking
# for the antivirus, which is exactly what Defender's AI watches for.

param([int]$Hours = 168, [switch]$Follow, [switch]$All)

$log = 'Microsoft-Windows-Windows Defender/Operational'
$ids = 1006, 1007, 1015, 1116, 1117, 1118, 1119, 5000, 5001, 5007
$names = @{ 1006 = 'FOUND'; 1007 = 'ACTION'; 1015 = 'BEHAVIOR'; 1116 = 'DETECTED'; 1117 = 'ACTION'; 1118 = 'ACTION FAILED'; 1119 = 'ACTION FAILED'; 5000 = 'PROTECTION ON'; 5001 = 'PROTECTION OFF'; 5007 = 'SETTING CHANGED' }

function Show($e) {
    $f = @{}
    foreach ($line in ($e.Message -split "`r?`n")) {
        if ($line -match '^\s*([A-Za-z ]+):\s*(.*)$') { $f[$Matches[1].Trim()] = $Matches[2].Trim() }
    }
    $path = ($f['Path'] -replace 'file:_', '' -split ';' | Where-Object { $_ -match '\S' } | Select-Object -First 1)
    $line = '{0:yyyy-MM-dd HH:mm:ss}  {1,-15}' -f $e.TimeCreated, $names[$e.Id]
    if ($f['Name']) { $line += "  $($f['Name'])" }
    $act = if ($f['Action']) { $f['Action'] } else { $f['Action Name'] }
    if ($act -and $act -ne 'Not Applicable') { $line += "  -> $act" }
    if ($path) { $line += "`n      file:    $path" }
    if ($f['Process Name'] -and $f['Process Name'] -ne 'Unknown') { $line += "`n      by:      $($f['Process Name'])" }
    if ($f['Detection Source']) { $line += "`n      source:  $($f['Detection Source'])" }
    if ($e.Id -eq 5007 -and $f['New Value']) { $line += "`n      now:     $($f['New Value'])" }
    Write-Output $line
}

function Wanted($e) {
    $All -or $e.Id -in 5000, 5001 -or $e.Message -match 'hotswap|hsengine'
}

$since = (Get-Date).AddHours(-$Hours)
$events = Get-WinEvent -FilterHashtable @{ LogName = $log; Id = $ids; StartTime = $since } -ErrorAction SilentlyContinue | Sort-Object TimeCreated
$shown = 0
foreach ($e in $events) { if (Wanted $e) { Show $e; $shown++ } }
if (-not $Follow) {
    if ($shown -eq 0) { Write-Output "Nothing about HotSwap in the last $Hours hours." }
    $s = Get-MpComputerStatus -ErrorAction SilentlyContinue
    $p = Get-MpPreference -ErrorAction SilentlyContinue
    if ($s) { Write-Output ("Now: real-time protection {0}, behavior monitoring {1}, cloud protection {2}" -f $(if ($s.RealTimeProtectionEnabled) { 'on' } else { 'OFF' }), $(if ($s.BehaviorMonitorEnabled) { 'on' } else { 'OFF' }), $(if ($p.MAPSReporting -gt 0) { 'on' } else { 'OFF' })) }
    return
}

Write-Output "Watching Defender for HotSwap events (Ctrl+C to stop)..."
$last = if ($events) { ($events | Select-Object -Last 1).RecordId } else { 0 }
while ($true) {
    Start-Sleep -Seconds 2
    $new = Get-WinEvent -FilterHashtable @{ LogName = $log; Id = $ids; StartTime = (Get-Date).AddMinutes(-5) } -ErrorAction SilentlyContinue | Where-Object { $_.RecordId -gt $last } | Sort-Object RecordId
    foreach ($e in $new) {
        $last = [Math]::Max($last, $e.RecordId)
        if (Wanted $e) { Show $e }
    }
}
