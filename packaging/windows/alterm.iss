#define MyAppName "Alterm"
#ifndef MyAppVersion
  #define MyAppVersion "0.0.0"
#endif
#ifndef MyAppPublisher
  #define MyAppPublisher "Russell Haskell"
#endif
#ifndef MyAppURL
  #define MyAppURL "https://github.com/"
#endif
#ifndef MySourceDir
  #define MySourceDir "."
#endif
#ifndef MyOutputDir
  #define MyOutputDir "."
#endif
#ifndef MyOutputArch
  #define MyOutputArch "x64"
#endif

[Setup]
AppId={{A8D3DE77-C614-4E05-8D44-AF5D9B3C92F8}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
SetupIconFile={#MySourceDir}\alterm.ico
UninstallDisplayIcon={app}\alterm.exe
OutputDir={#MyOutputDir}
OutputBaseFilename=alterm-{#MyAppVersion}-windows-{#MyOutputArch}-setup
Compression=lzma
SolidCompression=yes
WizardStyle=modern

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional icons:"

[Files]
Source: "{#MySourceDir}\alterm.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#MySourceDir}\alterm.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#MySourceDir}\README.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#MySourceDir}\config.toml.example"; DestDir: "{userappdata}\alterm"; DestName: "config.toml"; Flags: onlyifdoesntexist uninsneveruninstall
Source: "{#MySourceDir}\hooks.lua.example"; DestDir: "{userappdata}\alterm"; Flags: onlyifdoesntexist uninsneveruninstall

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\alterm.exe"; IconFilename: "{app}\alterm.ico"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\alterm.exe"; IconFilename: "{app}\alterm.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\alterm.exe"; Description: "Launch {#MyAppName}"; Flags: nowait postinstall skipifsilent
