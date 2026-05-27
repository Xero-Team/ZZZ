# syntax=docker/dockerfile:1.11
# escape=`
# check=skip=JSONArgsRecommended;error=true

ARG BASE=alpine
FROM --platform=$BUILDPLATFORM ${BASE}:3.20 AS build
WORKDIR /src
COPY --from=nginx:latest /etc/nginx/nginx.conf /tmp/nginx.conf
COPY <<-"EOF" /entrypoint.sh
  echo "hello ${TARGETOS}"
EOF
RUN --mount=type=cache,target=/root/.cache `
    --network=none `
    --security=sandbox `
    echo building
RUN <<EOF
echo shell heredoc
EOF
HEALTHCHECK --interval=5m --timeout=3s --start-period=10s --start-interval=5s --retries=3 CMD ["/bin/check-health"]
ENTRYPOINT ["/bin/sh", "/entrypoint.sh"]

FROM build AS final
ARG TARGETOS
ENV APP_HOME=/app BIN_DIR=${APP_HOME}/bin
LABEL org.opencontainers.image.title="demo image"
ADD --checksum=sha256:24454f830cdb571e2c4ad15481119c43b3cafd48dd869a9b2945d1036d1dc68d https://example.com/archive.tar.gz /tmp/
EXPOSE 80/tcp 443/udp
USER 1000:1000
VOLUME ["/data", "/cache"]
STOPSIGNAL SIGKILL
SHELL ["/bin/sh", "-c"]
ONBUILD COPY --from=build /out/app /usr/local/bin/app