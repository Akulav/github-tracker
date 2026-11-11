; Akulav Tracker - Inno Setup script.
; One file installs the app the first time and updates it later:
;   * first run: asks for the install folder (the app AND its cache/settings go there)
;   * later runs and in-app updates (/UPDATE): reuse the remembered folder without asking
; Build: iscc /DAppVersion=X.Y.Z /DSourceExe=path\to\AkulavTracker.exe installer\akulav-tracker.iss

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceExe
  #define SourceExe "..\target\release\AkulavTracker.exe"
#endif

#define AppName "Akulav Tracker"
#define AppExe "AkulavTracker.exe"
#define SetupName "AkulavTracker-setup"
; Marks the install folder; the app keeps its data next to it (src/backend/install).
#define Marker ".akulav-tracker-install"

[Setup]
; AppId never changes: installing over an existing copy upgrades it in place, in the same folder.
; The app reads the folder back from this key (src/backend/install/windows.rs).
AppId={{5F0E937D-4C4D-402A-BCAC-7957030F45E9}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=Akulav
AppPublisherURL=https://github.com/Akulav/github-tracker
AppSupportURL=https://github.com/Akulav/github-tracker/issues
AppUpdatesURL=https://github.com/Akulav/github-tracker/releases
AppCopyright=Made by Akulav
VersionInfoVersion={#AppVersion}
VersionInfoCompany=Akulav
VersionInfoDescription={#AppName} Setup
VersionInfoProductName={#AppName}
PrivilegesRequired=lowest
DefaultDirName={localappdata}\Programs\AkulavTracker
; Ask for the folder on the first install only; updates reuse the previous one.
DisableDirPage=auto
UsePreviousAppDir=yes
DirExistsWarning=no
DisableProgramGroupPage=yes
DefaultGroupName={#AppName}
OutputBaseFilename={#SetupName}
OutputDir=..\dist
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
WizardStyle=modern
CloseApplications=force
RestartApplications=no
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
Compression=lzma2/ultra64
SolidCompression=yes
LicenseFile=..\LICENSE

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
english.DeleteData=Also delete the repository cache, settings and saved GitHub sign-in?%n%nChoose No to keep them for a future reinstall.
english.LaunchApp=Launch Akulav Tracker
english.DataNote=Akulav Tracker and its data (repository cache, settings) will be stored in this folder.

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "{#AppExe}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"; Comment: "Your GitHub repositories at a glance - made by Akulav"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"; Comment: "Your GitHub repositories at a glance - made by Akulav"; Tasks: desktopicon

[Run]
; Interactive install: the usual "Launch" checkbox.
Filename: "{app}\{#AppExe}"; Parameters: "--installed"; WorkingDir: "{app}"; Description: "{cm:LaunchApp}"; Flags: nowait postinstall skipifsilent
; Silent in-app update (/UPDATE /FROM=<old version>): start the new version again.
Filename: "{app}\{#AppExe}"; Parameters: "--updated-from ""{param:FROM|}"""; WorkingDir: "{app}"; Flags: nowait; Check: IsUpdate

[UninstallDelete]
Type: files; Name: "{app}\{#Marker}"
Type: files; Name: "{app}\AkulavTracker.old.exe"
Type: files; Name: "{app}\AkulavTracker.new.exe"
; The downloaded update setup (in-app updates keep it in the app's own folder).
Type: filesandordirs; Name: "{app}\update"
; Window size and position remembered by the UI toolkit before 1.1.2 (now window.ron in {app}).
Type: filesandordirs; Name: "{userappdata}\akulav-tracker"

[Code]
function IsUpdate(): Boolean;
var
  I: Integer;
begin
  Result := False;
  for I := 1 to ParamCount do
    if CompareText(ParamStr(I), '/UPDATE') = 0 then
      Result := True;
end;

procedure InitializeWizard();
begin
  WizardForm.SelectDirLabel.Caption := WizardForm.SelectDirLabel.Caption + #13#10 + CustomMessage('DataNote');
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    SaveStringToFile(ExpandConstant('{app}\{#Marker}'), '{#AppVersion}', False);
end;

function HasAppData(Dir: String): Boolean;
begin
  Result := FileExists(Dir + '\settings.json') or FileExists(Dir + '\cache.json')
    or FileExists(Dir + '\token') or DirExists(Dir + '\avatars');
end;

{ Only what the app itself creates, never anything else the user keeps in that folder. }
procedure DeleteAppData(Dir: String);
begin
  DelTree(Dir + '\avatars', True, True, True);
  DelTree(Dir + '\update', True, True, True);
  DeleteFile(Dir + '\cache.json');
  DeleteFile(Dir + '\token');
  DeleteFile(Dir + '\window.ron');
  DeleteFile(Dir + '\settings.json');
  RemoveDir(Dir);
end;

{ The saved sign-in lives in Windows Credential Manager (target "github-token.akulav-tracker"). }
procedure DeleteSavedSignIn();
var
  Code: Integer;
begin
  Exec(ExpandConstant('{sys}\cmdkey.exe'), '/delete:github-token.akulav-tracker', '', SW_HIDE, ewWaitUntilTerminated, Code);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  AppDir: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    AppDir := ExpandConstant('{app}');
    { Never delete data when silent. }
    if (not UninstallSilent) and HasAppData(AppDir) then
      if MsgBox(CustomMessage('DeleteData'), mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
      begin
        DeleteAppData(AppDir);
        DeleteSavedSignIn();
      end;
    RemoveDir(AppDir);
  end;
end;
