# syntax=docker/dockerfile:1
FROM scratch
COPY --parents ./src/**/*.txt /parents/
COPY --link ./bin/app /usr/local/bin/app
ADD --link https://example.com/archive.tar.gz /tmp/archive.tar.gz