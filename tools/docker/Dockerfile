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
RUN groupadd ${TM_GID:+-g "${TM_GID}"} task-maker
RUN useradd -m -g task-maker ${TM_UID:+-u "${TM_UID}"} task-maker

# use /home/task-maker as work directory
WORKDIR /home/task-maker

# install task-maker-rust using .deb from releases
ARG TM_DEB_VERSION="${TM_VERSION}-1.ubuntu-24.04"
ARG TM_DEB_NAME="task-maker-rust_${TM_DEB_VERSION}_amd64.deb"

RUN (test -n "$TM_VERSION" || (echo "Please use --build-arg TM_VERSION=X.Y.Z" >&2 && exit 1)) \
  && wget "https://github.com/olimpiadi-informatica/task-maker-rust/releases/download/v${TM_VERSION}/${TM_DEB_NAME}" \
  && dpkg -i "${TM_DEB_NAME}" \
  && rm -rf "${TM_DEB_NAME}"

# run everything as a unprivileged user
# (docker still needs --privileged to run because rask-maker-rust needs privileges to create a sandbox)
USER task-maker

# server-client port
EXPOSE 27182
# server-worker port
EXPOSE 27183

# start task-maker-rust server and worker
ADD entrypoint.py healthcheck.sh /home/task-maker

ENTRYPOINT ["/home/task-maker/entrypoint.py"]

# check the status of the server and the workers
HEALTHCHECK --interval=5s \
  CMD /home/task-maker/healthcheck.sh || exit 1
