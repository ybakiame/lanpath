#Requires -Version 7.0
[CmdletBinding()]
param(
 [Parameter(Mandatory)][string]$Binary,
 [Parameter(Mandatory)][string]$WindowTools,
 [Parameter(Mandatory)][string]$OutputDirectory
)
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
Import-Module $WindowTools -Force
$run=Join-Path $OutputDirectory ([guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $run -Force | Out-Null
$oldData=$env:LANPATH_DESKTOP_DATA_DIR; $oldCore=$env:LANPATH_DESKTOP_CORE_BINARY
$process=$null; $window=[IntPtr]::Zero; $server=$null
$checks=[Collections.Generic.List[string]]::new()
function Check([bool]$Condition,[string]$Message) {
 if(-not $Condition){throw "Failed: $Message"}
 $checks.Add($Message); Write-Output "PASS $Message"
}
function Shot([string]$Name) {
 Save-DesktopWindowScreenshot $window (Join-Path $run "$Name.png")
 Get-DesktopElements $window | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $run "$Name.json")
}
function Set-Field([string]$Name,[string]$Value) {
 $editor=Show-DesktopElement $window $Name
 $editor.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($Value)
 Start-Sleep -Milliseconds 150
}
function Wait-Hosts([string]$Count) {
 $deadline=[DateTime]::UtcNow.AddSeconds(20)
 do {
  $entry=Find-DesktopElement $window 'mitm-derived' -ById
  if($entry -and $entry.Current.Name -eq "资源主机 · $Count"){return}
  if([DateTime]::UtcNow -gt $deadline){throw "Resource hosts did not become $Count"}
  Start-Sleep -Milliseconds 150
 }while($true)
}
try {
 $portFile=Join-Path $run 'fixture-port.txt'
 $server=Start-Job -ArgumentList $portFile -ScriptBlock {
  param($PortFile)
  $listener=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,0)
  $listener.Start()
  $port=$listener.LocalEndpoint.Port
  [IO.File]::WriteAllText($PortFile,[string]$port)
  try {
   for($i=0;$i -lt 2;$i++){
    $accepted=$listener.AcceptTcpClientAsync(); $deadline=[DateTime]::UtcNow.AddSeconds(120)
    while(-not $accepted.Wait(100)){if([DateTime]::UtcNow -gt $deadline){throw 'Fixture request timed out'}}
    $client=$accepted.Result; $stream=$client.GetStream()
    $reader=[IO.StreamReader]::new($stream,[Text.Encoding]::ASCII,$false,1024,$true)
    $line=$reader.ReadLine()
    do{$header=$reader.ReadLine()}while($header)
    $body=if($line.StartsWith('GET /module.sgmodule ')){
     "[Script]`nhook=type=http-response,pattern=^https://api.fixture.invalid/,script-path=http://127.0.0.1:$port/response.js,requires-body=true,max-size=-1`n[MITM]`nhostname=%APPEND% api.fixture.invalid, -excluded.fixture.invalid"
    }elseif($line.StartsWith('GET /response.js ')){'$done({});'}else{throw 'Unexpected fixture path'}
    $bytes=[Text.Encoding]::UTF8.GetBytes($body)
    $headers=[Text.Encoding]::ASCII.GetBytes("HTTP/1.1 200 OK`r`nContent-Length: $($bytes.Length)`r`nConnection: close`r`n`r`n")
    $stream.Write($headers);$stream.Write($bytes);$client.Dispose()
   }
  }finally{$listener.Stop()}
 }
 $deadline=[DateTime]::UtcNow.AddSeconds(15)
 while(-not(Test-Path $portFile)){if([DateTime]::UtcNow -gt $deadline){throw 'Fixture did not start'};Start-Sleep -Milliseconds 100}
 $port=[int](Get-Content $portFile -Raw)
 $env:LANPATH_DESKTOP_DATA_DIR=Join-Path $run 'userdata'
 Remove-Item Env:LANPATH_DESKTOP_CORE_BINARY -ErrorAction SilentlyContinue
 $process=Start-Process -FilePath $Binary -WorkingDirectory (Split-Path $Binary) -RedirectStandardError (Join-Path $run 'stderr.log') -PassThru
 $deadline=[DateTime]::UtcNow.AddSeconds(30)
 do {
  Start-Sleep -Milliseconds 100;$process.Refresh();$window=$process.MainWindowHandle
  if($process.HasExited){throw "GUI exited: $($process.ExitCode)"}
  if([DateTime]::UtcNow -gt $deadline){throw 'No visible window'}
 }while($window -eq [IntPtr]::Zero)
 Wait-DesktopElement $window 'subscription-status' -ById -TimeoutSeconds 30 | Out-Null
 Set-DesktopWindowSize $window 1180 800
 Invoke-DesktopElement $window '展开工具与扩展'
 Invoke-DesktopElement $window 'MITM'
 Wait-Hosts '0'
 Check ($null -eq (Find-DesktopElement $window 'mitm-reload' -ById) -and $null -eq (Find-DesktopElement $window 'mitm-refresh' -ById)) 'manual reload and status refresh commands removed'
 Check ($null -eq (Find-DesktopElement $window 'mitm-copy-ca' -ById)) 'CA details collapsed by default'
 Check ($null -ne (Wait-DesktopElement $window 'mitm-install-ca' -ById) -and $null -ne (Find-DesktopElement $window 'mitm-remove-ca' -ById)) 'CA install and remove stay directly available'
 Invoke-DesktopElement $window 'mitm-ca-details' -ById
 Wait-DesktopElement $window 'mitm-copy-ca' -ById | Out-Null
 Shot 'ca-details'
 Invoke-DesktopElement $window 'mitm-ca-details' -ById
 Invoke-DesktopElement $window 'mitm-manual-toggle' -ById
 Set-Field '手动 MITM 主机' 'draft.fixture.invalid'
 Invoke-DesktopElement $window '脚本'
 Invoke-DesktopElement $window '添加资源…'
 Set-Field '名称' 'MITM 主机夹具'
 Set-Field 'URL' "http://127.0.0.1:$port/module.sgmodule"
 Invoke-DesktopElement $window '配置片段'
 Invoke-DesktopElement $window 'Surge'
 Send-DesktopKeys $window '^s'
 Wait-DesktopElement $window '编辑资源 MITM 主机夹具' -TimeoutSeconds 20 | Out-Null
 Invoke-DesktopElement $window '更新资源'
 $deadline=[DateTime]::UtcNow.AddSeconds(25)
 do {
  $status=(Wait-DesktopElement $window 'scripts-status' -ById).Current.Name
  if($status.Contains('更新完成')){break}
  if([DateTime]::UtcNow -gt $deadline){throw "Resource update did not finish: $status"}
  Start-Sleep -Milliseconds 100
 }while($true)
 Check ($status.Contains('主机 2')) 'manual update reports the number of parsed hosts'
 Check (($server | Wait-Job -Timeout 5).State -eq 'Completed') 'module and associated JS fetched from loopback before leaving scripts'
 Receive-Job $server -ErrorAction Stop | Out-Null
 Invoke-DesktopElement $window 'MITM'
 Wait-Hosts '2'
 Check ($null -ne (Wait-DesktopElement $window 'api.fixture.invalid · MITM 主机夹具')) 'entering MITM refreshes derived hosts without manual reload'
 $editor=Wait-DesktopElement $window '手动 MITM 主机'
 Check ($editor.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value -eq 'draft.fixture.invalid') 'automatic sync preserves unsaved manual host draft'
 Shot 'resource-hosts'
 # Change only this owned fixture's saved cache: staying on the page must not
 # reload static settings. Leaving and entering must pick up the new declaration.
 $cache=Get-ChildItem (Join-Path $env:LANPATH_DESKTOP_DATA_DIR 'cache/remotes') -Filter 'remote-*.json' | Select-Object -First 1
 if(-not $cache){throw 'Module cache fixture missing'}
 $cached=[System.Text.Json.Nodes.JsonNode]::Parse((Get-Content $cache.FullName -Raw))
 $cached['hostnames'].AsArray().Add([System.Text.Json.Nodes.JsonNode]::Parse('"entry.fixture.invalid"'))
 [IO.File]::WriteAllText($cache.FullName,$cached.ToJsonString(),[Text.UTF8Encoding]::new($false))
 Start-Sleep -Seconds 7
 Check ((Wait-DesktopElement $window 'mitm-derived' -ById).Current.Name -eq '资源主机 · 2') 'static host snapshot stays unchanged while the page remains open'
 Check ((Wait-DesktopElement $window '手动 MITM 主机').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value -eq 'draft.fixture.invalid') 'waiting on page preserves manual host draft'
 Invoke-DesktopElement $window '脚本'
 Invoke-DesktopElement $window 'MITM'
 Wait-Hosts '3'
 Check ($null -ne (Wait-DesktopElement $window 'entry.fixture.invalid · MITM 主机夹具')) 'reentering loads the latest saved host snapshot'
 Check ((Wait-DesktopElement $window '手动 MITM 主机').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value -eq 'draft.fixture.invalid') 'entry reload preserves manual host draft'
 Send-DesktopKeys $window '^s'
 Wait-DesktopElement $window 'MITM 设置已保存；下次启动核心时生效' -TimeoutSeconds 15 | Out-Null
 Check ((Get-Content (Join-Path $env:LANPATH_DESKTOP_DATA_DIR 'app.toml') -Raw).Contains('draft.fixture.invalid')) 'Ctrl+S persists only manual host settings'
 Invoke-DesktopElement $window '请求记录 · 0'
 Shot 'traffic-empty'
 Invoke-DesktopElement $window '主机规则'
 Invoke-DesktopElement $window '脚本'
 Invoke-DesktopElement $window '启用'
 Invoke-DesktopElement $window 'MITM'
 Wait-Hosts '0'
 Check ($null -eq (Find-DesktopElement $window 'api.fixture.invalid · MITM 主机夹具')) 'disabled module no longer contributes derived hosts on entry'
 Invoke-DesktopElement $window 'mitm-manual-toggle' -ById
 Invoke-DesktopElement $window '外观'
 Invoke-DesktopElement $window '界面缩放百分比'
 Send-DesktopKeys $window '{DOWN 3}'
 Send-DesktopKeys $window '{ENTER}'
 Invoke-DesktopElement $window 'MITM'
 Set-DesktopWindowSize $window 960 640
 Shot 'minimum-150'
 $bounds=[System.Windows.Automation.AutomationElement]::FromHandle($window).Current.BoundingRectangle
 $save=Wait-DesktopElement $window 'mitm-save' -ById
 $hosts=Wait-DesktopElement $window 'mitm-derived' -ById
 Check ($save.Current.BoundingRectangle.Bottom -le $bounds.Bottom -and $hosts.Current.BoundingRectangle.Height -gt 40) '150 percent minimum window keeps save and host viewport visible'
 Invoke-DesktopElement $window '外观'
 Invoke-DesktopElement $window '恢复 100%'
 $process.CloseMainWindow() | Out-Null
 Check ($process.WaitForExit(10000) -and $process.ExitCode -eq 0) 'owned GUI exits normally'
 Check ((Get-Item (Join-Path $run 'stderr.log')).Length -eq 0) 'stderr is empty'
 [pscustomobject]@{checks=$checks.Count;artifacts=$run;realCoreStarted=$false;systemTrustChanged=$false} | ConvertTo-Json
}catch{
 if($process -and -not $process.HasExited -and $window -ne [IntPtr]::Zero){Shot 'failure'}
 Write-Output "ARTIFACTS=$run"
 Write-Output $_.ToString()
 Write-Output $_.ScriptStackTrace
 throw
}finally{
 if($process -and -not $process.HasExited){Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue}
 if($server){$server | Stop-Job -ErrorAction SilentlyContinue;$server | Remove-Job -Force -ErrorAction SilentlyContinue}
 $env:LANPATH_DESKTOP_DATA_DIR=$oldData;$env:LANPATH_DESKTOP_CORE_BINARY=$oldCore
}
