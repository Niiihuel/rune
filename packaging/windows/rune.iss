; RUNE's per-user installer for Windows (docs/UPDATES.md#windows), built
; with Inno Setup 6 by the release workflow:
;
;   ISCC /DVersion=0.1.0 packaging\windows\rune.iss
;
; (/DProgram=<rune.exe> names another build; /DLoader=<WebView2Loader.dll>
; adds the loader a MinGW build needs beside it, which MSVC links in). It
; installs into
; %LOCALAPPDATA%\Programs\RUNE without administrator rights, so RUNE can
; replace itself: its updater runs this installer with /VERYSILENT
; /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS once its window has
; closed, and the installer starts RUNE again. The person's data lives
; elsewhere (%LOCALAPPDATA%\RUNE, %APPDATA%\RUNE): updating and
; uninstalling never touch it.

#ifndef Version
  #error Name the version: ISCC /DVersion=x.y.z rune.iss
#endif
#ifndef Program
  #define Program "..\..\target\x86_64-pc-windows-msvc\release\rune.exe"
#endif

[Setup]
; Never change it: an update or the uninstaller finds the installed RUNE by it.
AppId={{9A354FE5-8128-4442-8261-E11202381361}
AppName=RUNE
AppVersion={#Version}
AppVerName=RUNE {#Version}
UninstallDisplayName=RUNE
UninstallDisplayIcon={app}\rune.exe
; For this person only, never elevated.
PrivilegesRequired=lowest
; The updater recognizes RUNE only here (rune_update::install::Install).
DefaultDirName={localappdata}\Programs\RUNE
DisableDirPage=yes
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Toasts and the rest of rune-platform's Windows services need Windows 10.
MinVersion=10.0
; A RUNE still running is closed, also when silent: after an update it is
; already on its way out (the updater exits as it starts this); by hand,
; the wizard asks first. Forced, in case RUNE does not quit when the
; Restart Manager asks. [Run] starts it again.
CloseApplications=force
RestartApplications=no
SetupIconFile=..\..\assets\icons\rune.ico
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
OutputDir=..\..\target
OutputBaseFilename=rune-x86_64-windows-setup
; The language of Windows when it is one of these, English otherwise.
ShowLanguageDialog=no
; A silent update shows nothing: what it did stays in
; %TEMP%\Setup Log <date> #<n>.txt.
SetupLogging=yes

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "es"; MessagesFile: "compiler:Languages\Spanish.isl"

[InstallDelete]
; The notices of the version before, which may have dropped one.
Type: filesandordirs; Name: "{app}\licenses"

[Files]
; ignoreversion: a release always replaces the program, even one whose
; version information is the same (0.2.0-rc.1 and 0.2.0 are both 0.2.0.0).
Source: "{#Program}"; DestDir: "{app}"; DestName: "rune.exe"; Flags: ignoreversion
#ifdef Loader
Source: "{#Loader}"; DestDir: "{app}"; DestName: "WebView2Loader.dll"; Flags: ignoreversion
#endif
; The notices beside the program (docs/UPDATES.md#avisos-de-terceros).
Source: "..\..\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\assets\licenses\*"; DestDir: "{app}\licenses"; Flags: ignoreversion

[Icons]
; The application ID RUNE's toasts are shown under (rune-platform's
; notifications): Windows wants a Start menu shortcut carrying it.
Name: "{autoprograms}\RUNE"; Filename: "{app}\rune.exe"; AppUserModelID: "RUNE.Desktop"

[Registry]
; What RUNE itself writes as it runs, so that uninstalling leaves nothing
; behind: the toasts' name, and starting with the session.
Root: HKCU; Subkey: "Software\Classes\AppUserModelId\RUNE.Desktop"; ValueType: string; ValueName: "DisplayName"; ValueData: "RUNE"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "RUNE"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"; ValueType: none; ValueName: "RUNE"; Flags: uninsdeletevalue dontcreatekey

[Run]
; No skipifsilent: after an update the installer is what starts RUNE again.
Filename: "{app}\rune.exe"; Description: "{cm:LaunchProgram,RUNE}"; Flags: postinstall nowait
