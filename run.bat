@echo off
rem Build and run TadTor.
rem   run.bat          debug build (fast to compile, shows a console with logs)
rem   run.bat release  optimized build -> target\release\tadtor.exe
cd /d "%~dp0"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

set "PROFILE=debug"
set "FLAGS="
if /i "%~1"=="release" (
  set "PROFILE=release"
  set "FLAGS=--release"
)

echo Building TadTor (%PROFILE%)... the first build takes 5-15 minutes.
cargo build %FLAGS%
if errorlevel 1 (
  echo.
  echo Build failed.
  pause
  exit /b 1
)
if "%PROFILE%"=="release" (
  start "" "target\release\tadtor.exe"
) else (
  "target\debug\tadtor.exe"
)
