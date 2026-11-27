@echo off
rem Starts Vire with a diagnostic log (Data\debug.log (next to Vire.exe)). Use it while a problem happens, then
rem tell the developer roughly WHEN (the log has timestamps). If Vire ever froze, also send
rem Data\hang.log (it is written by itself and names the step that got stuck).
taskkill /f /im Vire.exe >nul 2>&1
set VIRE_DEBUG=1
start "" "%~dp0Vire.exe" --tray
echo Vire is running with the diagnostic log. Close this window any time.
pause