#Requires -Version 7.0
# Focused shell review. Owns only a temporary data root and its GUI; never starts
# a core, applies a proxy, modifies CA trust, or recovers another session.
[CmdletBinding()]
param(
 [Parameter(Mandatory)][string]$Binary,
 [Parameter(Mandatory)][string]$WindowTools,
 [Parameter(Mandatory)][string]$OutputDirectory
)
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
Import-Module $WindowTools -Force
if(-not ('StatusBarReview.Native' -as [type])){
 Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace StatusBarReview {
 public static class Native {
  [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct Point { public int X,Y; }
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr window,out Rect rect);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr window,ref Point point);
 }
}
'@
}
$run=Join-Path $OutputDirectory ([guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $run -Force | Out-Null
$checks=[Collections.Generic.List[string]]::new()
$oldData=$env:LANPATH_DESKTOP_DATA_DIR
$oldCore=$env:LANPATH_DESKTOP_CORE_BINARY
$process=$null; $window=[IntPtr]::Zero
function Check([bool]$Condition,[string]$Message) {
 if(-not $Condition){throw "Failed: $Message"}
 $checks.Add($Message);Write-Output "PASS $Message"
}
function Start-Owned {
 $script:process=Start-Process -FilePath $Binary -WorkingDirectory (Split-Path $Binary) -RedirectStandardError (Join-Path $run 'stderr.log') -PassThru
 $deadline=[DateTime]::UtcNow.AddSeconds(30)
 do {
  Start-Sleep -Milliseconds 100;$process.Refresh();$script:window=$process.MainWindowHandle
  if($process.HasExited){throw "Owned GUI exited: $($process.ExitCode)"}
  if([DateTime]::UtcNow -gt $deadline){throw 'No owned window'}
 }while($window -eq [IntPtr]::Zero)
 Wait-DesktopElement $window 'desktop-status-bar' -ById -TimeoutSeconds 30 | Out-Null
}
function Stop-Owned {
 $process.CloseMainWindow() | Out-Null
 if(-not $process.WaitForExit(10000)){
  # A foreign global recovery journal can block even this unconfigured GUI.
  # No core/guardian was started here; terminate only this exact owned GUI.
  $process.Kill();$process.WaitForExit()
  Write-Output 'CLEANUP=owned-unconfigured-gui-forced; foreign recovery evidence untouched'
 }else{Check ($process.ExitCode -eq 0) 'owned GUI closes normally'}
}
function Invoke-Bar([string]$Id) {
 $entry=Wait-DesktopElement $window $Id -ById
 Check ($entry.Current.IsEnabled -and -not $entry.Current.IsOffscreen) "$Id is available"
 $entry.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
 Start-Sleep -Milliseconds 200
}
function Shot([string]$Name) {
 Save-DesktopWindowScreenshot $window (Join-Path $run "$Name.png")
 Get-DesktopElements $window | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $run "$Name.json")
}
function Check-Layout([string]$Name) {
 Shot $Name
 $bar=(Wait-DesktopElement $window 'desktop-status-bar' -ById).Current.BoundingRectangle
 $content=(Wait-DesktopElement $window 'desktop-content' -ById).Current.BoundingRectangle
 $navigation=(Wait-DesktopElement $window 'desktop-navigation-scroll' -ById).Current.BoundingRectangle
 # GetWindowRect includes invisible Windows resize borders. Compare with the
 # actual client boundary, not a guessed physical-pixel offset.
 $rect=[StatusBarReview.Native+Rect]::new()
 $origin=[StatusBarReview.Native+Point]::new()
 if(-not [StatusBarReview.Native]::GetClientRect($window,[ref]$rect) -or -not [StatusBarReview.Native]::ClientToScreen($window,[ref]$origin)){throw 'Client geometry unavailable'}
 Check ([Math]::Abs($bar.Bottom-($origin.Y+$rect.Bottom)) -le 1) "$Name bar sits at window bottom"
 Check ([Math]::Abs($content.Bottom-$bar.Top) -le 1) "$Name content ends at status bar"
 Check ([Math]::Abs($navigation.Bottom-$bar.Top) -le 1) "$Name navigation ends at status bar"
 $previous=$null
 foreach($id in @('desktop-about','status-subscription','status-core','status-node','desktop-appearance','desktop-keyboard-help','core-status')){
  $entry=(Wait-DesktopElement $window $id -ById).Current
  $bounds=$entry.BoundingRectangle
  Check ($bounds.Width -gt 0 -and $bounds.Top -ge $bar.Top -and $bounds.Bottom -le $bar.Bottom -and $bounds.Right -le $bar.Right) "$Name $id stays inside bar"
  if($null -ne $previous){Check ($bounds.Left -ge $previous.Right-1) "$Name $id does not overlap previous item"}
  $previous=$bounds
 }
 $command=Find-DesktopElement $window 'core-start' -ById
 if(-not $command){$command=Wait-DesktopElement $window 'core-stop' -ById}
 $bounds=$command.Current.BoundingRectangle
 Check ($bounds.Left -ge $previous.Right-1 -and $bounds.Bottom -le $bar.Bottom -and $bounds.Right -le $bar.Right) "$Name lifecycle action remains visible at right"
 Shot $Name
}
try {
 $env:LANPATH_DESKTOP_DATA_DIR=Join-Path $run 'userdata'
 Remove-Item Env:LANPATH_DESKTOP_CORE_BINARY -ErrorAction SilentlyContinue
 Start-Owned
 Set-DesktopWindowSize $window 1180 800
 Check ((Wait-DesktopElement $window 'status-subscription' -ById).Current.Name.Contains('未选择订阅')) 'empty active subscription is explicit'
 Check ((Wait-DesktopElement $window 'status-core' -ById).Current.Name.Contains('尚未选择核心')) 'missing selected core is explicit'
 Check-Layout 'default-dark'
 Invoke-Bar 'desktop-about'
 Wait-DesktopElement $window 'about-app-version' -ById | Out-Null
 Check ($true) 'version shortcut opens About'
 Invoke-Bar 'status-subscription'
 Wait-DesktopElement $window 'subscription-status' -ById | Out-Null
 $content=Wait-DesktopElement $window 'desktop-content' -ById
 $content.SetFocus()
 Send-DesktopKeys $window '{F6}'
 Check ([System.Windows.Automation.AutomationElement]::FocusedElement.Current.AutomationId -eq 'desktop-status-bar') 'F6 moves content to status bar'
 Send-DesktopKeys $window '+{F6}'
 Check ([System.Windows.Automation.AutomationElement]::FocusedElement.Current.AutomationId -eq 'desktop-content') 'Shift+F6 reverses to content'
 Send-DesktopKeys $window '{F6}'
 Send-DesktopKeys $window '{F6}'
 Send-DesktopKeys $window '{END}'
 Send-DesktopKeys $window '{ENTER}'
 Wait-DesktopElement $window 'about-app-version' -ById | Out-Null
 Check ($true) 'F6 moves status bar to keyboard-operable navigation'
 Invoke-Bar 'status-subscription'
 (Wait-DesktopElement $window 'desktop-content' -ById).SetFocus()
 Send-DesktopKeys $window '{F6}'
 Send-DesktopKeys $window '{TAB}'
 Check ([System.Windows.Automation.AutomationElement]::FocusedElement.Current.AutomationId -eq 'desktop-about') 'Tab reaches first status command'
 Send-DesktopKeys $window '{ENTER}'
 Wait-DesktopElement $window 'about-app-version' -ById | Out-Null
 Invoke-Bar 'status-node'
 Wait-DesktopElement $window 'proxy-test-selected' -ById | Out-Null
 Check ($true) 'node shortcut opens proxy policy'
 Invoke-Bar 'desktop-keyboard-help'
 Check ((Wait-DesktopElement $window 'desktop-keyboard-shortcuts' -ById).Current.Name.Contains('切换导航、内容与状态栏')) 'keyboard help describes all three regions accessibly'
 Send-DesktopKeys $window '{ESC}'
 Check ($null -eq (Find-DesktopElement $window 'desktop-keyboard-shortcuts' -ById)) 'Escape dismisses keyboard help'
 Invoke-Bar 'core-status'
 Wait-DesktopElement $window 'desktop-session-details' -ById | Out-Null
 Check ($true) 'status opens accessible full details'
 Send-DesktopKeys $window '{ESC}'
 Check ($null -eq (Find-DesktopElement $window 'desktop-session-details' -ById)) 'status details dismiss without changing lifecycle'
 Invoke-Bar 'desktop-appearance'
 Invoke-DesktopElement $window '浅色主题'
 Check-Layout 'default-light'
 Invoke-DesktopElement $window '深色主题'
 Set-DesktopWindowSize $window 960 640
 Check-Layout 'minimum-dark'
 Stop-Owned
 # Seed only the owned root to exercise actual persisted active-source text and
 # large zoom without a download, a real core, or touching the installed app.
 $id=[guid]::NewGuid().ToString()
 $name='香港与日本专线订阅 · Very long active subscription name for truncation'
 $sub="[[subscriptions]]`nid = '$id'`nname = '$name'`nurl = 'https://example.invalid/status-fixture'`nenabled = true`nnode_count = 42`n"
 [IO.File]::WriteAllText((Join-Path $env:LANPATH_DESKTOP_DATA_DIR 'subscriptions.toml'),$sub,[Text.UTF8Encoding]::new($false))
 $app=Join-Path $env:LANPATH_DESKTOP_DATA_DIR 'app.toml'
 $text=(Get-Content $app -Raw).Replace('[client]',"[client]`nactive_subscription_id = '$id'")
 $text=$text -replace 'scale_percent = 100','scale_percent = 150'
 [IO.File]::WriteAllText($app,$text,[Text.UTF8Encoding]::new($false))
 Start-Owned
 Set-DesktopWindowSize $window 960 640
 Check ((Wait-DesktopElement $window 'status-subscription' -ById).Current.Name.Contains($name)) 'long active subscription retains full accessible name'
 Check-Layout 'minimum-150-long-source'
 Stop-Owned
 $checks | ConvertTo-Json | Set-Content (Join-Path $run 'checks.json')
 Write-Output "ARTIFACTS=$run"
}catch{
 if($window -ne [IntPtr]::Zero -and $process -and -not $process.HasExited){Shot 'failure'}
 throw
}finally{
 if($process -and -not $process.HasExited){$process.Kill();$process.WaitForExit()}
 $env:LANPATH_DESKTOP_DATA_DIR=$oldData;$env:LANPATH_DESKTOP_CORE_BINARY=$oldCore
}
