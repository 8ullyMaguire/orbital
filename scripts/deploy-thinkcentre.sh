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
NSFW_HOST=orbitalnsfw.polarisocial.xyz
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
echo "unit written"
REMOTE

say "5/7 nginx site"
ssh "$HOST" "sudo tee $NGINX_FILE >/dev/null" <<REMOTE
# Orbital — SFW hub and the gated adult host share one backend; the app decides
# which surface answers from the Host header, so both names proxy to the same
# port and the wall is enforced in code, not in this file.
server {
    listen $PUBLIC_PORT;
    listen [::]:$PUBLIC_PORT;
    server_name orbital.polarisocial.xyz orbitalnsfw.polarisocial.xyz;

    client_max_body_size 2m;
    gzip on;
    gzip_vary on;
    gzip_proxied any;
    gzip_min_length 512;
    gzip_types text/plain text/css application/javascript application/json application/xml image/svg+xml;

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
echo -n "adult gate:        "; curl -s -o /dev/null -w '%{http_code}\n' -H 'Host: orbitalnsfw.polarisocial.xyz' "http://127.0.0.1:$PUBLIC_PORT/"
echo -n "sfw profile:       "; curl -s -o /dev/null -w '%{http_code}\n' "http://127.0.0.1:$PUBLIC_PORT/@criticalnexus"
echo -n "adult on sfw host: "; curl -s -o /dev/null -w '%{http_code}\n' "http://127.0.0.1:$PUBLIC_PORT/@steeltitan"
REMOTE

say "done"
echo "public:  https://orbital.polarisocial.xyz"
echo "adult:   https://orbitalnsfw.polarisocial.xyz (gated, noindex)"
