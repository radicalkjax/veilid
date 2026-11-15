$ProgressPreference='SilentlyContinue'
$errs = $null
[System.Management.Automation.Language.Parser]::ParseFile("$env:USERPROFILE\code\veilid\dev-setup\setup_windows.ps1", [ref]$null, [ref]$errs) | Out-Null
if ($errs) { Write-Output ("PARSE ERRORS: " + $errs.Count); $errs | ForEach-Object { Write-Output ("  " + $_.Extent.StartLineNumber + ": " + $_.Message) } } else { Write-Output "PARSE OK" }
