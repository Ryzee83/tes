; Skrip Inno Setup -> menghasilkan installer/Output/LumaBeat-Setup.exe
; Dijalankan otomatis oleh GitHub Actions (lihat .github/workflows/build.yml).

#define AppName "LumaBeat"
#define AppVersion "0.1.0"
#define AppExe "lumabeat.exe"

[Setup]
AppId={{6B3F1C52-4D0E-4B8A-9C57-2E1A7D9F0A41}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppName}
; Per-user: tanpa permintaan admin, terpasang di %LOCALAPPDATA%\Programs\LumaBeat
PrivilegesRequired=lowest
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
OutputDir=Output
OutputBaseFilename=LumaBeat-Setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayIcon={app}\{#AppExe}
; aplikasi membuat mutex ini; setup akan meminta aplikasi ditutup (klik kanan ikon tray -> Quit)
AppMutex=LumaBeat.SingleInstance

[Tasks]
Name: "desktopicon"; Description: "Buat pintasan di &desktop"; Flags: unchecked
Name: "autostart"; Description: "Jalankan LumaBeat otomatis saat Windows dimulai"

[Files]
Source: "..\target\release\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "LumaBeat"; ValueData: """{app}\{#AppExe}"""; Flags: uninsdeletevalue; Tasks: autostart

[Run]
Filename: "{app}\{#AppExe}"; Description: "Jalankan LumaBeat sekarang"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; hapus entri autostart jika dinyalakan dari menu aplikasi
Filename: "{sys}\reg.exe"; Parameters: "delete HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v LumaBeat /f"; Flags: runhidden; RunOnceId: "DelAutostart"

[UninstallDelete]
Type: filesandordirs; Name: "{userappdata}\LumaBeat"
