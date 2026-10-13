# Published browser runtime, resolved and acquired on 2026-09-09.
FROM mcr.microsoft.com/playwright@sha256:eff16c30e6f3f4af0a03fa4b706120d5e9b0891c344a27d64559aff5900a4a27
ENV DEBIAN_FRONTEND=noninteractive PIP_DISABLE_PIP_VERSION_CHECK=1 DO_NOT_TRACK=1 SEMGREP_SEND_METRICS=off
RUN apt-get update && apt-get install -y --no-install-recommends python3-venv && rm -rf /var/lib/apt/lists/*
WORKDIR /opt/tools
RUN npm install --ignore-scripts --no-audit --no-fund --save-exact @playwright/test@1.63.0
RUN python3 -m venv /opt/python && /opt/python/bin/pip install --no-cache-dir schemathesis==4.26.1 semgrep==1.176.0
ENV PATH="/opt/python/bin:/opt/tools/node_modules/.bin:${PATH}" NODE_PATH=/opt/tools/node_modules
# Runs supply --network none, a read-only rootfs and source mount, bounded tmpfs,
# non-root user, dropped capabilities, pids/CPU/memory limits. No daemon socket mount.
WORKDIR /tmp
USER pwuser
