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
# Nightly dump, and a restore test that proves the dump is a backup rather than a file. The test
# runs monthly, not nightly: it drops and recreates a scratch database, and that is not a thing to
# do while somebody is using the machine.
cat > "$HOME/.config/systemd/user/orbital-backup.service" <<'UNIT'
[Unit]
Description=Orbital — nightly database dump

[Service]
Type=oneshot
EnvironmentFile=%h/.config/systemd/user/orbital-db.env
ExecStart=/bin/bash -c 'mkdir -p %h/backups/orbital && pg_dump -Fc "$DATABASE_URL" > %h/backups/orbital/orbital-$(date +%%F).dump.part && mv %h/backups/orbital/orbital-$(date +%%F).dump.part %h/backups/orbital/orbital-$(date +%%F).dump && find %h/backups/orbital -name "*.dump" -mtime +14 -delete'
Nice=10
NoNewPrivileges=true
PrivateTmp=true
UNIT

cat > "$HOME/.config/systemd/user/orbital-backup.timer" <<'UNIT'
[Unit]
Description=Nightly Orbital dump

[Timer]
OnCalendar=*-*-* 03:40:00
Persistent=true
RandomizedDelaySec=600

[Install]
WantedBy=timers.target
UNIT

cat > "$HOME/.config/systemd/user/orbital-restore-test.service" <<'UNIT'
[Unit]
Description=Orbital — prove the newest dump restores (destroys and recreates a scratch database)

[Service]
Type=oneshot
ExecStart=/bin/bash %h/code/rust/orbital/scripts/restore-test.sh
Nice=15
UNIT

cat > "$HOME/.config/systemd/user/orbital-restore-test.timer" <<'UNIT'
[Unit]
Description=Monthly Orbital restore test

[Timer]
OnCalendar=*-*-01 04:10:00
Persistent=true

[Install]
WantedBy=timers.target
UNIT

cat > "$HOME/.config/systemd/user/orbital-prune.service" <<'UNIT'
[Unit]
Description=Orbital — forget anonymous visitors who stopped coming

[Service]
Type=oneshot
EnvironmentFile=%h/.config/systemd/user/orbital-db.env
ExecStart=%h/.cargo-target/orbital/release/orbital prune --days 180
Nice=15
NoNewPrivileges=true
UNIT

cat > "$HOME/.config/systemd/user/orbital-prune.timer" <<'UNIT'
[Unit]
Description=Weekly Orbital prune

[Timer]
OnCalendar=Sun 03:20:00
Persistent=true
RandomizedDelaySec=900

[Install]
WantedBy=timers.target
UNIT

# ------------------------------------------------------------------ snapshots
# "You moved 12 → 9" needs yesterday, and yesterday cannot be reconstructed later. Weekly is the
# cadence the dashboard sentence implies; running it more often only costs rows.
cat > "$HOME/.config/systemd/user/orbital-snapshot.service" <<'UNIT'
[Unit]
Description=Orbital — record today's rank for every member in every vertical

[Service]
Type=oneshot
WorkingDirectory=/personal/documents/code/rust/orbital
EnvironmentFile=%h/.config/systemd/user/orbital-db.env
EnvironmentFile=%h/.config/systemd/user/orbital-app.env
ExecStart=%h/.cargo-target/orbital/release/orbital snapshot
Nice=15
NoNewPrivileges=true
UNIT

cat > "$HOME/.config/systemd/user/orbital-snapshot.timer" <<'UNIT'
[Unit]
Description=Weekly Orbital rank snapshot

[Timer]
OnCalendar=Sun 03:50:00
Persistent=true
RandomizedDelaySec=900

[Install]
WantedBy=timers.target
UNIT

# ------------------------------------------------------------------ titles and cards
# Both are cosmetic and both are expensive per item, so they get one daily pass instead of running
# inside a request: fetch the missing page titles, then re-render the cards that mention them.
cat > "$HOME/.config/systemd/user/orbital-metadata.service" <<'UNIT'
[Unit]
Description=Orbital — fetch page titles and favicons, then refresh the social cards

[Service]
Type=oneshot
WorkingDirectory=/personal/documents/code/rust/orbital
EnvironmentFile=%h/.config/systemd/user/orbital-db.env
EnvironmentFile=%h/.config/systemd/user/orbital-app.env
ExecStart=%h/.cargo-target/orbital/release/orbital fetch-titles --limit 200
ExecStartPost=%h/.cargo-target/orbital/release/orbital og --out /personal/documents/code/rust/orbital/static/og
Nice=15
NoNewPrivileges=true
PrivateTmp=true
UNIT

cat > "$HOME/.config/systemd/user/orbital-metadata.timer" <<'UNIT'
[Unit]
Description=Daily Orbital metadata pass

[Timer]
OnCalendar=*-*-* 05:10:00
Persistent=true
RandomizedDelaySec=1800

[Install]
WantedBy=timers.target
UNIT

# ------------------------------------------------------------------ uptime probe
# The probe writes a timestamp, and `orbital doctor` reads it. That is the whole alerting path: a
# bot that messages you is a liability to maintain; a file that goes stale is not.
cat > "$HOME/.config/systemd/user/orbital-uptime.service" <<'UNIT'
[Unit]
Description=Orbital — probe our own health endpoint and stamp the result

[Service]
Type=oneshot
EnvironmentFile=%h/.config/systemd/user/orbital-app.env
ExecStart=/bin/bash -c 'code=$(curl -s -o /dev/null -w "%%{http_code}" --max-time 10 "http://127.0.0.1:8041/healthz"); printf "%%s %%s\n" "$(date -Is)" "$code" > %h/backups/orbital/uptime.txt'
Nice=15
NoNewPrivileges=true
UNIT

cat > "$HOME/.config/systemd/user/orbital-uptime.timer" <<'UNIT'
[Unit]
Description=Orbital uptime probe every ten minutes

[Timer]
OnCalendar=*:0/10
Persistent=false

[Install]
WantedBy=timers.target
UNIT

systemctl --user daemon-reload
systemctl --user enable --now orbital-checklinks.timer orbital-backup.timer orbital-restore-test.timer orbital-prune.timer orbital-snapshot.timer orbital-metadata.timer orbital-uptime.timer
echo "units written; checklinks, backup, restore-test, prune, snapshot, metadata and uptime timers enabled"
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
# The tuner is unauthenticated and every apply writes rows: it needs a harder limit than the API.
limit_req_zone  \$orbital_client zone=orbital_write:10m rate=20r/m;
limit_conn_zone \$orbital_client zone=orbital_api_conn:10m;

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

        # No proxy_cache here on purpose. With a cache in front, nginx answers 200 from the
        # cache and never consults the app's ETag, so a conditional request stops being one.
        # Cloudflare's edge absorbs the burst instead - which only works because the app stopped
        # sending Set-Cookie on /api/v1.

        include /etc/nginx/snippets/estate-security-headers.conf;
        # 429 without a Retry-After is a client that cannot back off politely.
        add_header Retry-After 2 always;
    }

    # POST /prefs and /prefs/reset: a form a person submits a few times, and an endpoint a script
    # can fill a table with. 20 a minute with a burst of 6 is generous for the former.
    location ^~ /prefs {
        limit_req zone=orbital_write burst=6 nodelay;
        limit_req_status 429;

        proxy_pass http://127.0.0.1:$BACKEND_PORT;
        proxy_http_version 1.1;
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto https;

        include /etc/nginx/snippets/estate-security-headers.conf;
        add_header Retry-After 3 always;
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

say "6b/7 estate list, social cards, and the rasteriser they need"
# The status page lists whatever nginx actually serves, so it cannot drift from the network it
# describes. The card renderer needs a rasteriser; without one it writes SVG and says so.
ssh "$HOST" 'cd /personal/documents/code/rust/orbital && mkdir -p static/og &&   { grep -rhoE "server_name[[:space:]]+[^;]+" /etc/nginx/sites-available /etc/nginx/sites-enabled 2>/dev/null     | sed -E "s/server_name[[:space:]]+//" | tr " " "\n" | grep -E "^[a-z0-9.-]+\.[a-z]{2,}$" | grep -vE "^(example|localhost|www)\." | sort -u     | awk -F. "{print \$0\" | \"\$1}" ; } > static/estate.txt && wc -l < static/estate.txt'
ssh "$HOST" 'command -v rsvg-convert >/dev/null || sudo apt-get install -y -qq librsvg2-bin' || true
ssh "$HOST" 'cd /personal/documents/code/rust/orbital && set -a && . ~/.config/systemd/user/orbital-db.env && . ~/.config/systemd/user/orbital-app.env && set +a && ~/.cargo-target/orbital/release/orbital og --out /personal/documents/code/rust/orbital/static/og 2>&1 | tail -1'
ssh "$HOST" 'cd /personal/documents/code/rust/orbital && set -a && . ~/.config/systemd/user/orbital-db.env && . ~/.config/systemd/user/orbital-app.env && set +a && ~/.cargo-target/orbital/release/orbital doctor 2>&1 | tail -3'

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
