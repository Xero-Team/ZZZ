set ANSWER=42
set /p ANSWER=Enter value:
setlocal enableextensions enabledelayedexpansion
endlocal
help dir
echo off
echo Build started
 assoc .txt=txtfile
assoc .log
assoc .bak=
ftype txtfile="%SystemRoot%\System32\NOTEPAD.EXE" "%1"
ftype txtfile=
attrib +r -h +a archive\* /s /d /l
start /node 1 /affinity 0x3 /wait /b cmd /c "echo hi"
cd /d C:\Temp
cd ..
cd C:
pushd ..\scripts
popd
copy /d /v /n /y /z /l source.txt /a + second.txt /b target.txt /a
move /y source1.txt,source2.txt archive\
move olddir newdir
xcopy src\*.rs build\ /d:01-01-2024 /e /exclude:skip.txt+tmp.txt /compress /-sparse
del /p /f /s /q /a:-h build\*.obj temp\*.tmp
erase /q scratch\*.tmp
rename old_name.txt new_name.bak
mkdir build\output
rmdir /s /q build\output
more /e /c /p /s /t4 +10 script.cmd notes.txt
type script.cmd | more /e /c
type script.cmd notes.txt
find /v /n /i /offline "needle" script.cmd other.cmd
mklink /d docs-link docs\source
tree
tree C:\Windows /f /a
chcp
chcp 65001
subst
subst Z: C:\Tools
subst Z: /d
sort /r
sort /+3 /m 512 /l C /rec 8192 input.txt /t C:\Temp /o output.txt
fc /b first.bin second.bin
fc /a /lb10 /123 first.txt second.txt
comp left.bin right.bin /d /a /n=10 /c /offline /m
mode
mode con /status
mode lpt1:=com2:
mode con cp select=65001
mode con cp /status
mode con cols=120 lines=40
mode con rate=32 delay=1
mode com1: baud=9600 parity=n data=8 stop=1 to=on xon=off odsr=on octs=off dtr=hs rts=tg idsr=off
doskey /reinstall /listsize=200 /insert /overstrike /exename=cmd /macrofile=macros.dos /macros /history
doskey /macros:all
doskey /macros:cmd
doskey build=msbuild $*
tasklist
tasklist /m
tasklist /s server /u domain\user /p password /svc /fi "STATUS eq RUNNING" /fo csv /nh
taskkill /im notepad.exe
taskkill /f /fi "PID ge 1000" /im notepad.exe /t
systeminfo
systeminfo /s server /u domain\user /p password /fo csv /nh
shutdown /?
shutdown /r /m \\server /t 60 /d p:2:4 /c "planned restart" /f
driverquery
driverquery /s server /u domain\user /p password /fo csv /nh /si
openfiles /query /?
openfiles /local /?
schtasks /query /?
schtasks /showsid /?
sc \\server query type= service state= all
sc query eventlog
sc start MyService
sc boot ok
sc querylock
gpresult /r
gpresult /s server /u domain\user /p password /scope computer /user targetuser /z
gpresult /h report.html /f
bcdedit /store C:\boot\bcd /enum /v
bcdedit /copy {current} /d "Cloned entry"
compact
compact /c /s:C:\Src /a /i /f /q /exe:lzx file1.txt file2.txt
compact /compactos:query /windir:C:\Windows
replace source.txt C:\Dest /p /r /s /w /u
convert C: /fs:ntfs /v /cvtarea:contig.sys /nosecurity /x
chkdsk C: /f /r /x /l:4096 /scan /perf
chkntfs /d
chkntfs /t:30
chkntfs /x C: D:
chkntfs /c C:
print /d:lpt1 report.txt notes.txt
icacls C:\Temp\file.txt /grant:r User:F /t /c /l /q
icacls C:\Temp /restore acl.txt /c /l /q
cacls file.txt /e /g user:f /c
recover C:\Temp\broken.txt
format C: /fs:ntfs /v:DATA /q /a:4096 /x /p:1 /s:enable
fsutil behavior
fsutil file queryCaseSensitiveInfo C:\Temp\demo.txt
label C: WORK
label /mp C:\Mount DATA
cmd /q /d /v:on /c "echo hi && exit /b 0"
cmd /a /e:off /f:on /k dir
robocopy C:\Src D:\Dst *.txt *.md /e /copy:DAT /r:2 /w:5 /log:copy.log
robocopy \\server\share C:\Backup /mir /xd node_modules dist /xf *.tmp *.bak
robocopy C:\Src D:\Dst *.txt /copy:DAT /dcopy:DA /ia:RASH /xa:SH /r:2 /w:5 /log:copy.log /job:nightly /save:weekly
robocopy C:\Src D:\Dst *.txt /lev:2 /mon:5 /mot:10 /rh:0100-0500 /ipg:8 /mt:16 /iomaxsize:1M /iorate:10M /threshold:512K /max:1024 /min:10 /maxage:30 /minage:2 /maxlad:30 /minlad:2 /r:3 /w:7 /lfsm:1G
path
path C:\Windows;C:\Tools;%PATH%
path ;
prompt $p$g$+$m
verify on
verify
vol C:
vol
title Build %COMPUTERNAME% Console
color 0a
color
date /t
date 2026-05-24
time 12:34:56.78
dir C:\Windows\*.cmd /a:-d /o:n /t:w /s
findstr /r /n /m /o /p /f:list.txt /c:"^:label" /g:patterns.txt /d:src;tests /a:0c /offline script.cmd other.cmd
call build.cmd one two
call :done one two
goto :eof
goto :done
pause
cls
ver
break
:done
