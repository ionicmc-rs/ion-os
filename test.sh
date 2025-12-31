echo make sure you have built in the docker build-env first!
make run-qemu-tests
if [ $? -eq 2 ]; then
    exit 0
else
    exit 1
fi