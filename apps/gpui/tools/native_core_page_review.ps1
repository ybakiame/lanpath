#Requires -Version 7.0
# Focused layout review in an owned temporary root. Never starts a core or
# mutates system networking; the supplied binary is copied/probed for metadata.
[CmdletBinding()]
param(
 [Parameter(Mandatory)][string]$Binary,
 [Parameter(Mandatory)][string]$WindowTools,
 [Parameter(Mandatory)][string]$SingBoxFixture,
 [Parameter(Mandatory)][string]$OutputDirectory
)
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
Import-Module $WindowTools -Force
$run=Join-Path $OutputDirectory ([guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $run -Force | Out-Null
$checks=[Collections.Generic.List[string]]::new()
$oldData=$env:LANPATH_DESKTOP_DATA_DIR;$oldCore=$env:LANPATH_DESKTOP_CORE_BINARY
$process=$null;$window=[IntPtr]::Zero
function Check([bool]$Condition,[string]$Message){
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
 if(Test-Path -LiteralPath (Join-Path $env:LOCALAPPDATA 'LanpathSystemProxyOwnership/recovery.dpapi')){
  # Closing an idle product normally can explicitly recover an abandoned global
  # journal. This read-only UI review must leave that decision to its real owner.
  $process.Kill();$process.WaitForExit()
  Write-Output 'CLEANUP=owned-unstarted-gui-forced; existing global recovery journal preserved'
  return
 }
 $process.CloseMainWindow() | Out-Null
 if(-not $process.WaitForExit(10000)){
  # Only this unstarted GUI may be terminated if foreign recovery evidence
  # blocks normal closing; never kill the installed app or another guardian.
  $process.Kill();$process.WaitForExit()
  Write-Output 'CLEANUP=owned-unstarted-gui-forced'
 }else{Check ($process.ExitCode -eq 0) 'owned GUI exits normally'}
}
function Invoke-Bar([string]$Id){
 (Wait-DesktopElement $window $Id -ById).GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
 Start-Sleep -Milliseconds 200
}
function Shot([string]$Name){
 Save-DesktopWindowScreenshot $window (Join-Path $run "$Name.png")
 Get-DesktopElements $window | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $run "$Name.json")
}
function Check-Layout([string]$Name,[int]$Zoom){
 Shot $Name
 $header=(Wait-DesktopElement $window 'core-management-header' -ById).Current.BoundingRectangle
 $card=(Wait-DesktopElement $window 'core-selected-summary' -ById).Current.BoundingRectangle
 $rem=16*($Zoom/100)*([DesktopReview.Native]::GetDpiForWindow($window)/96.0)
 Check ($header.Height -le 2.5*$rem) "$Name header remains one compact row"
 Check ($card.Top -ge $header.Bottom -and $card.Top-$header.Bottom -le 1.5*$rem) "$Name selected core follows header without empty bands"
 Check ($card.Height -le 6*$rem) "$Name build metadata does not expand summary"
 Check ($null -eq (Find-DesktopElement $window 'core-management-status' -ById)) "$Name idle instructional status row removed"
 Check ($null -eq (Find-DesktopElement $window 'core-startup-summary' -ById)) "$Name startup history removed from core page"
 Check ($null -eq (Find-DesktopElement $window 'core-build-information' -ById)) "$Name build details hidden until requested"
 foreach($id in @('core-type-sing-box','core-type-mihomo','core-refresh')){
  $bounds=(Wait-DesktopElement $window $id -ById).Current.BoundingRectangle
  Check ($bounds.Top -ge $header.Top -and $bounds.Bottom -le $header.Bottom) "$Name $id stays in header"
 }
}
try {
 $env:LANPATH_DESKTOP_DATA_DIR=Join-Path $run 'userdata'
 Remove-Item Env:LANPATH_DESKTOP_CORE_BINARY -ErrorAction SilentlyContinue
 Start-Owned;Stop-Owned
 $versionOutput=@(& $SingBoxFixture version)
 $versionText=$versionOutput[0]
 if($LASTEXITCODE -ne 0 -or $versionText -notmatch '^sing-box version (\S+)'){throw 'Supplied fixture did not report a sing-box version'}
 $version=$Matches[1]
 $directory=Join-Path $env:LANPATH_DESKTOP_DATA_DIR "cores/sing-box/$version"
 New-Item -ItemType Directory -Path $directory -Force | Out-Null
 Copy-Item -LiteralPath $SingBoxFixture -Destination (Join-Path $directory 'sing-box.exe')
 $app=Join-Path $env:LANPATH_DESKTOP_DATA_DIR 'app.toml'
 $text=Get-Content $app -Raw
 if($text -match '(?m)^\[core\]'){throw 'Fresh test root unexpectedly has a selected core'}
 $text+="`n[core]`ntype = 'sing-box'`nchannel = 'stable'`n[core.current]`nversion = '$version'`npath = 'cores/sing-box/$version/sing-box.exe'`n"
 [IO.File]::WriteAllText($app,$text,[Text.UTF8Encoding]::new($false))
 Start-Owned
 Set-DesktopWindowSize $window 1180 800
 Invoke-Bar 'status-core'
 Check ((Wait-DesktopElement $window 'core-selected-summary' -ById).Current.Name.Contains($version)) 'retained selected version is displayed'
 Check-Layout 'default-core' 100
 Invoke-DesktopElement $window 'core-build-details' -ById
 $detail=(Wait-DesktopElement $window 'core-build-information' -ById).Current.Name
 Check ($detail.Contains('Environment:') -and $detail.Contains('Tags:') -and $detail.Contains($version)) 'build details retain version environment tags and executable path'
 Shot 'build-details'
 Send-DesktopKeys $window '{ESC}'
 Check ($null -eq (Find-DesktopElement $window 'core-build-information' -ById)) 'Escape dismisses build details'
 Set-DesktopWindowSize $window 960 640
 Check-Layout 'minimum-core' 100
 Invoke-DesktopElement $window 'core-refresh' -ById
 Check ($null -eq (Find-DesktopElement $window 'core-management-status' -ById)) 'refresh does not recreate idle instructional band'
 Stop-Owned
 $text=Get-Content $app -Raw
 if($text -match '(?m)^\[appearance\]'){
  $text=$text -replace 'scale_percent = 100','scale_percent = 150'
 }else{
  $text+="`n[appearance]`ntheme = 'system'`naccent = 'blue'`npure_black = false`nscale_percent = 150`n"
 }
 Check ($text -match 'scale_percent = 150') 'owned appearance explicitly seeds 150 percent zoom'
 [IO.File]::WriteAllText($app,$text,[Text.UTF8Encoding]::new($false))
 Start-Owned
 Set-DesktopWindowSize $window 960 640
 Invoke-Bar 'status-core'
 Check-Layout 'minimum-core-150' 150
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
