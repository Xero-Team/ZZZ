# syntax=docker/dockerfile-upstream:master
FROM busybox
ENV FOO=/bar str=foobarbaz
WORKDIR ${FOO:-/fallback}
WORKDIR ${FOO:+/set}
WORKDIR ${str#f*b}
WORKDIR ${str##f*b}
WORKDIR ${str%b*}
WORKDIR ${str%%b*}
WORKDIR ${str/ba/fo}
WORKDIR ${str//ba/fo}
COPY \$FOO /quux
HEALTHCHECK NONE
ONBUILD RUN --mount=from=config,target=/opt/appconfig echo ok