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
 # Establish the product-owned root before seeding legacy files. Never use user data.
 Start-Owned; Stop-Owned
 $app=Join-Path $env:LANPATH_DESKTOP_DATA_DIR 'app.toml'
 Check (-not ((Get-Content $app -Raw) -match '(?m)^\[resources\]')) 'fresh fixture has no resource section'
 $remote=@{name='旧版资源';url='https://example.invalid/a.js';kind='Script';dialect='Surge';update_interval_secs=5400;enabled=$false;description='旧清单';argument_values=@();arguments=@()}
 Write-Fixture 'remotes.json' (ConvertTo-Json -InputObject @($remote) -Depth 5)
 $subscription=@{id=[guid]::NewGuid().ToString();name='迁移订阅';url='https://example.invalid/sub';enabled=$false;userinfo=$null;node_count=0;error=$null}
 Write-Fixture 'subscriptions.json' (ConvertTo-Json -InputObject @($subscription) -Depth 5)
 Write-Fixture 'remote_cache/local_imports.json' '[]'
 Write-Fixture 'remote_cache/local/opaque.json' 'persistent local source'
 Write-Fixture 'remote_cache/orphan.json' 'inactive remote fixture'
 Write-Fixture 'remote_refresh.json' '{"attempts":{}}'
 Write-Fixture 'icons/fixture.png' 'opaque image fixture'
 Write-Fixture 'subscription_cache/original.source.json' 'original response'
 Start-Owned
 foreach ($name in @('remotes.json','subscriptions.json','remote_cache','remote_refresh.json','icons','subscription_cache')) {
  Check (-not (Test-Path (Join-Path $env:LANPATH_DESKTOP_DATA_DIR $name))) "legacy $name retired after migration"
 }
 foreach ($name in @('cache/remotes/orphan.json','cache/remotes/refresh.json','cache/icons/fixture.png','cache/subscriptions/original.source.json','resources/imports/local/opaque.json')) {
  Check (Test-Path (Join-Path $env:LANPATH_DESKTOP_DATA_DIR $name)) "migrated $name exists"
 }
 $backups=@(Get-ChildItem (Join-Path $env:LANPATH_DESKTOP_DATA_DIR 'backup/migrations') -Directory -Filter 'resource-layout-v1-*')
 Check ($backups.Count -eq 1 -and (Test-Path (Join-Path $backups[0].FullName 'legacy/remotes.json'))) 'migration retains original manifest backup'
 Check ((Get-Content (Join-Path $env:LANPATH_DESKTOP_DATA_DIR 'subscriptions.toml') -Raw).Contains('迁移订阅')) 'subscription metadata uses standalone subscriptions.toml'
 Check (-not (Get-Content $app -Raw).Contains('迁移订阅')) 'app.toml excludes subscription manifest'
 Invoke-DesktopElement $window '展开工具与扩展'
 Invoke-DesktopElement $window '脚本'
 Check ($null -eq (Find-DesktopElement $window 'scripts-reload' -ById)) 'redundant local reload command removed'
 Shot 'scripts-migrated-list'
 Check ((Wait-DesktopElement $window 'remote-resource-旧版资源' -ById).Current.Name.Contains('90 分钟')) 'interval is human readable'
 Invoke-DesktopElement $window '编辑资源 旧版资源'
 Shot 'modal-initial'
 Check ($null -ne (Find-DesktopElement $window 'resource-form-scroll' -ById)) 'edit opens modal'
 Set-Field '描述' '保留草稿'
 Send-DesktopKeys $window '{ESC}'
 Invoke-DesktopElement $window '继续编辑'
 $field=Wait-DesktopElement $window '描述'
 Check ($field.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value -eq '保留草稿') 'Escape discard cancellation preserves draft'
 Set-Field '更新间隔' '0'
 Invoke-DesktopElement $window '保存资源'
 Check ($null -ne (Wait-DesktopElement $window '更新间隔必须大于 0')) 'invalid interval stays in modal with error'
 Set-Field '更新间隔' '2'
 Invoke-DesktopElement $window '更新间隔单位'
 Send-DesktopKeys $window '{DOWN}'
 Send-DesktopKeys $window '{ENTER}'
 Invoke-DesktopElement $window '保存资源'
 Wait-DesktopElement $window '编辑资源 旧版资源' | Out-Null
 Check ((Get-Content $app -Raw).Contains('update_interval_secs = 7200')) 'dropdown converts two hours to exact persisted seconds'
 Check ($null -eq (Find-DesktopElement $window 'remote-save' -ById)) 'successful save closes modal'
 Invoke-DesktopElement $window '添加资源…'
 Set-Field '名称' '未保存资源'
 Send-DesktopKeys $window '{ESC}'
 Invoke-DesktopElement $window '放弃修改'
 Check ($null -eq (Find-DesktopElement $window '编辑资源 未保存资源')) 'discarded new resource never enters list'
 Invoke-DesktopElement $window '添加资源…'
 Set-Field '名称' '新资源'
 Set-Field 'URL' 'https://example.invalid/new.js'
 Invoke-DesktopElement $window '启用远程资源'
 Send-DesktopKeys $window '^s'
 Wait-DesktopElement $window '编辑资源 新资源' -TimeoutSeconds 15 | Out-Null
 Check ($null -ne (Find-DesktopElement $window '编辑资源 旧版资源')) 'Ctrl+S creates resource while preserving migrated entries'
 # Small window at 150%: standard dialog body scrolls independently of footer.
 Invoke-DesktopElement $window '外观'
 Invoke-DesktopElement $window '界面缩放百分比'
 Send-DesktopKeys $window '{DOWN 3}'
 Send-DesktopKeys $window '{ENTER}'
 Check ((Get-Content $app -Raw).Contains('scale_percent = 150')) '150 percent scale is applied'
 Invoke-DesktopElement $window '脚本'
 Invoke-DesktopElement $window '编辑资源 旧版资源'
 Wait-DesktopElement $window 'resource-form-scroll' -ById | Out-Null
 Set-DesktopWindowSize $window 960 640
 Shot 'modal-minimum-150'
 $save=Wait-DesktopElement $window 'remote-save' -ById
 $cancel=Wait-DesktopElement $window 'remote-cancel' -ById
 $bounds=[System.Windows.Automation.AutomationElement]::FromHandle($window).Current.BoundingRectangle
 Check ($save.Current.BoundingRectangle.Bottom -le $bounds.Bottom -and $cancel.Current.BoundingRectangle.Width -gt 0) 'minimum window keeps modal footer visible at 150 percent'
 Send-DesktopKeys $window '{ESC}'
 Invoke-DesktopElement $window '外观'
 Invoke-DesktopElement $window '恢复 100%'
 Stop-Owned
 Start-Owned
 Invoke-DesktopElement $window '展开工具与扩展'
 Invoke-DesktopElement $window '脚本'
 Check ((Wait-DesktopElement $window 'remote-resource-旧版资源' -ById).Current.Name.Contains('2 小时')) 'reopen uses unified manifest and readable interval'
 Check (@(Get-ChildItem (Join-Path $env:LANPATH_DESKTOP_DATA_DIR 'backup/migrations') -Directory -Filter 'resource-layout-v1-*').Count -eq 1) 'migration is idempotent across reopen'
 Stop-Owned
 foreach ($stderr in Get-ChildItem $run -Filter 'stderr-*.log') { Check ($stderr.Length -eq 0) "empty $($stderr.Name)" }
 [pscustomobject]@{checks=$checks.Count;artifacts=$run;realCoreStarted=$false;userDataUsed=$false} | ConvertTo-Json
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
