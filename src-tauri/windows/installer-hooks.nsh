!macro NSIS_HOOK_PREINSTALL
  IfFileExists "$INSTDIR\orange-installer.exe" 0 orange_preinstall_done
  ClearErrors
  ExecWait '"$INSTDIR\orange-installer.exe" prepare-upgrade' $0
  IfErrors orange_preinstall_exec_failed
  IntCmp $0 0 orange_preinstall_done orange_preinstall_failed orange_preinstall_failed

orange_preinstall_exec_failed:
  SetErrorLevel 1
  Abort

orange_preinstall_failed:
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
  SetErrorLevel 1
  Abort

orange_postinstall_failed:
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
  SetErrorLevel 1
  Abort

orange_preuninstall_failed:
  SetErrorLevel $0
  Abort

orange_preuninstall_done:
!macroend
