@echo OFF
setlocal EnableDelayedExpansion

set "SILENT=0"
if /I "%~1"=="/silent" set "SILENT=1"
if /I "%~1"=="--silent" set "SILENT=1"

:: ==================== ADMIN ELEVATION =====================
>nul 2>&1 "%SYSTEMROOT%\system32\cacls.exe" "%SYSTEMROOT%\system32\config\system"
if '%errorlevel%' NEQ '0' (
    if "%SILENT%"=="1" (
        echo [ERROR] Administrative privileges required.
        exit /B 1
    )
    echo Requesting administrative privileges...
    goto UACPrompt
) else ( goto gotAdmin )

:UACPrompt
echo Set UAC = CreateObject^("Shell.Application"^) > "%temp%\getadmin.vbs"
set params = %*:"=""
echo UAC.ShellExecute "cmd.exe", "/c %~s0 %params%", "", "runas", 1 >> "%temp%\getadmin.vbs"
"%temp%\getadmin.vbs"
del "%temp%\getadmin.vbs"
exit /B

:gotAdmin
pushd "%CD%"
CD /D "%~dp0"

echo.
echo  B.A.S.H — BigAussie Script House
echo  ================================
echo  Discord: https://discord.gg/qsmKs5uKfR
echo.

set "SimbaPath=%LocalAppData%\Simba"
set "DesktopPath=%USERPROFILE%\Desktop"
set "SCRIPT_DIR=%~dp0"

if exist "%SimbaPath%" (
    echo An old Simba directory exists, it will be deleted.
    rmdir /S /Q "%SimbaPath%"
)

echo Creating directory tree...
md "%SimbaPath%" >nul 2>&1
md "%SimbaPath%\Data" >nul 2>&1
md "%SimbaPath%\Data\packages" >nul 2>&1
md "%SimbaPath%\Includes" >nul 2>&1
md "%SimbaPath%\Scripts" >nul 2>&1
md "%SimbaPath%\Fonts" >nul 2>&1

echo Downloading Simba 64-bit...
curl.exe -sL -o "%SimbaPath%\Simba64.exe" "https://github.com/Torwent/Simba/releases/latest/download/Simba-Win64.exe"
if not exist "%SimbaPath%\Simba64.exe" (
    echo [ERROR] Failed to download Simba64.exe
    if "%SILENT%"=="0" pause
    exit /B 1
)

echo Writing default.simba and packages.ini...
if exist "%SCRIPT_DIR%default.simba" (
    copy /Y "%SCRIPT_DIR%default.simba" "%SimbaPath%\Data\default.simba" >nul
) else (
    >"%SimbaPath%\Data\default.simba" (
        echo ^(* Thank you for choosing B.A.S.H — BigAussie Script House *^)
        echo ^(* Discord: https://discord.gg/qsmKs5uKfR *^)
        echo.
        echo ^(* To start simply double click the green play button. *^)
        echo.
        echo begin
        echo   SimbaRunInTab^(ScriptPath + 'bash-launcher.simba'^);
        echo end.
    )
)

(
    echo [BigWaspBackup/SRL-B]
    echo Name=SRL-T
    echo Templates=%SimbaPath%\Includes\SRL-T\templates
    echo.
    echo [BigWaspBackup/BashLib]
    echo Name=WaspLib
    echo Templates=%SimbaPath%\Includes\WaspLib\templates
) > "%SimbaPath%\Data\packages.ini"
copy /Y "%SimbaPath%\Data\packages.ini" "%SimbaPath%\Data\packages\packages.ini" >nul

echo Installing SRL-B as SRL-T...
curl.exe -sL -o "%TEMP%\srl-b.zip" "https://github.com/BigWaspBackup/SRL-B/archive/refs/heads/master.zip"
tar -xf "%TEMP%\srl-b.zip" -C "%TEMP%"
del "%TEMP%\srl-b.zip" >nul 2>&1
for /d %%D in ("%TEMP%\SRL-B-*") do (
    move /Y "%%~fD" "%SimbaPath%\Includes\SRL-T" >nul
)

echo Installing BashLib as WaspLib...
curl.exe -sL -o "%TEMP%\bashlib.zip" "https://github.com/BigWaspBackup/BashLib/archive/refs/heads/master.zip"
tar -xf "%TEMP%\bashlib.zip" -C "%TEMP%"
del "%TEMP%\bashlib.zip" >nul 2>&1
for /d %%D in ("%TEMP%\BashLib-*") do (
    move /Y "%%~fD" "%SimbaPath%\Includes\WaspLib" >nul
)

echo Installing B.A.S.H Launcher as bash-launcher.simba...
curl.exe -sL -o "%SimbaPath%\Scripts\bash-launcher.simba" "https://raw.githubusercontent.com/BigAussie/BASH/main/B.A.S.H%%20Launcher.simba"
if not exist "%SimbaPath%\Scripts\bash-launcher.simba" (
    echo [ERROR] Failed to download B.A.S.H Launcher
    if "%SILENT%"=="0" pause
    exit /B 1
)

echo Creating desktop shortcuts...
powershell -NoProfile -Command ^
  "$s=(New-Object -COM WScript.Shell).CreateShortcut('%DesktopPath%\Simba64.lnk'); $s.TargetPath='%SimbaPath%\Simba64.exe'; $s.Description='Simba 64 bits'; $s.Save()"
powershell -NoProfile -Command ^
  "$s=(New-Object -COM WScript.Shell).CreateShortcut('%DesktopPath%\SimbaFolder.lnk'); $s.TargetPath='%SimbaPath%'; $s.Description='Simba Folder'; $s.Save()"

echo.
echo Installation complete.
echo Launcher path: %SimbaPath%\Scripts\bash-launcher.simba
echo Discord: https://discord.gg/qsmKs5uKfR
echo.
if "%SILENT%"=="0" pause
endlocal
exit /B 0
