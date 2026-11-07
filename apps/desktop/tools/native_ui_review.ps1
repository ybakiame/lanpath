#Requires -Version 7.0
# Optional native UI review, not a CI gate. WindowTools is the workspace-level
# DesktopWindow.psm1; this caller owns only its child process and temporary data.
[CmdletBinding()]
param(
 [Parameter(Mandatory)][string]$Binary,
 [Parameter(Mandatory)][string]$WindowTools,
 [Parameter(Mandatory)][string]$OutputDirectory,
 [string]$MihomoFixture,
 [string]$SingBoxFixture,
 [switch]$ConfigurationOnly,
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
$oldBacktrace=$env:RUST_BACKTRACE
$process=$null
$server=$null
function Check([bool]$Condition,[string]$Message) {
 if (-not $Condition) { throw "Failed: $Message" }
 $checks.Add($Message); Write-Output "PASS $Message"
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
function Set-NetworkPort([string]$Value) {
 $editor=Show-DesktopElement $window '混合代理端口'
 $editor.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($Value)
 Start-Sleep -Milliseconds 200
}
function Shot([string]$Name) {
 Save-DesktopWindowScreenshot $window (Join-Path $run "$Name.png")
 Get-DesktopElements $window | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $run "$Name.json")
}
try {
 $env:LANPATH_DESKTOP_DATA_DIR=Join-Path $run 'userdata'
 $env:RUST_BACKTRACE='1'
 Remove-Item Env:LANPATH_DESKTOP_CORE_BINARY -ErrorAction SilentlyContinue
 $process=Start-Process -FilePath $Binary -WorkingDirectory (Split-Path $Binary) -RedirectStandardError (Join-Path $run "stderr.log") -PassThru
 $deadline=[DateTime]::UtcNow.AddSeconds(30)
 do {
  Start-Sleep -Milliseconds 100;$process.Refresh()
  if ($process.HasExited) { throw "GUI exited: $($process.ExitCode)" }
  $window=$process.MainWindowHandle
  if ([DateTime]::UtcNow -gt $deadline) { throw 'No visible window' }
 } while ($window -eq [IntPtr]::Zero)
 Start-Sleep -Milliseconds 1000
 Set-DesktopWindowSize $window 1180 800
 Check ($null -ne (Wait-DesktopElement $window 'subscription-status' -ById)) 'initial subscription workspace'
 Shot '01-subscriptions'
 if (-not $LayoutOnly) {
 if (-not $ConfigurationOnly) {
 Invoke-DesktopElement $window '外观'
 Invoke-DesktopElement $window '浅色主题'
 Wait-Setting 'theme = "light"'
 Shot '02-appearance-light'
 Invoke-DesktopElement $window '深色主题'
 Wait-Setting 'theme = "dark"'
 Invoke-DesktopElement $window '湖水青'
 Wait-Setting 'accent = "teal"'
 Invoke-DesktopElement $window '实色'
 Wait-Setting 'material = "solid"'
 Shot '03-appearance-solid'
 Invoke-DesktopElement $window '亚克力'
 Wait-Setting 'material = "acrylic"'
 Invoke-DesktopElement $window '纯黑背景'
 Wait-Setting 'pure_black = true'
 Shot '04-appearance-black'
 Invoke-DesktopElement $window '纯黑背景'
 Wait-Setting 'pure_black = false'
 Shot '05-appearance-acrylic'
 Send-DesktopKeys $window '{F6}'
 Send-DesktopKeys $window '{END}'
 Send-DesktopKeys $window '{ENTER}'
 Check ($null -ne (Wait-DesktopElement $window 'about-app-version' -ById)) 'F6 End Enter opens About through collapsed navigation'
 Check ($null -eq (Find-DesktopElement $window 'about-data-directory' -ById)) 'About omits data directory card'
 Check ($null -eq (Find-DesktopElement $window 'about-selected-core' -ById)) 'About omits selected core card'
 Send-DesktopKeys $window '{HOME}'
 Send-DesktopKeys $window '{ENTER}'
 Check ($null -ne (Wait-DesktopElement $window 'subscription-status' -ById)) 'Home Enter opens Subscriptions'
 Send-DesktopKeys $window '^,'
 Check ($null -ne (Wait-DesktopElement $window '浅色主题')) 'Ctrl+, opens Appearance'
 Invoke-DesktopElement $window '展开工具与扩展'
 $pages=@('订阅','代理策略','连接','统计','覆写','结构化配置','预览','日志','诊断','脚本','MITM','核心','网络请求','外观','备份','迁移与更新','关于')
 $ix=10
 foreach ($page in $pages) {
  Invoke-DesktopElement $window $page
  $process.Refresh()
  Check (-not $process.HasExited -and $process.Responding) "navigation $page remains responsive"
  $content=Wait-DesktopElement $window "desktop-content" -ById
  Check ($content.Current.Name -eq $page) "navigation $page opens matching content"
  Shot ("{0:D2}-page" -f $ix);$ix++
 }
 } else {
  Invoke-DesktopElement $window '展开设置'
  Invoke-DesktopElement $window '展开工具与扩展'
 }
 Invoke-DesktopElement $window '结构化配置'
 foreach ($tab in @('网络','DNS','出站','路由与缓存','规则')) {
  Invoke-DesktopElement $window $tab
  Check ($process.Responding) "structured section $tab responds"
  Shot ("structured-"+[array]::IndexOf(@('网络','DNS','出站','路由与缓存','规则'),$tab))
 }
 Invoke-DesktopElement $window '网络'
 Set-NetworkPort '17900'
 Invoke-DesktopElement $window 'DNS'
 Invoke-DesktopElement $window '网络'
 $portEditor=Wait-DesktopElement $window '混合代理端口'
 $value=$portEditor.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value
 Check ($value -eq '17900') 'network draft survives section switch'
 Send-DesktopKeys $window '^s'
 Wait-Setting 'mixed_port = 17900'
 $checks.Add('Ctrl+S saves active network section')
 Set-NetworkPort '17899'
 Send-DesktopKeys $window '^s'
 Wait-Setting 'mixed_port = 17899'
 Invoke-DesktopElement $window '规则'
 foreach ($tab in @('规则集','市场','路由规则')) {
  Invoke-DesktopElement $window $tab
  Check ($process.Responding) "rule section $tab responds"
 }
 Invoke-DesktopElement $window '核心'
 Invoke-DesktopElement $window 'core-type-mihomo' -ById
 Check ($null -ne (Find-DesktopElement $window '收起sing-box 配置')) 'browsing Mihomo inventory preserves sing-box menu'
 Invoke-DesktopElement $window 'core-type-sing-box' -ById
 if ($MihomoFixture -and $SingBoxFixture) {
  foreach ($fixture in @(@($MihomoFixture,'mihomo'),@($SingBoxFixture,'sing-box'))) {
   $source=$fixture[0];$kind=$fixture[1]
   $version=Split-Path (Split-Path $source) -Leaf
   $destination=Join-Path $env:LANPATH_DESKTOP_DATA_DIR "cores/$kind/$version"
   New-Item -ItemType Directory -Force $destination | Out-Null
   Copy-Item $source (Join-Path $destination "$kind.exe")
  }
  Invoke-DesktopElement $window 'core-type-mihomo' -ById
  Invoke-DesktopElement $window '预发布版'
  Invoke-DesktopElement $window '使用'
  Wait-Setting 'type = "mihomo"'
  Wait-DesktopElement $window '收起Mihomo 配置' | Out-Null
  Check ($null -eq (Find-DesktopElement $window '结构化配置')) 'actual Mihomo selection hides sing-box editor'
  Check ($null -eq (Find-DesktopElement $window '覆写')) 'Mihomo menu contains preview only'
  Invoke-DesktopElement $window '预览'
  Check ($null -ne (Wait-DesktopElement $window 'mihomo-configuration-placeholder' -ById)) 'Mihomo preview is an explicit placeholder'
  Shot 'mihomo-preview'
  Send-DesktopKeys $window '{F6}'
  Send-DesktopKeys $window '{HOME}'
  Send-DesktopKeys $window '{DOWN 4}'
  Send-DesktopKeys $window '{ENTER}'
  Check ($null -ne (Wait-DesktopElement $window 'mihomo-configuration-placeholder' -ById)) 'keyboard skips hidden sing-box destinations'
  Invoke-DesktopElement $window '核心'
  Invoke-DesktopElement $window 'core-type-sing-box' -ById
  Check ($null -ne (Find-DesktopElement $window '收起Mihomo 配置')) 'browsing sing-box inventory preserves Mihomo menu'
  Invoke-DesktopElement $window '使用'
  Wait-Setting 'type = "sing-box"'
  Wait-DesktopElement $window '结构化配置' | Out-Null
  Invoke-DesktopElement $window '结构化配置'
  Check ($null -ne (Wait-DesktopElement $window '路由规则')) 'switching back retains structured section selection'
 }
 if (-not $ConfigurationOnly) {
 $portFile=Join-Path $run 'fixture-port.txt'
 $server=Start-Job -ArgumentList $portFile -ScriptBlock {
  param($PortFile)
  $listener=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,0);$listener.Start()
  [IO.File]::WriteAllText($PortFile,([Net.IPEndPoint]$listener.LocalEndpoint).Port.ToString())
  $yaml='proxies:\n  - name: UI Review Node\n    type: ss\n    server: 192.0.2.1\n    port: 8388\n    cipher: aes-128-gcm\n    password: test-only\n'
  $bytes=[Text.Encoding]::UTF8.GetBytes($yaml.Replace('\n',[string][char]10))
  try {
   while ($true) {
    $client=$listener.AcceptTcpClient()
    try {
     $stream=$client.GetStream()
     $reader=[IO.StreamReader]::new($stream,[Text.Encoding]::ASCII,$false,1024,$true)
     do { $line=$reader.ReadLine() } while ($line)
     $crlf=[string][char]13+[char]10
     $header=[Text.Encoding]::ASCII.GetBytes([string]::Join($crlf,@('HTTP/1.1 200 OK','Content-Type: text/yaml',"Content-Length: $($bytes.Length)",'Connection: close','','')))
     $stream.Write($header);$stream.Write($bytes);$stream.Flush()
    } finally { $client.Dispose() }
   }
  } finally { $listener.Stop() }
 }
 for ($i=0;$i -lt 50 -and -not (Test-Path $portFile);$i++) { Start-Sleep -Milliseconds 100 }
 $port=Get-Content $portFile
 Invoke-DesktopElement $window '订阅'
 Invoke-DesktopElement $window '添加订阅…'
 Set-DesktopEditValue $window 'subscription-name' 'UI review fixture'
 Set-DesktopEditValue $window 'subscription-url' "http://127.0.0.1:$port/fixture.yaml"
 Invoke-DesktopElement $window '添加并刷新'
 Wait-DesktopElement $window '查看订阅 UI review fixture' -TimeoutSeconds 20 | Out-Null
 Invoke-DesktopElement $window '查看订阅 UI review fixture'
 Shot '30-subscription-card'
 Invoke-DesktopElement $window '订阅 UI review fixture 的更多操作'
 Shot '31-subscription-menu'
 Invoke-DesktopElement $window '停用'
 Start-Sleep -Milliseconds 300
 $saved=Get-Content (Join-Path $env:LANPATH_DESKTOP_DATA_DIR 'subscriptions.json') -Raw | ConvertFrom-Json
 Check (-not $saved[0].enabled) 'More menu disables only selected subscription'
 Invoke-DesktopElement $window '订阅 UI review fixture 的更多操作'
 Invoke-DesktopElement $window '启用'
 Start-Sleep -Milliseconds 300
 $saved=Get-Content (Join-Path $env:LANPATH_DESKTOP_DATA_DIR 'subscriptions.json') -Raw | ConvertFrom-Json
 Check ($saved[0].enabled) 'More menu re-enables selected subscription'
 Invoke-DesktopElement $window '订阅 UI review fixture 的更多操作'
 Invoke-DesktopElement $window '删除订阅…'
 Send-DesktopKeys $window '{ESC}'
 $saved=Get-Content (Join-Path $env:LANPATH_DESKTOP_DATA_DIR 'subscriptions.json') -Raw | ConvertFrom-Json
 Check ($saved[0].name -eq 'UI review fixture') 'Escape cancels delete dialog'
 }
 }
 Invoke-DesktopElement $window '外观'
 Invoke-DesktopElement $window '界面缩放百分比'
 Send-DesktopKeys $window '{DOWN 3}'
 Send-DesktopKeys $window '{ENTER}'
 Wait-Setting 'scale_percent = 150'
 Set-DesktopWindowSize $window 960 640
 Shot '40-appearance-minimum-150'
 if (Find-DesktopElement $window '展开工具与扩展') { Invoke-DesktopElement $window '展开工具与扩展' }
 foreach ($page in @('连接','统计','结构化配置','诊断')) {
  Invoke-DesktopElement $window $page
  if ($ConfigurationOnly -and $page -eq '结构化配置') { Invoke-DesktopElement $window '规则' }
  Check ($process.Responding) "minimum window 150% $page responds"
  Shot ("41-minimum-150-"+[array]::IndexOf(@('连接','统计','结构化配置','诊断'),$page))
 }
 Invoke-DesktopElement $window '外观'
 Invoke-DesktopElement $window '恢复 100%'
 Wait-Setting 'scale_percent = 100'
 Set-DesktopWindowSize $window 1180 800
 Shot '49-appearance-reset'
 $closed=$process.CloseMainWindow()
 Check ($closed -and $process.WaitForExit(15000)) 'graceful close completes'
 Check ($process.ExitCode -eq 0) 'normal exit code'
 $checks | ConvertTo-Json | Set-Content (Join-Path $run 'checks.json')
 Write-Output "ARTIFACTS=$run"
} catch {
 Write-Output $_.Exception.ToString()
 $process.Refresh()
 Write-Output "Child exited=$($process.HasExited) code=$($process.ExitCode)"
 if ($window -and -not $process.HasExited) { Shot 'failure' }
 Write-Output "ARTIFACTS=$run"
 throw
} finally {
 if ($server) { $server | Stop-Job; $server | Remove-Job -Force }
 if ($process -and -not $process.HasExited) { $process.CloseMainWindow() | Out-Null; $process.WaitForExit(10000) | Out-Null }
 $env:LANPATH_DESKTOP_DATA_DIR=$oldData
 $env:LANPATH_DESKTOP_CORE_BINARY=$oldCore
 $env:RUST_BACKTRACE=$oldBacktrace
}
