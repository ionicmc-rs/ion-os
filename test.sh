echo make sure you have built in the docker build-env first!
make run-qemu-tests
if [ $? -eq 2 ]; then
    echo Tests Passed! \(Ignore Make\'s error.\)
    exit 0
else
    echo Tests Failed!
    exit 1
fi