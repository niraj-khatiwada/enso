; The Windows installer for enso, built with Inno Setup 6 by the Release
; workflow: one per architecture, from that architecture's enso.exe.
;   iscc /DVersion=1.0.0 /DArch=x64 /DSource=path\to\enso.exe /DOutput=dist enso.iss
; It installs for the current user (no administrator prompt) unless the user
; chooses everyone, adds enso to the Start menu and to Open With for Markdown
; files, and can be removed from Settings > Apps.

#ifndef Version
  #define Version "0.0.0"
#endif
#ifndef Arch
  #define Arch "x64"
#endif
#ifndef Source
  #define Source "..\..\target\release\enso.exe"
#endif
#ifndef Output
  #define Output "..\..\dist"
#endif

[Setup]
AppId={{6E9B1F43-7C2A-4D5E-9A61-3F0C8B2D7E15}
AppName=enso
AppVersion={#Version}
AppVerName=enso {#Version}
AppPublisher=enso
AppPublisherURL=https://enso.codeforreal.com
AppSupportURL=https://github.com/niraj-khatiwada/enso/issues
AppUpdatesURL=https://github.com/niraj-khatiwada/enso/releases
DefaultDirName={autopf}\enso
DefaultGroupName=enso
DisableProgramGroupPage=yes
DisableWelcomePage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
#if Arch == "arm64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif
ChangesAssociations=yes
SetupIconFile=..\..\assets\enso.ico
UninstallDisplayIcon={app}\enso.exe
UninstallDisplayName=enso
OutputDir={#Output}
OutputBaseFilename=enso-{#Version}-windows-{#Arch}-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#Source}"; DestDir: "{app}"; DestName: "enso.exe"; Flags: ignoreversion
Source: "..\..\assets\font-licenses.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\enso"; Filename: "{app}\enso.exe"
Name: "{autodesktop}\enso"; Filename: "{app}\enso.exe"; Tasks: desktopicon

[Registry]
; "Open with enso" for Markdown files, without taking over their default app.
Root: HKA; Subkey: "Software\Classes\enso.markdown"; ValueType: string; ValueName: ""; ValueData: "Markdown Document"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\enso.markdown\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\enso.exe,0"
Root: HKA; Subkey: "Software\Classes\enso.markdown\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\enso.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.md\OpenWithProgids"; ValueType: string; ValueName: "enso.markdown"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.markdown\OpenWithProgids"; ValueType: string; ValueName: "enso.markdown"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\enso.exe\SupportedTypes"; ValueType: string; ValueName: ".md"; ValueData: ""; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\enso.exe\SupportedTypes"; ValueType: string; ValueName: ".markdown"; ValueData: ""

[Run]
Filename: "{app}\enso.exe"; Description: "{cm:LaunchProgram,enso}"; Flags: nowait postinstall skipifsilent
