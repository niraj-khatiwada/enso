#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef Arch
  #define Arch "x64"
#endif
#ifndef Source
  #define Source "..\..\target\release\enso.exe"
#endif
#ifndef OutputName
  #define OutputName "enso-windows-" + Arch + "-setup"
#endif

[Setup]
AppId={{6F1C2E9A-3B7D-4E2A-9C51-8D4F0A7B2E63}
AppName=enso
AppVersion={#AppVersion}
AppVerName=enso {#AppVersion}
AppPublisher=enso
AppPublisherURL=https://github.com/viai957/enso
AppSupportURL=https://github.com/viai957/enso/issues
AppUpdatesURL=https://github.com/viai957/enso/releases
DefaultDirName={autopf}\enso
DefaultGroupName=enso
DisableProgramGroupPage=yes
DisableDirPage=auto
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
UninstallDisplayIcon={app}\enso.exe
UninstallDisplayName=enso
SetupIconFile=..\..\assets\enso.ico
WizardStyle=modern
Compression=lzma2/ultra64
SolidCompression=yes
OutputDir=..\..\dist
OutputBaseFilename={#OutputName}
ChangesAssociations=yes
CloseApplications=yes
#if Arch == "arm64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "markdown"; Description: "Open Markdown files (.md) with enso"; GroupDescription: "File types:"

[Files]
Source: "{#Source}"; DestDir: "{app}"; DestName: "enso.exe"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion isreadme

[Icons]
Name: "{autoprograms}\enso"; Filename: "{app}\enso.exe"
Name: "{autodesktop}\enso"; Filename: "{app}\enso.exe"; Tasks: desktopicon

[Registry]
Root: HKA; Subkey: "Software\Classes\enso.Markdown"; ValueType: string; ValueName: ""; ValueData: "Markdown document"; Flags: uninsdeletekey; Tasks: markdown
Root: HKA; Subkey: "Software\Classes\enso.Markdown\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\enso.exe,0"; Tasks: markdown
Root: HKA; Subkey: "Software\Classes\enso.Markdown\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\enso.exe"" ""%1"""; Tasks: markdown
Root: HKA; Subkey: "Software\Classes\.md\OpenWithProgids"; ValueType: string; ValueName: "enso.Markdown"; ValueData: ""; Flags: uninsdeletevalue; Tasks: markdown
Root: HKA; Subkey: "Software\Classes\.markdown\OpenWithProgids"; ValueType: string; ValueName: "enso.Markdown"; ValueData: ""; Flags: uninsdeletevalue; Tasks: markdown

[Run]
Filename: "{app}\enso.exe"; Description: "{cm:LaunchProgram,enso}"; Flags: nowait postinstall skipifsilent
