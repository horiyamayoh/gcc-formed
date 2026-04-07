ARG GCC_IMAGE=gcc:15
FROM ${GCC_IMAGE}

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    git \
    python3 \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
ENV PATH="/root/.cargo/bin:${PATH}"

RUN rustup toolchain install 1.94.1 --profile minimal --component cargo --component rustfmt --component clippy && \
    rustup default 1.94.1

WORKDIR /workspace
