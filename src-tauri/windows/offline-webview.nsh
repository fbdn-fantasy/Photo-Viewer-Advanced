; The native Tauri missing-runtime path uses the embedded offline installer.
; Its minimum-version upgrade path uses EdgeUpdate instead. Keep that setting
; unset and check/update older runtimes here using the same embedded payload.
; Registry lookup order matches the native Tauri Windows x64 installer.
!define PVA_MINIMUM_WEBVIEW2 "157.0.0.0"

!macro PVA_READ_WEBVIEW_VERSION
  StrCpy $R8 ""
  ${If} ${RunningX64}
    ReadRegStr $R8 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\${WEBVIEW2APPGUID}" "pv"
  ${Else}
    ReadRegStr $R8 HKLM "SOFTWARE\Microsoft\EdgeUpdate\Clients\${WEBVIEW2APPGUID}" "pv"
  ${EndIf}
  ${If} $R8 == ""
    ReadRegStr $R8 HKCU "SOFTWARE\Microsoft\EdgeUpdate\Clients\${WEBVIEW2APPGUID}" "pv"
  ${EndIf}
  ${If} $R8 == ""
    StrCpy $R8 "0.0.0.0"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  Push $R7
  Push $R8
  Push $R9
  !insertmacro PVA_READ_WEBVIEW_VERSION
  ${VersionCompare} "${PVA_MINIMUM_WEBVIEW2}" "$R8" $R9
  ${If} $R9 = 1
    DetailPrint "Updating Microsoft WebView2 from the embedded offline installer..."
    InitPluginsDir
    File "/oname=$PLUGINSDIR\PVA-WebView2-Offline.exe" "${WEBVIEW2INSTALLERPATH}"
    ClearErrors
    ExecWait '"$PLUGINSDIR\PVA-WebView2-Offline.exe" /silent /install' $R7
    ${If} ${Errors}
      MessageBox MB_ICONSTOP|MB_OK "The bundled Microsoft WebView2 installer could not be started. Setup cannot continue."
      Abort "Could not start the offline WebView2 installer."
    ${EndIf}
    ${If} $R7 = 3010
      SetRebootFlag true
    ${ElseIf} $R7 != 0
      MessageBox MB_ICONSTOP|MB_OK "The offline Microsoft WebView2 installation failed (code $R7). Close applications using WebView2 and try setup again."
      Abort "Offline WebView2 installation failed (code $R7)."
    ${EndIf}
    !insertmacro PVA_READ_WEBVIEW_VERSION
    ${VersionCompare} "${PVA_MINIMUM_WEBVIEW2}" "$R8" $R9
    ${If} $R9 = 1
      MessageBox MB_ICONSTOP|MB_OK "Microsoft WebView2 ${PVA_MINIMUM_WEBVIEW2} or later is required. The installed version is $R8. Restart Windows if an update is pending, then run setup again."
      Abort "The required WebView2 version was not installed."
    ${EndIf}
    Delete "$PLUGINSDIR\PVA-WebView2-Offline.exe"
  ${EndIf}
  Pop $R9
  Pop $R8
  Pop $R7
!macroend
