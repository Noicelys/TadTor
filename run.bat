@echo off
rem Build and open TadTor.
rem   run.bat        optimized build (no console window), creates TadTor shortcuts on the Desktop and Start Menu
rem   run.bat debug  debug build with a console window showing logs
cd /d "%~dp0"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

if /i "%~1"=="debug" (
  cargo build
  if errorlevel 1 goto failed
  "target\debug\tadtor.exe"
  exit /b
)

echo Building TadTor... the first build takes 5-15 minutes.
cargo build --release
if errorlevel 1 goto failed
"target\release\tadtor.exe" --create-shortcuts
if errorlevel 1 echo Could not create shortcuts. You can still open target\release\tadtor.exe directly.
start "" "target\release\tadtor.exe"
exit /b

:failed
echo.
echo Build failed.
pause
exit /b 1
