mode
mode con /status
mode lpt1:=com2:
mode con cp select=65001
mode con cp /status
mode con cols=120 lines=40
mode con rate=32 delay=1
mode com1: baud=9600 parity=n data=8 stop=1 to=on xon=off odsr=on octs=off dtr=hs rts=tg idsr=off
