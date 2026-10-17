; ClipSift 安装脚本（Inno Setup 6）
;
; 设计取向：装成本地普通软件，不碰系统目录。
; - 默认装到 %LOCALAPPDATA%\Programs\ClipSift，普通用户权限就能装，不用 UAC
; - 卸载走「设置 → 应用 → 已安装的应用」，点一下就干净删掉
; - 卸载时问一句要不要保留配置，不替你决定
;
; 编译方式（本地或 CI）：
;   ISCC.exe /DAppVersion=0.3.0 /DSourceDir=..\target\release installer\clipsift.iss

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

#ifndef SourceDir
  #define SourceDir "..\target\release"
#endif

#define AppName "ClipSift"
#define AppPublisher "soybeanmilka"
#define AppURL "https://github.com/soybeanmilka/clipsift"
#define AppExeName "clipsift-gui.exe"

[Setup]
; AppId 认的是产品不是版本——升级时靠它认出「装过的是同一个东西」，
; 所以一旦发布就不能改，改了会导致新旧版本并存。
AppId={{7E3B9F41-2C6A-4D8B-9A15-3F1C0D6E8B24}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppURL}
AppSupportURL={#AppURL}/issues
AppUpdatesURL={#AppURL}/releases

; 当前用户安装：不需要管理员权限，卸载项也落在 HKCU 而不是 HKLM。
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog

; %LOCALAPPDATA%\Programs 是 Windows 上非管理员安装的惯例位置
; （VS Code、Chrome 的按用户安装都在这），不放 Program Files 就不必提权。
DefaultDirName={localappdata}\Programs\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes

OutputDir=..\dist
OutputBaseFilename={#AppName}-{#AppVersion}-setup
SetupIconFile=..\assets\clipsift.ico
UninstallDisplayIcon={app}\{#AppExeName}
UninstallDisplayName={#AppName}

; LZMA2 固实压缩：两个 exe 里有大量共同的 Rust 标准库代码，固实压缩能吃掉这部分重复。
Compression=lzma2/max
SolidCompression=yes

; 64 位程序要标出来，否则 WOW64 会把它当 32 位。
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

; 装完直接能跑，不用先重启资源管理器。
ChangesAssociations=no
CloseApplications=yes
RestartApplications=no

; 美观：现代向导样式 + 深色/浅色自适应
WizardStyle=modern
WizardSizePercent=110
MinVersion=10.0

[Languages]
; 中文用的是 Inno Setup 官方仓库里的简体中文翻译（放在 installer/ChineseSimplified.isl，
; 跟着项目一起走，避免构建时依赖网络）。
Name: "chinesesimplified"; MessagesFile: "ChineseSimplified.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
chinesesimplified.WelcomeLabel2=这会把 [name] 装到你的电脑上。%n%n装上之后不用做任何事——它在后台盯着剪贴板，复制来的隐形字符（零宽空格、伪空格、弯引号）会被自动去掉。%n%n要退出就是关窗口，要卸载去「设置 → 应用 → 已安装的应用」。
chinesesimplified.CreateDesktopIcon=创建桌面快捷方式
chinesesimplified.AutoStart=开机自动启动（推荐）
chinesesimplified.LaunchNow=装完立刻运行 ClipSift
chinesesimplified.AdditionalIcons=附加快捷方式：
chinesesimplified.KeepSettings=要保留你的规则设置吗？
chinesesimplified.KeepSettingsDesc=选「否」会连配置文件一起删掉，下次安装是全新状态。
chinesesimplified.RunningWarning=ClipSift 正在运行，安装程序会先把它关掉。

english.WelcomeLabel2=This will install [name] on your computer.%n%nAfter that you don't have to do anything — it watches the clipboard in the background and strips invisible characters (zero-width spaces, non-breaking spaces, curly quotes) from whatever you copy.%n%nQuit by closing the window; uninstall from Settings → Apps.
english.CreateDesktopIcon=Create a desktop shortcut
english.AutoStart=Start with Windows (recommended)
english.LaunchNow=Launch ClipSift now
english.AdditionalIcons=Additional shortcuts:
english.KeepSettings=Keep your rule settings?
english.KeepSettingsDesc=Choose No to delete the config file too, so the next install starts fresh.
english.RunningWarning=ClipSift is running. Setup will close it first.

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: checkedonce
Name: "autostart"; Description: "{cm:AutoStart}"; Flags: checkedonce

[Files]
; 图形界面（主程序）和命令行（给爱用终端的人）
Source: "{#SourceDir}\clipsift-gui.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\clipsift.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion isreadme
Source: "..\README.en.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\docs\*"; DestDir: "{app}\docs"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"; WorkingDir: "{app}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; WorkingDir: "{app}"; Tasks: desktopicon

[Registry]
; 开机自启：写 HKCU 的 Run 键。和程序里那个「开机自启」复选框读写的是同一个值，
; 所以装完之后界面上的勾选状态能正确显示。
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; \
    ValueName: "ClipSift"; ValueData: """{app}\{#AppExeName}"""; Flags: uninsdeletevalue; Tasks: autostart

[Run]
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchNow}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; 注意：这里不列 config.json。它是否被删由下面的 [Code] 决定——
; 用户选择「保留设置」时就该留下来，写在 [UninstallDelete] 里会被无条件删掉，
; 那个选择就成了假的。

[Code]
const
  RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run';

function IsClipSiftRunning(): Boolean;
var
  ResultCode: Integer;
begin
  // tasklist 退出码 0 = 找到进程，1 = 没找到
  Result := Exec('cmd.exe', '/c tasklist /FI "IMAGENAME eq clipsift-gui.exe" /NH | find /I "clipsift-gui.exe" >nul',
    '', SW_HIDE, ewWaitUntilTerminated, ResultCode) and (ResultCode = 0);
end;

procedure KillClipSift();
var
  ResultCode: Integer;
begin
  Exec('taskkill.exe', '/F /IM clipsift-gui.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec('taskkill.exe', '/F /IM clipsift.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  // 覆盖安装/升级时旧进程会占用 exe，先关掉，否则文件替换会失败。
  if IsClipSiftRunning() then
  begin
    if MsgBox(ExpandConstant('{cm:RunningWarning}'), mbConfirmation, MB_OKCANCEL) = IDCANCEL then
    begin
      Result := 'ClipSift 正在运行，请先退出再继续安装。';
      Exit;
    end;
    KillClipSift();
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  ConfigFile: String;
  ResultCode: Integer;
begin
  // usUninstall 在删除文件之前跑，这是唯一还能读到 config.json 的时机。
  // usPostUninstall 时 {app} 已经整个被删了，FileExists 永远为假。
  if CurUninstallStep = usUninstall then
  begin
    ConfigFile := ExpandConstant('{app}\config.json');
    if FileExists(ConfigFile) then
    begin
      // 静默卸载（/SILENT、/VERYSILENT）下绝不能弹框——脚本和 CI 会卡死等点击。
      // 静默就当「不保留」，删干净；只有走图形的用户才有得选。
      // 注意：这里必须用 UninstallSilent，WizardSilent 在卸载阶段调用会抛
      // "Cannot call WizardSilent function during Uninstall" 直接崩掉。
      if UninstallSilent() then
        DeleteFile(ConfigFile)
      else if MsgBox(ExpandConstant('{cm:KeepSettings}'), mbConfirmation, MB_YESNO) = IDNO then
        DeleteFile(ConfigFile);
    end;
  end;

  if CurUninstallStep = usPostUninstall then
  begin
    // 自启项由 [Registry] 的 uninsdeletevalue 负责，这里兜一次底：
    // 万一用户中途取消过安装、那个值残留下来，卸载时也该消失。
    RegDeleteValue(HKCU, RunKey, 'ClipSift');
  end;
end;
