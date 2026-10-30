; Per-user app installer. Runtime is downloaded from its original vendor, not bundled.
#ifndef AppVersion
  #define AppVersion "0.0.1-alpha.1"
#endif
#define RuntimeVersion "1.28.8"
#define RuntimeInstaller "gstreamer-1.0-msvc-x86_64-" + RuntimeVersion + ".exe"
#define RuntimeSHA256 "e4c41ee6ee9c6c32a45d198463ec08d79bac3bd5fa58adc93d13fe4a08abc9d9"
[Setup]
AppId={{11586C82-2720-4F34-992A-E2E907946D3F}
AppName=Trinix
AppVersion={#AppVersion}
AppPublisher=Trinix contributors
AppPublisherURL=https://github.com/reduckyLang/Trinix
DefaultDirName={localappdata}\Programs\Trinix
DefaultGroupName=Trinix
DisableProgramGroupPage=yes
UsePreviousAppDir=no
UsePreviousGroup=no
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir=..\..\output\installers
OutputBaseFilename=Trinix-{#AppVersion}-windows-x64-setup
SetupIconFile=trinix.ico
UninstallDisplayIcon={app}\trinix.ico
LicenseFile=..\..\LICENSE
InfoBeforeFile=installer-info.txt
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
[InstallDelete]
; Remove files left by earlier packages.
Type: filesandordirs; Name: "{localappdata}\Programs\MightyMonitor"
Type: filesandordirs; Name: "{userprograms}\MightyMonitor"
Type: files; Name: "{userdesktop}\MightyMonitor.lnk"
Type: filesandordirs; Name: "{localappdata}\MightyMonitor"
[Files]
Source: "..\..\target\release\trinix-viewer.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\target\release\trinix-host.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\target\release\trinix-launcher.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "trinix.ico"; DestDir: "{app}"
Source: "..\..\LICENSE"; DestDir: "{app}"
Source: "..\..\NOTICE"; DestDir: "{app}"
[Icons]
Name: "{group}\Trinix"; Filename: "{app}\trinix-launcher.exe"; IconFilename: "{app}\trinix.ico"
Name: "{autodesktop}\Trinix"; Filename: "{app}\trinix-launcher.exe"; IconFilename: "{app}\trinix.ico"; Tasks: desktopicon
[Tasks]
Name: desktopicon; Description: "Create a desktop shortcut"; Flags: unchecked
[Run]
Filename: "{app}\trinix-launcher.exe"; Description: "Open Trinix"; Flags: nowait postinstall skipifsilent
[Code]
function RuntimeRoot: String;
begin
  Result := ExpandConstant('{localappdata}\Trinix\gstreamer-{#RuntimeVersion}');
end;
function RuntimePresent: Boolean;
begin
  Result := FileExists(RuntimeRoot + '\bin\gstreamer-1.0-0.dll') and
    FileExists(RuntimeRoot + '\lib\gstreamer-1.0\gstlibav.dll') and
    FileExists(RuntimeRoot + '\lib\gstreamer-1.0\gstx264.dll');
end;
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ResultCode: Integer;
begin
  Result := '';
  if RuntimePresent then exit;
  try
    WizardForm.StatusLabel.Caption := 'Downloading the video runtime...';
    DownloadTemporaryFile('https://gstreamer.freedesktop.org/data/pkg/windows/{#RuntimeVersion}/msvc/{#RuntimeInstaller}', '{#RuntimeInstaller}', '{#RuntimeSHA256}', nil);
    WizardForm.StatusLabel.Caption := 'Installing the video runtime...';
    if not Exec(ExpandConstant('{tmp}\{#RuntimeInstaller}'), '/VERYSILENT /NORESTART /CURRENTUSER /TYPE=devel /DIR="' + RuntimeRoot + '"', '', SW_HIDE, ewWaitUntilTerminated, ResultCode) then
      RaiseException('Could not start the video runtime installer.');
    if ResultCode <> 0 then RaiseException('Video runtime installation failed: ' + IntToStr(ResultCode));
    if not RuntimePresent then RaiseException('Required video runtime components were not installed.');
  except
    Result := 'Trinix could not install its video runtime. Check your internet connection and run Setup again.' + #13#10 + GetExceptionMessage;
  end;
end;
