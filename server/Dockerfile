FROM golang:1.24-alpine AS build
WORKDIR /src
COPY go.mod ./
COPY *.go ./
RUN CGO_ENABLED=0 go build -trimpath -ldflags="-s -w" -o /bts-server .

FROM alpine:3.20
RUN adduser -D -u 10001 bts && mkdir /data && chown bts /data
COPY --from=build /bts-server /usr/local/bin/bts-server
USER bts
ENV BTS_ADDR=:4001 BTS_DATA=/data
EXPOSE 4001
VOLUME /data
HEALTHCHECK --interval=30s --timeout=5s CMD ["bts-server", "-health"]
ENTRYPOINT ["bts-server"]
