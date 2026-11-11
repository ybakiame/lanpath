#Requires -Version 7.0
[CmdletBinding()]
param(
 [Parameter(Mandatory)][string]$Binary,
 [Parameter(Mandatory)][string]$WindowTools,
 [Parameter(Mandatory)][string]$OutputDirectory,
 [switch]$LayoutOnly
)
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
Import-Module $WindowTools -Force
$run=Join-Path $OutputDirectory ([guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $run -Force | Out-Null
$checks=[Collections.Generic.List[string]]::new()
$oldData=$env:LANPATH_DESKTOP_DATA_DIR
$oldCore=$env:LANPATH_DESKTOP_CORE_BINARY
$process=$null
$window=[IntPtr]::Zero
function Check([bool]$Condition,[string]$Message) {
 if (-not $Condition) { throw "Failed: $Message" }
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
function Get-Field([string]$Name) {
 $editor=Wait-DesktopElement $window $Name
 $editor.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value
}
function Wait-Setting([string]$Expected) {
 $deadline=[DateTime]::UtcNow.AddSeconds(8)
 do {
  $text=Get-Content (Join-Path $env:LANPATH_DESKTOP_DATA_DIR 'app.toml') -Raw -ErrorAction SilentlyContinue
  if ($text -and $text.Contains($Expected)) { $checks.Add("persisted $Expected"); return }
  Start-Sleep -Milliseconds 100
 } while ([DateTime]::UtcNow -lt $deadline)
 throw "Setting not persisted: $Expected"
}
try {
 $env:LANPATH_DESKTOP_DATA_DIR=Join-Path $run 'userdata'
 Remove-Item Env:LANPATH_DESKTOP_CORE_BINARY -ErrorAction SilentlyContinue
 $process=Start-Process -FilePath $Binary -WorkingDirectory (Split-Path $Binary) -RedirectStandardError (Join-Path $run 'stderr.log') -PassThru
 $deadline=[DateTime]::UtcNow.AddSeconds(30)
 do {
  Start-Sleep -Milliseconds 100; $process.Refresh()
  if ($process.HasExited) { throw "GUI exited: $($process.ExitCode)" }
  $window=$process.MainWindowHandle
  if ([DateTime]::UtcNow -gt $deadline) { throw 'No visible window' }
 } while ($window -eq [IntPtr]::Zero)
 Wait-DesktopElement $window 'subscription-status' -ById | Out-Null
 Set-DesktopWindowSize $window 1180 800
 Invoke-DesktopElement $window '展开工具与扩展'
 Invoke-DesktopElement $window '结构化配置'
 foreach ($tab in @('网络','DNS','出站','路由与缓存','规则')) {
  Invoke-DesktopElement $window $tab
  $save=Wait-DesktopElement $window '保存配置'
  $content=Wait-DesktopElement $window 'desktop-content' -ById
  Check ($save.Current.BoundingRectangle.Top -gt $content.Current.BoundingRectangle.Bottom-150) "$tab save action stays in footer"
  Shot ('config-'+[array]::IndexOf(@('网络','DNS','出站','路由与缓存','规则'),$tab))
 }
 if (-not $LayoutOnly) {
  Invoke-DesktopElement $window '网络'
  Set-Field '混合代理端口' '17901'
  Invoke-DesktopElement $window 'DNS'
  Invoke-DesktopElement $window '网络'
  Check ((Get-Field '混合代理端口') -eq '17901') 'network draft survives tab switch'
  Send-DesktopKeys $window '^s'
  Wait-Setting 'mixed_port = 17901'
  Set-Field '混合代理端口' '17899'
  Send-DesktopKeys $window '^s'
  Wait-Setting 'mixed_port = 17899'
 }
 Invoke-DesktopElement $window '连接'
 $summary=Wait-DesktopElement $window 'connections-summary' -ById
 Check ($summary.Current.BoundingRectangle.Height -ge 20) 'connection summary remains visible'
 Check ($null -ne (Find-DesktopElement $window '活动连接')) 'connection view selector remains visible'
 $small=(Wait-DesktopElement $window 'connections-list' -ById).Current.BoundingRectangle.Top
 Shot 'connections-800'
 Set-DesktopWindowSize $window 1180 1000
 $large=(Wait-DesktopElement $window 'connections-list' -ById).Current.BoundingRectangle.Top
 Check ([Math]::Abs($small-$large) -le 4) 'connection list top follows content rather than window height'
 Shot 'connections-1000'
 Set-DesktopWindowSize $window 1180 800
 Invoke-DesktopElement $window '代理策略'
 Shot 'proxies'
 if (-not $LayoutOnly) {
  Set-Field '搜索当前组节点' 'test'
  Invoke-DesktopElement $window '延迟'
  Set-Field '搜索当前组节点' ''
  Invoke-DesktopElement $window '直连'
  Wait-Setting 'rule_mode = "direct"'
  Invoke-DesktopElement $window '规则'
  Wait-Setting 'rule_mode = "rule"'
  Check ($process.Responding) 'proxy search sorting and offline mode remain responsive'
 }
 Invoke-DesktopElement $window '脚本'
 Shot 'scripts-list-empty'
 if (-not $LayoutOnly) {
  Invoke-DesktopElement $window '添加资源…'
  Set-Field '名称' 'UI review resource'
  Set-Field 'URL' 'https://example.com/ui-review.js'
  Invoke-DesktopElement $window '启用远程资源'
  Set-Field '更新间隔' '2'
  Invoke-DesktopElement $window 'remote-cancel' -ById
  Invoke-DesktopElement $window '继续编辑'
  Check ((Get-Field '名称') -eq 'UI review resource') 'cancel discard keeps new resource draft'
  Shot 'script-form'
  Send-DesktopKeys $window '^s'
  Wait-DesktopElement $window '编辑资源 UI review resource' -TimeoutSeconds 15 | Out-Null
  Check ($null -ne (Find-DesktopElement $window '编辑资源 UI review resource')) 'Ctrl+S saves new resource and closes modal'
  Wait-Setting 'update_interval_secs = 172800'
  Check (-not (Test-Path (Join-Path $env:LANPATH_DESKTOP_DATA_DIR 'remotes.json'))) 'remote manifest lives in app.toml'
  Shot 'script-card'
  Invoke-DesktopElement $window '编辑资源 UI review resource'
  Set-Field '描述' 'Updated in native review'
  Invoke-DesktopElement $window 'remote-cancel' -ById
  Invoke-DesktopElement $window '继续编辑'
  Check ((Get-Field '描述') -eq 'Updated in native review') 'cancel discard keeps edited resource draft'
  Invoke-DesktopElement $window '保存资源'
  Wait-DesktopElement $window '编辑资源 UI review resource' -TimeoutSeconds 15 | Out-Null
  Invoke-DesktopElement $window '删除资源 UI review resource'
  Send-DesktopKeys $window '{ESC}'
  Check ($null -ne (Wait-DesktopElement $window '编辑资源 UI review resource')) 'Escape cancels resource deletion'
  Invoke-DesktopElement $window '定时任务'
  Shot 'scripts-tasks'
  Invoke-DesktopElement $window '配置导入'
  Shot 'scripts-import'
 }
 Invoke-DesktopElement $window '日志'
 Shot 'logs-memory'
 if (-not $LayoutOnly) {
  Invoke-DesktopElement $window '警告'
  Invoke-DesktopElement $window '全部'
  Invoke-DesktopElement $window '实时刷新'
  $before=(Wait-DesktopElement $window 'logs-status' -ById).Current.Name
  Invoke-DesktopElement $window '日志显示选项'
  Shot 'logs-options'
  Invoke-DesktopElement $window '最近 200 条'
  Invoke-DesktopElement $window '日志显示选项'
  Invoke-DesktopElement $window '清空内存日志…'
  Send-DesktopKeys $window '{ESC}'
  Check ((Wait-DesktopElement $window 'logs-status' -ById).Current.Name -eq $before) 'Escape cancels memory log clearing'
  Invoke-DesktopElement $window '磁盘日志'
  Check ($null -ne (Wait-DesktopElement $window 'disk-logs-status' -ById)) 'disk logs have separate tools and footer status'
  Shot 'logs-disk'
 }
 Invoke-DesktopElement $window '外观'
 Invoke-DesktopElement $window '界面缩放百分比'
 Send-DesktopKeys $window '{DOWN 3}'
 Send-DesktopKeys $window '{ENTER}'
 Wait-Setting 'scale_percent = 150'
 Set-DesktopWindowSize $window 960 640
 foreach ($page in @('结构化配置','代理策略','连接','脚本','日志')) {
  Invoke-DesktopElement $window $page
  Check ($process.Responding) "minimum window 150% $page responds"
  Shot ('minimum-'+[array]::IndexOf(@('结构化配置','代理策略','连接','脚本','日志'),$page))
 }
 Invoke-DesktopElement $window '外观'
 Invoke-DesktopElement $window '恢复 100%'
 Wait-Setting 'scale_percent = 100'
 Check ($process.CloseMainWindow() -and $process.WaitForExit(15000)) 'graceful close completes'
 Check ($process.ExitCode -eq 0) 'normal exit code'
 $checks | ConvertTo-Json | Set-Content (Join-Path $run 'checks.json')
 Write-Output "ARTIFACTS=$run"
} catch {
 if ($window -ne [IntPtr]::Zero -and -not $process.HasExited) { Shot 'failure' }
 Write-Output "ARTIFACTS=$run"
 throw
} finally {
 if ($process -and -not $process.HasExited) {
  $process.CloseMainWindow() | Out-Null
  if (-not $process.WaitForExit(10000)) { Stop-Process -InputObject $process; $process.WaitForExit() }
 }
 $env:LANPATH_DESKTOP_DATA_DIR=$oldData
 $env:LANPATH_DESKTOP_CORE_BINARY=$oldCore
}
