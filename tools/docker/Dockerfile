FROM ubuntu:18.04
LABEL maintainer="Edoardo Morassutto <edoardo.morassutto@gmail.com>"

ARG UID=1000
ARG GID=1000

# install dependencies
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -yy \
    asymptote \
    build-essential \
    fpc \
    latexmk \
    libseccomp-dev \
    python \
    python-sortedcontainers \
    python3 \
    python3-sortedcontainers \
    texlive \
    texlive-latex-extra \
    wget \
  && rm -rf /var/lib/apt/lists/*

# task-maker-rust version (required)
ARG TM_VERSION

# install task-maker-rust
RUN (test -n "$TM_VERSION" || (echo "Please use --build-arg TM_VERSION=0.3.X" >&2 && exit 1)) \
  && wget https://github.com/edomora97/task-maker-rust/releases/download/v${TM_VERSION}/task-maker-rust_${TM_VERSION}_amd64.deb \
  && dpkg -i task-maker-rust_${TM_VERSION}_amd64.deb \
  && rm task-maker-rust_${TM_VERSION}_amd64.deb

# drop root privileges
RUN groupadd -g $GID user \
  && useradd -m -g $GID -u $UID user
USER user

# server-client port
EXPOSE 27182
# server-worker port
EXPOSE 27183

# start task-maker-rust server and worker
ADD entrypoint.sh healthcheck.sh /
CMD /entrypoint.sh

# check the status of the server and the workers
HEALTHCHECK --interval=5s CMD /healthcheck.sh