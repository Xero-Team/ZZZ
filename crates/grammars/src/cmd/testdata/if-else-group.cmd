if exist "file.txt" (echo found) else echo missing
if exist "README.md" (echo exists) else (echo not-found)
if defined TEMP (echo defined) else (echo not-defined)
if not defined UNDEFINED (echo ok)
if "%1"=="" (echo no-args) else (echo has-args)