@echo off
title ShardVault Studio
echo ========================================================
echo  Starting ShardVault Studio (Database Cockpit & Editor)
echo ========================================================
cd /d "%~dp0"

set "DATA_DIR=%~1"
if "%DATA_DIR%"=="" (
    if exist "E:\DocuTrack\data" (
        set "DATA_DIR=E:\DocuTrack\data"
    ) else (
        set "DATA_DIR=.\data"
    )
)

if exist "target\debug\shardvault-studio.exe" (
    echo Menjalankan binary shardvault-studio.exe (%DATA_DIR%)...
    start "" "target\debug\shardvault-studio.exe" "%DATA_DIR%"
) else (
    echo Mengompilasi dan menjalankan ShardVault Studio (%DATA_DIR%)...
    cargo run --bin shardvault-studio -- "%DATA_DIR%"
)
