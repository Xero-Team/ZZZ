xcopy src\*.rs build\ /d:01-01-2024 /e /exclude:skip.txt+tmp.txt /compress /-sparse
xcopy src build /s /i /y
