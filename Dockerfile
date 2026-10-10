# フォントのビルドと検証に使う道具一式。
# ベースイメージは digest で、apt は snapshot.debian.org の日付で固定する (同じ入力から同じ道具)。
FROM debian:trixie-slim@sha256:a29215f6a35e51e22adffa17f89e9d2ef06214e64a2bad10d765c46aea49f11f
ARG SNAPSHOT=20261001T000000Z
RUN rm -f /etc/apt/sources.list.d/debian.sources \
 && echo "deb [check-valid-until=no] http://snapshot.debian.org/archive/debian/${SNAPSHOT} trixie main" > /etc/apt/sources.list \
 && echo "deb [check-valid-until=no] http://snapshot.debian.org/archive/debian-security/${SNAPSHOT} trixie-security main" >> /etc/apt/sources.list \
 && apt-get -o Acquire::Retries=5 update \
 && apt-get -o Acquire::Retries=5 install -y --no-install-recommends \
      fontforge-nox python3-fonttools python3-brotli woff2 \
      python3-freetype python3-numpy p7zip-full lhasa ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /w
