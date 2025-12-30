FROM randomdude/gcc-cross-x86_64-elf

# Ensuring everything is up to date
RUN apt-get update
RUN apt-get upgrade -y

# Tools
# Assembly
RUN apt-get install -y nasm
RUN apt-get install -y xorriso 

# grub
RUN apt-get install -y grub-pc-bin
RUN apt-get install -y grub-common

# Rust
RUN apt-get install -y rustc

RUN apt-get install -y curl build-essential

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal \
    && export PATH="/root/.cargo/bin:${PATH}" \
    && rustup default nightly \
    && rustup component add rust-src

RUN apt-get install -y qemu-system


VOLUME /root/env
WORKDIR /root/env