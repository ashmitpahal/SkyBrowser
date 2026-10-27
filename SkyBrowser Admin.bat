@echo off
REM SkyBrowser elevated launcher: Windows only lets an elevated process
REM click protected windows like the on-screen keyboard. One UAC prompt
REM per launch; everything else behaves exactly the same.
powershell -NoProfile -Command "Start-Process '%~dp0SkyBrowser.exe' -Verb RunAs -WorkingDirectory '%~dp0'"
