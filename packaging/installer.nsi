!ifndef VERSION
!error "VERSION is required"
!endif
!ifndef PAYLOAD_DIR
!error "PAYLOAD_DIR is required"
!endif
!ifndef OUTPUT_FILE
!error "OUTPUT_FILE is required"
!endif
!ifndef ICON_FILE
!error "ICON_FILE is required"
!endif
!ifndef INSTALL_SIZE_KB
!error "INSTALL_SIZE_KB is required"
!endif

Unicode true
Name "AnyCaptions ${VERSION}"
OutFile "${OUTPUT_FILE}"
Icon "${ICON_FILE}"
UninstallIcon "${ICON_FILE}"
InstallDir "$LOCALAPPDATA\AnyCaptions"
RequestExecutionLevel user
SetCompressor /SOLID lzma
SetDatablockOptimize on
SetDateSave off
CRCCheck on
SetOverwrite on
ShowInstDetails show
ShowUninstDetails show

VIProductVersion "${VERSION}.0"
VIAddVersionKey /LANG=1033 "ProductName" "AnyCaptions"
VIAddVersionKey /LANG=1033 "ProductVersion" "${VERSION}"
VIAddVersionKey /LANG=1033 "CompanyName" "EduardoViniciusJ"
VIAddVersionKey /LANG=1033 "FileDescription" "AnyCaptions Installer"
VIAddVersionKey /LANG=1033 "FileVersion" "${VERSION}"

Function .onInit
    SetRegView 64
    SetShellVarContext current
    ; Tauri stores its install location in this key's default value.
    StrCmp $INSTDIR "$LOCALAPPDATA\AnyCaptions" use_fallback_install_dir
    Goto install_dir_ready
    ReadRegStr $0 HKCU "Software\EduardoViniciusJ\AnyCaptions" ""
    IfErrors use_fallback_install_dir
    StrCmp $0 "" use_fallback_install_dir
    StrCpy $INSTDIR $0
    Goto install_dir_ready
use_fallback_install_dir:
    ReadRegStr $0 HKCU "Software\AnyCaptions" "InstallDir"
    IfErrors install_dir_ready
    StrCmp $0 "" install_dir_ready
    StrCpy $INSTDIR $0
install_dir_ready:
FunctionEnd

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Section "AnyCaptions" SEC_INSTALL
    SetRegView 64
    SetShellVarContext current
    SetOutPath "$INSTDIR"
    File "${PAYLOAD_DIR}\anycaptions.exe"
    File "${PAYLOAD_DIR}\abseil_dll.dll"
    File "${PAYLOAD_DIR}\ctranslate2.dll"
    File "${PAYLOAD_DIR}\ggml-base.dll"
    File "${PAYLOAD_DIR}\ggml-cpu.dll"
    File "${PAYLOAD_DIR}\ggml-vulkan.dll"
    File "${PAYLOAD_DIR}\ggml.dll"
    File "${PAYLOAD_DIR}\libprotobuf-lite.dll"
    File "${PAYLOAD_DIR}\libprotobuf.dll"
    File "${PAYLOAD_DIR}\libprotoc.dll"
    File "${PAYLOAD_DIR}\libutf8_range.dll"
    File "${PAYLOAD_DIR}\libutf8_validity.dll"
    File "${PAYLOAD_DIR}\llama.dll"
    File "${PAYLOAD_DIR}\nemo_speech_asr_c.dll"
    File "${PAYLOAD_DIR}\nemo_speech_asr.dll"
    File "${PAYLOAD_DIR}\nemo_speech_nmt_c.dll"
    File "${PAYLOAD_DIR}\nemo_speech_nmt.dll"
    File "${PAYLOAD_DIR}\opus_ct2_c.dll"

    SetOutPath "$INSTDIR\models\asr\nemotron-3.5-asr-streaming-0.6b"
    File "${PAYLOAD_DIR}\models\asr\nemotron-3.5-asr-streaming-0.6b\nemotron-3.5-asr-streaming-0.6b.q8_0.gguf"
    SetOutPath "$INSTDIR\models\translation\opus-mt-en-pt"
    File "${PAYLOAD_DIR}\models\translation\opus-mt-en-pt\config.json"
    File "${PAYLOAD_DIR}\models\translation\opus-mt-en-pt\model.bin"
    File "${PAYLOAD_DIR}\models\translation\opus-mt-en-pt\shared_vocabulary.json"
    File "${PAYLOAD_DIR}\models\translation\opus-mt-en-pt\source.spm"
    File "${PAYLOAD_DIR}\models\translation\opus-mt-en-pt\target.spm"
    SetOutPath "$INSTDIR\models\translation\opus-mt-pt-en"
    File "${PAYLOAD_DIR}\models\translation\opus-mt-pt-en\config.json"
    File "${PAYLOAD_DIR}\models\translation\opus-mt-pt-en\model.bin"
    File "${PAYLOAD_DIR}\models\translation\opus-mt-pt-en\shared_vocabulary.json"
    File "${PAYLOAD_DIR}\models\translation\opus-mt-pt-en\source.spm"
    File "${PAYLOAD_DIR}\models\translation\opus-mt-pt-en\target.spm"

    WriteUninstaller "$INSTDIR\Uninstall.exe"
    WriteRegStr HKCU "Software\EduardoViniciusJ\AnyCaptions" "" "$INSTDIR"
    WriteRegStr HKCU "Software\AnyCaptions" "InstallDir" "$INSTDIR"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AnyCaptions" "DisplayName" "AnyCaptions"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AnyCaptions" "DisplayVersion" "${VERSION}"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AnyCaptions" "Publisher" "EduardoViniciusJ"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AnyCaptions" "DisplayIcon" "$INSTDIR\anycaptions.exe,0"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AnyCaptions" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AnyCaptions" "UninstallString" '"$INSTDIR\Uninstall.exe"'
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AnyCaptions" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AnyCaptions" "URLInfoAbout" "https://anycaptions.app"
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AnyCaptions" "EstimatedSize" ${INSTALL_SIZE_KB}
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AnyCaptions" "NoModify" 1
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AnyCaptions" "NoRepair" 1

    CreateDirectory "$SMPROGRAMS\AnyCaptions"
    CreateShortcut "$SMPROGRAMS\AnyCaptions\AnyCaptions.lnk" "$INSTDIR\anycaptions.exe"
SectionEnd

Section "Uninstall"
    SetRegView 64
    SetShellVarContext current
    Delete "$SMPROGRAMS\AnyCaptions\AnyCaptions.lnk"
    RMDir "$SMPROGRAMS\AnyCaptions"

    Delete "$INSTDIR\anycaptions.exe"
    Delete "$INSTDIR\abseil_dll.dll"
    Delete "$INSTDIR\ctranslate2.dll"
    Delete "$INSTDIR\ggml-base.dll"
    Delete "$INSTDIR\ggml-cpu.dll"
    Delete "$INSTDIR\ggml-vulkan.dll"
    Delete "$INSTDIR\ggml.dll"
    Delete "$INSTDIR\libprotobuf-lite.dll"
    Delete "$INSTDIR\libprotobuf.dll"
    Delete "$INSTDIR\libprotoc.dll"
    Delete "$INSTDIR\libutf8_range.dll"
    Delete "$INSTDIR\libutf8_validity.dll"
    Delete "$INSTDIR\llama.dll"
    Delete "$INSTDIR\nemo_speech_asr_c.dll"
    Delete "$INSTDIR\nemo_speech_asr.dll"
    Delete "$INSTDIR\nemo_speech_nmt_c.dll"
    Delete "$INSTDIR\nemo_speech_nmt.dll"
    Delete "$INSTDIR\opus_ct2_c.dll"
    Delete "$INSTDIR\Uninstall.exe"
    RMDir /r "$INSTDIR\models\asr\nemotron-3.5-asr-streaming-0.6b"
    RMDir /r "$INSTDIR\models\translation\opus-mt-en-pt"
    RMDir /r "$INSTDIR\models\translation\opus-mt-pt-en"
    RMDir "$INSTDIR\models\asr"
    RMDir "$INSTDIR\models\translation"
    RMDir "$INSTDIR\models"
    RMDir "$INSTDIR"

    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AnyCaptions"
    DeleteRegValue HKCU "Software\EduardoViniciusJ\AnyCaptions" ""
    DeleteRegKey /ifempty HKCU "Software\EduardoViniciusJ\AnyCaptions"
    DeleteRegKey HKCU "Software\AnyCaptions"
SectionEnd
