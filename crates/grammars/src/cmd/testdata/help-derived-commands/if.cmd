if exist "script.cmd" echo found
if not "a"=="b" echo mismatch
if defined PATH echo ok
if cmdextversion 2 echo ok
if /i "%ERRORLEVEL%" LEQ 1 goto :okay
:okay