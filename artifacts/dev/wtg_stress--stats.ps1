# WTG developer/demo stats stress helper.
# Retained for reproducibility and historical validation context.
# This is not the formal probe-fields validation harness.
# Run from a WTG binary/test drop context as originally intended.

$env:NO_COLOR = "1"; Start-Process powershell -WorkingDirectory (Get-Location) -ArgumentList "-NoExit","-EncodedCommand",([Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes('$host.UI.RawUI.WindowTitle="WTG"; & .\wtg.exe --watch --stats --interval 50 2>$null | Tee-Object -FilePath (''stress_'' + (Get-Date -Format yyyyMMdd_HHmmss) + ''.txt'') -Append'))); Start-Sleep 2; ollama run llama3.1 "explain the bible in 500 words"; Start-Sleep 2; Stop-Process -Name wtg -Force