; Quickshort Windows installer — NSIS.
; Built on CI: makensis /DVERSION=1.2.3 /DOUT=Quickshort-Windows-Setup.exe installer.nsi
; Unsigned build: SmartScreen will warn once ("More info" -> "Run anyway").

!define APPNAME "Quickshort"
!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!ifndef OUT
  !define OUT "Quickshort-Windows-Setup.exe"
!endif
!ifndef BIN
  !define BIN "..\..\target\release\quickshort.exe"
!endif
!ifndef EDITOR_BIN
  !define EDITOR_BIN "..\..\target\release\qs-editor.exe"
!endif

!include "MUI2.nsh"
!define MUI_ICON "quickshort.ico"
!define MUI_UNICON "quickshort.ico"
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_RIGHT
!define MUI_HEADERIMAGE_BITMAP "setup-header.bmp"
!define MUI_WELCOMEFINISHPAGE_BITMAP "setup-welcome.bmp"

Name "${APPNAME}"
OutFile "${OUT}"
Unicode true
InstallDir "$PROGRAMFILES64\${APPNAME}"
InstallDirRegKey HKCU "Software\${APPNAME}" "InstallDir"
RequestExecutionLevel admin
SetCompressor /SOLID lzma

!define MUI_WELCOMEPAGE_TITLE "Welcome to Quickshort"
!define MUI_WELCOMEPAGE_TEXT "Capture your screen, annotate screenshots, and share them from the system tray.$\r$\n$\r$\nSetup installs Quickshort ${VERSION} and its editor."
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\quickshort.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Open Quickshort"
!define MUI_FINISHPAGE_TITLE "Quickshort is ready"
!define MUI_FINISHPAGE_TEXT "Open Quickshort from the Start menu or the system tray. Press Print Screen to start a capture."
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

Section "Install"
  SetOutPath "$INSTDIR"
  File "${BIN}"
  File "${EDITOR_BIN}"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateDirectory "$SMPROGRAMS\${APPNAME}"
  CreateShortcut "$SMPROGRAMS\${APPNAME}\${APPNAME}.lnk" "$INSTDIR\quickshort.exe"
  CreateShortcut "$SMPROGRAMS\${APPNAME}\Uninstall ${APPNAME}.lnk" "$INSTDIR\uninstall.exe"

  WriteRegStr HKCU "Software\${APPNAME}" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APPNAME}" \
    "DisplayName" "${APPNAME}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APPNAME}" \
    "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APPNAME}" \
    "UninstallString" "$INSTDIR\uninstall.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APPNAME}" \
    "DisplayIcon" "$INSTDIR\quickshort.exe"
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APPNAME}" \
    "NoModify" 1
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APPNAME}" \
    "NoRepair" 1

SectionEnd

Section "Uninstall"
  ; launch-at-login entry the app writes from its settings
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${APPNAME}"
  Delete "$INSTDIR\quickshort.exe"
  Delete "$INSTDIR\qs-editor.exe"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\${APPNAME}\${APPNAME}.lnk"
  Delete "$SMPROGRAMS\${APPNAME}\Uninstall ${APPNAME}.lnk"
  RMDir "$SMPROGRAMS\${APPNAME}"
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APPNAME}"
  DeleteRegKey HKCU "Software\${APPNAME}"
SectionEnd
