; The Windows installer (Inno Setup 6). `cargo xtask package-windows` runs it as
;   ISCC /DAppVersion=<version> /DAppNumericVersion=<X.Y.Z> /DSourceDir=<repo root> /DOutputDir=<folder> method-storyteller.iss
; and it writes Method-Storyteller-windows-x64-setup.exe (a version-free name, so the website
; can link releases/latest/download/<file>).
;
; Installs for the current user by default (no administrator prompt); the first page offers
; all users instead. Uninstall from Settings > Apps.

#ifndef AppVersion
  #error Pass /DAppVersion=<version> /DAppNumericVersion=<X.Y.Z>
#endif
#ifndef SourceDir
  #define SourceDir "..\.."
#endif
#ifndef TargetDir
  #define TargetDir SourceDir + "\target"
#endif
#ifndef OutputDir
  #define OutputDir TargetDir + "\bundle\windows"
#endif

[Setup]
; Never change AppId: Windows uses it to find the installed copy when updating or removing it.
AppId={{5E41DAC2-72E3-40A3-A8DD-32723FA0D7F1}
AppName=Method Storyteller
AppVersion={#AppVersion}
AppVerName=Method Storyteller {#AppVersion}
AppPublisher=Method Production Suite
AppPublisherURL=https://methodstoryteller.com
AppSupportURL=https://github.com/method-production-suite/method-storyteller/issues
AppUpdatesURL=https://github.com/method-production-suite/method-storyteller/releases
AppCopyright=Copyright (c) 2026 Method Production Suite Team and the Method Storyteller contributors
VersionInfoVersion={#AppNumericVersion}
DefaultDirName={autopf}\Method Storyteller
DefaultGroupName=Method Storyteller
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir={#OutputDir}
OutputBaseFilename=Method-Storyteller-windows-x64-setup
SetupIconFile={#SourceDir}\packaging\icons\icon.ico
UninstallDisplayIcon={app}\method-storyteller-app.exe
UninstallDisplayName=Method Storyteller
LicenseFile={#SourceDir}\LICENSE-MIT
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#TargetDir}\release\method-storyteller-app.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE-MIT"; DestDir: "{app}"; DestName: "LICENSE-MIT.txt"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE-APACHE"; DestDir: "{app}"; DestName: "LICENSE-APACHE.txt"; Flags: ignoreversion
Source: "{#SourceDir}\NOTICE"; DestDir: "{app}"; DestName: "NOTICE.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Method Storyteller"; Filename: "{app}\method-storyteller-app.exe"
Name: "{autodesktop}\Method Storyteller"; Filename: "{app}\method-storyteller-app.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\method-storyteller-app.exe"; Description: "{cm:LaunchProgram,Method Storyteller}"; Flags: nowait postinstall skipifsilent
