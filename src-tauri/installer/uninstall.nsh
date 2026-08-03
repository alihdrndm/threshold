; Uninstall cleanup for Threshold.
;
; Leaving a machine with a modified hosts file, orphaned "managed by your
; organization" browsers, or elevated scheduled tasks pointing at a binary that
; no longer exists is not acceptable. Uninstalling has to put everything back.
;
; The helper is asked to unblock first, because it is the only component that
; knows how to remove its own hosts block without disturbing anything else in
; the file. The registry and task cleanup below is the belt to that braces: it
; runs whether or not the helper was reachable.

; Register the elevated helper while the installer already holds elevation.
;
; Without this, a fresh install has no ThresholdHelper task, so committing to a
; session silently blocks nothing — the app looks like it works and does not.
; The relaunch triggers are per-user and register themselves on first run, so
; they are deliberately not done here: the installer may be elevated as a
; different account than the one that will use the app.
!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToLog '"$INSTDIR\threshold.exe" --register-helper="$INSTDIR\threshold-helper.exe"'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Ask the helper to lift any block while it still exists on disk.
  CreateDirectory "C:\ProgramData\Threshold"
  FileOpen $0 "C:\ProgramData\Threshold\request.json" w
  FileWrite $0 '{"action":"emergency_unblock"}'
  FileClose $0
  nsExec::ExecToLog 'schtasks /run /tn "ThresholdHelper"'
  Sleep 3000
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Scheduled tasks: the elevated helper and the three relaunch triggers.
  nsExec::ExecToLog 'schtasks /delete /tn "ThresholdHelper" /f'
  nsExec::ExecToLog 'schtasks /delete /tn "ThresholdLogon" /f'
  nsExec::ExecToLog 'schtasks /delete /tn "ThresholdUnlock" /f'
  nsExec::ExecToLog 'schtasks /delete /tn "ThresholdResume" /f'

  ; DNS-over-HTTPS policies. Only the values Threshold sets are removed; any
  ; other policy in these keys belongs to someone else and stays.
  DeleteRegValue HKLM "SOFTWARE\Policies\Google\Chrome" "DnsOverHttpsMode"
  DeleteRegValue HKLM "SOFTWARE\Policies\Microsoft\Edge" "DnsOverHttpsMode"
  DeleteRegValue HKLM "SOFTWARE\Policies\Mozilla\Firefox\DNSOverHTTPS" "Enabled"
  DeleteRegValue HKLM "SOFTWARE\Policies\Mozilla\Firefox\DNSOverHTTPS" "Locked"
  ; DeleteRegKey /ifempty removes these only when nothing else is left in them.
  DeleteRegKey /ifempty HKLM "SOFTWARE\Policies\Google\Chrome"
  DeleteRegKey /ifempty HKLM "SOFTWARE\Policies\Google"
  DeleteRegKey /ifempty HKLM "SOFTWARE\Policies\Microsoft\Edge"
  DeleteRegKey /ifempty HKLM "SOFTWARE\Policies\Mozilla\Firefox\DNSOverHTTPS"
  DeleteRegKey /ifempty HKLM "SOFTWARE\Policies\Mozilla\Firefox"
  DeleteRegKey /ifempty HKLM "SOFTWARE\Policies\Mozilla"

  ; Autostart entry.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Threshold"

  ; Machine-wide state. The local database in %APPDATA% is deliberately left
  ; alone: it is the user's own history, and destroying it on uninstall would
  ; be a surprise rather than a courtesy.
  RMDir /r "C:\ProgramData\Threshold"
!macroend
