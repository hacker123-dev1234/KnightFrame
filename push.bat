@echo off
setlocal EnableExtensions DisableDelayedExpansion

cd /d "%~dp0"
if errorlevel 1 goto :fail_directory

where git >nul 2>&1
if errorlevel 1 goto :fail_git

git rev-parse --is-inside-work-tree >nul 2>&1
if errorlevel 1 goto :fail_repository

for /f "delims=" %%B in ('git branch --show-current') do set "BRANCH=%%B"
if not defined BRANCH goto :fail_branch

git remote get-url origin >nul 2>&1
if errorlevel 1 goto :fail_origin

set "MESSAGE=%~1"
if defined MESSAGE goto :message_ready
set /p "MESSAGE=Commit message [update KnightFrame]: "
if not defined MESSAGE set "MESSAGE=update KnightFrame"

:message_ready
echo.
echo [1/4] Staging changes...
git add -A
if errorlevel 1 goto :fail_stage

git diff --cached --quiet
if errorlevel 2 goto :fail_diff
if errorlevel 1 goto :commit
echo No local changes to commit.
goto :sync

:commit
echo [2/4] Creating commit...
git commit -m "%MESSAGE%"
if errorlevel 1 goto :fail_commit

:sync
echo [3/4] Synchronizing origin/%BRANCH%...
git pull --rebase origin "%BRANCH%"
if errorlevel 1 goto :fail_pull

echo [4/4] Pushing origin/%BRANCH%...
git push -u origin "%BRANCH%"
if errorlevel 1 goto :fail_push

echo.
echo Push completed successfully.
git log -1 --oneline
goto :success

:fail_directory
echo ERROR: Cannot enter the repository directory.
goto :failure
:fail_git
echo ERROR: Git is not installed or is not available in PATH.
goto :failure
:fail_repository
echo ERROR: This file is not inside a Git repository.
goto :failure
:fail_branch
echo ERROR: Detached HEAD is not supported. Check out a branch first.
goto :failure
:fail_origin
echo ERROR: Git remote "origin" is missing.
goto :failure
:fail_stage
echo ERROR: Failed to stage changes.
goto :failure
:fail_diff
echo ERROR: Failed to inspect staged changes.
goto :failure
:fail_commit
echo ERROR: Commit failed. Review the Git output above.
goto :failure
:fail_pull
echo ERROR: Rebase failed. Resolve the conflict, then run this file again.
goto :failure
:fail_push
echo ERROR: Push failed. Check authentication and network access.
goto :failure

:failure
echo.
if "%~1"=="" pause
exit /b 1

:success
echo.
if "%~1"=="" pause
exit /b 0
