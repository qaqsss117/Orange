Var OrangeUpgradePreviousDisplayVersion
Var OrangeUpgradePreviousEstimatedSize

Function OrangeRollbackUpgrade
  IfFileExists "$INSTDIR\.orange-upgrade-backup\ready.v1" 0 orange_rollback_no_backup

  ClearErrors
  ReadINIStr $OrangeUpgradePreviousDisplayVersion "$INSTDIR\.orange-upgrade-backup\rollback.ini" "rollback" "display-version"
  ReadINIStr $OrangeUpgradePreviousEstimatedSize "$INSTDIR\.orange-upgrade-backup\rollback.ini" "rollback" "estimated-size"
  IfErrors orange_rollback_failed
  StrCmp $OrangeUpgradePreviousDisplayVersion "" orange_rollback_failed 0
  StrCmp $OrangeUpgradePreviousEstimatedSize "" orange_rollback_failed 0

  ClearErrors
  ExecWait '"$INSTDIR\orange-installer.exe" prepare-upgrade' $8
  IfErrors orange_rollback_failed
  IntCmp $8 0 orange_rollback_restore_files orange_rollback_failed orange_rollback_failed

orange_rollback_restore_files:
  ClearErrors
  CopyFiles /SILENT "$INSTDIR\.orange-upgrade-backup\orange-app.exe" "$INSTDIR"
  CopyFiles /SILENT "$INSTDIR\.orange-upgrade-backup\orange-control-plane.exe" "$INSTDIR"
  CopyFiles /SILENT "$INSTDIR\.orange-upgrade-backup\orange-service.exe" "$INSTDIR"
  CopyFiles /SILENT "$INSTDIR\.orange-upgrade-backup\orange-installer.exe" "$INSTDIR"
  CopyFiles /SILENT "$INSTDIR\.orange-upgrade-backup\orange-data-plane.exe" "$INSTDIR"
  CopyFiles /SILENT "$INSTDIR\.orange-upgrade-backup\uninstall.exe" "$INSTDIR"
  IfErrors orange_rollback_failed

  ClearErrors
  ExecWait '"$INSTDIR\orange-installer.exe" install' $8
  IfErrors orange_rollback_failed
  IntCmp $8 0 orange_rollback_restore_registry orange_rollback_failed orange_rollback_failed

orange_rollback_restore_registry:
  ClearErrors
  WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\Orange" "DisplayVersion" "$OrangeUpgradePreviousDisplayVersion"
  WriteRegDWORD SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\Orange" "EstimatedSize" $OrangeUpgradePreviousEstimatedSize
  IfErrors orange_rollback_failed
  RMDir /r "$INSTDIR\.orange-upgrade-backup"
  IfFileExists "$INSTDIR\.orange-upgrade-backup" orange_rollback_failed 0
  Push 0
  Return

orange_rollback_no_backup:
  Push 2
  Return

orange_rollback_failed:
  Push 1
FunctionEnd

!macro NSIS_HOOK_PREINSTALL
  IfFileExists "$INSTDIR\orange-installer.exe" 0 orange_preinstall_done

  IfFileExists "$INSTDIR\.orange-upgrade-backup\ready.v1" 0 orange_preinstall_remove_partial_backup
  Call OrangeRollbackUpgrade
  Pop $0
  IntCmp $0 0 orange_preinstall_remove_partial_backup orange_preinstall_stale_rollback_failed orange_preinstall_stale_rollback_failed

orange_preinstall_remove_partial_backup:
  RMDir /r "$INSTDIR\.orange-upgrade-backup"
  IfFileExists "$INSTDIR\.orange-upgrade-backup" orange_preinstall_backup_failed 0

  IfFileExists "$INSTDIR\orange-app.exe" 0 orange_preinstall_backup_failed
  IfFileExists "$INSTDIR\orange-control-plane.exe" 0 orange_preinstall_backup_failed
  IfFileExists "$INSTDIR\orange-service.exe" 0 orange_preinstall_backup_failed
  IfFileExists "$INSTDIR\orange-installer.exe" 0 orange_preinstall_backup_failed
  IfFileExists "$INSTDIR\orange-data-plane.exe" 0 orange_preinstall_backup_failed
  IfFileExists "$INSTDIR\uninstall.exe" 0 orange_preinstall_backup_failed

  ClearErrors
  CreateDirectory "$INSTDIR\.orange-upgrade-backup"
  CopyFiles /SILENT "$INSTDIR\orange-app.exe" "$INSTDIR\.orange-upgrade-backup"
  CopyFiles /SILENT "$INSTDIR\orange-control-plane.exe" "$INSTDIR\.orange-upgrade-backup"
  CopyFiles /SILENT "$INSTDIR\orange-service.exe" "$INSTDIR\.orange-upgrade-backup"
  CopyFiles /SILENT "$INSTDIR\orange-installer.exe" "$INSTDIR\.orange-upgrade-backup"
  CopyFiles /SILENT "$INSTDIR\orange-data-plane.exe" "$INSTDIR\.orange-upgrade-backup"
  CopyFiles /SILENT "$INSTDIR\uninstall.exe" "$INSTDIR\.orange-upgrade-backup"
  IfErrors orange_preinstall_backup_failed

  ClearErrors
  ReadRegStr $OrangeUpgradePreviousDisplayVersion SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\Orange" "DisplayVersion"
  ReadRegDWORD $OrangeUpgradePreviousEstimatedSize SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\Orange" "EstimatedSize"
  IfErrors orange_preinstall_backup_failed

  ClearErrors
  WriteINIStr "$INSTDIR\.orange-upgrade-backup\rollback.ini" "rollback" "display-version" "$OrangeUpgradePreviousDisplayVersion"
  WriteINIStr "$INSTDIR\.orange-upgrade-backup\rollback.ini" "rollback" "estimated-size" "$OrangeUpgradePreviousEstimatedSize"
  IfErrors orange_preinstall_backup_failed

  ClearErrors
  FileOpen $9 "$INSTDIR\.orange-upgrade-backup\ready.v1" w
  FileWrite $9 "schema-version=1$\r$\n"
  FileClose $9
  IfErrors orange_preinstall_backup_failed

  ClearErrors
  ExecWait '"$INSTDIR\orange-installer.exe" prepare-upgrade' $0
  IfErrors orange_preinstall_exec_failed
  IntCmp $0 0 orange_preinstall_done orange_preinstall_failed orange_preinstall_failed

orange_preinstall_backup_failed:
  RMDir /r "$INSTDIR\.orange-upgrade-backup"
  IfSilent +2 0
  MessageBox MB_OK|MB_ICONSTOP "Orange could not create a complete upgrade rollback backup."
  SetErrorLevel 30
  Abort

orange_preinstall_stale_rollback_failed:
  IfSilent +2 0
  MessageBox MB_OK|MB_ICONSTOP "Orange could not recover the previous interrupted upgrade."
  SetErrorLevel 31
  Abort

orange_preinstall_exec_failed:
  Call OrangeRollbackUpgrade
  Pop $1
  IfSilent +2 0
  MessageBox MB_OK|MB_ICONSTOP "Orange upgrade preparation could not be started."
  SetErrorLevel 1
  Abort

orange_preinstall_failed:
  Call OrangeRollbackUpgrade
  Pop $1
  IfSilent +2 0
  MessageBox MB_OK|MB_ICONSTOP "Orange upgrade preparation failed (code $0)."
  SetErrorLevel $0
  Abort

orange_preinstall_done:
!macroend

!macro NSIS_HOOK_POSTINSTALL
  IfErrors orange_postinstall_payload_failed
  ClearErrors
  ExecWait '"$INSTDIR\orange-installer.exe" install' $0
  IfErrors orange_postinstall_exec_failed
  IntCmp $0 0 orange_postinstall_commit orange_postinstall_failed orange_postinstall_failed

orange_postinstall_commit:
  RMDir /r "$INSTDIR\.orange-upgrade-backup"
  IfFileExists "$INSTDIR\.orange-upgrade-backup" orange_postinstall_cleanup_failed orange_postinstall_done

orange_postinstall_payload_failed:
  Call OrangeRollbackUpgrade
  Pop $1
  IntCmp $1 0 orange_postinstall_payload_rolled_back orange_postinstall_rollback_failed orange_postinstall_rollback_failed

orange_postinstall_payload_rolled_back:
  IfSilent +2 0
  MessageBox MB_OK|MB_ICONSTOP "Orange upgrade payload failed and the previous version was restored."
  SetErrorLevel 32
  Abort

orange_postinstall_exec_failed:
  Call OrangeRollbackUpgrade
  Pop $1
  IntCmp $1 0 orange_postinstall_exec_rolled_back orange_postinstall_rollback_failed orange_postinstall_rollback_failed

orange_postinstall_exec_rolled_back:
  IfSilent +2 0
  MessageBox MB_OK|MB_ICONSTOP "Orange system service installation failed and the previous version was restored."
  SetErrorLevel 33
  Abort

orange_postinstall_failed:
  Call OrangeRollbackUpgrade
  Pop $1
  IntCmp $1 0 orange_postinstall_failed_rolled_back orange_postinstall_rollback_failed orange_postinstall_rollback_failed

orange_postinstall_failed_rolled_back:
  IfSilent +2 0
  MessageBox MB_OK|MB_ICONSTOP "Orange system service installation failed (code $0); the previous version was restored."
  SetErrorLevel $0
  Abort

orange_postinstall_cleanup_failed:
  Call OrangeRollbackUpgrade
  Pop $1
  IntCmp $1 0 orange_postinstall_cleanup_rolled_back orange_postinstall_rollback_failed orange_postinstall_rollback_failed

orange_postinstall_cleanup_rolled_back:
  IfSilent +2 0
  MessageBox MB_OK|MB_ICONSTOP "Orange could not commit the upgrade and restored the previous version."
  SetErrorLevel 34
  Abort

orange_postinstall_rollback_failed:
  IfSilent +2 0
  MessageBox MB_OK|MB_ICONSTOP "Orange upgrade rollback failed; repair is required before use."
  SetErrorLevel 35
  Abort

orange_postinstall_done:
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ClearErrors
  ${GetOptions} $CMDLINE "/DELETEAPPDATA" $1
  ${IfNot} ${Errors}
    StrCpy $DeleteAppDataCheckboxState 1
  ${EndIf}

  ClearErrors
  ${If} $UpdateMode = 1
    ExecWait '"$INSTDIR\orange-installer.exe" prepare-upgrade' $0
  ${Else}
    ExecWait '"$INSTDIR\orange-installer.exe" uninstall' $0
  ${EndIf}
  IfErrors orange_preuninstall_exec_failed
  IntCmp $0 0 orange_preuninstall_done orange_preuninstall_failed orange_preuninstall_failed

orange_preuninstall_exec_failed:
  IfSilent +2 0
  MessageBox MB_OK|MB_ICONSTOP "Orange system cleanup could not be started."
  SetErrorLevel 1
  Abort

orange_preuninstall_failed:
  IfSilent +2 0
  MessageBox MB_OK|MB_ICONSTOP "Orange system cleanup failed (code $0)."
  SetErrorLevel $0
  Abort

orange_preuninstall_done:
!macroend
