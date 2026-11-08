#Requires -Version 7.0
[CmdletBinding()]
param(
 [Parameter(Mandatory)][string]$Binary,
 [Parameter(Mandatory)][string]$WindowTools,
 [Parameter(Mandatory)][string]$OutputDirectory,
 [switch]$AdvisoryOnly
)
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
Import-Module $WindowTools -Force
$run=Join-Path $OutputDirectory ([guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $run -Force | Out-Null
$oldData=$env:LANPATH_DESKTOP_DATA_DIR; $oldCore=$env:LANPATH_DESKTOP_CORE_BINARY
$process=$null; $window=[IntPtr]::Zero; $launch=0
$checks=[Collections.Generic.List[string]]::new()
function Check([bool]$Condition,[string]$Message) {
 if (-not $Condition) { throw "Failed: $Message" }
 $checks.Add($Message); Write-Output "PASS $Message"
}
function Start-Owned {
 $script:launch++
 $script:process=Start-Process -FilePath $Binary -WorkingDirectory (Split-Path $Binary) -RedirectStandardError (Join-Path $run "stderr-$launch.log") -PassThru
 $deadline=[DateTime]::UtcNow.AddSeconds(30)
 do {
  Start-Sleep -Milliseconds 100; $process.Refresh()
  if ($process.HasExited) { throw "GUI exited: $($process.ExitCode)" }
  $script:window=$process.MainWindowHandle
  if ([DateTime]::UtcNow -gt $deadline) { throw 'No visible window' }
 } while ($window -eq [IntPtr]::Zero)
 Wait-DesktopElement $window 'subscription-status' -ById -TimeoutSeconds 30 | Out-Null
 Set-DesktopWindowSize $window 1180 800
}
function Stop-Owned {
 $process.CloseMainWindow() | Out-Null
 if (-not $process.WaitForExit(10000)) { throw 'Owned GUI did not close normally' }
 Check ($process.ExitCode -eq 0) "owned launch $launch exits normally"
}
function Shot([string]$Name) {
 Save-DesktopWindowScreenshot $window (Join-Path $run "$Name.png")
 Get-DesktopElements $window | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $run "$Name.json")
}
function Write-Fixture([string]$Name,[string]$Content) {
 $path=Join-Path $env:LANPATH_DESKTOP_DATA_DIR $Name
 New-Item -ItemType Directory -Path (Split-Path $path) -Force | Out-Null
 [IO.File]::WriteAllText($path,$Content,[Text.UTF8Encoding]::new($false))
}
function Set-Field([string]$Name,[string]$Value) {
 $editor=Show-DesktopElement $window $Name
 $editor.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($Value)
 Start-Sleep -Milliseconds 150
}
try {
 $env:LANPATH_DESKTOP_DATA_DIR=Join-Path $run 'userdata'
 Remove-Item Env:LANPATH_DESKTOP_CORE_BINARY -ErrorAction SilentlyContinue
 Start-Owned
 Invoke-DesktopElement $window '展开工具与扩展'
 Invoke-DesktopElement $window '脚本'
 Invoke-DesktopElement $window '本地资源'
 Check ($null -eq (Find-DesktopElement $window '配置导入')) 'one-shot import replaced by managed local resources'
 Invoke-DesktopElement $window 'local-add' -ById
 Set-Field '本地资源名称' '本地模块'
 $snippet=if($AdvisoryOnly){"[Script]`nhook = type=http-response,pattern=^https://example.invalid/,script-path=hook.js,requires-body=true,max-size=-1"}else{"[Script]`nhook = type=http-request,pattern=^https://example.invalid/,script-path=hook.js`n[MITM]`nhostname = example.invalid"}
 Set-Field '本地配置片段' $snippet
 Invoke-DesktopElement $window 'local-save' -ById
 Check ((Wait-DesktopElement $window 'local-form-status' -ById).Current.Name.Contains('关联 JS')) 'unbound JS blocks saving and keeps the modal'
 Invoke-DesktopElement $window 'local-identify' -ById
 Invoke-DesktopElement $window '关联 JS · 1'
 Set-Field '关联 JS 1' '$done({first:true});'
 if($AdvisoryOnly){
  $status=(Wait-DesktopElement $window 'local-form-status' -ById).Current.Name
  Check ($status.Contains('32 MiB') -and -not $status.Contains('[detail]')) 'accepted declaration keeps its complete body-limit advisory after JS editing'
  Shot 'response-body-advisory-modal'
  Invoke-DesktopElement $window 'local-save' -ById
  $index=Join-Path $env:LANPATH_DESKTOP_DATA_DIR 'resources/imports/local_imports.json'
  $deadline=[DateTime]::UtcNow.AddSeconds(15)
  while(-not(Test-Path $index)){if([DateTime]::UtcNow -gt $deadline){throw 'Advisory blocked saving'};Start-Sleep -Milliseconds 100}
  $ids=@(Get-Content $index -Raw | ConvertFrom-Json)
  $saved=Get-Content (Join-Path $env:LANPATH_DESKTOP_DATA_DIR "resources/imports/local/$($ids[0]).json") -Raw | ConvertFrom-Json
  Check ($saved.scripts.Count -eq 1 -and $saved.local.content.Contains('max-size=-1')) 'supported response declaration saves without losing the hook'
  Check ($null -eq (Find-DesktopElement $window 'local-save' -ById)) 'successful advisory-only save closes modal'
  Shot 'response-body-advisory-saved'
  Stop-Owned
  $records=@(Get-ChildItem (Join-Path $env:LANPATH_DESKTOP_DATA_DIR 'logs') -File -Recurse | ForEach-Object { Get-Content $_.FullName -Raw }) -join "`n"
  Check ($records.Contains('32 MiB inspection envelope') -and $records.Contains('larger responses skip the script and pass through unchanged')) 'actual application log retains the complete body-limit explanation'
  Check (-not $records.Contains('Lanpath[detail]')) 'application log no longer truncates the static diagnostic'
  [pscustomobject]@{checks=$checks.Count;artifacts=$run;realCoreStarted=$false;advisoryOnly=$true} | ConvertTo-Json
  return
 }

 Invoke-DesktopElement $window '配套图标'
 # A generated PNG is test data, never an application asset or personal file.
 $icon=Join-Path $run 'fixture-icon.png'
 $bitmap=[Drawing.Bitmap]::new(16,16)
 $canvas=[Drawing.Graphics]::FromImage($bitmap)
 $canvas.Clear([Drawing.Color]::FromArgb(255,100,68,155)); $bitmap.Save($icon,[Drawing.Imaging.ImageFormat]::Png)
 $canvas.Dispose(); $bitmap.Dispose()
 Invoke-DesktopElement $window '选择图标…'
 Select-DesktopFile $window $icon
 Wait-DesktopElement $window '移除图标' -TimeoutSeconds 15 | Out-Null
 Shot 'local-icon-modal'
 Invoke-DesktopElement $window 'local-save' -ById
 $index=Join-Path $env:LANPATH_DESKTOP_DATA_DIR 'resources/imports/local_imports.json'
 $deadline=[DateTime]::UtcNow.AddSeconds(15)
 while (-not (Test-Path $index)) {if ([DateTime]::UtcNow -gt $deadline) {throw 'Missing saved local registry'};Start-Sleep -Milliseconds 100}
 $ids=@(Get-Content $index -Raw | ConvertFrom-Json)
 Check ($ids.Count -eq 1) 'new resource registers one UUID'
 $id=$ids[0];$bundle=Join-Path $env:LANPATH_DESKTOP_DATA_DIR "resources/imports/local/$id.json"
 $saved=Get-Content $bundle -Raw | ConvertFrom-Json
 Check ($saved.local.name -eq '本地模块' -and $saved.scripts[0].source_id -eq 'hook.js') 'snippet and local JS identity share one owner'
 Check ($saved.local.icon.mime -eq 'image/png' -and $saved.local.icon.base64.Length -gt 0) 'selected icon copied into durable bundle'
 Check (-not ((Get-Content $bundle -Raw).Contains($run))) 'host file paths are not stored'
 Wait-DesktopElement $window "local-edit-$id" -ById | Out-Null
 Check ($null -eq (Find-DesktopElement $window 'local-save' -ById)) 'successful local save closes modal'
 Invoke-DesktopElement $window "local-edit-$id" -ById
 Set-Field '本地资源名称' '修改后的模块'
 Invoke-DesktopElement $window '关联 JS · 1'
 $js=Join-Path $run 'updated.js';[IO.File]::WriteAllText($js,'$done({updated:true});',[Text.UTF8Encoding]::new($false))
 Invoke-DesktopElement $window '从 JS 文件读取…'
 Select-DesktopFile $window $js
 $deadline=[DateTime]::UtcNow.AddSeconds(15)
 do {Start-Sleep -Milliseconds 100;$field=Find-DesktopElement $window '关联 JS 1';$value=if ($field) {$field.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value}else{''}} while ($value -ne '$done({updated:true});' -and [DateTime]::UtcNow -lt $deadline)
 Check ($value -eq '$done({updated:true});') 'associated JS can be replaced from a UTF-8 file'

 Send-DesktopKeys $window '^s'
 Wait-DesktopElement $window "local-edit-$id" -ById | Out-Null
 $saved=Get-Content $bundle -Raw | ConvertFrom-Json
 Check ($saved.local.name -eq '修改后的模块' -and $saved.scripts[0].source -eq '$done({updated:true});') 'Ctrl+S updates snippet bundle and associated JS'
 Check (@(Get-Content $index -Raw | ConvertFrom-Json).Count -eq 1) 'editing retains the original UUID instead of appending'
 Invoke-DesktopElement $window '停用'
 $deadline=[DateTime]::UtcNow.AddSeconds(10)
 do {Start-Sleep -Milliseconds 100;$saved=Get-Content $bundle -Raw | ConvertFrom-Json} while ($saved.local.enabled -and [DateTime]::UtcNow -lt $deadline)
 Check (-not $saved.local.enabled) 'resource enabled state is persisted'
 Invoke-DesktopElement $window "local-edit-$id" -ById
 Set-Field '本地资源名称' '待丢弃的名称'
 Send-DesktopKeys $window '{ESC}'
 Invoke-DesktopElement $window '继续编辑'
 Check ((Wait-DesktopElement $window '本地资源名称').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value -eq '待丢弃的名称') 'canceling discard preserves local draft'
 Send-DesktopKeys $window '{ESC}';Invoke-DesktopElement $window '放弃修改'
 Check ((Get-Content $bundle -Raw | ConvertFrom-Json).local.name -eq '修改后的模块') 'discard leaves persisted resource unchanged'
 Invoke-DesktopElement $window '删除'
 Invoke-DesktopElement $window '取消'
 Check (Test-Path $bundle) 'canceling delete preserves entire resource'
 Shot 'local-managed-list'
 Stop-Owned
 Start-Owned
 Invoke-DesktopElement $window '展开工具与扩展'
 Invoke-DesktopElement $window '脚本';Invoke-DesktopElement $window '本地资源'
 Invoke-DesktopElement $window "local-edit-$id" -ById
 Check ((Wait-DesktopElement $window '本地资源名称').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value -eq '修改后的模块') 'reopen restores the managed resource'
 Invoke-DesktopElement $window '配套图标'
 Wait-DesktopElement $window '替换图标…' | Out-Null
 Check ($true) 'resource icon remains editable after restart'
 Send-DesktopKeys $window '{ESC}'
 Invoke-DesktopElement $window '外观'
 Invoke-DesktopElement $window '界面缩放百分比'
 Send-DesktopKeys $window '{DOWN 3}'
 Send-DesktopKeys $window '{ENTER}'
 Set-DesktopWindowSize $window 1180 1000
 Invoke-DesktopElement $window '脚本'
 Invoke-DesktopElement $window '本地资源'
 Invoke-DesktopElement $window "local-edit-$id" -ById
 Invoke-DesktopElement $window '配套图标'
 Set-DesktopWindowSize $window 960 640
 Shot 'local-minimum-modal'
 $bounds=[System.Windows.Automation.AutomationElement]::FromHandle($window).Current.BoundingRectangle
 Check ((Wait-DesktopElement $window 'local-save' -ById).Current.BoundingRectangle.Bottom -le $bounds.Bottom) 'minimum window keeps local modal save footer visible at 150 percent'
 Invoke-DesktopElement $window '移除图标'
 Invoke-DesktopElement $window 'local-save' -ById
 Wait-DesktopElement $window "local-edit-$id" -ById | Out-Null
 Check ($null -eq (Get-Content $bundle -Raw | ConvertFrom-Json).local.icon) 'icon removal persists with resource'
 Set-DesktopWindowSize $window 1180 800
 Invoke-DesktopElement $window '外观'
 Invoke-DesktopElement $window '恢复 100%'
 Invoke-DesktopElement $window '脚本'
 Invoke-DesktopElement $window '本地资源'
 Invoke-DesktopElement $window '删除';Invoke-DesktopElement $window '删除资源'
 $deadline=[DateTime]::UtcNow.AddSeconds(10)
 while ((Test-Path $bundle) -and [DateTime]::UtcNow -lt $deadline) {Start-Sleep -Milliseconds 100}
 Check (-not (Test-Path $bundle) -and @(Get-Content $index -Raw | ConvertFrom-Json).Count -eq 0) 'delete removes snippet, JS and icon ownership together'
 Invoke-DesktopElement $window '代理策略'
 $selected=Wait-DesktopElement $window 'proxy-test-selected' -ById
 $group=Wait-DesktopElement $window 'proxy-test-group' -ById
 Check ([Math]::Abs($selected.Current.BoundingRectangle.Top-$group.Current.BoundingRectangle.Top) -lt 3) 'both speed test actions share the top toolbar'
 Check ($null -eq (Find-DesktopElement $window '重新读取代理策略')) 'proxy manual reload action removed'
 Invoke-DesktopElement $window '直连'
 $deadline=[DateTime]::UtcNow.AddSeconds(15);$app=Join-Path $env:LANPATH_DESKTOP_DATA_DIR 'app.toml'
 while (-not ((Get-Content $app -Raw).Contains('rule_mode = "direct"'))) {if([DateTime]::UtcNow -gt $deadline){throw 'Mode not persisted'};Start-Sleep -Milliseconds 100}
 Check ($true) 'proxy mode persists offline'
 Shot 'proxy-top-toolbar'
 Stop-Owned
 foreach ($stderr in Get-ChildItem $run -Filter 'stderr-*.log') {Check ($stderr.Length -eq 0) "empty $($stderr.Name)"}
 [pscustomobject]@{checks=$checks.Count;artifacts=$run;realCoreStarted=$false;userDataUsed=$false;fileDialogsVerified=$true;stoppedFixtureIconSeeded=$false} | ConvertTo-Json
} catch {
 if ($window -ne [IntPtr]::Zero -and $process -and -not $process.HasExited) { Shot 'failure' }
 Write-Output "ARTIFACTS=$run"
 throw
} finally {
 if ($process -and -not $process.HasExited) {
  $process.CloseMainWindow() | Out-Null
  if (-not $process.WaitForExit(5000)) { Stop-Process -Id $process.Id -Force }
 }
 $env:LANPATH_DESKTOP_DATA_DIR=$oldData; $env:LANPATH_DESKTOP_CORE_BINARY=$oldCore
}
