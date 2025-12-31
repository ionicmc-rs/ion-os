make run-test
if [ $? == 16 ] then
    exit 0
else
    exit 1
fi