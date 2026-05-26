set VAR=before
set LIST=
set /p answer=Enter value: 
start "title" /d C:\Temp /b cmd /c "echo hi"
copy /y /a file1.txt+b.txt output.txt /b
dir /a:-d /o:n /t:w *.cmd
findstr /r /n /c:"^:label" script.cmd
