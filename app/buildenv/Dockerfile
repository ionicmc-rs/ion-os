FROM randomdude/gcc-cross-x86_64-elf

# Ensuring everything is up to date
RUN apt-get update \
    && apt-get install -y nasm \
    && apt-get install -y xorriso  \
    && apt-get install -y grub-pc-bin \
    && apt-get install -y grub-common \
    && apt-get install -y curl build-essential 

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal
ENV PATH="/root/.cargo/bin:${PATH}"
RUN rustup default nightly \
    && rustup update \
    && rustup component add rust-src

VOLUME /root/env
WORKDIR /root/env