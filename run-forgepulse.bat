@echo off
cd /d C:\Users\gianl\Documents\ForgePulse

start "ForgePulse Service" cmd /k target\debug\forge-service.exe console

timeout /t 2 /nobreak >nul

start "ForgePulse Frontend" cmd /k cd /d C:\Users\gianl\Documents\ForgePulse\forge-ui ^&^& npm.cmd run dev

timeout /t 3 /nobreak >nul

start "" C:\Users\gianl\Documents\ForgePulse\target\debug\forge-ui.exe
