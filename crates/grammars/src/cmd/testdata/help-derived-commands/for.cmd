for %%I in (*.cmd) do @echo %%I
for /d %%D in (*) do @echo %%D
for /r C:\Temp %%F in (*.log) do @echo %%F
for /l %%I in (1,1,3) do @echo %%I
for /f "tokens=1 delims==" %%I in ('set') do @echo %%I
for /f "usebackq tokens=1" %%I in (`set`) do @echo %%I