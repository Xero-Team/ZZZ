robocopy C:\Src D:\Dst *.txt *.md /e /copy:DAT /r:2 /w:5 /log:copy.log
robocopy \\server\share C:\Backup /mir /xd node_modules dist /xf *.tmp *.bak
robocopy C:\Src D:\Dst *.txt /copy:DAT /dcopy:DA /ia:RASH /xa:SH /r:2 /w:5 /log:copy.log /job:nightly /save:weekly
robocopy C:\Src D:\Dst *.txt /lev:2 /mon:5 /mot:10 /rh:0100-0500 /ipg:8 /mt:16 /iomaxsize:1M /iorate:10M /threshold:512K /max:1024 /min:10 /maxage:30 /minage:2 /maxlad:30 /minlad:2 /r:3 /w:7 /lfsm:1G
