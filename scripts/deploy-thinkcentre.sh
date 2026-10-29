#!/usr/bin/env bash
# Deploy Orbital to thinkcentre. Idempotent: run it again after any change.
#
#   ./scripts/deploy-thinkcentre.sh
#
# It builds the dashboard bundle, builds the release binary with that bundle
# embedded, installs the user unit + nginx site, and then proves the result by
# fetching real bodies (not status codes) through nginx.
set -euo pipefail

HOST="${HOST:-thinkcentre}"
APP="/personal/documents/code/rust/orbital"
TARGET="\$HOME/.cargo-target/orbital"
BIN="$TARGET/release/orbital"
PUBLIC_PORT="${PUBLIC_PORT:-8034}"
BACKEND_PORT="${BACKEND_PORT:-8041}"
NGINX_FILE="/etc/nginx/sites-available/orbital"
UNIT="\$HOME/.config/systemd/user/orbital.service"
APP_ENV="\$HOME/.config/systemd/user/orbital-app.env"

say() { printf '\n== %s\n' "$*"; }

say "0/7 make the host's tree match the remote — it builds from ITS checkout, not yours"
ssh "$HOST" bash -s <<REMOTE
set -euo pipefail
cd $APP
git fetch -q origin main
git checkout -q -B main origin/main
git log --oneline -1
REMOTE

say "1/7 build the dashboard bundle (it is embedded in the binary, so this comes first)"
ssh "$HOST" bash -s <<REMOTE
set -euo pipefail
cd $APP/web
npm ci --no-audit --no-fund >/dev/null
node node_modules/vite/bin/vite.js build
test -f dist/index.html || { echo "no dist/index.html"; exit 1; }
echo "bundle: \$(ls dist/assets 2>/dev/null | wc -l) chunky files"
REMOTE

say "2/7 build the release binary"
ssh "$HOST" bash -s <<REMOTE
set -euo pipefail
cd $APP
export CARGO_TARGET_DIR=$TARGET
cargo build --release 2>&1 | tail -2
test -x $BIN || { echo "no binary at $BIN"; exit 1; }
stat -c '%y %n' $BIN
REMOTE

say "3/7 app environment (idempotent; keeps the session salt stable across deploys)"
ssh "$HOST" bash -s <<REMOTE
set -euo pipefail
f=$APP_ENV
umask 077
if [ ! -f "\$f" ]; then
  SALT=\$(openssl rand -hex 32)
  cat > "\$f" <<EOF
BIND_ADDR=127.0.0.1:$BACKEND_PORT
PUBLIC_BASE=https://orbital.polarisocial.xyz
SECURE_COOKIES=1
SESSION_SALT=\$SALT
HUB_HANDLE=criticalnexus
RUST_LOG=orbital=info,tower_http=warn
EOF
  echo "wrote \$f"
else
  echo "kept existing \$f"
fi
chmod 600 "\$f"
REMOTE

say "4/7 user unit"
ssh "$HOST" bash -s <<'REMOTE'
set -euo pipefail
mkdir -p "$HOME/.config/systemd/user"
cat > "$HOME/.config/systemd/user/orbital.service" <<'UNIT'
[Unit]
Description=Orbital — a directory of makers, ranked by what people click
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
WorkingDirectory=/personal/documents/code/rust/orbital
EnvironmentFile=%h/.config/systemd/user/orbital-db.env
EnvironmentFile=%h/.config/systemd/user/orbital-app.env
ExecStart=%h/.cargo-target/orbital/release/orbital serve
Restart=always
RestartSec=3
# The binary embeds the whole dashboard; a modest cap is plenty and keeps a
# runaway allocation from taking the other sites down with it.
MemoryMax=512M
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=default.target
UNIT

# The link checker: a directory of outbound links rots silently, so something has to ask whether
# they still resolve. Oneshot, daily. No restart policy: if a run fails the next one is a day away
# and the failure belongs in the journal, not in a restart loop.
cat > "$HOME/.config/systemd/user/orbital-checklinks.service" <<'UNIT'
[Unit]
Description=Orbital — check every outbound link and record which ones still answer
After=network-online.target
Wants=network-online.target

[Service]
Type=oneshot
WorkingDirectory=/personal/documents/code/rust/orbital
EnvironmentFile=%h/.config/systemd/user/orbital-db.env
EnvironmentFile=%h/.config/systemd/user/orbital-app.env
ExecStart=%h/.cargo-target/orbital/release/orbital check-links --limit 500
Nice=10
NoNewPrivileges=true
PrivateTmp=true
UNIT

cat > "$HOME/.config/systemd/user/orbital-checklinks.timer" <<'UNIT'
[Unit]
Description=Daily Orbital link check

[Timer]
OnCalendar=*-*-* 04:20:00
Persistent=true
RandomizedDelaySec=1800

[Install]
WantedBy=timers.target
UNIT
systemctl --user daemon-reload
systemctl --user enable --now orbital-checklinks.timer
echo "unit written; link checker timer enabled"
REMOTE

say "5/7 nginx site"
# The cache directory has to exist before nginx parses proxy_cache_path, and nginx must own it.
ssh "$HOST" "sudo mkdir -p /var/cache/nginx/orbital_api && sudo chown -R www-data:www-data /var/cache/nginx/orbital_api"
ssh "$HOST" "sudo tee $NGINX_FILE >/dev/null" <<REMOTE
# Orbital — one surface (DECISIONS D9). The app no longer reads the Host header:
# every name reaches the same directory. The old adult hostname is kept only so
# links already shared keep working, and it 301s to the canonical origin.

# --- the public read API is public, which means it is a target -----------------
# Cloudflare terminates TLS, so \$remote_addr is a Cloudflare edge address for every
# visitor. Keying the limiter on it would put the whole internet in one bucket and
# rate-limit the estate to a crawl, so the key is CF-Connecting-IP when present and
# the socket address when the request did not come through the tunnel.
map \$http_cf_connecting_ip \$orbital_client {
    ""      \$binary_remote_addr;
    default \$http_cf_connecting_ip;
}
limit_req_zone  \$orbital_client zone=orbital_api:10m rate=60r/m;
limit_conn_zone \$orbital_client zone=orbital_api_conn:10m;

# A five-second cache in front of the origin. The app already says max-age=60; the
# point of this one is to absorb a burst without the app doing the work twice.
proxy_cache_path /var/cache/nginx/orbital_api levels=1:2 keys_zone=orbital_api_cache:10m
                 max_size=64m inactive=10m use_temp_path=off;

server {
    listen $PUBLIC_PORT;
    listen [::]:$PUBLIC_PORT;
    server_name orbital.polarisocial.xyz;

    client_max_body_size 2m;
    gzip on;
    gzip_vary on;
    gzip_proxied any;
    gzip_min_length 512;
    gzip_types text/plain text/css application/javascript application/json application/xml image/svg+xml;

    # A scrape of /api/v1 can be a lot of queries per second from one address; 40 at
    # once is generous for a client and cheap for us.
    limit_conn orbital_api_conn 40;

    # The read API: limited, cached, and still carrying the estate security headers.
    # NOTE: any add_header inside this location stops inheritance from the http
    # block, so the snippet is repeated here on purpose.
    location /api/v1/ {
        limit_req zone=orbital_api burst=30 nodelay;
        limit_req_status 429;

        proxy_pass http://127.0.0.1:$BACKEND_PORT;
        proxy_http_version 1.1;
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto https;
        proxy_read_timeout 30s;

        proxy_cache orbital_api_cache;
        proxy_cache_key "\$scheme\$host\$request_uri";
        proxy_cache_valid 200 5s;
        proxy_cache_use_stale error timeout updating http_500 http_502 http_503 http_504;
        proxy_cache_background_update on;
        proxy_cache_lock on;

        include /etc/nginx/snippets/estate-security-headers.conf;
        add_header X-Cache-Status \$upstream_cache_status always;
        # 429 without a Retry-After is a client that cannot back off politely.
        add_header Retry-After 2 always;
    }

    location / {
        proxy_pass http://127.0.0.1:$BACKEND_PORT;
        proxy_http_version 1.1;
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        # The origin is only reachable through Cloudflare's tunnel, which
        # speaks plain HTTP on the loopback hop. Forwarding \$scheme would tell
        # the app "http" and it would then write http:// into canonicals and
        # og:url for pages the visitor loaded over https.
        proxy_set_header X-Forwarded-Proto https;
        proxy_set_header Upgrade \$http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_read_timeout 30s;
    }

    # The dashboard is a hashed bundle: cache it hard, but never cache the shell
    # or an API response, because a stale shell pins a dead chunk hash.
    location /assets/ {
        proxy_pass http://127.0.0.1:$BACKEND_PORT;
        proxy_set_header Host \$host;
        proxy_set_header Accept-Encoding "";
        expires 7d;
        add_header Cache-Control "public, max-age=604800";
    }
}

# The retired adult surface. A 301, not a 404: people and bots already hold URLs
# on this name, and D9 moved the content rather than deleting it.
server {
    listen $PUBLIC_PORT;
    listen [::]:$PUBLIC_PORT;
    server_name orbitalnsfw.polarisocial.xyz;
    return 301 https://orbital.polarisocial.xyz\$request_uri;
}
REMOTE

say "6/7 enable, migrate, seed, restart"
ssh "$HOST" bash -s <<REMOTE
set -euo pipefail
sudo ln -sfn $NGINX_FILE /etc/nginx/sites-enabled/orbital
sudo nginx -t
sudo systemctl reload nginx
systemctl --user daemon-reload
systemctl --user enable orbital >/dev/null
systemctl --user restart orbital
for i in \$(seq 1 30); do
  if curl -sf -o /dev/null --max-time 2 http://127.0.0.1:$BACKEND_PORT/healthz; then break; fi
  sleep 0.5
done
curl -s http://127.0.0.1:$BACKEND_PORT/healthz
echo
set -a; . \$HOME/.config/systemd/user/orbital-db.env; set +a
$BIN migrate
$BIN seed
REMOTE

say "7/7 verify through nginx and the public hostname"
ssh "$HOST" bash -s <<REMOTE
set -euo pipefail
hdr=\$(curl -sI "http://127.0.0.1:$PUBLIC_PORT/" | head -1)
echo "nginx /            \$hdr"
echo -n "title:             "; curl -s "http://127.0.0.1:$PUBLIC_PORT/" | grep -o '<title>[^<]*' | head -1
echo -n "dashboard shell:   "; curl -s "http://127.0.0.1:$PUBLIC_PORT/app/" | grep -c 'app-' || true
echo -n "adult 301:         "; curl -s -o /dev/null -w '%{http_code}\n' -H 'Host: orbitalnsfw.polarisocial.xyz' "http://127.0.0.1:$PUBLIC_PORT/"
echo -n "sfw profile:       "; curl -s -o /dev/null -w '%{http_code}\n' "http://127.0.0.1:$PUBLIC_PORT/@criticalnexus"
echo -n "adult profile:     "; curl -s -o /dev/null -w '%{http_code}\n' "http://127.0.0.1:$PUBLIC_PORT/@steeltitan"
REMOTE

say "done"
echo "public:  https://orbital.polarisocial.xyz"
echo "adult:   https://orbitalnsfw.polarisocial.xyz (301 → orbital.polarisocial.xyz)"
