:: Adapted from Baconadors Automated_Force_Update_Tool for B.A.S.H

@echo off
setlocal EnableDelayedExpansion

call :CheckAdmin
if %errorlevel% neq 0 (
    echo.
    echo [ERROR] Required administrative elevation failed or was denied.
    pause
    exit /b 1
)

:: ==================== FEATURE TOGGLES =====================
set "debugRunInstallSimba=true"
set "debugRunInstallRuneLite=true"
set "debugRunBackup=true"
set "debugRunRestore=true"
set "debugRunRemoveSimba=true"
set "debugRunUninstallRuneLite=true"
set "debugRunCleanup=true"

if not exist "%LOCALAPPDATA%\BashForceUpdate" mkdir "%LOCALAPPDATA%\BashForceUpdate"

:: ==================== DEFINE PATHS =====================
call :DefinePaths

:: ==================== SETUP LOGGING =====================
call :InitLogging

:: ==================== ROTATE LOGS & BACKUPS =====================
call :RotateLogs
call :RotateBackups
call :RotateProfileBackups

:: ==================== SETUP REQUIREMENTS =====================
call :Setup7Zip
call :CreateFolders

:: ==================== DEPENDENCIES =====================
call :InstallVC2015

:: ==================== CLEANUP OLD INSTALLATIONS =====================
call :CleanRegistry
call :KillProcesses

:: ==================== CHECK DISPLAY SCALING =====================
call :CheckAndSetDisplayScaling

call :AddDefenderExclusions

call :Log "[INFO] Flushing DNS resolver cache..."
ipconfig /flushdns >> "%logFile%" 2>&1
if %errorlevel% neq 0 (
    call :Log "[FAILED] Could not flush DNS cache."
) else (
    call :Log "[SUCCESS] DNS resolver cache flushed."
)

:: ==================== BACKUP =====================
if /I "%debugRunBackup%"=="true" (
    call :BackupData
    call :CompressBackup
) else (
    call :Log "[INFO] Skipping Backup and Compression (debugRunBackup=false)."
)

:: ==================== REMOVE OLD INSTALLS =====================
if /I "%debugRunRemoveSimba%"=="true" (
    call :RemoveOldSimba
) else (
    call :Log "[INFO] Skipping RemoveOldSimba (debugRunRemoveSimba=false)."
)

if /I "%debugRunUninstallRuneLite%"=="true" (
    call :UninstallRuneLite
) else (
    call :Log "[INFO] Skipping UninstallRuneLite (debugRunUninstallRuneLite=false)."
)

:: ==================== INSTALL NEW VERSIONS =====================
if /I "%debugRunInstallSimba%"=="true" (
    call :InstallSimba
    call :ConfigureSimba
) else (
    call :Log "[INFO] Skipping InstallSimba and ConfigureSimba (debugRunInstallSimba=false)."
)

if /I "%debugRunInstallRuneLite%"=="true" (
    call :InstallRuneLite
) else (
    call :Log "[INFO] Skipping InstallRuneLite (debugRunInstallRuneLite=false)."
)

:: ==================== RESTORE =====================
if /I "%debugRunRestore%"=="true" (
    call :AutoRestore
) else (
    call :Log "[INFO] Skipping AutoRestore (debugRunRestore=false)."
)

:: ==================== SHORTCUTS & CLEANUP =====================
call :CreateShortcuts

if /I "%debugRunCleanup%"=="true" (
    call :FinalCleanup
) else (
    call :Log "[INFO] Skipping FinalCleanup (debugRunCleanup=false)."
)

:: ==================== FINISH =====================
call :Log "[INFO] Backup location: %backupZipPath%"
call :Log "[INFO] Script complete."

for /f "tokens=* usebackq" %%a in (`powershell -NoProfile -Command "Get-Date -Format 'ddd, dd/MM/yyyy @ HH:mm:ss'"`) do set "rundate=%%a"
call :Log "[DONE] Run finished on %rundate%"
echo. >> "%logFile%"

echo.
powershell -NoProfile -Command "Write-Host '=====================================================' -ForegroundColor White"
powershell -NoProfile -Command "Write-Host '           B.A.S.H FORCE UPDATE COMPLETE              ' -ForegroundColor White"
powershell -NoProfile -Command "Write-Host '=====================================================' -ForegroundColor White"
powershell -NoProfile -Command "Write-Host 'Discord: https://discord.gg/qsmKs5uKfR' -ForegroundColor Cyan"
echo.

endlocal
echo Press any key to finish and exit...
pause >nul
exit /b 0

:CheckAdmin
net session >nul 2>&1
if %errorlevel% equ 0 exit /b 0
echo Requesting administrative privileges...
powershell -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
exit /b 1

:DefinePaths
for /f %%I in ('powershell -NoProfile -Command "Get-Date -Format ddMMyyyy_HHmmss" 2^>nul') do set "datetime=%%I"
set "simbaPath=%LOCALAPPDATA%\Simba"
set "runeLitePath=%LOCALAPPDATA%\RuneLite"
set "runeLiteProfilePath=%USERPROFILE%\.runelite"
set "tempBackupPath=%LOCALAPPDATA%\BashBackupTMP"
set "forceUpdatePath=%LOCALAPPDATA%\BashForceUpdate"
set "backupRootPath=%LOCALAPPDATA%\BashBackups"
set "backupSessionPath=%backupRootPath%\Backup_%datetime%"
set "backupZipPath=%backupRootPath%\Simba_RuneLite_Backup_%datetime%.7z"
set "bashSetupExePath=%forceUpdatePath%\bash-setup_%datetime%.exe"
set "runeLiteSetupPath=%forceUpdatePath%\RuneLiteSetup_%datetime%.exe"
set "simba64ExePath=%simbaPath%\Simba64.exe"
set "runeLiteUninstallerPath=%runeLitePath%\unins000.exe"
set "simba64ShortcutPath=%USERPROFILE%\Desktop\Simba64.lnk"
set "portable7zDir=%LOCALAPPDATA%\BashTools"
set "portable7zPath=%portable7zDir%\7zr.exe"
set "logFile=%backupRootPath%\BashUpdate_%datetime%.log"
set "runeLiteProfiles2=%USERPROFILE%\.runelite\profiles2"
set "profilesJson=%runeLiteProfiles2%\profiles.json"
set "credentialsFile=%simbaPath%\credentials.simba"
set "scriptDir=%~dp0"
set "repoRoot=%scriptDir%.."
set "localBashSetupExe=%repoRoot%\rust\target\release\bash-setup.exe"
set "localBashSetupCmd=%repoRoot%\windows\bash-setup.cmd"
set "localBashProfile=%repoRoot%\windows\bash-profile.properties"
set "bashSetupReleaseUrl=https://github.com/BigWaspBackup/bash-setup/releases/latest/download/bash-setup.exe"
set "bashProfileRawUrl=https://raw.githubusercontent.com/BigWaspBackup/bash-setup/main/windows/bash-profile.properties"
exit /b

:InitLogging
if not exist "%backupRootPath%" mkdir "%backupRootPath%"
echo ===================================================== >> "%logFile%"
echo B.A.S.H Simba + RuneLite Update Log - %datetime% >> "%logFile%"
echo ===================================================== >> "%logFile%"
exit /b

:Log
set "msg=%~1"
set "curtime=%time: =0%"
set "curtime=%curtime:~0,8%"
set "color=White"
echo %msg% | find "[INFO]" >nul && set "color=White"
echo %msg% | find "[SUCCESS]" >nul && set "color=Green"
echo %msg% | find "[FAILED]" >nul && set "color=Red"
echo %msg% | find "[ERROR]" >nul && set "color=Red"
echo %msg% | find "[WARN]" >nul && set "color=Yellow"
echo %msg% | find "[STRT]" >nul && set "color=Cyan"
echo %msg% | find "[DONE]" >nul && set "color=Cyan"
powershell -NoProfile -Command "Write-Host '[%curtime%] %msg%' -ForegroundColor %color%"
echo [%curtime%] %msg% >> "%logFile%"
exit /b

:RotateLogs
if not exist "%backupRootPath%" mkdir "%backupRootPath%"
for /f "skip=10 delims=" %%F in ('2^>nul dir "%backupRootPath%\BashUpdate_*.log" /b /o-d') do (
    del "%backupRootPath%\%%F"
    call :Log "[INFO] Deleted old log %%F"
)
exit /b

:RotateBackups
if not exist "%backupRootPath%" mkdir "%backupRootPath%"
for /f "skip=10 delims=" %%F in ('2^>nul dir "%backupRootPath%\Simba_RuneLite_Backup_*.7z" /b /o-d') do (
    del "%backupRootPath%\%%F"
    call :Log "[INFO] Deleted old backup %%F"
)
exit /b

:RotateProfileBackups
if not exist "%runeLiteProfiles2%" mkdir "%runeLiteProfiles2%"
for /f "skip=10 delims=" %%F in ('2^>nul dir "%runeLiteProfiles2%\profiles.json.bak_*" /b /o-d') do (
    del "%runeLiteProfiles2%\%%F"
    call :Log "[INFO] Deleted old profiles.json backup %%F"
)
exit /b

:Setup7Zip
if not exist "%portable7zDir%" mkdir "%portable7zDir%"
if not exist "%portable7zPath%" (
    call :Log "[INFO] 7-Zip not found. Downloading..."
    curl.exe -s -L -o "%portable7zPath%" "https://www.7-zip.org/a/7zr.exe" >> "%logFile%" 2>&1
)
exit /b

:CreateFolders
call :Log "[INFO] Ensuring backup and update folders exist..."
if not exist "%forceUpdatePath%" mkdir "%forceUpdatePath%"
if not exist "%backupSessionPath%" mkdir "%backupSessionPath%"
if not exist "%runeLiteProfiles2%" mkdir "%runeLiteProfiles2%"
exit /b

:InstallVC2015
call :Log "[INFO] Checking for Visual C++ 2015-2022 Redistributable..."
set "vcFound=false"
for /f "tokens=*" %%a in ('powershell -NoProfile -Command "Get-ItemProperty HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*, HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\* -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName -like '*Visual C++ 2015-2022*Redistributable (x64)*' -or $_.DisplayName -like '*Visual C++ 2015*Redistributable (x64)*' } | Select-Object -ExpandProperty DisplayName"') do set "vcFound=true"
if "%vcFound%"=="true" (
    call :Log "[INFO] Visual C++ 2015-2022 already installed."
    exit /b 0
)
call :Log "[WARN] Visual C++ 2015-2022 (x64) not found. Downloading..."
set "vcInstaller=%forceUpdatePath%\vc_redist.x64.exe"
curl.exe -s -L --fail -o "%vcInstaller%" "https://aka.ms/vs/17/release/vc_redist.x64.exe" >> "%logFile%" 2>&1
if not exist "%vcInstaller%" exit /b 1
call :Log "[INFO] Installing VC++ 2015-2022 (x64) silently..."
start /wait "" "%vcInstaller%" /quiet /norestart
call :Log "[SUCCESS] Visual C++ 2015-2022 installed."
if exist "%vcInstaller%" del "%vcInstaller%"
exit /b

:CleanRegistry
call :Log "[INFO] Cleaning up old Simba registry key..."
reg query "HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\Simba" >nul 2>&1 && (
    reg delete "HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\Simba" /f >> "%logFile%" 2>&1
)
exit /b

:KillProcesses
call :Log "[INFO] Killing Simba, RuneLite, Jagex Launcher processes..."
for %%p in (Simba32.exe Simba64.exe RuneLite.exe JagexLauncher.exe bash-setup.exe) do (
    taskkill /f /im %%p >> "%logFile%" 2>&1
)
exit /b

:CheckAndSetDisplayScaling
set "currentDPI="
for /f "tokens=*" %%a in ('powershell -NoProfile -Command ^
    "$path = 'HKCU:\Control Panel\Desktop\PerMonitorSettings';" ^
    "if (Test-Path $path) {" ^
    "  $monitors = Get-ChildItem $path;" ^
    "  if ($monitors.Count -gt 0) {" ^
    "    $dpiValue = (Get-ItemProperty -Path $monitors[0].PSPath -Name DpiValue -ErrorAction SilentlyContinue).DpiValue;" ^
    "    if ($null -ne $dpiValue) {" ^
    "      switch ($dpiValue) { 0 { Write-Output 96 } 1 { Write-Output 120 } 2 { Write-Output 144 } 3 { Write-Output 168 } 4 { Write-Output 192 } default { Write-Output 96 } }" ^
    "    }" ^
    "  }" ^
    "}"') do set "currentDPI=%%a"
if not defined currentDPI set "currentDPI=96"
if "%currentDPI%"=="96" exit /b 0
call :Log "[INFO] Updating display scaling to 100%%..."
reg add "HKCU\Control Panel\Desktop" /v LogPixels /t REG_DWORD /d 96 /f >> "%logFile%" 2>&1
reg add "HKCU\Control Panel\Desktop" /v Win8DpiScaling /t REG_DWORD /d 1 /f >> "%logFile%" 2>&1
reg add "HKCU\Control Panel\Desktop\WindowMetrics" /v AppliedDPI /t REG_DWORD /d 96 /f >> "%logFile%" 2>&1
powershell -NoProfile -Command "$c='[DllImport(\"user32.dll\",CharSet=CharSet.Auto)] public static extern IntPtr SendMessageTimeout(IntPtr h,uint m,IntPtr w,string l,uint f,uint t,out IntPtr r); [DllImport(\"user32.dll\")] public static extern bool SystemParametersInfo(uint a,uint p,IntPtr v,uint i);'; $t=Add-Type -MemberDefinition $c -Name 'NM' -Namespace 'W32' -PassThru; $t::SystemParametersInfo(0x009F,0,[IntPtr]::Zero,0x03); $r=[IntPtr]::Zero; $t::SendMessageTimeout(0xFFFF,0x001A,[IntPtr]::Zero,'WindowMetrics',0x0002,5000,[ref]$r); $t::SendMessageTimeout(0xFFFF,0x001A,[IntPtr]::Zero,'ImmersiveColorSet',0x0002,5000,[ref]$r); $t::SendMessageTimeout(0xFFFF,0x007E,[IntPtr]::Zero,$null,0x0002,5000,[ref]$r)" >> "%logFile%" 2>&1
taskkill /f /im explorer.exe >> "%logFile%" 2>&1
timeout /t 2 /nobreak >nul
start explorer.exe
timeout /t 4 /nobreak >nul
call :Log "[SUCCESS] Display scaling updated to 100%%"
exit /b 0

:AddDefenderExclusions
call :Log "[INFO] Adding Defender exclusions..."
set "failFlag=0"
powershell -Command "Add-MpPreference -ExclusionPath '%simbaPath%'" >> "%logFile%" 2>&1
if %errorlevel% neq 0 set "failFlag=1"
powershell -Command "Add-MpPreference -ExclusionPath '%tempBackupPath%'" >> "%logFile%" 2>&1
if %errorlevel% neq 0 set "failFlag=1"
powershell -Command "Add-MpPreference -ExclusionPath '%forceUpdatePath%'" >> "%logFile%" 2>&1
if %errorlevel% neq 0 set "failFlag=1"
if %failFlag%==0 call :Log "[SUCCESS] Defender exclusions added successfully"
exit /b

:BackupData
call :Log "[INFO] Backing up existing data..."
if exist "%simbaPath%" (
    xcopy /s /e /y "%simbaPath%" "%backupSessionPath%\Simba\" >> "%logFile%" 2>&1
    call :Log "[SUCCESS] Backed up Simba folder"
)
if exist "%runeLiteProfilePath%" (
    xcopy /s /e /y "%runeLiteProfilePath%" "%backupSessionPath%\.runelite\" >> "%logFile%" 2>&1
    call :Log "[SUCCESS] Backed up .runelite folder"
)
exit /b

:CompressBackup
call :Log "[INFO] Compressing backup. Please wait..."
if exist "%portable7zPath%" (
    "%portable7zPath%" a -t7z -mx1 "%backupZipPath%" "%backupSessionPath%\*" >> "%logFile%" 2>&1
    if exist "%backupZipPath%" call :Log "[SUCCESS] Backup created."
)
exit /b

:RemoveOldSimba
call :Log "[INFO] Removing old Simba folder..."
if exist "%simbaPath%" rmdir /s /q "%simbaPath%"
exit /b

:UninstallRuneLite
call :Log "[INFO] Running RuneLite uninstaller if available..."
if exist "%runeLiteUninstallerPath%" (
    start /wait "" "%runeLiteUninstallerPath%" /Silent
    call :Log "[SUCCESS] RuneLite uninstalled."
)
exit /b

:InstallSimba
call :Log "[INFO] Installing Simba via B.A.S.H setup..."

:: 1) Prefer locally built GUI installer
if exist "%localBashSetupExe%" (
    call :Log "[INFO] Using local bash-setup.exe"
    start /wait "" "%localBashSetupExe%" --silent
    if exist "%simba64ExePath%" (
        call :Log "[SUCCESS] Simba installation completed via local bash-setup.exe"
        exit /b 0
    )
    call :Log "[WARN] Local bash-setup.exe did not produce Simba64.exe"
)

:: 2) Prefer sibling .cmd
if exist "%localBashSetupCmd%" (
    call :Log "[INFO] Using local windows\bash-setup.cmd /silent"
    call "%localBashSetupCmd%" /silent
    if exist "%simba64ExePath%" (
        call :Log "[SUCCESS] Simba installation completed via bash-setup.cmd"
        exit /b 0
    )
    call :Log "[WARN] Local bash-setup.cmd did not produce Simba64.exe"
)

:: 3) Download release exe
call :Log "[INFO] Downloading bash-setup.exe from GitHub releases..."
del /q "%forceUpdatePath%\bash-setup_*.exe" >nul 2>&1
curl.exe -s -L --fail -o "%bashSetupExePath%" "%bashSetupReleaseUrl%" >> "%logFile%" 2>&1
if exist "%bashSetupExePath%" (
    start /wait "" "%bashSetupExePath%" --silent
    if exist "%simba64ExePath%" (
        call :Log "[SUCCESS] Simba installation completed via downloaded bash-setup.exe"
        exit /b 0
    )
)

call :Log "[ERROR] Failed to install Simba. Build rust\target\release\bash-setup.exe or publish a release."
exit /b 1

:ConfigureSimba
call :Log "[INFO] Configuring Simba post-install..."
if not exist "%simbaPath%\Data" mkdir "%simbaPath%\Data"
ftype simba.script="%simbaPath%\Simba64.exe" "%%1" >> "%logFile%" 2>&1
assoc .simba=simba.script >> "%logFile%" 2>&1
call :Log "[SUCCESS] Simba file associations applied."
exit /b

:InstallRuneLite
call :Log "[INFO] Downloading RuneLite installer..."
del /q "%forceUpdatePath%\RuneLiteSetup_*.exe" >nul 2>&1
curl.exe -s -L -o "%runeLiteSetupPath%" "https://github.com/runelite/launcher/releases/latest/download/RuneLiteSetup.exe" >> "%logFile%" 2>&1
start /wait "" "%runeLiteSetupPath%" /Silent
call :Log "[SUCCESS] RuneLite installation completed."

set "tempProfileFile=%forceUpdatePath%\bash-profile.properties"
if exist "%localBashProfile%" (
    copy /y "%localBashProfile%" "%tempProfileFile%" >> "%logFile%" 2>&1
) else (
    curl.exe -s -L -o "%tempProfileFile%" "%bashProfileRawUrl%" >> "%logFile%" 2>&1
)

if not exist "%tempProfileFile%" (
    call :Log "[WARN] bash-profile.properties not found; skipping profile injection."
    exit /b 0
)

set "chars=abcdefghijklmnopqrstuvwxyz0123456789"
set "name="
for /l %%i in (1,1,8) do (
    set /a "r=!random! %% 36"
    for %%j in (!r!) do set "name=!name!!chars:~%%j,1!"
)
set "id_sum=0"
for /l %%i in (0,1,7) do (
    set "char=!name:~%%i,1!"
    for /f "delims=" %%k in ('powershell -NoProfile -Command "[int][char]'!char!'"') do set /a "id_sum+=%%k"
)
set "id=!id_sum!"
copy /y "%tempProfileFile%" "%runeLiteProfiles2%\%name%-%id%.properties" >> "%logFile%" 2>&1
call :Log "[SUCCESS] bash-profile.properties copied as %name%-%id%.properties"
start "" /min powershell -WindowStyle Hidden -Command "Start-Process -FilePath '%runeLitePath%\RuneLite.exe' -WindowStyle Hidden; Start-Sleep -Seconds 3; Stop-Process -Name 'RuneLite' -Force"
call :UpdateProfilesJson "%name%" "%id%"
exit /b

:UpdateProfilesJson
set "newProfileName=%~1"
set "newProfileId=%~2"
call :Log "[INFO] Updating profiles.json..."
if exist "%profilesJson%" copy "%profilesJson%" "%profilesJson%.bak_%datetime%" >nul
if not exist "%profilesJson%" echo {"profiles":[]} > "%profilesJson%"
powershell -NoProfile -Command ^
    "$f='%profilesJson%';" ^
    "$j=Get-Content $f -Raw | ConvertFrom-Json;" ^
    "if($null -eq $j.profiles){$j=@{profiles=@()}};" ^
    "$c=@();" ^
    "foreach($p in $j.profiles){" ^
    "  if($p.id -eq %newProfileId% -or $p.name -eq '%newProfileName%'){continue}" ^
    "  $p.active=$false;" ^
    "  $c+=$p" ^
    "};" ^
    "$n=[PSCustomObject]@{id=[long]%newProfileId%; name='%newProfileName%'; sync=$true; active=$true; rev=-1; defaultForRsProfiles=@()};" ^
    "$c+=$n;" ^
    "$j.profiles=$c;" ^
    "($j | ConvertTo-Json -Depth 3 -Compress) -replace '\s+', '' | Set-Content $f -Encoding ASCII"
exit /b

:AutoRestore
call :Log "[INFO] Restoring backed up credentials and configs..."
if exist "%backupSessionPath%\Simba\credentials.simba" copy /y "%backupSessionPath%\Simba\credentials.simba" "%simbaPath%\" >> "%logFile%" 2>&1
if exist "%backupSessionPath%\Simba\Configs" xcopy /s /e /y "%backupSessionPath%\Simba\Configs" "%simbaPath%\Configs\" >> "%logFile%" 2>&1
if exist "%backupSessionPath%\Simba\Includes\WaspLib\overrides.simba" xcopy /y /i "%backupSessionPath%\Simba\Includes\WaspLib\overrides.simba" "%simbaPath%\Includes\WaspLib\" >> "%logFile%" 2>&1
exit /b

:CreateShortcuts
call :Log "[INFO] Creating desktop shortcut..."
powershell "$s=(New-Object -COM WScript.Shell).CreateShortcut('%simba64ShortcutPath%'); $s.TargetPath='%simbaPath%\Simba64.exe'; $s.Save()" >> "%logFile%" 2>&1
if exist "%simbaPath%\Simba32.exe" del "%simbaPath%\Simba32.exe"
exit /b

:FinalCleanup
call :Log "[INFO] Performing Final Cleanup..."
if exist "%tempBackupPath%" rmdir /s /q "%tempBackupPath%"
if exist "%backupSessionPath%" rmdir /s /q "%backupSessionPath%"
for /d %%D in ("%backupRootPath%\*") do rmdir /s /q "%%~fD"
for /f "skip=10 delims=" %%F in ('dir "%backupRootPath%\Simba_RuneLite_Backup_*.7z" /b /o-d') do del "%backupRootPath%\%%F"
for /f "skip=10 delims=" %%F in ('dir "%backupRootPath%\BashUpdate_*.log" /b /o-d') do del "%backupRootPath%\%%F"
for /f "skip=10 delims=" %%F in ('dir "%runeLiteProfiles2%\profiles.json.bak_*" /b /o-d') do del "%runeLiteProfiles2%\%%F"
call :Log "[SUCCESS] Cleanup finished."
exit /b
