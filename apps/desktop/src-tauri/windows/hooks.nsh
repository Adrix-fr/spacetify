; Steps spacetify adds to the Windows installer made by Tauri.
;
; The work itself is done by `spacetify.exe installer post-install` and
; `spacetify.exe installer pre-uninstall` (crates/core/src/setup.rs), which
; are tested and print what they do in the installer's details view:
; - the install folder is added to the user Path, so `spacetify` works in
;   any new terminal;
; - notifications get spacetify's own name instead of "Windows PowerShell";
; - the startup entry of the service is moved to the installed copy;
; - on uninstall, the service is stopped and all of that is taken back.
; Mods, settings and backups are never deleted.

!macro NSIS_HOOK_PREINSTALL
  ; Updating: stop the running service so its file can be replaced.
  ${If} ${FileExists} "$INSTDIR\spacetify.exe"
    nsExec::ExecToLog '"$INSTDIR\spacetify.exe" service stop'
    Pop $0
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToLog '"$INSTDIR\spacetify.exe" installer post-install'
  Pop $0
  ; Tells Windows the Path changed, so new terminals see it without logging out.
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; A real uninstall (not an update): offer to put Spotify back as it was.
  ; "No" is the default, and nothing is asked in silent mode.
  ${If} $UpdateMode <> 1
  ${AndIfNot} ${Silent}
    ${If} $LANGUAGE = 1036
      StrCpy $1 "Remettre Spotify d'origine avant de désinstaller spacetify ?$\n$\nOui : les thèmes et extensions sont retirés de Spotify (Spotify sera fermé s'il est ouvert).$\nNon : Spotify garde ses mods jusqu'à sa prochaine mise à jour.$\n$\nTes mods et tes sauvegardes restent dans leurs dossiers dans tous les cas."
    ${Else}
      StrCpy $1 "Put Spotify back to its original state before uninstalling spacetify?$\n$\nYes: themes and extensions are removed from Spotify (Spotify is closed if it is open).$\nNo: Spotify keeps its mods until its next update.$\n$\nYour mods and backups stay in their folders either way."
    ${EndIf}
    MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 "$1" /SD IDNO IDNO spacetify_skip_restore
      nsExec::ExecToLog '"$INSTDIR\spacetify.exe" restore --yes'
      Pop $0
    spacetify_skip_restore:
  ${EndIf}
  nsExec::ExecToLog '"$INSTDIR\spacetify.exe" installer pre-uninstall'
  Pop $0
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
!macroend
