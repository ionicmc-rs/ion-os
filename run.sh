# deprecated, do not use

make build-x86_64
if [ $? != 0 ]; then
    echo run.sh: Failed To Run Make.
    exit
fi
qemu-system-x86_64 dist/x86_64/kernel.iso
if [ $? != 0 ]; then
    echo run.sh: Failed To Run QEMU.
    exit
fi