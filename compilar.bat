@echo off
setlocal enabledelayedexpansion
title ChatLAN - Compilando...
cd /d "%~dp0"

echo ============================================
echo   ChatLAN - Instalacion y compilacion
echo ============================================
echo.

REM ---------------------------------------------------------------
REM 1) Verificar que Node.js este instalado
REM ---------------------------------------------------------------
where node >nul 2>nul
if errorlevel 1 (
    echo [ERROR] No se encontro Node.js instalado.
    echo.
    echo Instalalo desde https://nodejs.org ^(version 18 o mas nueva^)
    echo y despues volve a ejecutar este archivo.
    echo.
    pause
    exit /b 1
)
for /f "tokens=*" %%v in ('node -v') do echo Node.js encontrado: %%v

REM ---------------------------------------------------------------
REM 2) Verificar que Rust/Cargo este instalado
REM ---------------------------------------------------------------
where cargo >nul 2>nul
if errorlevel 1 (
    echo.
    echo [ERROR] No se encontro Rust/Cargo instalado.
    echo.
    echo Instalalo desde https://rustup.rs
    echo Durante la instalacion, cuando te pregunte por
    echo "MSVC build tools", acepta instalarlas ^(hacen falta
    echo para compilar la app^).
    echo.
    echo Despues de instalar, CERRA esta ventana, abri una consola
    echo nueva y volve a ejecutar este archivo.
    echo.
    pause
    exit /b 1
)
for /f "tokens=*" %%v in ('cargo -V') do echo Rust encontrado: %%v
echo.

REM ---------------------------------------------------------------
REM 3) Instalar dependencias de Node (npm install)
REM ---------------------------------------------------------------
echo ============================================
echo   Instalando dependencias del frontend...
echo ============================================
call npm install
if errorlevel 1 (
    echo.
    echo [ERROR] Fallo "npm install". Revisa el mensaje de arriba.
    pause
    exit /b 1
)
echo.

REM ---------------------------------------------------------------
REM 4) Compilar la app (Rust + React), version final de release
REM ---------------------------------------------------------------
echo ============================================
echo   Compilando ChatLAN ^(puede tardar varios
echo   minutos la primera vez^)...
echo ============================================
call npm run tauri build
if errorlevel 1 (
    echo.
    echo [ERROR] Fallo la compilacion. Revisa el mensaje de arriba.
    echo Si es un error de Rust, copialo y pasaselo a Claude para
    echo que te ayude a corregirlo.
    pause
    exit /b 1
)

REM ---------------------------------------------------------------
REM 5) Copiar el ejecutable final a una carpeta facil de encontrar
REM ---------------------------------------------------------------
set "SRC_EXE=src-tauri\target\release\chatlan.exe"
set "OUT_DIR=Compilado"
set "NSIS_DIR=src-tauri\target\release\bundle\nsis"

if not exist "%OUT_DIR%" mkdir "%OUT_DIR%"

if exist "%SRC_EXE%" (
    copy /y "%SRC_EXE%" "%OUT_DIR%\ChatLAN.exe" >nul
    echo.
    echo Ejecutable portable copiado a: %OUT_DIR%\ChatLAN.exe
)

if exist "%NSIS_DIR%" (
    for %%f in ("%NSIS_DIR%\*-setup.exe") do (
        copy /y "%%f" "%OUT_DIR%\" >nul
        echo Instalador copiado a: %OUT_DIR%\%%~nxf
    )
)

echo.
echo ============================================
echo   Listo! ChatLAN se compilo correctamente.
echo ============================================
echo.
echo Encontras los archivos finales en la carpeta "%OUT_DIR%":
echo   - ChatLAN.exe            (version portable, no necesita instalar)
echo   - ^(nombre^)-setup.exe    (instalador, si se genero)
echo.
pause
