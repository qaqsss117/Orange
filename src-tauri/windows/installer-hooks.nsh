!macro NSIS_HOOK_PREINSTALL
  IfFileExists "$INSTDIR\orange-installer.exe" 0 orange_preinstall_done
  ClearErrors
  ExecWait '"$INSTDIR\orange-installer.exe" prepare-upgrade' $0
  IfErrors orange_preinstall_exec_failed
  IntCmp $0 0 orange_preinstall_done orange_preinstall_failed orange_preinstall_failed

orange_preinstall_exec_failed:
  MessageBox MB_OK|MB_ICONSTOP "Orange upgrade preparation could not be started."
  SetErrorLevel 1
  Abort

orange_preinstall_failed:
  MessageBox MB_OK|MB_ICONSTOP "Orange upgrade preparation failed (code $0)."
  SetErrorLevel $0
  Abort

orange_preinstall_done:
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ClearErrors
  ExecWait '"$INSTDIR\orange-installer.exe" install' $0
  IfErrors orange_postinstall_exec_failed
  IntCmp $0 0 orange_postinstall_done orange_postinstall_failed orange_postinstall_failed

orange_postinstall_exec_failed:
  MessageBox MB_OK|MB_ICONSTOP "Orange system service installation could not be started."
  SetErrorLevel 1
  Abort

orange_postinstall_failed:
  MessageBox MB_OK|MB_ICONSTOP "Orange system service installation failed (code $0)."
  SetErrorLevel $0
  Abort

orange_postinstall_done:
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ClearErrors
  ExecWait '"$INSTDIR\orange-installer.exe" uninstall' $0
  IfErrors orange_preuninstall_exec_failed
  IntCmp $0 0 orange_preuninstall_done orange_preuninstall_failed orange_preuninstall_failed

orange_preuninstall_exec_failed:
  MessageBox MB_OK|MB_ICONSTOP "Orange system cleanup could not be started."
  SetErrorLevel 1
  Abort

orange_preuninstall_failed:
  MessageBox MB_OK|MB_ICONSTOP "Orange system cleanup failed (code $0)."
  SetErrorLevel $0
  Abort

orange_preuninstall_done:
!macroend
