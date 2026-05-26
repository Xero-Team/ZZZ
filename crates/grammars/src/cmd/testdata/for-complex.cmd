for /f "skip=2 tokens=1,2 delims=, " %%A in (data.csv) do echo %%A %%B
for /f "eol=; tokens=*" %%L in (config.ini) do echo %%L
for /f "usebackq delims=" %%X in ("my file.txt") do echo %%X
for /f "tokens=1-3" %%A in ('date /t') do (
  set month=%%A
  set day=%%B
  set year=%%C
)