dir /b && echo ok
dir /b || echo failed
dir /b & echo always
dir /b | findstr /i ".cmd"
echo one && echo two && echo three
echo ok || echo fail || echo also-fail
type nul | find /c "x"