# Ubuntu 16.04 to keep the library versions backward-compatible
FROM ubuntu:16.04

LABEL org.opencontainers.image.source="https://github.com/edomora97/task-maker-rust"
LABEL maintainer="Edoardo Morassutto <edoardo.morassutto@gmail.com>"

# we want to use bash, not sh
SHELL ["/bin/bash", "-c"]

# install dependencies
RUN apt update && \
    apt install -yy curl build-essential libseccomp-dev

# install rustup and rust stable
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal

# install cargo-deb
RUN source $HOME/.cargo/env && cargo install cargo-deb

# add the build script
ADD build_release.sh /

# where the source code will be mounted
VOLUME /source

# build the release on `docker run`
CMD /build_release.sh