FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 sdkwork \
    && useradd --uid 10001 --gid 10001 --no-create-home --home-dir /nonexistent --shell /usr/sbin/nologin sdkwork

WORKDIR /app

# The build context is the verified sdkwork-webserver/ root extracted from the release archive.
COPY --chown=10001:10001 . /app/

ENV SDKWORK_WEBSERVER_APP_ROOT=/app \
    SDKWORK_WEBSERVER_ENVIRONMENT=production \
    SDKWORK_WEBSERVER_CONFIG_PROFILE=production \
    SDKWORK_WEBSERVER_DEPLOYMENT_PROFILE=cloud \
    SDKWORK_WEBSERVER_RUNTIME_TARGET=container \
    SDKWORK_WEBSERVER_SERVER_CONFIG_FILE=/app/etc/data-plane/website.cloud.config.json

USER 10001:10001
EXPOSE 8080

ENTRYPOINT ["/app/bin/sdkwork-webserver-website-delivery-edge-runtime"]
