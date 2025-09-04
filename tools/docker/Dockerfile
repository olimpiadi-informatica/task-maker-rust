FROM ubuntu:24.04

# Example invocation of docker build
# $ docker build \
#     --build-arg TM_VERSION=X.Y.Z \
#     --build-arg VCS_REF="$(git rev-parse HEAD)" \
#     --build-arg BUILD_DATE="$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
#     -t <yourrepo>/task-maker-rust:X.Y.Z
#       .

ARG TM_VERSION        # task-maker-rust version (required)
ARG VCS_REF           # git commit SHA for v${TM_VERSION}
ARG BUILD_DATE        # e.g. 2025-09-04T12:34:56Z (RFC 3339 UTC)
ARG IMAGE_URL
ARG DOCS_URL='https://github.com/olimpiadi-informatica/task-maker-rust#readme'
ARG VENDOR='Olimpiadi Italiane di Informatica'
ARG BASE_NAME='ubuntu:24.04'
ARG BASE_DIGEST='sha256:9cbed754112939e914291337b5e554b07ad7c392491dba6daf25eef1332a22e8'

LABEL \
  org.opencontainers.image.title="task-maker-rust" \
  org.opencontainers.image.description="task-maker-rust server and worker to build programming tasks for CMS." \
  org.opencontainers.image.version="${TM_VERSION}" \
  org.opencontainers.image.revision="${VCS_REF}" \
  org.opencontainers.image.created="${BUILD_DATE}" \
  org.opencontainers.image.url="${IMAGE_URL}" \
  org.opencontainers.image.documentation="${DOCS_URL}" \
  org.opencontainers.image.source="https://github.com/olimpiadi-informatica/task-maker-rust" \
  org.opencontainers.image.authors="task-maker-rust contributors" \
  org.opencontainers.image.vendor="${VENDOR}" \
  org.opencontainers.image.licenses="MPL-2.0" \
  org.opencontainers.image.base.name="${BASE_NAME}" \
  org.opencontainers.image.base.digest="${BASE_DIGEST}"

ARG TM_UID=1000
ARG TM_GID=1000

ENV RUST_LOG='info'
ENV RUST_BACKTRACE=1

# run the following as root
USER root

# install dependencies
RUN apt-get update \
  && DEBIAN_FRONTEND=noninteractive apt-get install -yy \
    asymptote \
    build-essential \
    fpc \
    latexmk \
    libseccomp-dev \
    libssl-dev \
    python3 \
    python3-sortedcontainers \
    texlive \
    texlive-latex-extra \
    wget \
    curl \
  && rm -rf /var/lib/apt/lists/*

# delete default ubuntu user, this removes also the group
RUN userdel --remove ubuntu

# create a group and a user called taskmaker, create the home directory
RUN groupadd ${TM_GID:+-g "${TM_GID}"} taskmaker
RUN useradd -m -g taskmaker ${TM_UID:+-u "${TM_UID}"} taskmaker

# use /opt/rustup for rustup
RUN mkdir -p /opt/rustup && chown taskmaker:taskmaker /opt/rustup

# use /opt/task-maker-rust as work directory
RUN mkdir -p /opt/task-maker-rust && chown taskmaker:taskmaker /opt/task-maker-rust

# install rust and build task-maker-rust as unprivileged user
USER taskmaker

# install rust and cargo
WORKDIR /opt/rustup
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o rustup-init \
  && chmod a+x ./rustup-init \
  && ./rustup-init -y

# install task-maker-rust
WORKDIR /opt/task-maker-rust
RUN (test -n "$TM_VERSION" || (echo "Please use --build-arg TM_VERSION=X.Y.Z" >&2 && exit 1)) \
  && wget https://github.com/olimpiadi-informatica/task-maker-rust/archive/refs/tags/"v${TM_VERSION}.tar.gz" \
  && tar -xvzf "v${TM_VERSION}.tar.gz" \
  && rm "v${TM_VERSION}.tar.gz"

# cargo build
RUN . /home/taskmaker/.cargo/env \
  && (cd /opt/task-maker-rust/"task-maker-rust-${TM_VERSION}" && cargo build --release)

# symlink `task-maker` and `tusk-maker-tools` into /usr/local/bin/
USER root
RUN ln -s /opt/task-maker-rust/"task-maker-rust-${TM_VERSION}"/target/release/task-maker /usr/local/bin/ \
   && ln -s /opt/task-maker-rust/"task-maker-rust-${TM_VERSION}"/target/release/task-maker /usr/local/bin/task-maker-rust \
   && ln -s /opt/task-maker-rust/"task-maker-rust-${TM_VERSION}"/target/release/task-maker-tools /usr/local/bin/

# run everything as a unprivileged user
# (docker still needs --privileged to run because rask-maker-rust needs privileges to create a sandbox)
USER taskmaker
WORKDIR /home/taskmaker

# server-client port
EXPOSE 27182
# server-worker port
EXPOSE 27183

# start task-maker-rust server and worker
ADD entrypoint.sh healthcheck.sh /home/taskmaker

ENTRYPOINT ["/home/taskmaker/entrypoint.sh"]

# check the status of the server and the workers
HEALTHCHECK --interval=5s \
  CMD /home/taskmaker/healthcheck.sh || exit 1
